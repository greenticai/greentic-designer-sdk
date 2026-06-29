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

#[test]
fn from_str_rejects_multibyte_without_panicking() {
    // 62 ascii + one 2-byte char = 64 bytes, 63 chars. Must error, not panic.
    let s = format!("{}{}", "a".repeat(62), "é");
    assert_eq!(s.len(), 64, "test string must be exactly 64 bytes");
    let parsed: Result<greentic_extension_sdk_contract::Sha256, _> = s.parse();
    assert!(parsed.is_err());
}

#[test]
fn from_str_rejects_midchar_boundary_without_panicking() {
    // 61 ascii + one 3-byte char (中) = 64 bytes, 62 chars.
    // The slice s[60..62] splits inside the 3-byte char → char-boundary panic
    // in the old str-slice implementation. Must return an error, never panic.
    let s = format!("{}{}", "a".repeat(61), "中");
    assert_eq!(s.len(), 64, "test string must be exactly 64 bytes");
    let parsed: Result<greentic_extension_sdk_contract::Sha256, _> = s.parse();
    assert!(parsed.is_err());
}
