use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::error::RegistryError;
use crate::registry::ExtensionRegistry;
use crate::types::{ExtensionArtifact, ExtensionMetadata, ExtensionSummary, SearchQuery};

/// Upper bound on a downloaded artifact (256 MiB). Caps memory use so a
/// malicious or misbehaving registry cannot OOM the client with a huge body.
const DEFAULT_MAX_ARTIFACT_BYTES: usize = 256 * 1024 * 1024;

pub struct GreenticStoreRegistry {
    name: String,
    base_url: String,
    token: Option<String>,
    client: Client,
    max_artifact_bytes: usize,
    insecure_allowed: bool,
}

impl GreenticStoreRegistry {
    pub fn new(
        name: impl Into<String>,
        base_url: impl Into<String>,
        token: Option<String>,
    ) -> Self {
        Self {
            name: name.into(),
            base_url: base_url.into(),
            token,
            client: Client::builder()
                .user_agent(concat!("gtdx/", env!("CARGO_PKG_VERSION")))
                .build()
                .expect("reqwest client"),
            max_artifact_bytes: DEFAULT_MAX_ARTIFACT_BYTES,
            insecure_allowed: false,
        }
    }

    /// Override the maximum downloadable artifact size (bytes).
    #[must_use]
    pub fn with_max_artifact_bytes(mut self, limit: usize) -> Self {
        self.max_artifact_bytes = limit;
        self
    }

    /// Opt-in to talk to a non-HTTPS, non-loopback registry URL. Required for
    /// publishing into a Greentic Store that has not yet been fronted by TLS;
    /// off by default because anything that *can* be reached over HTTPS *must*
    /// be (otherwise the bearer token + signed describe travel in cleartext).
    /// Callers should only set this when they have out-of-band assurance that
    /// the network path is trusted (private VPC, SSH tunnel, dev loopback).
    #[must_use]
    pub fn with_insecure_allowed(mut self, allowed: bool) -> Self {
        self.insecure_allowed = allowed;
        self
    }

    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url.trim_end_matches('/'))
    }

    fn with_auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(token) = &self.token {
            req.bearer_auth(token)
        } else {
            req
        }
    }

    /// Guard called before every network request so an insecure base URL can
    /// never leak a bearer token or fetch an artifact over cleartext http.
    fn ensure_secure_url(&self) -> Result<(), RegistryError> {
        validate_registry_url(&self.base_url, self.insecure_allowed)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SummaryDto {
    name: String,
    latest_version: String,
    kind: greentic_extension_sdk_contract::ExtensionKind,
    summary: String,
    #[serde(default)]
    downloads: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MetadataDto {
    describe: greentic_extension_sdk_contract::DescribeJson,
    artifact_sha256: String,
    #[serde(default)]
    published_at: String,
    #[serde(default)]
    yanked: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PublishMetadata<'a> {
    ext_id: &'a str,
    ext_name: &'a str,
    version: &'a str,
    kind: greentic_extension_sdk_contract::ExtensionKind,
    artifact_sha256: &'a str,
    describe: &'a serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    signature: Option<&'a crate::publish::SignatureBlob>,
    force: bool,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct PublishResponseDto {
    url: Option<String>,
    artifact_sha256: Option<String>,
    published_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Reject registry URLs that would send bearer tokens or download artifacts in
/// cleartext. `https://` is always allowed; `http://` is allowed only for
/// loopback hosts (`localhost` / `127.0.0.1` / `::1`) to keep local dev and
/// tests working. Callers that have out-of-band assurance the path is trusted
/// (private VPC, SSH tunnel, the migration window before a Store gets TLS)
/// can pass `allow_insecure = true` to opt back into cleartext.
fn validate_registry_url(url: &str, allow_insecure: bool) -> Result<(), RegistryError> {
    if let Some(rest) = url.strip_prefix("https://") {
        if rest.is_empty() {
            return Err(RegistryError::InsecureRegistryUrl(url.into()));
        }
        return Ok(());
    }
    if let Some(rest) = url.strip_prefix("http://") {
        let host = rest
            .split(['/', ':'])
            .next()
            .unwrap_or("")
            .trim_end_matches(']')
            .trim_start_matches('[');
        if matches!(host, "localhost" | "127.0.0.1" | "::1") {
            return Ok(());
        }
        if allow_insecure {
            // The opt-in escape hatch is downgrading a non-loopback request to
            // cleartext — the bearer token and signed describe cross the wire
            // unencrypted. Make that auditable rather than silent (audit N6).
            tracing::warn!(
                host,
                "GTDX_ALLOW_INSECURE_REGISTRY: talking to remote registry over plaintext HTTP; \
                 credentials and artifacts are NOT encrypted in transit"
            );
            return Ok(());
        }
        return Err(RegistryError::InsecureRegistryUrl(url.into()));
    }
    Err(RegistryError::InsecureRegistryUrl(url.into()))
}

fn extract_existing_sha(body: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    v.get("existing_sha")
        .or_else(|| v.get("artifactSha256"))
        .and_then(|x| x.as_str())
        .map(str::to_string)
}

#[async_trait]
impl ExtensionRegistry for GreenticStoreRegistry {
    fn name(&self) -> &str {
        &self.name
    }

    async fn search(&self, query: SearchQuery) -> Result<Vec<ExtensionSummary>, RegistryError> {
        self.ensure_secure_url()?;
        let mut req = self.client.get(self.url("/api/v1/extensions"));
        if let Some(k) = query.kind {
            req = req.query(&[("kind", k.dir_name())]);
        }
        if let Some(cap) = &query.capability {
            req = req.query(&[("capability", cap.as_str())]);
        }
        if let Some(q) = &query.query {
            req = req.query(&[("q", q.as_str())]);
        }
        req = req.query(&[("page", query.page), ("limit", query.limit)]);

        let resp = self.with_auth(req).send().await?.error_for_status()?;
        let dtos: Vec<SummaryDto> = resp.json().await?;
        Ok(dtos
            .into_iter()
            .map(|d| ExtensionSummary {
                name: d.name,
                latest_version: d.latest_version,
                kind: d.kind,
                summary: d.summary,
                downloads: d.downloads,
            })
            .collect())
    }

    async fn metadata(
        &self,
        name: &str,
        version: &str,
    ) -> Result<ExtensionMetadata, RegistryError> {
        self.ensure_secure_url()?;
        let resp = self
            .with_auth(
                self.client
                    .get(self.url(&format!("/api/v1/extensions/{name}/{version}"))),
            )
            .send()
            .await?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(RegistryError::NotFound {
                name: name.into(),
                version: version.into(),
            });
        }
        let dto: MetadataDto = resp.error_for_status()?.json().await?;
        Ok(ExtensionMetadata {
            name: dto.describe.metadata.id.clone(),
            version: dto.describe.metadata.version.clone(),
            describe: dto.describe,
            artifact_sha256: dto.artifact_sha256,
            published_at: dto.published_at,
            yanked: dto.yanked,
        })
    }

    async fn fetch(&self, name: &str, version: &str) -> Result<ExtensionArtifact, RegistryError> {
        self.ensure_secure_url()?;
        let metadata = self.metadata(name, version).await?;
        let mut response = self
            .with_auth(
                self.client
                    .get(self.url(&format!("/api/v1/extensions/{name}/{version}/artifact"))),
            )
            .send()
            .await?
            .error_for_status()?;

        // Read the body in chunks with a hard size cap so a huge (or zip-bomb)
        // response cannot exhaust memory before we ever inspect it.
        let mut bytes: Vec<u8> = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if bytes.len() + chunk.len() > self.max_artifact_bytes {
                return Err(RegistryError::ArtifactTooLarge {
                    limit: self.max_artifact_bytes,
                });
            }
            bytes.extend_from_slice(&chunk);
        }

        // Verify the downloaded bytes against the digest the registry advertised
        // in its metadata. This catches truncation, corruption, and an artifact
        // swapped on the (separate) artifact endpoint. It does NOT establish
        // publisher trust — that is the trust-root work tracked under D.5.
        // Constant-time compare so a forged digest can't be probed byte-by-byte
        // via response timing (audit P0-3).
        let computed = greentic_extension_sdk_contract::artifact_sha256(&bytes);
        crate::digest::verify_digest(&metadata.artifact_sha256, &computed)?;

        Ok(ExtensionArtifact {
            name: metadata.name,
            version: metadata.version,
            describe: metadata.describe,
            bytes,
            signature: None,
        })
    }

    async fn publish(
        &self,
        req: crate::publish::PublishRequest,
    ) -> Result<crate::publish::PublishReceipt, RegistryError> {
        self.ensure_secure_url()?;
        let token = self.token.as_deref().ok_or_else(|| {
            RegistryError::AuthRequired(format!(
                "no token configured for registry '{}'; run: gtdx login --registry {}",
                self.name, self.name
            ))
        })?;

        let describe_bytes = serde_json::to_vec(&req.describe)?;
        let describe_value: serde_json::Value = serde_json::from_slice(&describe_bytes)?;
        let metadata = PublishMetadata {
            ext_id: &req.ext_id,
            ext_name: &req.ext_name,
            version: &req.version,
            kind: req.kind,
            artifact_sha256: &req.artifact_sha256,
            describe: &describe_value,
            signature: req.signature.as_ref(),
            force: req.force,
        };
        let metadata_json = serde_json::to_string(&metadata)?;

        let form = reqwest::multipart::Form::new()
            .text("metadata", metadata_json)
            .part(
                "artifact",
                reqwest::multipart::Part::bytes(req.artifact_bytes)
                    .file_name(format!("{}-{}.gtxpack", req.ext_name, req.version))
                    .mime_str("application/zip")
                    .map_err(|e| RegistryError::Storage(format!("mime: {e}")))?,
            );

        let resp = self
            .client
            .post(self.url("/api/v1/extensions"))
            .bearer_auth(token)
            .multipart(form)
            .send()
            .await?;

        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(RegistryError::AuthRequired(format!(
                "401 from '{}'. Token expired? Re-run: gtdx login --registry {}",
                self.name, self.name
            )));
        }
        if status == reqwest::StatusCode::CONFLICT {
            let body = resp.text().await.unwrap_or_default();
            return Err(RegistryError::VersionExists {
                existing_sha: extract_existing_sha(&body).unwrap_or_else(|| "unknown".into()),
            });
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(RegistryError::Storage(format!(
                "store publish failed: {status} {body}"
            )));
        }
        let dto: PublishResponseDto = resp.json().await.unwrap_or_default();
        Ok(crate::publish::PublishReceipt {
            url: dto.url.unwrap_or_else(|| {
                format!(
                    "{}/api/v1/extensions/{}/{}",
                    self.base_url.trim_end_matches('/'),
                    req.ext_id,
                    req.version
                )
            }),
            sha256: dto
                .artifact_sha256
                .unwrap_or_else(|| req.artifact_sha256.clone()),
            published_at: dto.published_at.unwrap_or_else(chrono::Utc::now),
            signed: req.signature.is_some(),
        })
    }

    async fn list_versions(&self, name: &str) -> Result<Vec<String>, RegistryError> {
        #[derive(Deserialize)]
        struct Dto {
            versions: Vec<String>,
        }
        self.ensure_secure_url()?;
        let resp = self
            .with_auth(
                self.client
                    .get(self.url(&format!("/api/v1/extensions/{name}"))),
            )
            .send()
            .await?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(Vec::new());
        }
        let dto: Dto = resp.error_for_status()?.json().await?;
        Ok(dto.versions)
    }
}

#[cfg(test)]
mod tests {
    use super::{GreenticStoreRegistry, validate_registry_url};
    use crate::error::RegistryError;
    use crate::registry::ExtensionRegistry;
    use crate::types::SearchQuery;

    #[test]
    fn https_url_is_allowed() {
        assert!(validate_registry_url("https://store.greentic.ai", false).is_ok());
    }

    #[test]
    fn http_localhost_is_allowed() {
        assert!(validate_registry_url("http://127.0.0.1:8080", false).is_ok());
        assert!(validate_registry_url("http://localhost:3000/api", false).is_ok());
    }

    #[test]
    fn http_remote_is_rejected() {
        assert!(matches!(
            validate_registry_url("http://store.greentic.ai", false),
            Err(RegistryError::InsecureRegistryUrl(_))
        ));
    }

    #[test]
    fn http_remote_allowed_when_insecure_opt_in() {
        // Escape hatch for trusted-path Stores that have not been fronted by
        // TLS yet. `allow_insecure = true` must let the URL through even when
        // the host is neither HTTPS nor loopback.
        assert!(validate_registry_url("http://62.171.174.152:3030", true).is_ok());
        assert!(validate_registry_url("http://store.greentic.ai", true).is_ok());
    }

    #[test]
    fn non_http_scheme_is_rejected_even_with_opt_in() {
        // The escape hatch is for `http://` only. Anything else (ftp, raw
        // hostname) is still a hard reject — the opt-in does not turn off
        // scheme parsing.
        assert!(matches!(
            validate_registry_url("ftp://store.greentic.ai", false),
            Err(RegistryError::InsecureRegistryUrl(_))
        ));
        assert!(matches!(
            validate_registry_url("ftp://store.greentic.ai", true),
            Err(RegistryError::InsecureRegistryUrl(_))
        ));
        assert!(matches!(
            validate_registry_url("store.greentic.ai", false),
            Err(RegistryError::InsecureRegistryUrl(_))
        ));
        assert!(matches!(
            validate_registry_url("store.greentic.ai", true),
            Err(RegistryError::InsecureRegistryUrl(_))
        ));
    }

    #[tokio::test]
    async fn insecure_url_refuses_network_request() {
        // A registry pointed at a cleartext remote must refuse before sending,
        // so a bearer token never crosses the wire.
        let reg =
            GreenticStoreRegistry::new("evil", "http://store.greentic.ai", Some("secret".into()));
        let err = reg.search(SearchQuery::default()).await.unwrap_err();
        assert!(
            matches!(err, RegistryError::InsecureRegistryUrl(_)),
            "expected InsecureRegistryUrl, got {err}"
        );
    }
}
