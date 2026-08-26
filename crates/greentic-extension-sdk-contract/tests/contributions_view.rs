//! `contributions.views[]` — a UI page an extension contributes to a host
//! surface. These tests pin the full field set, because a field missing from
//! this struct is unreachable in production no matter what the author writes:
//! `deny_unknown_fields` turns it into a parse error, and the describe is
//! signed, so it cannot be patched after the fact.

use greentic_extension_sdk_contract::describe::{Contributions, Surface, View, Visibility};

fn full_view_json() -> serde_json::Value {
    serde_json::json!({
        "id": "usage-dashboard",
        "surface": "admin",
        "title_key": "view.usage_dashboard.label",
        "title_fallback": "Usage",
        "icon": "bar-chart",
        "entry": "index.html",
        "placement": {
            "slot": "admin.tenantDetail",
            "path": ["access", "teams"],
            "order": 20
        },
        "min_visibility": "tenant_admin",
        "tools": ["fetch_usage"]
    })
}

#[test]
fn full_view_declaration_parses() {
    let v: View = serde_json::from_value(full_view_json()).expect("parses");
    assert_eq!(v.id, "usage-dashboard");
    assert_eq!(v.surface, Surface::Admin);
    assert_eq!(v.entry, "index.html");
    assert_eq!(v.placement.slot, "admin.tenantDetail");
    assert_eq!(v.placement.path, vec!["access", "teams"]);
    assert_eq!(v.placement.order, Some(20));
    assert_eq!(v.min_visibility, Visibility::TenantAdmin);
    assert_eq!(v.tools, vec!["fetch_usage"]);
}

#[test]
fn round_trip_preserves_every_field() {
    let original = full_view_json();
    let v: View = serde_json::from_value(original.clone()).expect("parses");
    let back = serde_json::to_value(&v).expect("serializes");
    assert_eq!(back, original);
}

/// The minimum an author must write. Everything else defaults, and the
/// defaults must not appear on the way back out — the describe is signed.
#[test]
fn minimal_view_parses_and_stays_minimal() {
    let minimal = serde_json::json!({
        "id": "hello",
        "surface": "designer",
        "title_key": "view.hello.label",
        "title_fallback": "Hello",
        "entry": "index.html",
        "placement": { "slot": "designer.sidebar" }
    });
    let v: View = serde_json::from_value(minimal.clone()).expect("parses");
    assert_eq!(v.min_visibility, Visibility::Member, "default floor is member");
    assert!(v.icon.is_none());
    assert!(v.tools.is_empty());
    assert!(v.placement.path.is_empty());
    assert!(v.placement.order.is_none());

    let back = serde_json::to_value(&v).expect("serializes");
    assert_eq!(
        back, minimal,
        "absent fields must stay absent — a re-serialized describe must not \
         sprout defaults the signature never covered"
    );
}

#[test]
fn unknown_view_field_is_rejected() {
    let typo = serde_json::json!({
        "id": "hello",
        "surface": "designer",
        "title_key": "k",
        "title_fallback": "Hello",
        "entry": "index.html",
        "placement": { "slot": "designer.sidebar" },
        "min_visibilty": "member"
    });
    let err = serde_json::from_value::<View>(typo).unwrap_err();
    assert!(
        err.to_string().contains("min_visibilty"),
        "the rejected field should be named: {err}"
    );
}

#[test]
fn unknown_surface_is_rejected() {
    let bad = serde_json::json!({
        "id": "hello",
        "surface": "mobile",
        "title_key": "k",
        "title_fallback": "Hello",
        "entry": "index.html",
        "placement": { "slot": "designer.sidebar" }
    });
    assert!(serde_json::from_value::<View>(bad).is_err());
}

/// `views` is additive: every describe written before it existed must still
/// parse, and an empty list must not serialize.
#[test]
fn contributions_without_views_parse_and_omit_the_key() {
    let c: Contributions = serde_json::from_value(serde_json::json!({})).expect("parses");
    assert!(c.views.is_empty());
    let s = serde_json::to_string(&c).expect("serializes");
    assert!(!s.contains("views"), "empty views must not serialize: {s}");
}
