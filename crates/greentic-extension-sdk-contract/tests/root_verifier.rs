#![cfg(feature = "testing")]

use ed25519_dalek::{Signer, SigningKey};
use greentic_extension_sdk_contract::{FixtureRootVerifier, PublisherCert, RootVerifier};
use rand::rngs::OsRng;

fn b64(bytes: &[u8]) -> String {
    use base64::{Engine, engine::general_purpose::STANDARD};
    STANDARD.encode(bytes)
}

#[test]
fn fixture_root_verifier_resolves_publisher_from_cert() {
    let root = SigningKey::generate(&mut OsRng);
    let publisher = SigningKey::generate(&mut OsRng);
    let pub_bytes = publisher.verifying_key().to_bytes();
    let cert = PublisherCert {
        publisher_public_key: b64(&pub_bytes),
        root_signature: b64(&root.sign(&pub_bytes).to_bytes()),
        key_id: None,
        not_after: None,
    };
    let verifier = FixtureRootVerifier::new(root.verifying_key());
    let resolved = verifier
        .verify_cert(&cert)
        .expect("cert chains to fixture root");
    assert_eq!(resolved.to_bytes(), pub_bytes);
}

#[test]
fn embedded_root_is_unavailable_until_provisioned() {
    let err = greentic_extension_sdk_contract::EmbeddedRootVerifier::from_embedded().unwrap_err();
    assert!(matches!(
        err,
        greentic_extension_sdk_contract::ContractError::TrustRootUnavailable(_)
    ));
}
