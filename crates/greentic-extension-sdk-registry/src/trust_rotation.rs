//! Publisher-key rotations this build vouches for.
//!
//! [`crate::trust_store::TrustStore::pin_or_verify`] refuses an artifact whose
//! signing key differs from the key pinned on first install. That refusal is
//! what stops a compromised registry from pushing an update under an id the
//! operator already trusts — and it also refuses a *legitimate* key rotation,
//! leaving hand-editing `trust/publishers.json` as the only way out.
//!
//! [`KNOWN_ROTATIONS`] is the narrow exception: exact `(from, to)` key pairs,
//! each scoped to an extension-id prefix, that are accepted in place of a
//! refusal and re-pinned to `to`.
//!
//! # Why a compiled-in list is trustworthy
//!
//! Trust in a listed rotation comes from it being compiled into the same
//! binary the operator already runs — the same root of trust as the
//! verification code itself. Whoever could alter this list could equally alter
//! `pin_or_verify` to accept anything, so the list grants no power the build
//! did not already have. It is **not** a remote claim: nothing a registry,
//! store or archive serves can add an entry.
//!
//! # Why this is not the general mechanism
//!
//! The stronger, general answer is a *signed rotation record*: a statement
//! "key A hands over to key B", signed by A's private key and shipped beside
//! the artifact, which any client could verify against its existing pin
//! without trusting a new build. That is not what this is, for two reasons:
//! nobody holding this code has access to the old private key to sign such a
//! record, and the store does not emit rotation proofs for a client to verify.
//! Until both exist, a reviewed compiled-in list is the smallest change that
//! lets a known rotation through without weakening the refusal for every other
//! key change.
//!
//! # Rules (each pinned by a test)
//!
//! - Keys compare by **full string equality** — never by prefix.
//! - A rotation is **directed**: `from → to` never accepts `to → from`.
//! - A chain `A → B → C` is accepted only by walking listed pairs, and only
//!   pairs whose `id_prefix` covers the extension id.
//! - The list must stay acyclic, so no chain can walk back to a retired key.

/// One publisher-key rotation vouched for by this build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnownRotation {
    /// Only extension ids starting with this prefix may use the rotation. The
    /// store's `greentic` publisher is itself restricted to `greentic.`, so a
    /// Greentic key rotation must not reach a third party's pinned id.
    pub id_prefix: &'static str,
    /// The retired key (base64 ed25519, exactly as pinned).
    pub from: &'static str,
    /// The replacement key (base64 ed25519, exactly as presented).
    pub to: &'static str,
}

/// Greentic's pre-store extension signing key: it signed the extensions the
/// designer vendored in `bundled/` (and the SDK's v1 fixtures), so every
/// designer that installed one of those pinned it.
const GREENTIC_LEGACY_SIGNING_KEY: &str = "66L1j2dtYwoRQ2R6Bt8qkshdZmdb2IW8lLRgibxAI60=";

/// The Greentic Store's `greentic` publisher key. The store re-signs every
/// artifact it serves with it; `GET https://store.greentic.cloud/api/v1/publishers/greentic`
/// returns it ("Greentic Official", created 2026-06-02).
const GREENTIC_STORE_SIGNING_KEY: &str = "oOGcH3dja4oRDTL+W1MrGEu/sQ+17d3WWl6hwNiRQAg=";

/// Rotations accepted by [`crate::trust_store::TrustStore::pin_or_verify`].
///
/// Add an entry only with evidence that BOTH keys belong to the same publisher,
/// and record that evidence here: a wrong entry silently hands every id under
/// `id_prefix` to whoever holds `to`.
///
/// - Legacy → store (2026-09): the designer's own vendored extensions moved
///   from the legacy key to the store key on an ordinary refresh; the same id
///   (`greentic.adaptive-cards`) ships under both keys in one designer tree,
///   and both keys' signatures verify over their artifacts. The store's live
///   publisher record names the new key as Greentic's.
pub const KNOWN_ROTATIONS: &[KnownRotation] = &[KnownRotation {
    id_prefix: "greentic.",
    from: GREENTIC_LEGACY_SIGNING_KEY,
    to: GREENTIC_STORE_SIGNING_KEY,
}];

/// Is `pinned → presented` reachable for `id` through `rotations`, walking
/// only listed, directed pairs whose `id_prefix` covers `id`?
///
/// `pinned == presented` is NOT a rotation and returns `false`; the caller
/// handles equality before asking. The walk tracks visited keys, so a cyclic
/// list cannot loop — and [`tests::the_shipped_list_is_acyclic`] keeps the
/// shipped list from containing a cycle in the first place.
pub(crate) fn is_known_rotation(
    rotations: &[KnownRotation],
    id: &str,
    pinned: &str,
    presented: &str,
) -> bool {
    if pinned == presented {
        return false;
    }
    let mut visited: Vec<&str> = vec![pinned];
    let mut frontier: Vec<&str> = vec![pinned];
    while let Some(current) = frontier.pop() {
        for rotation in rotations {
            if rotation.from != current || !id.starts_with(rotation.id_prefix) {
                continue;
            }
            if rotation.to == presented {
                return true;
            }
            if !visited.contains(&rotation.to) {
                visited.push(rotation.to);
                frontier.push(rotation.to);
            }
        }
    }
    false
}

/// First eight characters of a key, for log lines that must name a key
/// without printing it whole.
pub(crate) fn key_prefix(key: &str) -> &str {
    key.get(..8).unwrap_or(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
    const B: &str = "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB=";
    const C: &str = "CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC=";

    fn rot(from: &'static str, to: &'static str) -> KnownRotation {
        KnownRotation {
            id_prefix: "greentic.",
            from,
            to,
        }
    }

    #[test]
    fn a_listed_pair_is_a_rotation() {
        assert!(is_known_rotation(&[rot(A, B)], "greentic.x", A, B));
    }

    #[test]
    fn the_reverse_of_a_listed_pair_is_not() {
        assert!(!is_known_rotation(&[rot(A, B)], "greentic.x", B, A));
    }

    #[test]
    fn equal_keys_are_not_a_rotation() {
        assert!(!is_known_rotation(&[rot(A, B)], "greentic.x", A, A));
    }

    #[test]
    fn a_prefix_of_the_listed_key_is_not_the_listed_key() {
        assert!(!is_known_rotation(&[rot(A, B)], "greentic.x", A, &B[..20]));
        assert!(!is_known_rotation(&[rot(A, B)], "greentic.x", &A[..20], B));
    }

    #[test]
    fn a_chain_walks_only_listed_pairs() {
        let list = [rot(A, B), rot(B, C)];
        assert!(is_known_rotation(&list, "greentic.x", A, C));
        assert!(!is_known_rotation(&list, "greentic.x", C, A));
        assert!(!is_known_rotation(&[rot(A, B)], "greentic.x", A, C));
    }

    #[test]
    fn a_cyclic_list_terminates() {
        let list = [rot(A, B), rot(B, A)];
        assert!(!is_known_rotation(&list, "greentic.x", A, C));
    }

    #[test]
    fn an_id_outside_the_prefix_cannot_use_the_rotation() {
        assert!(!is_known_rotation(&[rot(A, B)], "acme.x", A, B));
        // `greentic` without the dot is a different namespace.
        assert!(!is_known_rotation(&[rot(A, B)], "greenticx.y", A, B));
    }

    #[test]
    fn the_shipped_legacy_to_store_rotation_is_listed_one_way() {
        let id = "greentic.bundle-standard";
        assert!(is_known_rotation(
            KNOWN_ROTATIONS,
            id,
            GREENTIC_LEGACY_SIGNING_KEY,
            GREENTIC_STORE_SIGNING_KEY
        ));
        assert!(!is_known_rotation(
            KNOWN_ROTATIONS,
            id,
            GREENTIC_STORE_SIGNING_KEY,
            GREENTIC_LEGACY_SIGNING_KEY
        ));
    }

    /// A cycle would let a chain walk back to a retired key, i.e. accept a
    /// reverse rotation. Refuse one at test time rather than at an operator's.
    #[test]
    fn the_shipped_list_is_acyclic() {
        for rotation in KNOWN_ROTATIONS {
            assert!(
                !is_known_rotation(
                    KNOWN_ROTATIONS,
                    rotation.id_prefix,
                    rotation.to,
                    rotation.from
                ),
                "KNOWN_ROTATIONS reaches {} back from {}",
                key_prefix(rotation.from),
                key_prefix(rotation.to)
            );
        }
    }

    #[test]
    fn shipped_keys_are_32_byte_ed25519_keys() {
        use base64::Engine as _;
        for rotation in KNOWN_ROTATIONS {
            for key in [rotation.from, rotation.to] {
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(key)
                    .expect("base64");
                assert_eq!(bytes.len(), 32, "{key}");
            }
        }
    }
}
