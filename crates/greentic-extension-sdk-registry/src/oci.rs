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

    fn reference(&self, name: &str, version: &str) -> Result<Reference, RegistryError> {
        let artifact = self.artifact_name.as_deref().unwrap_or(name);
        build_reference(&self.registry_host, &self.namespace, artifact, version)
    }
}

/// Build an OCI [`Reference`] from its parts, returning an error instead of
/// panicking when the inputs (often config- or registry-derived) don't form a
/// valid reference.
fn build_reference(
    host: &str,
    namespace: &str,
    artifact: &str,
    version: &str,
) -> Result<Reference, RegistryError> {
    format!("{host}/{namespace}/{artifact}:{version}")
        .parse()
        .map_err(|e| RegistryError::Oci(format!("invalid OCI reference: {e}")))
}

/// Verify a pulled OCI layer's actual digest against the digest declared in the
/// image manifest, in constant time. Both arguments are full descriptor digest
/// strings (e.g. `sha256:<hex>`). Returns [`RegistryError::ArtifactHashMismatch`]
/// on mismatch (audit P0-3).
fn verify_layer_digest(declared: &str, computed: &str) -> Result<(), RegistryError> {
    crate::digest::verify_digest(declared, computed)
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
        let reference = self.reference(name, version)?;
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
            .first()
            .ok_or_else(|| RegistryError::Storage("no layers in manifest".into()))?;

        // Defense in depth: re-verify the pulled layer bytes against the digest
        // the OCI image manifest committed to (audit P0-3). `oci-client` already
        // checks blob digests during transfer, but we do not want install
        // integrity to silently depend on that implementation detail — verify
        // here, unconditionally and constant-time, before trusting the bytes.
        let declared_digest = pulled
            .manifest
            .as_ref()
            .and_then(|m| m.layers.first())
            .map(|d| d.digest.as_str())
            .ok_or_else(|| {
                RegistryError::Oci("image manifest declares no layer digest to verify".into())
            })?;
        verify_layer_digest(declared_digest, &first_layer.sha256_digest())?;

        let bytes = first_layer.data.clone();

        // Parse the describe out of the zip on a blocking thread (zip indexing +
        // decompression is sync CPU/IO that would otherwise stall the async
        // worker). Move the bytes in and hand them back to avoid a copy
        // (audit cycle-2 P3).
        let (bytes, describe) = tokio::task::spawn_blocking(
            move || -> Result<(Vec<u8>, greentic_extension_sdk_contract::DescribeJson), RegistryError> {
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
                Ok((bytes, describe))
            },
        )
        .await
        .map_err(|e| RegistryError::Storage(format!("zip task failed: {e}")))??;

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
        let reference = self.reference(&req.ext_name, &req.version)?;

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

#[cfg(test)]
mod tests {
    use super::{build_reference, verify_layer_digest};
    use crate::error::RegistryError;
    use oci_client::client::ImageLayer;

    #[test]
    fn build_reference_ok_for_valid_parts() {
        let r = build_reference("ghcr.io", "greenticai", "ext", "1.0.0").unwrap();
        assert_eq!(r.whole(), "ghcr.io/greenticai/ext:1.0.0");
    }

    #[test]
    fn build_reference_errs_instead_of_panicking_on_invalid() {
        // A tag containing a space is not a valid OCI reference; the old code
        // `.expect("valid reference")` would panic here.
        let err = build_reference("ghcr.io", "greenticai", "ext", "bad tag").unwrap_err();
        assert!(
            matches!(err, RegistryError::Oci(_)),
            "expected RegistryError::Oci, got {err}"
        );
    }

    #[test]
    fn verify_layer_digest_accepts_matching_digest() {
        // A layer whose declared digest equals its actual sha256 passes.
        let layer = ImageLayer::new(b"hello world".to_vec(), "media".into(), None);
        let declared = layer.sha256_digest();
        verify_layer_digest(&declared, &layer.sha256_digest()).unwrap();
    }

    #[test]
    fn verify_layer_digest_rejects_tampered_bytes() {
        // Declared digest computed over the honest bytes; the layer actually
        // carries swapped bytes -> mismatch must be caught (audit P0-3).
        let honest = ImageLayer::new(b"hello world".to_vec(), "media".into(), None);
        let declared = honest.sha256_digest();
        let tampered = ImageLayer::new(b"evil payload".to_vec(), "media".into(), None);
        let err = verify_layer_digest(&declared, &tampered.sha256_digest()).unwrap_err();
        assert!(matches!(err, RegistryError::ArtifactHashMismatch { .. }));
    }

    #[test]
    fn verify_layer_digest_rejects_truncated_bytes() {
        // Truncation (a partial download) changes the digest and must be caught.
        let full = ImageLayer::new(b"hello world".to_vec(), "media".into(), None);
        let declared = full.sha256_digest();
        let truncated = ImageLayer::new(b"hello".to_vec(), "media".into(), None);
        assert!(verify_layer_digest(&declared, &truncated.sha256_digest()).is_err());
    }

    #[test]
    fn verify_layer_digest_reports_expected_and_computed() {
        // The error surfaces both digests so logs make the mismatch diagnosable.
        let err = verify_layer_digest("sha256:aaaa", "sha256:bbbb").unwrap_err();
        match err {
            RegistryError::ArtifactHashMismatch { expected, computed } => {
                assert_eq!(expected, "sha256:aaaa");
                assert_eq!(computed, "sha256:bbbb");
            }
            other => panic!("expected ArtifactHashMismatch, got {other}"),
        }
    }
}
