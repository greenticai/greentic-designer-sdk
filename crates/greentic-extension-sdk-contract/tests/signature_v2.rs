use greentic_extension_sdk_contract::describe::{Signature, SignatureAlgorithm};

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
