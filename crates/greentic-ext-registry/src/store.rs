use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::error::RegistryError;
use crate::registry::ExtensionRegistry;
use crate::types::{ExtensionArtifact, ExtensionMetadata, ExtensionSummary, SearchQuery};

pub struct GreenticStoreRegistry {
    name: String,
    base_url: String,
    token: Option<String>,
    client: Client,
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
        }
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
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SummaryDto {
    name: String,
    latest_version: String,
    kind: greentic_ext_contract::ExtensionKind,
    summary: String,
    #[serde(default)]
    downloads: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MetadataDto {
    describe: greentic_ext_contract::DescribeJson,
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
    kind: greentic_ext_contract::ExtensionKind,
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
        let metadata = self.metadata(name, version).await?;
        let bytes = self
            .with_auth(
                self.client
                    .get(self.url(&format!("/api/v1/extensions/{name}/{version}/artifact"))),
            )
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?
            .to_vec();
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
