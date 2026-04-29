use async_trait::async_trait;
use oci_client::client::{ClientConfig, Config, ImageLayer};
use oci_client::secrets::RegistryAuth;
use oci_client::{Client, Reference};

use crate::error::RegistryError;
use crate::registry::ExtensionRegistry;
use crate::types::{ExtensionArtifact, ExtensionMetadata, ExtensionSummary, SearchQuery};

/// OCI media type for the `.gtxpack` artifact layer.
pub const GTXPACK_LAYER_MEDIA_TYPE: &str = "application/vnd.greentic.gtxpack.v1";
/// OCI media type for the minimal JSON config blob referenced by the manifest.
pub const GTXPACK_CONFIG_MEDIA_TYPE: &str = "application/vnd.greentic.gtxpack.config.v1+json";

pub struct OciRegistry {
    name: String,
    registry_host: String,
    namespace: String,
    /// Optional override: if set, publish/pull targets
    /// `<host>/<namespace>/<artifact_name>:<version>` — ignoring the ext-name
    /// that `PublishRequest`/fetch arg would otherwise supply. Used when the
    /// CLI parses `oci://<host>/<namespace>/<artifact>` and wants that last
    /// segment to be the GHCR package name rather than the extension id.
    artifact_name: Option<String>,
    auth: RegistryAuth,
    client: Client,
}

impl OciRegistry {
    pub fn new(
        name: impl Into<String>,
        registry_host: impl Into<String>,
        namespace: impl Into<String>,
        auth: Option<(String, String)>,
    ) -> Self {
        let client = Client::new(ClientConfig::default());
        Self {
            name: name.into(),
            registry_host: registry_host.into(),
            namespace: namespace.into(),
            artifact_name: None,
            auth: auth.map_or(RegistryAuth::Anonymous, |(u, p)| RegistryAuth::Basic(u, p)),
            client,
        }
    }

    /// Builder helper: pin the artifact name segment in the OCI reference so
    /// publish/pull ignore the per-request ext-name and always target the
    /// same GHCR package.
    #[must_use]
    pub fn with_artifact_name(mut self, artifact_name: impl Into<String>) -> Self {
        self.artifact_name = Some(artifact_name.into());
        self
    }

    /// Builder helper: swap anonymous auth for a bearer token (GHCR / Docker
    /// registry v2 accept any string as the "username" when the password is a
    /// PAT; the convention is username=<user> / password=<token>).
    #[must_use]
    pub fn with_bearer_auth(
        mut self,
        username: impl Into<String>,
        token: impl Into<String>,
    ) -> Self {
        self.auth = RegistryAuth::Basic(username.into(), token.into());
        self
    }

    fn reference(&self, name: &str, version: &str) -> Reference {
        let artifact = self.artifact_name.as_deref().unwrap_or(name);
        format!(
            "{}/{}/{artifact}:{version}",
            self.registry_host, self.namespace
        )
        .parse()
        .expect("valid reference")
    }
}

#[async_trait]
impl ExtensionRegistry for OciRegistry {
    fn name(&self) -> &str {
        &self.name
    }

    async fn search(&self, _query: SearchQuery) -> Result<Vec<ExtensionSummary>, RegistryError> {
        Ok(Vec::new())
    }

    async fn metadata(
        &self,
        _name: &str,
        _version: &str,
    ) -> Result<ExtensionMetadata, RegistryError> {
        Err(RegistryError::Storage(
            "OCI metadata introspection not yet implemented; use fetch() to obtain describe.json"
                .into(),
        ))
    }

    async fn fetch(&self, name: &str, version: &str) -> Result<ExtensionArtifact, RegistryError> {
        let reference = self.reference(name, version);
        let pulled = self
            .client
            .pull(
                &reference,
                &self.auth,
                vec!["application/vnd.greentic.extension.v1+zip"],
            )
            .await
            .map_err(|e| RegistryError::Oci(e.to_string()))?;

        let first_layer = pulled
            .layers
            .into_iter()
            .next()
            .ok_or_else(|| RegistryError::Storage("no layers in manifest".into()))?;

        let bytes = first_layer.data;

        let describe = {
            let cursor = std::io::Cursor::new(&bytes);
            let mut archive = zip::ZipArchive::new(cursor)
                .map_err(|e| RegistryError::Storage(format!("zip open: {e}")))?;
            let mut describe_entry = archive
                .by_name("describe.json")
                .map_err(|e| RegistryError::Storage(format!("describe missing: {e}")))?;
            let value: serde_json::Value = serde_json::from_reader(&mut describe_entry)?;
            greentic_extension_sdk_contract::schema::validate_describe_json(&value)?;
            serde_json::from_value::<greentic_extension_sdk_contract::DescribeJson>(value)?
        };

        Ok(ExtensionArtifact {
            name: describe.metadata.id.clone(),
            version: describe.metadata.version.clone(),
            describe,
            bytes,
            signature: None,
        })
    }

    async fn list_versions(&self, _name: &str) -> Result<Vec<String>, RegistryError> {
        // Real implementation would call client.list_tags — which requires an
        // authenticated, reachable registry. For Plan 2 we ship an empty-list
        // stub to keep the trait total.
        Ok(Vec::new())
    }

    async fn publish(
        &self,
        req: crate::publish::PublishRequest,
    ) -> Result<crate::publish::PublishReceipt, RegistryError> {
        let reference = self.reference(&req.ext_name, &req.version);

        let layer = ImageLayer::new(
            req.artifact_bytes,
            GTXPACK_LAYER_MEDIA_TYPE.to_string(),
            None,
        );
        // Minimal JSON config — OCI manifests require a config blob, but for
        // non-runnable artifacts the spec lets us use an empty object.
        let config = Config {
            data: b"{}".to_vec(),
            media_type: GTXPACK_CONFIG_MEDIA_TYPE.to_string(),
            annotations: None,
        };

        let response = self
            .client
            .push(&reference, &[layer], config, &self.auth, None)
            .await
            .map_err(|e| map_oci_error(&e, &self.name, &reference))?;

        Ok(crate::publish::PublishReceipt {
            url: response.manifest_url,
            sha256: req.artifact_sha256,
            published_at: chrono::Utc::now(),
            signed: req.signature.is_some(),
        })
    }
}

fn map_oci_error(
    err: &oci_client::errors::OciDistributionError,
    registry: &str,
    reference: &Reference,
) -> RegistryError {
    let rendered = format!("{err}");
    // Best-effort status-code sniffing — oci-client's error variants stringify
    // differently across versions, so match on substrings rather than concrete
    // variants so future crate upgrades stay compatible.
    if rendered.contains("401") || rendered.to_lowercase().contains("unauthorized") {
        return RegistryError::AuthRequired(format!(
            "401 from '{registry}' pushing to '{reference}'. Check token scope \
             (write:packages required for GHCR). Re-run: gtdx login --registry {registry}"
        ));
    }
    if rendered.contains("403") || rendered.to_lowercase().contains("forbidden") {
        return RegistryError::AuthRequired(format!(
            "403 from '{registry}' pushing to '{reference}'. Token lacks permission — \
             ensure write:packages scope and that the token owner can write to this repo."
        ));
    }
    if rendered.contains("409") {
        return RegistryError::VersionExists {
            existing_sha: "unknown".into(),
        };
    }
    RegistryError::Oci(rendered)
}
