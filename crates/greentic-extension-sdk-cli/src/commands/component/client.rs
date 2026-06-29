//! Minimal HTTP client for the greentic-designer-admin tenant component-tools
//! write endpoint. Kept net-new and local to `gtdx component` because it talks
//! to designer-admin (a `gts_`-authed surface), not to an extension registry.

use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};

/// JSON body for `POST /api/v1/designer/tenant/me/component-tools`.
///
/// Path A: we register a source URL and never send `operations` — the Designer
/// introspects the component's operations after registration.
#[derive(Debug, Serialize)]
pub struct RegisterRequest {
    pub name: String,
    pub source_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_operations: Option<Vec<String>>,
    pub enabled: bool,
}

/// JSON body for `PUT .../component-tools/{id}/roles`.
#[derive(Debug, Serialize)]
pub struct RolesRequest {
    pub roles: Vec<String>,
}

/// The `component` object returned inside a 201 response.
#[derive(Debug, Deserialize)]
pub struct ComponentResponse {
    pub id: String,
}

#[derive(Debug, Deserialize)]
struct RegisterEnvelope {
    component: ComponentResponse,
}

pub struct AdminClient {
    base_url: String,
    token: String,
    tenant: String,
    user: String,
    client: Client,
}

impl AdminClient {
    pub fn new(base_url: &str, token: &str, tenant: &str, user: &str) -> anyhow::Result<Self> {
        let client = Client::builder()
            .user_agent(concat!("gtdx/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            token: token.to_string(),
            tenant: tenant.to_string(),
            user: user.to_string(),
            client,
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }

    /// POST the registration. On 201 returns the parsed component; otherwise
    /// surfaces a clear, status-specific error including the response body.
    pub async fn register(&self, body: &RegisterRequest) -> anyhow::Result<ComponentResponse> {
        let resp = self
            .client
            .post(self.url("/api/v1/designer/tenant/me/component-tools"))
            .bearer_auth(&self.token)
            .header("X-Greentic-Tenant", &self.tenant)
            .header("X-Greentic-User", &self.user)
            .json(body)
            .send()
            .await?;

        let status = resp.status();
        if status.is_success() {
            let envelope: RegisterEnvelope = resp.json().await?;
            return Ok(envelope.component);
        }

        let body_text = resp.text().await.unwrap_or_default();
        bail_register(status, &body_text)
    }

    /// PUT the role grants for a registered component-tool.
    pub async fn set_roles(&self, id: &str, body: &RolesRequest) -> anyhow::Result<()> {
        let resp = self
            .client
            .put(self.url(&format!(
                "/api/v1/designer/tenant/me/component-tools/{id}/roles"
            )))
            .bearer_auth(&self.token)
            .header("X-Greentic-Tenant", &self.tenant)
            .header("X-Greentic-User", &self.user)
            .json(body)
            .send()
            .await?;

        let status = resp.status();
        if status.is_success() {
            return Ok(());
        }
        let body_text = resp.text().await.unwrap_or_default();
        anyhow::bail!("setting roles failed: {status} {body_text}")
    }
}

/// Map a non-2xx registration response onto a clear, actionable error.
fn bail_register(status: StatusCode, body: &str) -> anyhow::Result<ComponentResponse> {
    match status {
        StatusCode::UNAUTHORIZED => anyhow::bail!(
            "401 Unauthorized: the gts_ token was rejected. \
             Check --admin-token / GREENTIC_ADMIN_TOKEN. ({body})"
        ),
        StatusCode::FORBIDDEN => anyhow::bail!(
            "403 Forbidden: the user is not a tenant admin for this tenant. \
             --user must be a tenant-admin email. ({body})"
        ),
        StatusCode::CONFLICT => anyhow::bail!(
            "409 Conflict: a component-tool with this name is already registered \
             for this tenant — choose a different --name. ({body})"
        ),
        StatusCode::BAD_REQUEST => {
            anyhow::bail!("400 Bad Request: the server rejected the payload. ({body})")
        }
        other => anyhow::bail!("component-tool registration failed: {other} {body}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_request_serializes_expected_shape() {
        let req = RegisterRequest {
            name: "tool".into(),
            source_url: "https://example/c.wasm".into(),
            component_ref: None,
            component_version: None,
            component_digest: None,
            allowed_operations: None,
            enabled: true,
        };
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v["name"], "tool");
        assert_eq!(v["source_url"], "https://example/c.wasm");
        assert_eq!(v["enabled"], true);
        assert!(v.get("allowed_operations").is_none());
        assert!(v.get("operations").is_none());
    }

    #[test]
    fn roles_request_serializes() {
        let req = RolesRequest {
            roles: vec!["flow_editor".into()],
        };
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v["roles"], serde_json::json!(["flow_editor"]));
    }

    #[test]
    fn url_joins_without_double_slash() {
        let c = AdminClient::new("https://admin.example/", "gts_x", "acme", "u@x").unwrap();
        assert_eq!(
            c.url("/api/v1/designer/tenant/me/component-tools"),
            "https://admin.example/api/v1/designer/tenant/me/component-tools"
        );
    }

    #[test]
    fn bail_register_conflict_mentions_already_registered() {
        let err = bail_register(StatusCode::CONFLICT, "dup").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("already registered"));
    }

    #[test]
    fn bail_register_forbidden_mentions_admin() {
        let err = bail_register(StatusCode::FORBIDDEN, "no").unwrap_err();
        assert!(err.to_string().contains("tenant admin"));
    }

    #[tokio::test]
    async fn register_then_set_roles_happy_path() {
        use wiremock::matchers::{body_json, header, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        // 201 on POST register, asserting method/path/auth/tenant/user headers
        // and the exact JSON body (incl. absence of `operations`).
        Mock::given(method("POST"))
            .and(path("/api/v1/designer/tenant/me/component-tools"))
            .and(header("authorization", "Bearer gts_secret"))
            .and(header("x-greentic-tenant", "acme"))
            .and(header("x-greentic-user", "admin@acme.test"))
            .and(body_json(serde_json::json!({
                "name": "my-tool",
                "source_url": "https://store.example/c.wasm",
                "enabled": true,
            })))
            .respond_with(
                ResponseTemplate::new(201)
                    .set_body_json(serde_json::json!({ "component": { "id": "ct_123" } })),
            )
            .expect(1)
            .mount(&server)
            .await;

        // 200 on PUT roles with the expected body.
        Mock::given(method("PUT"))
            .and(path(
                "/api/v1/designer/tenant/me/component-tools/ct_123/roles",
            ))
            .and(header("authorization", "Bearer gts_secret"))
            .and(body_json(serde_json::json!({ "roles": ["flow_editor"] })))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let client =
            AdminClient::new(&server.uri(), "gts_secret", "acme", "admin@acme.test").unwrap();
        let req = RegisterRequest {
            name: "my-tool".into(),
            source_url: "https://store.example/c.wasm".into(),
            component_ref: None,
            component_version: None,
            component_digest: None,
            allowed_operations: None,
            enabled: true,
        };
        let component = client.register(&req).await.unwrap();
        assert_eq!(component.id, "ct_123");

        client
            .set_roles(
                &component.id,
                &RolesRequest {
                    roles: vec!["flow_editor".into()],
                },
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn register_conflict_is_surfaced() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/designer/tenant/me/component-tools"))
            .respond_with(ResponseTemplate::new(409).set_body_string("duplicate name"))
            .mount(&server)
            .await;

        let client = AdminClient::new(&server.uri(), "gts_x", "acme", "u@x").unwrap();
        let req = RegisterRequest {
            name: "dup".into(),
            source_url: "https://x/c.wasm".into(),
            component_ref: None,
            component_version: None,
            component_digest: None,
            allowed_operations: None,
            enabled: true,
        };
        let err = client.register(&req).await.unwrap_err();
        assert!(err.to_string().contains("already registered"));
    }
}
