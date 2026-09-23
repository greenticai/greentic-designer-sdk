//! `MessagingChannel` — the messaging channel a provider extension offers, so
//! a consumer (designer, gtdx, store) can present it for selection and, when a
//! deployment is built, bake the right provider pack into the bundle.
//!
//! # Why the OCI reference is declared rather than derived
//!
//! A channel is only useful if something can resolve it to a deployable pack.
//! A provider extension's own `runtime.components` gtpack is the extension's
//! design-time artifact, not the messaging provider a deployed bundle runs, so
//! the deployable coordinate has to be stated explicitly.
//!
//! Deriving it from `metadata.id` (`greentic.provider.slack` →
//! `messaging-slack`) is the obvious shortcut and does not hold: a consumer
//! that measured this mapping against a real registry found it correct for 24
//! of 39 entries, with the 15 failures silent and in systematic families. A
//! wrong-but-plausible reference is worse than an absent one, because it fails
//! deep inside a bundle build rather than at declaration time.
//!
//! # Consumers must treat an absent channel as "this extension offers none"
//!
//! The field is optional, and every provider extension published before this
//! type existed omits it. That is the normal state, not a fault.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MessagingChannel {
    /// Stable channel id, e.g. `messaging-3aigent-gui`.
    ///
    /// This is the identity a consumer stores against a deployment and later
    /// resolves back to [`Self::oci_ref`], so it must not change across
    /// versions of the extension — renaming it orphans every deployment that
    /// already selected the channel.
    pub id: String,
    /// OCI reference of the messaging provider pack to bake into a bundle,
    /// e.g. `oci://ghcr.io/greenticai/packs/messaging/messaging-x@sha256:…`.
    ///
    /// Digest-pinned references are strongly preferred: a tag can be moved to
    /// different bytes after publication, and a bundle built from one is not
    /// reproducible. A consumer may warn on, or refuse, an unpinned reference.
    #[serde(rename = "ref")]
    pub oci_ref: String,
    /// Display name for the channel. Absent ⇒ a consumer should fall back to
    /// `metadata.name`. The field exists for the case where the channel's name
    /// and the extension's name legitimately differ.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// What a person reaches the worker through on this channel. Today the
    /// one value a consumer acts on is [`Self::SURFACE_BROWSER`]; absent means
    /// "not stated", which a consumer must read as "not a browser channel".
    ///
    /// # Why this exists
    ///
    /// A consumer that builds an environment has to guarantee there is a way
    /// in from a browser, so it adds a stock web-chat channel of its own. It
    /// had no way to know that a channel the operator ALREADY chose serves a
    /// browser, so it added the stock one anyway — and a deployment carrying a
    /// custom GUI channel shipped two channel providers, each asking for its
    /// own signing key under its own scope. A partner hand-seeded the same key
    /// under both scopes to keep it working, and reported it as the runtime
    /// reading the wrong pack id. It was two providers behaving correctly.
    ///
    /// # Why a string and not an enum
    ///
    /// This struct is `deny_unknown_fields`, and so is every type around it:
    /// a value an older parser does not recognise does not go unread, it fails
    /// the whole describe and the extension stops loading. An enum would make
    /// every future value (`voice`, say) exactly that failure on every
    /// consumer that had not upgraded. A string that consumers compare against
    /// known values degrades to "not a surface I act on" instead.
    ///
    /// **Rollout order still matters for the FIELD itself.** A consumer built
    /// before this field existed refuses any describe that carries it. So a
    /// provider must not declare `surface` until the consumers that load it
    /// are on a contract version that knows the field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface: Option<String>,
}

impl MessagingChannel {
    /// The surface a person reaches through a web browser.
    pub const SURFACE_BROWSER: &'static str = "browser";

    /// Whether this channel declares that it serves a browser.
    ///
    /// Exact, case-sensitive comparison on purpose: the value is a contract
    /// token, and accepting `Browser` or ` browser` here would make a typo in
    /// one describe behave differently across consumers that normalise and
    /// consumers that do not.
    #[must_use]
    pub fn serves_browser(&self) -> bool {
        self.surface.as_deref() == Some(Self::SURFACE_BROWSER)
    }
}

#[cfg(test)]
mod tests {
    use super::MessagingChannel;

    #[test]
    fn label_is_optional() {
        let c: MessagingChannel =
            serde_json::from_str(r#"{ "id": "messaging-x", "ref": "oci://ghcr.io/x@sha256:ab" }"#)
                .expect("decodes without a label");
        assert_eq!(c.id, "messaging-x");
        assert!(c.label.is_none());
    }

    /// The wire name is `ref`, not `oci_ref`: it matches the key
    /// `providers-registry.json` already uses for the same value, so an author
    /// copying a reference between the two does not have to rename it.
    #[test]
    fn the_reference_is_spelled_ref_on_the_wire() {
        let c = MessagingChannel {
            id: "messaging-x".to_string(),
            oci_ref: "oci://ghcr.io/x@sha256:ab".to_string(),
            label: None,
            surface: None,
        };
        let v = serde_json::to_value(&c).expect("serializes");
        assert!(v.get("ref").is_some(), "serialized as: {v}");
        assert!(v.get("oci_ref").is_none());
    }

    /// `deny_unknown_fields` is what makes a typo a build-time error for the
    /// extension author instead of a channel that silently never appears.
    #[test]
    fn an_unknown_field_is_refused() {
        let err = serde_json::from_str::<MessagingChannel>(
            r#"{ "id": "messaging-x", "ref": "oci://ghcr.io/x@sha256:ab", "labell": "X" }"#,
        );
        assert!(err.is_err());
    }

    #[test]
    fn a_describe_without_a_surface_still_decodes_and_is_not_a_browser() {
        let c: MessagingChannel =
            serde_json::from_str(r#"{ "id": "messaging-x", "ref": "oci://ghcr.io/x@sha256:ab" }"#)
                .expect("every describe published before this field keeps decoding");
        assert!(c.surface.is_none());
        assert!(
            !c.serves_browser(),
            "absent must never read as a browser channel"
        );
    }

    #[test]
    fn a_browser_surface_is_recognised() {
        let c: MessagingChannel = serde_json::from_str(
            r#"{ "id": "messaging-x", "ref": "oci://ghcr.io/x@sha256:ab", "surface": "browser" }"#,
        )
        .expect("decodes");
        assert!(c.serves_browser());
    }

    /// The reason this is a string: a value this build does not know must
    /// decode, not fail the describe and unload the extension.
    #[test]
    fn an_unknown_surface_decodes_and_is_simply_not_a_browser() {
        let c: MessagingChannel = serde_json::from_str(
            r#"{ "id": "messaging-x", "ref": "oci://ghcr.io/x@sha256:ab", "surface": "voice" }"#,
        )
        .expect("a future surface value must not make the extension unloadable");
        assert_eq!(c.surface.as_deref(), Some("voice"));
        assert!(!c.serves_browser());
    }

    #[test]
    fn the_surface_token_is_exact() {
        for token in ["Browser", " browser", "browser ", "BROWSER"] {
            let c = MessagingChannel {
                id: "messaging-x".to_string(),
                oci_ref: "oci://ghcr.io/x@sha256:ab".to_string(),
                label: None,
                surface: Some(token.to_string()),
            };
            assert!(!c.serves_browser(), "{token:?} must not count");
        }
    }

    #[test]
    fn an_absent_surface_is_not_written_back() {
        let c = MessagingChannel {
            id: "messaging-x".to_string(),
            oci_ref: "oci://ghcr.io/x@sha256:ab".to_string(),
            label: None,
            surface: None,
        };
        let v = serde_json::to_value(&c).expect("serializes");
        assert!(
            v.get("surface").is_none(),
            "a describe round-tripped through this type must not gain a field \
             older consumers refuse"
        );
    }
}
