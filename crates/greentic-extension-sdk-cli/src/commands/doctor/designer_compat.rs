//! Designer ⇄ extension compatibility matrix for `gtdx doctor`.
//!
//! An extension authored against the current SDK emits a `greentic.ai/v2`
//! describe. A designer build older than [`V2_MIN_DESIGNER`] screens those out
//! at boot with a terse "built for a newer designer" line and no hint about
//! which version would work, so the author sees an extension that simply never
//! appears in `/api/extensions`. This module encodes the matrix as executable
//! logic — designer version in, actionable verdict out — so `gtdx doctor` can
//! name the mismatch before the author goes hunting through the loader.
//!
//! The matrix has exactly two axes:
//!
//! | designer      | `greentic.ai/v1` | `greentic.ai/v2` |
//! |---------------|------------------|------------------|
//! | `< 1.2.0`     | loads            | skipped at boot  |
//! | `>= 1.2.0`    | loads (migrated) | loads            |
//!
//! On top of the contract axis, a v2 describe carries its own
//! `compat.min_designer_version` range (and a v1 describe the equivalent
//! `engine.greenticDesigner`), which is checked second.

use std::path::{Path, PathBuf};

use semver::{Version, VersionReq};
use serde_json::Value;

/// Path to the designer binary to interrogate.
///
/// `GREENTIC_DESIGNER_BIN` takes priority so an author running designer out of
/// a checkout (`target/release/greentic-designer`) can point the tooling at the
/// build they actually launch, which is usually not the one on `PATH`.
pub fn designer_binary() -> Option<PathBuf> {
    if let Some(explicit) = std::env::var_os("GREENTIC_DESIGNER_BIN") {
        let path = PathBuf::from(explicit);
        return path.exists().then_some(path);
    }
    which::which("greentic-designer").ok()
}

/// Version reported by a designer binary, or `None` if it cannot be run or
/// prints something unparseable.
pub fn designer_version(binary: &Path) -> Option<Version> {
    let output = std::process::Command::new(binary)
        .arg("--version")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_version_output(&String::from_utf8_lossy(&output.stdout))
}

/// Version of the designer installed on this machine, if there is one.
pub fn installed_designer_version() -> Option<Version> {
    designer_version(&designer_binary()?)
}

/// First designer release whose extension loader understands the
/// `greentic.ai/v2` describe contract.
///
/// Re-exported from the contract crate so the floor doctor reports and the
/// floor `gtdx new` stamps into every scaffold are the same value by
/// construction, not by two comments agreeing with each other.
pub use greentic_extension_sdk_contract::compat::MIN_DESIGNER_VERSION as V2_MIN_DESIGNER;

/// The `apiVersion` a describe carries when it omits the field. The v1
/// contract predates the field being mandatory.
const DEFAULT_API_VERSION: &str = "greentic.ai/v1";

/// The current describe contract.
const V2_API_VERSION: &str = "greentic.ai/v2";

/// Why (or whether) a given designer build can load a given extension.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// The designer build can load this extension.
    Compatible,
    /// The designer predates the extension's describe contract entirely, so it
    /// skips the extension at boot regardless of what `compat` asks for.
    ApiVersionUnsupported {
        /// The `apiVersion` the extension declares.
        api_version: String,
        /// Oldest designer that understands that contract.
        needs: Version,
    },
    /// The designer understands the contract but is older than the range the
    /// extension declares.
    DesignerTooOld {
        /// The range the extension asks for.
        needs: VersionReq,
    },
}

impl Verdict {
    /// One-line, actionable remedy shown under the extension in `doctor`
    /// output. `None` means the extension loads — callers treat the absence of
    /// a remedy as the pass case, so there is no separate "is it a problem?"
    /// predicate to keep in sync with this one.
    pub fn remedy(&self, designer: &Version) -> Option<String> {
        match self {
            Verdict::Compatible => None,
            Verdict::ApiVersionUnsupported { api_version, needs } => Some(format!(
                "declares {api_version}, which designer {designer} cannot load \
                 (it is skipped at boot as \"built for a newer designer\") — \
                 upgrade greentic-designer to >={needs}"
            )),
            Verdict::DesignerTooOld { needs } => Some(format!(
                "requires designer {needs}, but {designer} is installed — \
                 upgrade greentic-designer"
            )),
        }
    }
}

/// Decide whether `designer` can load the extension described by `describe`.
///
/// `describe` is taken as raw JSON rather than a parsed `Describe` on purpose:
/// doctor runs against whatever is already installed on the machine, including
/// describes this SDK build cannot deserialize, and a compatibility report is
/// exactly what those need most.
pub fn evaluate(designer: &Version, describe: &Value) -> Verdict {
    let api_version = describe
        .get("apiVersion")
        .and_then(Value::as_str)
        .unwrap_or(DEFAULT_API_VERSION);

    if api_version == V2_API_VERSION {
        let needs: Version = V2_MIN_DESIGNER
            .parse()
            .expect("V2_MIN_DESIGNER is a literal valid semver");
        let v2_gate =
            VersionReq::parse(&format!(">={needs}")).expect("V2_MIN_DESIGNER forms a valid range");
        if !satisfies_ignoring_prerelease(&v2_gate, designer) {
            return Verdict::ApiVersionUnsupported {
                api_version: api_version.to_string(),
                needs,
            };
        }
    }

    match declared_designer_req(describe) {
        Some(req) if !satisfies_ignoring_prerelease(&req, designer) => {
            Verdict::DesignerTooOld { needs: req }
        }
        _ => Verdict::Compatible,
    }
}

/// The designer range an extension declares, from either contract generation:
/// v2's `compat.min_designer_version` or v1's `engine.greenticDesigner`.
///
/// An unparseable range yields `None` — a junk constraint is a describe-schema
/// problem that `check_installed` already reports, and re-reporting it here as
/// a version mismatch would be actively misleading.
fn declared_designer_req(describe: &Value) -> Option<VersionReq> {
    let raw = describe
        .get("compat")
        .and_then(|c| c.get("min_designer_version"))
        .and_then(Value::as_str)
        .or_else(|| {
            describe
                .get("engine")
                .and_then(|e| e.get("greenticDesigner"))
                .and_then(Value::as_str)
        })?;
    raw.parse().ok()
}

/// `VersionReq::matches` rejects any pre-release version unless the range
/// itself pins the same `major.minor.patch` — so a stock `>=1.2.0` says *no*
/// to `1.3.2-research`. Every designer built off the `research` lineage carries
/// that suffix, so matching literally would report every research build as
/// incompatible with every extension. Compare on the release triple instead.
fn satisfies_ignoring_prerelease(req: &VersionReq, version: &Version) -> bool {
    let release_only = Version::new(version.major, version.minor, version.patch);
    req.matches(&release_only)
}

/// Extract the semver from a `greentic-designer --version` line.
///
/// Clap renders `<bin-name> <version>`, which every designer lineage back to
/// 1.1.x emits — that makes the binary, not the HTTP API, the version probe
/// doctor can rely on. `GET /api/app-info` would be richer but only exists on
/// designer >= 1.2.x, i.e. it is missing on exactly the builds this check is
/// meant to diagnose, and it needs a running server.
pub fn parse_version_output(stdout: &str) -> Option<Version> {
    stdout
        .split_whitespace()
        .find_map(|token| token.parse::<Version>().ok())
}

/// Warning to print immediately after installing an extension, or `None` when
/// there is nothing to say.
///
/// `gtdx doctor` already reports this, but install is where the author is
/// actually looking: `gtdx dev --once` otherwise reports a clean install of an
/// extension the local designer will silently refuse to load. `designer` is
/// `None` when no designer is installed locally — that is not a problem, so it
/// warns about nothing.
pub fn install_warning(designer: Option<&Version>, describe: &Value) -> Option<String> {
    let designer = designer?;
    let remedy = evaluate(designer, describe).remedy(designer)?;
    Some(format!(
        "installed, but this designer cannot load it: {remedy}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn v2_describe() -> Value {
        json!({
            "apiVersion": "greentic.ai/v2",
            "compat": { "min_designer_version": ">=1.2.0" }
        })
    }

    /// The reported inner-loop failure: `gtdx dev --once` says "installed" and
    /// the extension then never appears in Designer.
    #[test]
    fn install_warns_when_local_designer_cannot_load_it() {
        let warning = install_warning(Some(&v("1.1.7")), &v2_describe())
            .expect("a pre-v2 designer must be warned about");

        assert!(warning.contains("cannot load"), "{warning}");
        assert!(warning.contains(">=1.2.0"), "names the fix: {warning}");
    }

    #[test]
    fn install_is_quiet_when_designer_can_load_it() {
        assert_eq!(
            install_warning(Some(&v("1.3.2-research")), &v2_describe()),
            None
        );
    }

    /// No designer installed is a normal state for a CI box or a publish-only
    /// machine — it must not produce a warning about compatibility.
    #[test]
    fn install_is_quiet_when_no_designer_is_installed() {
        assert_eq!(install_warning(None, &v2_describe()), None);
    }

    #[test]
    fn parses_clap_version_line() {
        assert_eq!(
            parse_version_output("greentic-designer 1.1.7\n"),
            Some(v("1.1.7"))
        );
    }

    /// Research builds carry a pre-release suffix; it must survive parsing so
    /// the report shows the real build, not a truncated one.
    #[test]
    fn parses_prerelease_version_line() {
        assert_eq!(
            parse_version_output("greentic-designer 1.3.2-research\n"),
            Some(v("1.3.2-research"))
        );
    }

    #[test]
    fn rejects_output_without_a_version() {
        assert_eq!(parse_version_output("greentic-designer\n"), None);
        assert_eq!(parse_version_output(""), None);
    }

    fn v(s: &str) -> Version {
        s.parse().expect("valid semver")
    }

    /// The reported case: a current-SDK extension against a designer from the
    /// 1.1.x lineage. It must be named as a contract mismatch, not silently
    /// pass, and the remedy must state the version that would work.
    #[test]
    fn v2_extension_on_pre_v2_designer_is_unsupported() {
        let describe = json!({
            "apiVersion": "greentic.ai/v2",
            "compat": { "min_designer_version": ">=1.2.0" }
        });

        let verdict = evaluate(&v("1.1.7"), &describe);

        assert_eq!(
            verdict,
            Verdict::ApiVersionUnsupported {
                api_version: "greentic.ai/v2".into(),
                needs: v("1.2.0"),
            }
        );
        let remedy = verdict.remedy(&v("1.1.7")).expect("problem has a remedy");
        assert!(remedy.contains(">=1.2.0"), "remedy names the fix: {remedy}");
    }

    /// Guards the pre-release trap: `VersionReq(">=1.2.0")` does not match
    /// `1.3.2-research` under stock semver rules, which would flag every
    /// research-lineage designer as too old for every extension.
    #[test]
    fn research_prerelease_designer_satisfies_release_range() {
        let describe = json!({
            "apiVersion": "greentic.ai/v2",
            "compat": { "min_designer_version": ">=1.2.0" }
        });

        assert_eq!(
            evaluate(&v("1.3.2-research"), &describe),
            Verdict::Compatible
        );
    }

    #[test]
    fn v2_extension_on_current_designer_is_compatible() {
        let describe = json!({
            "apiVersion": "greentic.ai/v2",
            "compat": { "min_designer_version": ">=1.2.0" }
        });

        assert_eq!(evaluate(&v("1.3.2"), &describe), Verdict::Compatible);
    }

    /// The contract gate passes but the extension asks for something newer
    /// than the installed designer.
    #[test]
    fn designer_older_than_declared_range_is_too_old() {
        let describe = json!({
            "apiVersion": "greentic.ai/v2",
            "compat": { "min_designer_version": ">=1.4.0" }
        });

        let verdict = evaluate(&v("1.3.2-research"), &describe);

        assert_eq!(
            verdict,
            Verdict::DesignerTooOld {
                needs: VersionReq::parse(">=1.4.0").unwrap()
            }
        );
        let remedy = verdict.remedy(&v("1.3.2-research")).expect("has remedy");
        assert!(
            remedy.contains(">=1.4.0"),
            "remedy names the range: {remedy}"
        );
    }

    /// A v1 describe carries its constraint under `engine.greenticDesigner`;
    /// doctor must read that generation too rather than treating it as absent.
    #[test]
    fn v1_extension_uses_engine_constraint() {
        let describe = json!({
            "apiVersion": "greentic.ai/v1",
            "engine": { "greenticDesigner": ">=1.4.0" }
        });

        assert_eq!(
            evaluate(&v("1.1.7"), &describe),
            Verdict::DesignerTooOld {
                needs: VersionReq::parse(">=1.4.0").unwrap()
            }
        );
    }

    #[test]
    fn v1_extension_on_old_designer_is_compatible() {
        let describe = json!({
            "apiVersion": "greentic.ai/v1",
            "engine": { "greenticDesigner": ">=1.0.0" }
        });

        assert_eq!(evaluate(&v("1.1.7"), &describe), Verdict::Compatible);
    }

    /// A describe with no `apiVersion` is a v1 document, and one with no
    /// declared range constrains nothing — neither is a compatibility problem.
    #[test]
    fn describe_without_api_version_or_range_is_compatible() {
        assert_eq!(
            evaluate(&v("1.1.7"), &json!({ "metadata": { "id": "x" } })),
            Verdict::Compatible
        );
    }

    /// An unparseable range must not masquerade as a version mismatch.
    #[test]
    fn unparseable_declared_range_is_not_reported_as_too_old() {
        let describe = json!({
            "apiVersion": "greentic.ai/v1",
            "engine": { "greenticDesigner": "not-a-range" }
        });

        assert_eq!(evaluate(&v("1.1.7"), &describe), Verdict::Compatible);
    }

    #[test]
    fn compatible_verdict_has_no_remedy() {
        assert_eq!(Verdict::Compatible.remedy(&v("1.3.2")), None);
    }
}
