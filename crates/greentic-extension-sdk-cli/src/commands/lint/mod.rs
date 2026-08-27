//! `gtdx lint` — pre-publish static checks against `describe.json` that go
//! beyond the JSON Schema validation (`gtdx validate`).
//!
//! Each rule has a stable code (`E_*` for errors, `W_*` for warnings) the
//! test suite asserts on; the human report prefixes every line with the
//! code so authors can grep the docs.
//!
//! Audit-driven rules (May 2026):
//! - `E_VERSION_SEMVER` — `metadata.version` must parse as semver
//! - `E_RUNTIME_REF` — every `nodeTypes[].runtime_ref` and
//!   `tools[].runtime_ref` must name a key declared in `runtime.components`
//! - `E_CAP_CYCLE` — `capabilities.required[].id` must not appear in
//!   `capabilities.offered[].id` (an extension can't depend on a capability
//!   it itself provides — the resolver would happily satisfy via self and
//!   the runtime would then panic on dispatch)
//! - `W_DESCRIBE_DIFF_BREAKING` — the previously-installed describe under
//!   `<home>/extensions/<kind>/<id>-<version>/describe.json` (highest
//!   installed version of `id`) exposed contributions or capabilities that
//!   the current describe no longer does, AND
//!   `metadata.version` was not bumped. Warning, not error: prints the
//!   summary and exits zero so it's CI-noise rather than CI-breaking.
//!
//! S4 hygiene rules (June 2026):
//! - `E_PERMS_SECRETS_PLAIN_KEY` — `runtime.permissions.secrets` contains a
//!   plain field-name key (no `://`, no `*`, no trailing `/`). Such entries
//!   belong in the top-level `requiredSecrets` array; `permissions.secrets`
//!   is for read-permission grants only. See `docs/authoring-secrets.md`.
//!
//! S3/D2 key-format rules (June 2026):
//! - `E_SECRET_KEY_NOT_CANONICAL` — a declared secret key (in `requiredSecrets`
//!   or `contributions.tools[].secret_requirements`) is not in the canonical
//!   `namespace/name` lowercase form (`[a-z0-9._-/]`, no leading/trailing `/`,
//!   no `..` segment, no `://`). Keys that are non-canonical break secret
//!   resolution and must be renamed by the author.
//!
//! View rules (August 2026), for `contributions.views[]`:
//! - `E_VIEW_ID_PATTERN` — `id` does not match `^[a-z0-9][a-z0-9._-]*$`, the
//!   same pattern the schema declares. Checked before the id is joined into
//!   an asset path, so a traversal attempt is reported as an invalid id
//!   rather than surfacing as a confusing missing-file error.
//! - `E_VIEW_ENTRY_PATH` — `entry` escapes `assets/views/<id>/`
//! - `E_VIEW_ENTRY_MISSING` — `entry` names a file that is not in the project
//! - `E_VIEW_ENTRY_UNREADABLE` — `entry` names a file that exists but could
//!   not be read (not valid UTF-8, or a permissions error) — distinct from
//!   `E_VIEW_ENTRY_MISSING`, which only ever means the file was not found
//! - `E_VIEW_REMOTE_ASSET` — the entry HTML has a `<script src>`/`<img src>`
//!   or `<link href>` pointing at a remote origin, which would defeat the
//!   pack manifest's integrity. Scoped to the tag that owns the attribute, so
//!   an ordinary `<a href="https://…">` hyperlink does not trip it.
//! - `W_VIEW_SLOT_UNKNOWN` — `placement.slot` is not in the CLI's snapshot of
//!   host slots. A warning, because the snapshot goes stale by construction.
//!
//! Addon rules (August 2026), for `contributions.addons[]`:
//! - `E_ADDON_ID_PATTERN` — `id` does not match `^[a-z0-9][a-z0-9-]*$`
//! - `E_ADDON_OUTPUT_NAME` — an output name is not a valid environment
//!   variable identifier (`^[A-Za-z_][A-Za-z0-9_]*$`); outputs are injected
//!   as environment variables on the consuming service
//! - `E_ADDON_SECRET_IN_DESIRED_STATE` — spec D16: a top-level property of
//!   `desired_state_schema` looks like a credential. A credential there can
//!   never be read back by `observe`, so it diffs forever and no plan is
//!   ever clean; credentials reach the addon through its runtime binding
//!   instead
//! - `W_ADDON_FAMILY_UNKNOWN` — `family` is not one this SDK version knows.
//!   A warning, for the same reason as `W_VIEW_SLOT_UNKNOWN`
//!
//! Addon backup rules (August 2026), making `supports_backup` a verifiable
//! claim instead of an unchecked boolean by reading it against the
//! extension's own `wit/world.wit` (silent when that file is absent, e.g.
//! linting a packed or installed extension with no source tree):
//! - `E_ADDON_BACKUP_NOT_EXPORTED` — an addon declares `supports_backup:
//!   true` but `wit/world.wit` does not export
//!   `greentic:extension-addon/backup`. The platform would offer a
//!   pre-destroy snapshot and call an export that does not exist.
//! - `W_ADDON_BACKUP_UNDECLARED` — `wit/world.wit` exports
//!   `greentic:extension-addon/backup` but no addon declares
//!   `supports_backup: true`. Drift, not a lie — the capability exists and
//!   is simply never advertised — so a warning, not an error.

use std::path::{Path, PathBuf};

use clap::Args as ClapArgs;

mod rules;
mod rules_addons;
mod rules_secret_key;
mod rules_views;
#[cfg(test)]
mod tests;

use rules::{
    check_capability_cycle, check_describe_diff_breaking, check_engine_deprecated,
    check_export_form, check_id_pattern, check_perms_secrets_plain_key, check_runtime_refs,
    check_schema_host, check_sha256_zero, check_tool_naming, check_version_semver,
};
use rules_addons::check_addons;
use rules_secret_key::check_secret_key_canonical;
use rules_views::check_views;

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Path to an extension source directory containing describe.json.
    #[arg(long, default_value = ".")]
    pub dir: PathBuf,

    /// Enable publish-only rules (e.g. `E_SHA256_ZERO` rejects placeholder hashes).
    #[arg(long, default_value_t = false)]
    pub publish: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub code: &'static str,
    pub severity: Severity,
    pub message: String,
}

impl Violation {
    fn error(code: &'static str, message: String) -> Self {
        Self {
            code,
            severity: Severity::Error,
            message,
        }
    }

    fn warning(code: &'static str, message: String) -> Self {
        Self {
            code,
            severity: Severity::Warning,
            message,
        }
    }
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let prefix = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        write!(f, "{prefix}: {}: {}", self.code, self.message)
    }
}

pub fn run(args: &Args, home: &Path) -> anyhow::Result<()> {
    let describe_path = args.dir.join("describe.json");
    let bytes = std::fs::read(&describe_path)
        .map_err(|e| anyhow::anyhow!("read {}: {e}", describe_path.display()))?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| anyhow::anyhow!("parse describe.json: {e}"))?;

    let violations = collect_violations(&value, &args.dir, home, args.publish);
    for v in &violations {
        eprintln!("{v}");
    }
    let error_count = violations
        .iter()
        .filter(|v| v.severity == Severity::Error)
        .count();
    if error_count == 0 {
        if violations.is_empty() {
            println!("✓ {} lint clean", describe_path.display());
        }
        return Ok(());
    }
    anyhow::bail!("{error_count} error(s)");
}

fn collect_violations(
    describe: &serde_json::Value,
    dir: &Path,
    home: &Path,
    publish: bool,
) -> Vec<Violation> {
    let mut out = Vec::new();
    out.extend(check_version_semver(describe));
    out.extend(check_runtime_refs(describe));
    out.extend(check_capability_cycle(describe));
    out.extend(check_describe_diff_breaking(describe, home));
    out.extend(check_schema_host(describe));
    out.extend(check_export_form(describe));
    out.extend(check_engine_deprecated(describe));
    out.extend(check_id_pattern(describe));
    out.extend(check_tool_naming(describe));
    out.extend(check_sha256_zero(describe, publish));
    out.extend(check_perms_secrets_plain_key(describe));
    out.extend(check_secret_key_canonical(describe));
    out.extend(check_views(describe, dir));
    out.extend(check_addons(describe, dir));
    out
}
