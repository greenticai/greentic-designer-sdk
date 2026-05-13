use greentic_extension_sdk_contract::Sha256;

#[test]
fn parses_lowercase_hex() {
    let s = "0".repeat(64);
    let h: Sha256 = serde_json::from_value(serde_json::Value::String(s.clone())).unwrap();
    assert_eq!(h.as_hex(), s);
    assert_eq!(h.as_bytes(), &[0u8; 32]);
}

#[test]
fn rejects_short() {
    let r: Result<Sha256, _> = serde_json::from_value(serde_json::json!("abc"));
    assert!(r.is_err());
}

#[test]
fn rejects_uppercase() {
    let s = "A".repeat(64);
    let r: Result<Sha256, _> = serde_json::from_value(serde_json::Value::String(s));
    assert!(r.is_err());
}

#[test]
fn rejects_non_hex_chars() {
    let mut s = String::from("0").repeat(63);
    s.push('z');
    let r: Result<Sha256, _> = serde_json::from_value(serde_json::Value::String(s));
    assert!(r.is_err());
}

#[test]
fn roundtrips() {
    let bytes = [0x11u8; 32];
    let h = Sha256::from_bytes(bytes);
    let s = serde_json::to_string(&h).unwrap();
    assert_eq!(s, format!("\"{}\"", "11".repeat(32)));
    let back: Sha256 = serde_json::from_str(&s).unwrap();
    assert_eq!(back, h);
}
