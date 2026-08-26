use greentic_extension_sdk_contract::ExtensionKind;

#[test]
fn serializes_as_pascal_case_string() {
    assert_eq!(
        serde_json::to_string(&ExtensionKind::Design).unwrap(),
        "\"DesignExtension\""
    );
    assert_eq!(
        serde_json::to_string(&ExtensionKind::Bundle).unwrap(),
        "\"BundleExtension\""
    );
    assert_eq!(
        serde_json::to_string(&ExtensionKind::Deploy).unwrap(),
        "\"DeployExtension\""
    );
}

#[test]
fn round_trips_through_json() {
    for variant in [
        ExtensionKind::Design,
        ExtensionKind::Bundle,
        ExtensionKind::Deploy,
    ] {
        let s = serde_json::to_string(&variant).unwrap();
        let back: ExtensionKind = serde_json::from_str(&s).unwrap();
        assert_eq!(back, variant);
    }
}

#[test]
fn dir_name_matches_spec() {
    assert_eq!(ExtensionKind::Design.dir_name(), "design");
    assert_eq!(ExtensionKind::Bundle.dir_name(), "bundle");
    assert_eq!(ExtensionKind::Deploy.dir_name(), "deploy");
}

#[test]
fn provider_kind_serde_roundtrip() {
    let original = ExtensionKind::Provider;
    let json = serde_json::to_string(&original).unwrap();
    assert_eq!(json, "\"ProviderExtension\"");
    let parsed: ExtensionKind = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, original);
}

#[test]
fn provider_kind_dir_name() {
    assert_eq!(ExtensionKind::Provider.dir_name(), "provider");
}

/// `wasix:mcp/router` uses a colon and a forward-slash — serde must
/// deserialize the literal string, not a pascal-case alias.
#[test]
fn wasix_mcp_router_deserializes_from_json_string() {
    let parsed: ExtensionKind = serde_json::from_str("\"wasix:mcp/router\"").unwrap();
    assert_eq!(parsed, ExtensionKind::WasixMcpRouter);
}

#[test]
fn wasix_mcp_router_serializes_to_correct_json_string() {
    let json = serde_json::to_string(&ExtensionKind::WasixMcpRouter).unwrap();
    assert_eq!(json, "\"wasix:mcp/router\"");
}

#[test]
fn wasix_mcp_router_serde_roundtrip() {
    let original = ExtensionKind::WasixMcpRouter;
    let serialized = serde_json::to_string(&original).unwrap();
    let deserialized: ExtensionKind = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized, original);
}

#[test]
fn wasix_mcp_router_dir_name() {
    assert_eq!(ExtensionKind::WasixMcpRouter.dir_name(), "mcp");
}

#[test]
fn addon_kind_serde_roundtrip() {
    let original = ExtensionKind::Addon;
    let json = serde_json::to_string(&original).unwrap();
    assert_eq!(json, "\"AddonExtension\"");
    let parsed: ExtensionKind = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, original);
}

#[test]
fn addon_kind_dir_name() {
    assert_eq!(ExtensionKind::Addon.dir_name(), "addon");
}

/// `wire_name` must agree with what serde actually emits. If someone adds a
/// variant and sets `#[serde(rename)]` without updating `wire_name`, this
/// catches it — the two are separate declarations and will otherwise drift.
#[test]
fn wire_name_matches_serde() {
    for kind in ExtensionKind::ALL.iter().copied() {
        let json = serde_json::to_string(&kind).expect("kind serializes");
        let expected = format!("\"{}\"", kind.wire_name());
        assert_eq!(
            json, expected,
            "wire_name disagrees with serde for {kind:?}"
        );
    }
}

#[test]
fn from_wire_round_trips_every_variant() {
    for kind in ExtensionKind::ALL.iter().copied() {
        assert_eq!(
            ExtensionKind::from_wire(kind.wire_name()),
            Some(kind),
            "from_wire failed to round-trip {kind:?}"
        );
    }
}

#[test]
fn from_dir_name_round_trips_every_variant() {
    for kind in ExtensionKind::ALL.iter().copied() {
        assert_eq!(
            ExtensionKind::from_dir_name(kind.dir_name()),
            Some(kind),
            "from_dir_name failed to round-trip {kind:?}"
        );
    }
}

#[test]
fn unknown_strings_are_rejected() {
    // `AddonExtension`/`addon` used to be the canonical "not a kind yet"
    // example here; now that `ExtensionKind::Addon` exists, `BogusExtension`/
    // `bogus` fill that role instead.
    assert_eq!(ExtensionKind::from_wire("BogusExtension"), None);
    assert_eq!(ExtensionKind::from_wire(""), None);
    assert_eq!(ExtensionKind::from_dir_name("bogus"), None);
    assert_eq!(ExtensionKind::from_dir_name(""), None);
}
