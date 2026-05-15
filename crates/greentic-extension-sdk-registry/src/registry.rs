use async_trait::async_trait;

use crate::error::RegistryError;
use crate::types::{ExtensionArtifact, ExtensionMetadata, ExtensionSummary, SearchQuery};
use greentic_extension_sdk_contract::ExtensionKind;

#[async_trait]
pub trait ExtensionRegistry: Send + Sync {
    fn name(&self) -> &str;

    async fn search(&self, query: SearchQuery) -> Result<Vec<ExtensionSummary>, RegistryError>;

    async fn metadata(&self, name: &str, version: &str)
    -> Result<ExtensionMetadata, RegistryError>;

    async fn fetch(&self, name: &str, version: &str) -> Result<ExtensionArtifact, RegistryError>;

    async fn publish(
        &self,
        req: crate::publish::PublishRequest,
    ) -> Result<crate::publish::PublishReceipt, RegistryError> {
        let _ = req;
        Err(RegistryError::NotImplemented {
            hint: format!("publish not supported for registry '{}'", self.name()),
        })
    }

    async fn list_versions(&self, name: &str) -> Result<Vec<String>, RegistryError>;

    async fn list_by_kind(
        &self,
        kind: ExtensionKind,
    ) -> Result<Vec<ExtensionSummary>, RegistryError> {
        let all = self.search(SearchQuery::default()).await?;
        Ok(all.into_iter().filter(|s| s.kind == kind).collect())
    }

    async fn get_describe(
        &self,
        name: &str,
        version: &str,
    ) -> Result<greentic_extension_sdk_contract::DescribeJson, RegistryError> {
        let metadata = self.metadata(name, version).await?;
        Ok(metadata.describe)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_extension_sdk_contract::ExtensionKind;

    struct FakeRegistry {
        summaries: Vec<ExtensionSummary>,
        metadata: ExtensionMetadata,
    }

    #[async_trait::async_trait]
    impl ExtensionRegistry for FakeRegistry {
        fn name(&self) -> &'static str {
            "fake"
        }

        async fn search(
            &self,
            _query: SearchQuery,
        ) -> Result<Vec<ExtensionSummary>, RegistryError> {
            Ok(self.summaries.clone())
        }

        async fn metadata(
            &self,
            _name: &str,
            _version: &str,
        ) -> Result<ExtensionMetadata, RegistryError> {
            Ok(self.metadata.clone())
        }

        async fn fetch(
            &self,
            _name: &str,
            _version: &str,
        ) -> Result<ExtensionArtifact, RegistryError> {
            Err(RegistryError::NotFound {
                name: "artifact".into(),
                version: "0.0.0".into(),
            })
        }

        async fn list_versions(&self, _name: &str) -> Result<Vec<String>, RegistryError> {
            Ok(vec!["0.1.0".into()])
        }
    }

    fn describe() -> greentic_extension_sdk_contract::DescribeJson {
        serde_json::from_value(serde_json::json!({
            "apiVersion": "greentic.ai/v1",
            "kind": "DesignExtension",
            "metadata": {
                "id": "greentic.fake",
                "name": "Fake",
                "version": "0.1.0",
                "summary": "fake",
                "author": { "name": "Greentic" },
                "license": "MIT"
            },
            "engine": { "greenticDesigner": "*", "extRuntime": "*" },
            "capabilities": { "offered": [], "required": [] },
            "runtime": {
                "component": "extension.wasm",
                "permissions": {}
            },
            "contributions": {}
        }))
        .unwrap()
    }

    fn fake_registry() -> FakeRegistry {
        FakeRegistry {
            summaries: vec![
                ExtensionSummary {
                    name: "greentic.design".into(),
                    latest_version: "0.1.0".into(),
                    kind: ExtensionKind::Design,
                    summary: "design".into(),
                    downloads: 0,
                },
                ExtensionSummary {
                    name: "greentic.bundle".into(),
                    latest_version: "0.1.0".into(),
                    kind: ExtensionKind::Bundle,
                    summary: "bundle".into(),
                    downloads: 0,
                },
            ],
            metadata: ExtensionMetadata {
                name: "greentic.fake".into(),
                version: "0.1.0".into(),
                describe: describe(),
                artifact_sha256: "0".repeat(64),
                published_at: String::new(),
                yanked: false,
            },
        }
    }

    #[tokio::test]
    async fn default_publish_reports_not_implemented() {
        let registry = fake_registry();
        let err = registry
            .publish(crate::publish::PublishRequest {
                ext_name: "greentic.fake".into(),
                ext_id: "greentic.fake".into(),
                version: "0.1.0".into(),
                kind: ExtensionKind::Design,
                artifact_bytes: Vec::new(),
                artifact_sha256: "0".repeat(64),
                describe: describe(),
                signature: None,
                force: false,
            })
            .await
            .unwrap_err();
        assert!(err.to_string().contains("publish not supported"));
    }

    #[tokio::test]
    async fn list_by_kind_filters_search_results() {
        let registry = fake_registry();
        let design = registry.list_by_kind(ExtensionKind::Design).await.unwrap();
        assert_eq!(design.len(), 1);
        assert_eq!(design[0].name, "greentic.design");
    }

    #[tokio::test]
    async fn get_describe_returns_metadata_describe() {
        let registry = fake_registry();
        let describe = registry
            .get_describe("greentic.fake", "0.1.0")
            .await
            .unwrap();
        assert_eq!(describe.metadata.id, "greentic.fake");
    }
}
