use greentic_extension_sdk_contract::DescribeJson;
use greentic_extension_sdk_contract::describe::{Signature, SignatureAlgorithm};

/// Build an unsigned `DescribeJson` test fixture.
fn sample_describe() -> DescribeJson {
    let json = serde_json::json!({
        "apiVersion": "greentic.ai/v2",
        "kind": "DesignExtension",
        "compat": {
            "min_designer_version": ">=1.0.0",
            "min_runner_version": "^0.12.0",
            "contract_version": "1.2.0"
        },
        "metadata": {
            "id": "greentic.canonicalize-test",
            "name": "Canonicalize Test",
            "version": "0.1.0",
            "summary": "test fixture",
            "author": { "name": "test" },
            "license": "MIT"
        },
        "engine": { "greenticDesigner": "*", "extRuntime": "*" },
        "capabilities": { "offered": [], "required": [] },
        "runtime": {
            "memoryLimitMB": 64,
            "permissions": { "network": [], "secrets": [], "callExtensionKinds": [] },
            "components": {
                "stub": {
                    "oci_ref": "oci://ghcr.io/example/stub:latest",
                    "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    "world": "greentic:component/stub@0.1.0"
                }
            }
        },
        "contributions": {},
        "signature": null
    });
    serde_json::from_value(json).expect("sample describe parses")
}

#[test]
fn parses_ed25519_with_key_id() {
    let v = serde_json::json!({
        "algorithm": "ed25519",
        "publicKey": "AAA=",
        "value": "BBB=",
        "keyId": "greentic-root-2026-05"
    });
    let s: Signature = serde_json::from_value(v).unwrap();
    assert_eq!(s.algorithm, SignatureAlgorithm::Ed25519);
    assert_eq!(s.key_id.as_deref(), Some("greentic-root-2026-05"));
}

#[test]
fn key_id_optional() {
    let v = serde_json::json!({
        "algorithm": "ed25519",
        "publicKey": "AAA=",
        "value": "BBB="
    });
    let s: Signature = serde_json::from_value(v).unwrap();
    assert!(s.key_id.is_none());
}

#[test]
fn rejects_unknown_algorithm() {
    let v = serde_json::json!({
        "algorithm": "rsa-pss-sha256",
        "publicKey": "AAA=",
        "value": "BBB="
    });
    let r: Result<Signature, _> = serde_json::from_value(v);
    assert!(r.is_err());
}

#[test]
fn verify_with_key_rejects_mismatched_signer() {
    use ed25519_dalek::{SigningKey, VerifyingKey};
    use greentic_extension_sdk_contract::{
        sign_describe, verify_describe_self_consistent, verify_describe_with_key,
    };
    use rand::rngs::OsRng;

    let signer = SigningKey::generate(&mut OsRng);
    let mut describe = sample_describe();
    sign_describe(&mut describe, &signer).unwrap();

    verify_describe_self_consistent(&describe).expect("self-consistent");
    verify_describe_with_key(&describe, &signer.verifying_key()).expect("anchored ok");

    let attacker: VerifyingKey = SigningKey::generate(&mut OsRng).verifying_key();
    assert!(verify_describe_with_key(&describe, &attacker).is_err());
}
