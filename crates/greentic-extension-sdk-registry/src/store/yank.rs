//! Yank / unyank for the Greentic Store.
//!
//! Withdrawing a bad release is the one remediation the store offers that does
//! not rewrite history: a yanked version stays downloadable, so a lockfile
//! pinning it still resolves, but it disappears from the extension's version
//! list and is never chosen as `latestVersion`. Nobody installs it fresh.
//!
//! Deliberately NOT on the [`ExtensionRegistry`](crate::registry::ExtensionRegistry)
//! trait: yanking is a store-server concept with no meaning for a local
//! filesystem registry or an OCI reference, so it lives as an inherent method
//! rather than forcing two backends to implement a `todo!()`.
//!
//! The alternative — republishing over the bad version with `--force` — mutates
//! the bytes served under a version number consumers may have pinned by
//! sha256, and cannot be undone. Prefer yanking.

use reqwest::StatusCode;

use crate::error::RegistryError;

use super::GreenticStoreRegistry;

/// Body of a yank request.
///
/// Always serialized and sent, even when `reason` is `None`. The `OpenAPI` spec marks
/// the request body `required: false`, but the deployed store rejects a
/// bodyless POST with `415 Unsupported Media Type — Expected request with
/// Content-Type: application/json`. An omitted reason is `{}`, not "no body".
#[derive(serde::Serialize)]
struct YankBody<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'a str>,
}

impl GreenticStoreRegistry {
    /// Mark a published version do-not-install-fresh.
    ///
    /// `reason` is stored by the server and shown to anyone who inspects the
    /// version; pass one — six months later nobody remembers why.
    ///
    /// # Errors
    ///
    /// [`RegistryError::AuthRequired`] when no token is configured or the
    /// server rejects it, [`RegistryError::NotFound`] when the version does not
    /// exist, is owned by someone else, or is already yanked (the server does
    /// not distinguish these), and [`RegistryError::Storage`] for any other
    /// non-success status.
    pub async fn yank(
        &self,
        name: &str,
        version: &str,
        reason: Option<&str>,
    ) -> Result<(), RegistryError> {
        let token = self.require_token()?;
        let resp = self
            .client
            .post(self.yank_url(name, version))
            .bearer_auth(token)
            .json(&YankBody { reason })
            .send()
            .await?;
        self.interpret_yank_status(resp.status(), resp, name, version, "yank")
            .await
    }

    /// Reverse a [`yank`](Self::yank), putting the version back in circulation.
    ///
    /// # Errors
    ///
    /// Same shape as [`yank`](Self::yank); `NotFound` additionally covers "was
    /// not yanked in the first place".
    pub async fn unyank(&self, name: &str, version: &str) -> Result<(), RegistryError> {
        let token = self.require_token()?;
        let resp = self
            .client
            .delete(self.yank_url(name, version))
            .bearer_auth(token)
            .send()
            .await?;
        self.interpret_yank_status(resp.status(), resp, name, version, "unyank")
            .await
    }

    fn yank_url(&self, name: &str, version: &str) -> String {
        self.url(&format!("/api/v1/extensions/{name}/{version}/yank"))
    }

    /// Token check runs before the URL guard is even reached, and both run
    /// before any bytes leave the process — an insecure base URL must never
    /// carry a bearer token in cleartext.
    fn require_token(&self) -> Result<&str, RegistryError> {
        self.ensure_secure_url()?;
        self.token.as_deref().ok_or_else(|| {
            RegistryError::AuthRequired(format!(
                "no token configured for registry '{}'; run: gtdx login --registry {}",
                self.name, self.name
            ))
        })
    }

    async fn interpret_yank_status(
        &self,
        status: StatusCode,
        resp: reqwest::Response,
        name: &str,
        version: &str,
        action: &str,
    ) -> Result<(), RegistryError> {
        if status.is_success() {
            return Ok(());
        }
        if status == StatusCode::UNAUTHORIZED {
            return Err(RegistryError::AuthRequired(format!(
                "401 from '{}'. Token expired? Re-run: gtdx login --registry {}",
                self.name, self.name
            )));
        }
        if status == StatusCode::NOT_FOUND {
            return Err(RegistryError::NotFound {
                name: name.to_string(),
                version: version.to_string(),
            });
        }
        let body = resp.text().await.unwrap_or_default();
        Err(RegistryError::Storage(format!(
            "store {action} failed: {status} {body}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn registry(uri: &str, token: Option<&str>) -> GreenticStoreRegistry {
        GreenticStoreRegistry::new("greentic-store", uri, token.map(str::to_string))
            .with_insecure_allowed(true)
    }

    #[tokio::test]
    async fn yank_posts_the_reason_with_bearer_auth() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/extensions/greentic.telco-x/1.0.0/yank"))
            .and(header("authorization", "Bearer tok"))
            .and(body_json(serde_json::json!({ "reason": "inert build" })))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;

        registry(&server.uri(), Some("tok"))
            .yank("greentic.telco-x", "1.0.0", Some("inert build"))
            .await
            .expect("yank succeeds");
    }

    #[tokio::test]
    async fn yank_without_reason_still_sends_a_json_body() {
        let server = MockServer::start().await;
        // Regression pin. The OpenAPI marks the body `required: false`, so the
        // first cut sent no body at all — and the deployed store answered
        // `415 Unsupported Media Type: Expected request with Content-Type:
        // application/json`. Asserting the header here is the point of the
        // test; a mock server accepts a bodyless POST happily, which is
        // exactly why this shipped broken until it ran against the real store.
        Mock::given(method("POST"))
            .and(path("/api/v1/extensions/ext/1.0.0/yank"))
            .and(header("content-type", "application/json"))
            .and(body_json(serde_json::json!({})))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;

        registry(&server.uri(), Some("tok"))
            .yank("ext", "1.0.0", None)
            .await
            .expect("yank succeeds");
    }

    #[tokio::test]
    async fn unyank_deletes_the_same_path() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/api/v1/extensions/ext/1.0.0/yank"))
            .and(header("authorization", "Bearer tok"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;

        registry(&server.uri(), Some("tok"))
            .unyank("ext", "1.0.0")
            .await
            .expect("unyank succeeds");
    }

    #[tokio::test]
    async fn missing_token_fails_before_any_request() {
        // No mock is mounted: if a request went out, wiremock answers 404 and
        // the assertion below would see NotFound instead of AuthRequired.
        let server = MockServer::start().await;
        let err = registry(&server.uri(), None)
            .yank("ext", "1.0.0", None)
            .await
            .unwrap_err();
        assert!(
            matches!(err, RegistryError::AuthRequired(_)),
            "expected AuthRequired, got {err}"
        );
    }

    #[tokio::test]
    async fn insecure_url_refuses_before_sending_the_token() {
        // A cleartext remote must be rejected client-side; the bearer token
        // must never cross the wire to find out.
        let reg = GreenticStoreRegistry::new(
            "evil",
            "http://store.greentic.ai",
            Some("super-secret".into()),
        );
        let err = reg.yank("ext", "1.0.0", None).await.unwrap_err();
        assert!(
            matches!(err, RegistryError::InsecureRegistryUrl(_)),
            "expected InsecureRegistryUrl, got {err}"
        );
    }

    #[tokio::test]
    async fn unauthorized_is_reported_as_auth_required() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let err = registry(&server.uri(), Some("stale"))
            .yank("ext", "1.0.0", None)
            .await
            .unwrap_err();
        assert!(
            matches!(err, RegistryError::AuthRequired(_)),
            "expected AuthRequired, got {err}"
        );
    }

    #[tokio::test]
    async fn not_found_covers_unknown_unowned_and_already_yanked() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let err = registry(&server.uri(), Some("tok"))
            .yank("ext", "9.9.9", None)
            .await
            .unwrap_err();
        match err {
            RegistryError::NotFound { name, version } => {
                assert_eq!(name, "ext");
                assert_eq!(version, "9.9.9");
            }
            other => panic!("expected NotFound, got {other}"),
        }
    }

    #[tokio::test]
    async fn other_failures_surface_the_server_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
            .mount(&server)
            .await;

        let err = registry(&server.uri(), Some("tok"))
            .yank("ext", "1.0.0", None)
            .await
            .unwrap_err();
        let rendered = err.to_string();
        assert!(
            rendered.contains("boom") && rendered.contains("yank"),
            "server body and action must reach the user: {rendered}"
        );
    }
}
