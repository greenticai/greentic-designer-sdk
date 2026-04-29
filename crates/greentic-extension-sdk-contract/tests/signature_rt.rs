use ed25519_dalek::SigningKey;
use greentic_extension_sdk_contract::{
    DescribeJson, artifact_sha256, canonical_signing_payload, sign_describe, sign_ed25519,
    verify_describe, verify_ed25519,
};
use rand::rngs::OsRng;

#[test]
fn sha256_is_deterministic() {
    assert_eq!(artifact_sha256(b"hello"), artifact_sha256(b"hello"));
    assert_ne!(artifact_sha256(b"hello"), artifact_sha256(b"world"));
}

#[test]
fn round_trip_sign_verify() {
    let sk = SigningKey::generate(&mut OsRng);
    let pk = sk.verifying_key();
    let pk_b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, pk.to_bytes());
    let payload = b"arbitrary payload";
    let sig = sign_ed25519(&sk, payload);
    verify_ed25519(&pk_b64, &sig, payload).expect("signature must verify");
}

#[test]
fn tampered_payload_fails_verification() {
    let sk = SigningKey::generate(&mut OsRng);
    let pk = sk.verifying_key();
    let pk_b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, pk.to_bytes());
    let sig = sign_ed25519(&sk, b"original");
    let err = verify_ed25519(&pk_b64, &sig, b"tampered").unwrap_err();
    assert!(format!("{err}").contains("verify"));
}

fn sample_describe_with_sig(sig_value: Option<&str>) -> DescribeJson {
    let json = serde_json::json!({
        "apiVersion": "greentic.ai/v1",
        "kind": "DesignExtension",
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
        "runtime": { "component": "x.wasm", "memoryLimitMB": 64, "permissions": {} },
        "contributions": {},
        "signature": sig_value.map(|v| serde_json::json!({
            "algorithm": "ed25519",
            "publicKey": "AAAA",
            "value": v
        }))
    });
    serde_json::from_value(json).expect("sample describe parses")
}

#[test]
fn canonical_payload_omits_signature_field() {
    let with_sig = sample_describe_with_sig(Some("SIG_A"));
    let bytes_with = canonical_signing_payload(&with_sig).expect("canonicalize with sig");
    let without_sig = sample_describe_with_sig(None);
    let bytes_without = canonical_signing_payload(&without_sig).expect("canonicalize without sig");
    assert_eq!(
        bytes_with, bytes_without,
        "canonical bytes must ignore .signature"
    );
}

#[test]
fn canonical_payload_is_deterministic_across_serde_round_trip() {
    let d1 = sample_describe_with_sig(None);
    let json = serde_json::to_string(&d1).unwrap();
    let d2: DescribeJson = serde_json::from_str(&json).unwrap();
    let b1 = canonical_signing_payload(&d1).unwrap();
    let b2 = canonical_signing_payload(&d2).unwrap();
    assert_eq!(b1, b2, "canonical form must survive serde round trip");
}

#[test]
fn sign_describe_populates_signature_field() {
    let sk = SigningKey::generate(&mut OsRng);
    let mut d = sample_describe_with_sig(None);
    assert!(d.signature.is_none());
    sign_describe(&mut d, &sk).expect("sign");
    let sig = d.signature.as_ref().expect("signature populated");
    assert_eq!(sig.algorithm, "ed25519");
    assert_eq!(sig.public_key.len(), 44, "base64 of 32 bytes is 44 chars");
    assert_eq!(sig.value.len(), 88, "base64 of 64 bytes is 88 chars");
}

#[test]
fn sign_describe_strips_preexisting_signature_before_signing() {
    // If caller passes a describe that already has a stale signature,
    // sign_describe should canonicalize as-if signature was None so the
    // new sig is not computed over a signed payload.
    let sk = SigningKey::generate(&mut OsRng);
    let mut d_preexisting = sample_describe_with_sig(Some("STALE"));
    let mut d_fresh = sample_describe_with_sig(None);
    sign_describe(&mut d_preexisting, &sk).expect("sign");
    sign_describe(&mut d_fresh, &sk).expect("sign");
    assert_eq!(
        d_preexisting.signature.as_ref().unwrap().value,
        d_fresh.signature.as_ref().unwrap().value,
        "signing a stale-signed describe must produce same signature as signing clean",
    );
}

#[test]
fn sign_describe_then_verify_describe_roundtrip() {
    let sk = SigningKey::generate(&mut OsRng);
    let mut d = sample_describe_with_sig(None);
    sign_describe(&mut d, &sk).expect("sign");
    verify_describe(&d).expect("verify");
}

#[test]
fn verify_describe_missing_signature_fails() {
    let d = sample_describe_with_sig(None);
    let err = verify_describe(&d).unwrap_err();
    assert!(matches!(
        err,
        greentic_extension_sdk_contract::ContractError::SignatureInvalid(_)
    ));
    assert!(format!("{err}").contains("missing signature"));
}

#[test]
fn verify_describe_rejects_tampered_metadata() {
    let sk = SigningKey::generate(&mut OsRng);
    let mut d = sample_describe_with_sig(None);
    sign_describe(&mut d, &sk).expect("sign");
    d.metadata.version = "99.99.99".into();
    let err = verify_describe(&d).unwrap_err();
    assert!(matches!(
        err,
        greentic_extension_sdk_contract::ContractError::SignatureInvalid(_)
    ));
}

#[test]
fn verify_describe_rejects_non_ed25519_algorithm() {
    let sk = SigningKey::generate(&mut OsRng);
    let mut d = sample_describe_with_sig(None);
    sign_describe(&mut d, &sk).expect("sign");
    d.signature.as_mut().unwrap().algorithm = "sha256-hmac".into();
    let err = verify_describe(&d).unwrap_err();
    assert!(format!("{err}").contains("unsupported algorithm"));
}

#[test]
fn verify_describe_survives_serde_round_trip() {
    // Field-order-independence test: sign, re-serialize through serde_json,
    // re-parse, verify still passes. Proves JCS canonicalization is stable.
    let sk = SigningKey::generate(&mut OsRng);
    let mut d1 = sample_describe_with_sig(None);
    sign_describe(&mut d1, &sk).expect("sign");
    let json = serde_json::to_string(&d1).unwrap();
    let d2: DescribeJson = serde_json::from_str(&json).unwrap();
    verify_describe(&d2).expect("verify after serde roundtrip");
}
