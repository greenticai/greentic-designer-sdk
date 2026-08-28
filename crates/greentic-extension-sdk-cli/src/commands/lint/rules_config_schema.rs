//! Top-level `configSchema` lint rules.
//!
//! `configSchema` is the extension-wide, **non-secret** operator
//! configuration the admin console renders as a form. Two things can go
//! wrong with it that nothing else catches:
//!
//! 1. It is a *string* holding JSON, so a typo inside it is invisible to
//!    `describe-v2.json` (which only types it `"string"`). A non-object
//!    value renders as an empty form with no error, so the operator is told
//!    the extension needs no configuration.
//! 2. An author reaches for it to ask for an API key, because it is the
//!    field that produces a form. That value is stored and echoed back as
//!    plain tenant configuration. `requiredSecrets` is the field for
//!    credentials, and it exists precisely so the value takes a different
//!    storage path.
//!
//! Rule 1 is also enforced by the contract deserializer, which is the
//! backstop for a describe that never passes through `gtdx lint`. It is
//! repeated here so the author gets it as a lint line naming the field,
//! next to every other pre-publish check, rather than as a parse failure.
//!
//! Note the deliberate asymmetry with `rules_addons`, which does *not*
//! check an addon's `config_schema` for credentials. That check is skipped
//! there because the addon rule is about reconciliation (a secret in
//! desired state can never be read back, so it diffs forever) and addon
//! config is not reconciled. The rule here rests on a different fact -
//! where the value is *stored* - so it applies even though the addon one
//! does not.

use super::Violation;
use super::rules_addons::{looks_like_a_secret, walk_schema_properties};

pub(super) fn check_config_schema(describe: &serde_json::Value) -> Vec<Violation> {
    let mut out = Vec::new();

    // Absent is the norm and always fine: the field is optional, and an
    // extension with no operator configuration should omit it rather than
    // declare an empty object, which would render as an empty form.
    let Some(raw) = describe.get("configSchema") else {
        return out;
    };

    let Some(text) = raw.as_str() else {
        out.push(Violation::error(
            "E_CONFIG_SCHEMA_INVALID",
            "configSchema must be a string holding a JSON Schema, not an inline object - it \
             is stringly-encoded because it is a payload passed to a renderer, not host \
             control data"
                .to_string(),
        ));
        return out;
    };

    let parsed = match serde_json::from_str::<serde_json::Value>(text) {
        Ok(value @ serde_json::Value::Object(_)) => value,
        Ok(_) => {
            out.push(Violation::error(
                "E_CONFIG_SCHEMA_INVALID",
                "configSchema parses as JSON but is not a JSON object - a JSON Schema for a \
                 form must be an object, and a non-object renders as an empty form with no \
                 error at all"
                    .to_string(),
            ));
            return out;
        }
        Err(e) => {
            out.push(Violation::error(
                "E_CONFIG_SCHEMA_INVALID",
                format!("configSchema is not valid JSON: {e}"),
            ));
            return out;
        }
    };

    walk_schema_properties(&parsed, "", &mut |property, path| {
        if looks_like_a_secret(property) {
            out.push(Violation::error(
                "E_CONFIG_SCHEMA_SECRET",
                format!(
                    "configSchema declares {path:?}, which names a credential. configSchema \
                     is non-secret operator configuration: its values are stored and handed \
                     back as the plain tenant overlay. Declare the credential in the \
                     top-level `requiredSecrets` instead, which has `key`, `required`, \
                     `format`, `description` and `examples` for exactly this."
                ),
            ));
        }
    });

    out
}
