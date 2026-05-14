use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use greentic_extension_sdk_contract::{
    ArtifactKind, ArtifactToolOutput, GeneratedArtifact, hex, validate_artifact_tool_output,
};
use sha2::{Digest, Sha256};

pub fn assert_valid_artifact_output_json(json: &str) {
    let output: ArtifactToolOutput =
        serde_json::from_str(json).expect("artifact output JSON must parse");
    validate_artifact_tool_output(&output).expect("artifact output must validate");
}

#[must_use]
pub fn fixture_generated_artifact(kind: &str, filename: &str, bytes: &[u8]) -> GeneratedArtifact {
    GeneratedArtifact {
        kind: ArtifactKind(kind.into()),
        filename: filename.into(),
        media_type: infer_media_type(filename).into(),
        sha256: hex::encode(&Sha256::digest(bytes)),
        bytes_base64: Some(B64.encode(bytes)),
        uri: None,
        metadata_json: None,
    }
}

fn infer_media_type(filename: &str) -> &'static str {
    match filename.rsplit_once('.').map(|(_, ext)| ext) {
        Some("json") => "application/json",
        Some("txt" | "md") => "text/plain",
        Some("yaml" | "yml") => "application/yaml",
        Some("gtpack") => "application/vnd.greentic.gtpack",
        Some("gtbundle") => "application/vnd.greentic.gtbundle",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use greentic_extension_sdk_contract::ArtifactToolOutput;

    use super::*;

    #[test]
    fn fixture_generated_artifact_validates() {
        let artifact = fixture_generated_artifact("example", "example.json", b"{}");
        let output = ArtifactToolOutput {
            artifacts: vec![artifact],
            diagnostics: Vec::new(),
            preview_json: None,
        };
        let json = serde_json::to_string(&output).unwrap();
        assert_valid_artifact_output_json(&json);
    }
}
