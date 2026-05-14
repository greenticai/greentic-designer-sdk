use std::path::Path;

use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::ContractError;
use crate::hex;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GeneratedArtifact {
    pub kind: ArtifactKind,
    pub filename: String,
    pub media_type: String,
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes_base64: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata_json: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct ArtifactKind(pub String);

impl ArtifactKind {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ArtifactDiagnostic {
    pub severity: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ArtifactToolOutput {
    #[serde(default)]
    pub artifacts: Vec<GeneratedArtifact>,
    #[serde(default)]
    pub diagnostics: Vec<ArtifactDiagnostic>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_json: Option<serde_json::Value>,
}

pub fn validate_generated_artifact(artifact: &GeneratedArtifact) -> Result<(), ContractError> {
    if artifact.kind.as_str().trim().is_empty() {
        return Err(ContractError::GeneratedArtifactInvalid(
            "kind must not be empty".into(),
        ));
    }
    if artifact.filename.trim().is_empty() {
        return Err(ContractError::GeneratedArtifactInvalid(
            "filename must not be empty".into(),
        ));
    }
    if !is_safe_relative_path(&artifact.filename) {
        return Err(ContractError::GeneratedArtifactInvalid(format!(
            "filename must be a relative path without traversal: {}",
            artifact.filename
        )));
    }
    if artifact.media_type.trim().is_empty() {
        return Err(ContractError::GeneratedArtifactInvalid(
            "media_type must not be empty".into(),
        ));
    }
    if !is_lower_hex_sha256(&artifact.sha256) {
        return Err(ContractError::GeneratedArtifactInvalid(format!(
            "sha256 must be lowercase 64-character hex: {}",
            artifact.sha256
        )));
    }
    if artifact.bytes_base64.is_none() && artifact.uri.is_none() {
        return Err(ContractError::GeneratedArtifactInvalid(
            "at least one of bytes_base64 or uri must be present".into(),
        ));
    }
    if let Some(uri) = &artifact.uri
        && is_absolute_local_uri(uri)
    {
        return Err(ContractError::GeneratedArtifactInvalid(format!(
            "uri must not be an absolute local path: {uri}"
        )));
    }
    if let Some(bytes_base64) = &artifact.bytes_base64 {
        let bytes = B64.decode(bytes_base64).map_err(|e| {
            ContractError::GeneratedArtifactInvalid(format!("bytes_base64 is invalid: {e}"))
        })?;
        let actual = hex::encode(&Sha256::digest(&bytes));
        if actual != artifact.sha256 {
            return Err(ContractError::GeneratedArtifactInvalid(format!(
                "bytes_base64 sha256 mismatch: expected {}, got {actual}",
                artifact.sha256
            )));
        }
    }
    Ok(())
}

pub fn validate_artifact_tool_output(output: &ArtifactToolOutput) -> Result<(), ContractError> {
    for artifact in &output.artifacts {
        validate_generated_artifact(artifact)?;
    }
    Ok(())
}

fn is_safe_relative_path(filename: &str) -> bool {
    if filename.contains('\\')
        || (filename.len() > 2
            && filename.as_bytes()[1] == b':'
            && filename.as_bytes()[0].is_ascii_alphabetic())
    {
        return false;
    }
    let path = Path::new(filename);
    if path.is_absolute() {
        return false;
    }
    path.components()
        .all(|component| matches!(component, std::path::Component::Normal(_)))
}

fn is_lower_hex_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn is_absolute_local_uri(uri: &str) -> bool {
    Path::new(uri).is_absolute()
        || uri.starts_with("file:/")
        || (uri.len() > 2
            && uri.as_bytes()[1] == b':'
            && uri.as_bytes()[0].is_ascii_alphabetic()
            && matches!(uri.as_bytes()[2], b'/' | b'\\'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sha(bytes: &[u8]) -> String {
        hex::encode(&Sha256::digest(bytes))
    }

    fn valid_artifact() -> GeneratedArtifact {
        let bytes = br#"{"hello":"artifact"}"#;
        GeneratedArtifact {
            kind: ArtifactKind("example".into()),
            filename: "example-artifact.json".into(),
            media_type: "application/json".into(),
            sha256: sha(bytes),
            bytes_base64: Some(B64.encode(bytes)),
            uri: None,
            metadata_json: Some(serde_json::json!({ "source": "test" })),
        }
    }

    #[test]
    fn valid_generated_artifact_passes() {
        validate_generated_artifact(&valid_artifact()).unwrap();
    }

    #[test]
    fn invalid_sha_is_rejected() {
        let mut artifact = valid_artifact();
        artifact.sha256 = "ABC".into();
        let err = validate_generated_artifact(&artifact).unwrap_err();
        assert!(err.to_string().contains("sha256"));
    }

    #[test]
    fn bytes_hash_mismatch_is_rejected() {
        let mut artifact = valid_artifact();
        artifact.sha256 = "0".repeat(64);
        let err = validate_generated_artifact(&artifact).unwrap_err();
        assert!(err.to_string().contains("mismatch"));
    }

    #[test]
    fn path_traversal_filename_is_rejected() {
        let mut artifact = valid_artifact();
        artifact.filename = "../escape.json".into();
        let err = validate_generated_artifact(&artifact).unwrap_err();
        assert!(err.to_string().contains("relative path"));
    }

    #[test]
    fn windows_style_traversal_filename_is_rejected() {
        let mut artifact = valid_artifact();
        artifact.filename = r"..\escape.json".into();
        let err = validate_generated_artifact(&artifact).unwrap_err();
        assert!(err.to_string().contains("relative path"));
    }

    #[test]
    fn empty_media_type_is_rejected() {
        let mut artifact = valid_artifact();
        artifact.media_type.clear();
        let err = validate_generated_artifact(&artifact).unwrap_err();
        assert!(err.to_string().contains("media_type"));
    }

    #[test]
    fn json_roundtrip_preserves_artifact() {
        let artifact = valid_artifact();
        let json = serde_json::to_string(&artifact).unwrap();
        let parsed: GeneratedArtifact = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, artifact);
    }

    #[test]
    fn artifact_tool_output_roundtrip_and_validate() {
        let output = ArtifactToolOutput {
            artifacts: vec![valid_artifact()],
            diagnostics: vec![ArtifactDiagnostic {
                severity: "info".into(),
                message: "created".into(),
                code: Some("generated".into()),
            }],
            preview_json: Some(serde_json::json!({ "title": "Example artifact" })),
        };
        validate_artifact_tool_output(&output).unwrap();
        let json = serde_json::to_string(&output).unwrap();
        let parsed: ArtifactToolOutput = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, output);
    }

    #[test]
    fn absolute_local_uri_is_rejected() {
        let mut artifact = valid_artifact();
        artifact.bytes_base64 = None;
        artifact.uri = Some("/tmp/artifact.json".into());
        let err = validate_generated_artifact(&artifact).unwrap_err();
        assert!(err.to_string().contains("absolute local path"));
    }
}
