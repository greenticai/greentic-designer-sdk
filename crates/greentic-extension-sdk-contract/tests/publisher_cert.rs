use ed25519_dalek::{Signer, SigningKey};
use greentic_extension_sdk_contract::PublisherCert;
use rand::rngs::OsRng;

fn b64(bytes: &[u8]) -> String {
    use base64::{Engine, engine::general_purpose::STANDARD};
    STANDARD.encode(bytes)
}

#[test]
fn cert_verifies_against_issuing_root_and_rejects_wrong_root() {
    let root = SigningKey::generate(&mut OsRng);
    let publisher = SigningKey::generate(&mut OsRng);
    let pub_bytes = publisher.verifying_key().to_bytes();
    let root_sig = root.sign(&pub_bytes);

    let cert = PublisherCert {
        publisher_public_key: b64(&pub_bytes),
        root_signature: b64(&root_sig.to_bytes()),
        key_id: None,
        not_after: None,
    };

    let resolved = cert.verify(&root.verifying_key()).expect("valid cert");
    assert_eq!(resolved.to_bytes(), pub_bytes);

    let other_root = SigningKey::generate(&mut OsRng);
    assert!(cert.verify(&other_root.verifying_key()).is_err());
}

#[test]
fn cert_rejects_malformed_fields() {
    let root = SigningKey::generate(&mut OsRng);
    let bad = PublisherCert {
        publisher_public_key: "!!!not-base64!!!".into(),
        root_signature: "also-bad".into(),
        key_id: None,
        not_after: None,
    };
    assert!(bad.verify(&root.verifying_key()).is_err());
    // wrong-length but valid base64 publisher key:
    let short = PublisherCert {
        publisher_public_key: b64(&[0u8; 16]),
        root_signature: b64(&[0u8; 64]),
        key_id: None,
        not_after: None,
    };
    assert!(short.verify(&root.verifying_key()).is_err());
}
