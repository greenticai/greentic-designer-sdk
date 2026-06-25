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
//!   `<home>/extensions/<kind>/<id>/describe.json` exposed contributions or
//!   capabilities that the current describe no longer does, AND
//!   `metadata.version` was not bumped. Warning, not error: prints the
//!   summary and exits zero so it's CI-noise rather than CI-breaking.
//!
//! S4 hygiene rules (June 2026):
//! - `W_PERMS_SECRETS_PLAIN_KEY` — `runtime.permissions.secrets` contains a
//!   plain field-name key (no `://`, no `*`, no trailing `/`). Such entries
//!   belong in the top-level `requiredSecrets` array; `permissions.secrets`
//!   is for read-permission grants only. See `docs/authoring-secrets.md`.
//!
//! S3/D2 key-format rules (June 2026):
//! - `W_SECRET_KEY_NOT_CANONICAL` — a declared secret key (in `requiredSecrets`
//!   or `contributions.tools[].secret_requirements`) is not in the canonical
//!   `namespace/name` lowercase form (`[a-z0-9._-/]`, no leading/trailing `/`,
//!   no `..` segment, no `://`). Keys that are non-canonical break secret
//!   resolution and should be renamed by the author.

use std::path::{Path, PathBuf};

use clap::Args as ClapArgs;

mod rules;
mod rules_secret_key;
#[cfg(test)]
mod tests;

use rules::{
    check_capability_cycle, check_describe_diff_breaking, check_engine_deprecated,
    check_export_form, check_id_pattern, check_perms_secrets_plain_key, check_runtime_refs,
    check_schema_host, check_sha256_zero, check_tool_naming, check_version_semver,
};
use rules_secret_key::check_secret_key_canonical;

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

    let violations = collect_violations(&value, home, args.publish);
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

fn collect_violations(describe: &serde_json::Value, home: &Path, publish: bool) -> Vec<Violation> {
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
    out
}
