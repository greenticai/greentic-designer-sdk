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
}
