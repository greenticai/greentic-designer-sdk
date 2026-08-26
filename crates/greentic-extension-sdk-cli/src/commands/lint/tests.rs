use super::*;
use rules::{check_perms_secrets_plain_key, is_breaking_bump};
use rules_secret_key::check_secret_key_canonical;
use serde_json::json;

fn empty_home() -> tempfile::TempDir {
    tempfile::tempdir().expect("tempdir")
}

#[test]
fn version_semver_passes_on_valid() {
    let d = json!({"metadata": {"version": "1.2.3"}});
    assert!(check_version_semver(&d).is_empty());
}

#[test]
fn version_semver_fails_on_invalid() {
    let d = json!({"metadata": {"version": "not-a-version"}});
    let v = check_version_semver(&d);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].code, "E_VERSION_SEMVER");
}

#[test]
fn breaking_bump_detection() {
    let v = |s: &str| semver::Version::parse(s).unwrap();
    assert!(is_breaking_bump(&v("1.0.0"), &v("2.0.0"))); // major bump
    assert!(!is_breaking_bump(&v("1.0.0"), &v("1.0.1"))); // patch bump
    assert!(!is_breaking_bump(&v("1.0.0"), &v("1.1.0"))); // minor bump (>=1.0)
    assert!(!is_breaking_bump(&v("2.0.0"), &v("1.0.0"))); // downgrade
    assert!(!is_breaking_bump(&v("1.0.0"), &v("1.0.0"))); // equal
    assert!(is_breaking_bump(&v("0.1.0"), &v("0.2.0"))); // 0.x minor = breaking
    assert!(!is_breaking_bump(&v("0.1.0"), &v("0.1.1"))); // 0.x patch
    assert!(is_breaking_bump(&v("0.9.0"), &v("1.0.0"))); // 0.x -> 1.0
}

fn write_installed(home: &Path, id: &str, describe: &serde_json::Value) {
    let dir = home.join("extensions").join("design").join(id);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("describe.json"),
        serde_json::to_vec(describe).unwrap(),
    )
    .unwrap();
}

#[test]
fn describe_diff_warns_on_breaking_change_with_only_patch_bump() {
    let home = empty_home();
    let prev = json!({
        "metadata": {"id": "x", "version": "1.0.0"},
        "kind": "DesignExtension",
        "contributions": {"tools": [{"name": "t1"}]}
    });
    write_installed(home.path(), "x", &prev);
    let current = json!({
        "metadata": {"id": "x", "version": "1.0.1"},
        "kind": "DesignExtension",
        "contributions": {"tools": []}
    });
    let v = check_describe_diff_breaking(&current, home.path());
    assert_eq!(
        v.len(),
        1,
        "a patch bump must not suppress a breaking-change warning"
    );
    assert_eq!(v[0].code, "W_DESCRIBE_DIFF_BREAKING");
}

#[test]
fn describe_diff_suppressed_on_major_bump() {
    let home = empty_home();
    let prev = json!({
        "metadata": {"id": "x", "version": "1.0.0"},
        "kind": "DesignExtension",
        "contributions": {"tools": [{"name": "t1"}]}
    });
    write_installed(home.path(), "x", &prev);
    let current = json!({
        "metadata": {"id": "x", "version": "2.0.0"},
        "kind": "DesignExtension",
        "contributions": {"tools": []}
    });
    assert!(
        check_describe_diff_breaking(&current, home.path()).is_empty(),
        "a major bump signals the break and should suppress the warning"
    );
}

#[test]
fn runtime_ref_passes_when_declared() {
    let d = json!({
        "runtime": {"components": {"ext": {}}},
        "contributions": {"nodeTypes": [{"type_id": "a", "runtime_ref": "ext"}]},
    });
    assert!(check_runtime_refs(&d).is_empty());
}

#[test]
fn runtime_ref_fails_when_dangling() {
    let d = json!({
        "runtime": {"components": {"ext": {}}},
        "contributions": {"nodeTypes": [{"type_id": "a", "runtime_ref": "missing"}]},
    });
    let v = check_runtime_refs(&d);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].code, "E_RUNTIME_REF");
    assert!(v[0].message.contains("missing"));
}

#[test]
fn capability_cycle_passes_when_disjoint() {
    let d = json!({
        "capabilities": {
            "offered": [{"id": "greentic:test/a", "version": "1.0.0"}],
            "required": [{"id": "greentic:test/b", "version": "1.0.0"}],
        },
    });
    assert!(check_capability_cycle(&d).is_empty());
}

#[test]
fn capability_cycle_fails_when_self_required() {
    let d = json!({
        "capabilities": {
            "offered": [{"id": "greentic:test/a", "version": "1.0.0"}],
            "required": [{"id": "greentic:test/a", "version": "1.0.0"}],
        },
    });
    let v = check_capability_cycle(&d);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].code, "E_CAP_CYCLE");
}

#[test]
fn describe_diff_skips_when_no_installed_copy() {
    let home = empty_home();
    let d = json!({
        "kind": "DesignExtension",
        "metadata": {"id": "com.example.x", "version": "0.1.0"},
    });
    assert!(check_describe_diff_breaking(&d, home.path()).is_empty());
}

#[test]
fn describe_diff_skips_when_version_bumped() {
    let home = empty_home();
    let installed_dir = home.path().join("extensions/design/com.example.x");
    std::fs::create_dir_all(&installed_dir).unwrap();
    std::fs::write(
        installed_dir.join("describe.json"),
        serde_json::to_vec(&json!({
            "kind": "DesignExtension",
            "metadata": {"id": "com.example.x", "version": "0.1.0"},
            "contributions": {"tools": [{"name": "a"}, {"name": "b"}]},
        }))
        .unwrap(),
    )
    .unwrap();
    let current = json!({
        "kind": "DesignExtension",
        "metadata": {"id": "com.example.x", "version": "0.2.0"},
        "contributions": {"tools": [{"name": "a"}]},
    });
    assert!(check_describe_diff_breaking(&current, home.path()).is_empty());
}

#[test]
fn describe_diff_warns_when_tool_removed_without_bump() {
    let home = empty_home();
    let installed_dir = home.path().join("extensions/design/com.example.x");
    std::fs::create_dir_all(&installed_dir).unwrap();
    std::fs::write(
        installed_dir.join("describe.json"),
        serde_json::to_vec(&json!({
            "kind": "DesignExtension",
            "metadata": {"id": "com.example.x", "version": "0.1.0"},
            "contributions": {"tools": [{"name": "a"}, {"name": "b"}]},
        }))
        .unwrap(),
    )
    .unwrap();
    let current = json!({
        "kind": "DesignExtension",
        "metadata": {"id": "com.example.x", "version": "0.1.0"},
        "contributions": {"tools": [{"name": "a"}]},
    });
    let v = check_describe_diff_breaking(&current, home.path());
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].code, "W_DESCRIBE_DIFF_BREAKING");
    assert_eq!(v[0].severity, Severity::Warning);
    assert!(v[0].message.contains("\"b\""), "msg: {}", v[0].message);
}

#[test]
fn describe_diff_warns_when_capability_offered_removed_without_bump() {
    let home = empty_home();
    let installed_dir = home.path().join("extensions/design/com.example.y");
    std::fs::create_dir_all(&installed_dir).unwrap();
    std::fs::write(
        installed_dir.join("describe.json"),
        serde_json::to_vec(&json!({
            "kind": "DesignExtension",
            "metadata": {"id": "com.example.y", "version": "0.1.0"},
            "capabilities": {
                "offered": [{"id": "greentic:test/a", "version": "1.0.0"}],
                "required": [],
            },
        }))
        .unwrap(),
    )
    .unwrap();
    let current = json!({
        "kind": "DesignExtension",
        "metadata": {"id": "com.example.y", "version": "0.1.0"},
        "capabilities": {"offered": [], "required": []},
    });
    let v = check_describe_diff_breaking(&current, home.path());
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].code, "W_DESCRIBE_DIFF_BREAKING");
    assert!(
        v[0].message.contains("capabilities.offered removed"),
        "msg: {}",
        v[0].message
    );
}

// --- Governance rules (2026-06) ---

#[test]
fn schema_host_rejects_wrong_host() {
    let d = json!({ "$schema": "https://store.greentic.ai/schemas/describe-v2.json" });
    let v = check_schema_host(&d);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].code, "E_SCHEMA_HOST");
}

#[test]
fn schema_host_accepts_canonical() {
    let d = json!({ "$schema": "https://store.greentic.cloud/schemas/describe-v2.json" });
    assert!(check_schema_host(&d).is_empty());
}

#[test]
fn export_form_rejects_short_form() {
    let d = json!({
        "contributions": { "tools": [
            { "name": "parse_yaml", "export": "invoke-tool", "runtime_ref": "main" }
        ]}
    });
    let v = check_export_form(&d);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].code, "E_EXPORT_FORM");
}

#[test]
fn export_form_accepts_canonical() {
    let d = json!({
        "contributions": { "tools": [
            { "name": "parse_yaml",
              "export": "greentic:extension-design/tools.invoke-tool",
              "runtime_ref": "main" }
        ]}
    });
    assert!(check_export_form(&d).is_empty());
}

#[test]
fn export_form_accepts_non_tool_interfaces() {
    // A tool may be backed by validation/knowledge interfaces, not just tools.
    let d = json!({
        "contributions": { "tools": [
            { "name": "validate_card", "export": "greentic:extension-design/validation.validate-content" },
            { "name": "get_example", "export": "greentic:extension-design/knowledge.get-entry" }
        ]}
    });
    assert!(check_export_form(&d).is_empty());
}

#[test]
fn export_form_rejects_qualified_without_member() {
    let d = json!({
        "contributions": { "tools": [
            { "name": "x", "export": "greentic:extension-design/tools" }
        ]}
    });
    let v = check_export_form(&d);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].code, "E_EXPORT_FORM");
}

#[test]
fn engine_deprecated_rejects_present_engine() {
    let d = json!({ "engine": { "greenticDesigner": ">=1.2.0", "extRuntime": "^1.2.0" } });
    let v = check_engine_deprecated(&d);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].code, "E_ENGINE_DEPRECATED");
}

#[test]
fn engine_deprecated_accepts_absent_engine() {
    let d = json!({ "compat": { "min_runner_version": "^1.2.0" } });
    assert!(check_engine_deprecated(&d).is_empty());
}

#[test]
fn sha256_zero_rejects_placeholder_under_publish() {
    let d = json!({
        "runtime": { "components": {
            "main": {
                "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
                "world": "greentic:x/y"
            }
        }}
    });
    let v = check_sha256_zero(&d, true);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].code, "E_SHA256_ZERO");
}

#[test]
fn sha256_zero_skipped_without_publish() {
    let d = json!({
        "runtime": { "components": {
            "main": {
                "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
                "world": "greentic:x/y"
            }
        }}
    });
    assert!(check_sha256_zero(&d, false).is_empty());
}

#[test]
fn sha256_zero_accepts_real_hash_under_publish() {
    let d = json!({
        "runtime": { "components": {
            "main": {
                "sha256": "abc1230000000000000000000000000000000000000000000000000000000000",
                "gtpack": { "sha256": "def4560000000000000000000000000000000000000000000000000000000000" },
                "world": "greentic:x/y"
            }
        }}
    });
    assert!(check_sha256_zero(&d, true).is_empty());
}

#[test]
fn id_pattern_rejects_bad_id() {
    for bad in [
        "sorla",
        "greentic.",
        "greentic.Sorla",
        "greentic.-x",
        "com.example.x",
    ] {
        let d = json!({ "metadata": { "id": bad } });
        let v = check_id_pattern(&d);
        assert_eq!(v.len(), 1, "expected violation for {bad:?}");
        assert_eq!(v[0].code, "E_ID_PATTERN");
    }
}

#[test]
fn id_pattern_accepts_good_id() {
    for good in [
        "greentic.sorla",
        "greentic.telco-x-tools",
        "greentic.operala",
    ] {
        let d = json!({ "metadata": { "id": good } });
        assert!(
            check_id_pattern(&d).is_empty(),
            "expected no violation for {good:?}"
        );
    }
}

#[test]
fn tool_naming_rejects_non_snake_case() {
    let d = json!({ "contributions": { "tools": [ { "name": "parseSorlaYaml" } ] } });
    let v = check_tool_naming(&d);
    assert!(v.iter().any(|x| x.code == "E_TOOL_NAMING"));
}

#[test]
fn tool_naming_rejects_near_duplicate_pair() {
    let d = json!({
        "contributions": { "tools": [
            { "name": "generate_gtpack" },
            { "name": "generate_gtpack_from_sorla_yaml" }
        ]}
    });
    let v = check_tool_naming(&d);
    assert!(v.iter().any(|x| x.code == "E_TOOL_NAMING"));
}

#[test]
fn tool_naming_accepts_clean_names() {
    let d = json!({
        "contributions": { "tools": [
            { "name": "tx_resolve_prefix" },
            { "name": "tx_analyse_top_peers" }
        ]}
    });
    assert!(check_tool_naming(&d).is_empty());
}

// --- E_PERMS_SECRETS_PLAIN_KEY (S4) ---

#[test]
fn perms_secrets_plain_key_errors_on_field_name_entry() {
    // A plain env-var-style key like "SLACK_BOT_TOKEN" has no "://" or "*"
    // and does not end with "/" — it belongs in requiredSecrets, not here.
    let d = json!({
        "runtime": {
            "permissions": {
                "secrets": ["SLACK_BOT_TOKEN"]
            }
        }
    });
    let v = check_perms_secrets_plain_key(&d);
    assert_eq!(v.len(), 1, "expected one error for plain key");
    assert_eq!(v[0].code, "E_PERMS_SECRETS_PLAIN_KEY");
    assert_eq!(v[0].severity, Severity::Error);
    assert!(
        v[0].message.contains("SLACK_BOT_TOKEN"),
        "message should name the offending key: {}",
        v[0].message
    );
    assert!(
        v[0].message.contains("requiredSecrets"),
        "message should point to requiredSecrets: {}",
        v[0].message
    );
}

#[test]
fn perms_secrets_plain_key_clean_on_grant_uri() {
    // A proper URI grant — has "://" — must not trigger the warning.
    let d = json!({
        "runtime": {
            "permissions": {
                "secrets": ["secret://tavily/", "secret://slack/"]
            }
        }
    });
    assert!(
        check_perms_secrets_plain_key(&d).is_empty(),
        "URI-style grants must not produce an error"
    );
}

#[test]
fn perms_secrets_plain_key_clean_on_wildcard() {
    // A bare "*" wildcard is a valid grant and must not trigger the warning.
    let d = json!({
        "runtime": {
            "permissions": {
                "secrets": ["*"]
            }
        }
    });
    assert!(
        check_perms_secrets_plain_key(&d).is_empty(),
        "wildcard * must not produce an error"
    );
}

#[test]
fn perms_secrets_plain_key_clean_on_prefix_ending_slash() {
    // A path prefix ending with "/" (not a URI but a valid grant prefix) must not warn.
    let d = json!({
        "runtime": {
            "permissions": {
                "secrets": ["tavily/"]
            }
        }
    });
    assert!(
        check_perms_secrets_plain_key(&d).is_empty(),
        "prefix ending with / must not produce an error"
    );
}

#[test]
fn perms_secrets_plain_key_errors_multiple_bad_keys() {
    // Multiple plain keys → one error per key.
    let d = json!({
        "runtime": {
            "permissions": {
                "secrets": ["SLACK_BOT_TOKEN", "secret://valid/", "OPENAI_API_KEY"]
            }
        }
    });
    let v = check_perms_secrets_plain_key(&d);
    assert_eq!(v.len(), 2, "expected two errors for two plain keys: {v:?}");
    assert!(v.iter().all(|x| x.code == "E_PERMS_SECRETS_PLAIN_KEY"));
}

#[test]
fn perms_secrets_plain_key_clean_when_permissions_absent() {
    // No permissions block at all — must not panic or warn.
    let d = json!({ "runtime": {} });
    assert!(check_perms_secrets_plain_key(&d).is_empty());
}

#[test]
fn perms_secrets_plain_key_clean_when_secrets_array_empty() {
    let d = json!({
        "runtime": {
            "permissions": {
                "secrets": []
            }
        }
    });
    assert!(check_perms_secrets_plain_key(&d).is_empty());
}

// --- E_SECRET_KEY_NOT_CANONICAL (S3 / D2) ---

#[test]
fn secret_key_canonical_errors_on_uppercase_required_secret() {
    // SLACK_BOT_TOKEN is uppercase — not canonical D2 form.
    let d = json!({
        "requiredSecrets": [{ "key": "SLACK_BOT_TOKEN", "description": "Slack bot token" }]
    });
    let v = check_secret_key_canonical(&d);
    assert_eq!(v.len(), 1, "expected one error for uppercase key: {v:?}");
    assert_eq!(v[0].code, "E_SECRET_KEY_NOT_CANONICAL");
    assert_eq!(v[0].severity, Severity::Error);
    assert!(
        v[0].message.contains("SLACK_BOT_TOKEN"),
        "message should name the offending key: {}",
        v[0].message
    );
}

#[test]
fn secret_key_canonical_clean_on_canonical_required_secret() {
    // tavily/api_key matches [a-z0-9._-/], no leading/trailing /, no .. segment.
    let d = json!({
        "requiredSecrets": [{ "key": "tavily/api_key", "description": "Tavily API key" }]
    });
    assert!(
        check_secret_key_canonical(&d).is_empty(),
        "canonical key must not produce an error"
    );
}

#[test]
fn secret_key_canonical_errors_on_uppercase_in_tool_secret_requirements() {
    // Uppercase key under contributions.tools[].secret_requirements also errors.
    let d = json!({
        "contributions": {
            "tools": [{
                "name": "my_tool",
                "secret_requirements": [{ "key": "OPENAI_API_KEY", "description": "OpenAI key" }]
            }]
        }
    });
    let v = check_secret_key_canonical(&d);
    assert_eq!(
        v.len(),
        1,
        "expected one error for tool-level uppercase key: {v:?}"
    );
    assert_eq!(v[0].code, "E_SECRET_KEY_NOT_CANONICAL");
    assert!(
        v[0].message.contains("OPENAI_API_KEY"),
        "message should name the offending key: {}",
        v[0].message
    );
}

#[test]
fn secret_key_canonical_clean_when_no_secrets_declared() {
    let d = json!({ "metadata": { "id": "greentic.x" } });
    assert!(check_secret_key_canonical(&d).is_empty());
}

#[test]
fn secret_key_canonical_rejects_leading_slash() {
    let d = json!({ "requiredSecrets": [{ "key": "/bad/key" }] });
    let v = check_secret_key_canonical(&d);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].code, "E_SECRET_KEY_NOT_CANONICAL");
}

#[test]
fn secret_key_canonical_rejects_dotdot_segment() {
    let d = json!({ "requiredSecrets": [{ "key": "foo/../bar" }] });
    let v = check_secret_key_canonical(&d);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].code, "E_SECRET_KEY_NOT_CANONICAL");
}

#[test]
fn secret_key_canonical_rejects_uri_scheme() {
    // A key with "://" is a URI, not a canonical D2 key.
    let d = json!({ "requiredSecrets": [{ "key": "secret://tavily/api_key" }] });
    let v = check_secret_key_canonical(&d);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].code, "E_SECRET_KEY_NOT_CANONICAL");
}

#[test]
fn secret_key_canonical_rejects_star_wildcard() {
    let d = json!({ "requiredSecrets": [{ "key": "*" }] });
    let v = check_secret_key_canonical(&d);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].code, "E_SECRET_KEY_NOT_CANONICAL");
}

/// The scaffold's default id must satisfy the linter shipped beside it.
///
/// `gtdx new` defaulted to `com.example.<name>` while `E_ID_PATTERN` requires
/// the `greentic.` namespace, so an untouched scaffold failed `gtdx lint` with
/// exit 1 for every kind — the tool's own output rejected by its own governance
/// rule, with nothing anywhere connecting the two. Both sides were deliberate
/// and neither knew about the other.
///
/// This asserts the pair rather than either half: change the namespace the rule
/// enforces, or the namespace the scaffold defaults to, and this fails.
#[test]
fn the_scaffold_default_id_passes_the_id_rule() {
    for name in ["demo", "telco-x", "a1"] {
        let id = crate::commands::new::default_id(name);
        let d = json!({ "metadata": { "id": id } });
        let v = check_id_pattern(&d);
        assert!(
            v.is_empty(),
            "gtdx new {name} scaffolds id {:?}, which gtdx lint rejects: {v:?}",
            crate::commands::new::default_id(name)
        );
    }
}

// --- contributions.views[] (August 2026) ---

use rules_views::check_views;

fn view_project(entry: &str, html: Option<&str>) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    if let Some(body) = html {
        let asset_dir = dir.path().join("assets/views/hello");
        std::fs::create_dir_all(&asset_dir).unwrap();
        std::fs::write(asset_dir.join(entry), body).unwrap();
    }
    dir
}

fn describe_with_view(entry: &str, slot: &str) -> serde_json::Value {
    json!({
        "contributions": {
            "views": [{
                "id": "hello",
                "surface": "designer",
                "title_key": "k",
                "title_fallback": "Hello",
                "entry": entry,
                "placement": { "slot": slot }
            }]
        }
    })
}

#[test]
fn view_entry_present_is_clean() {
    let dir = view_project(
        "index.html",
        Some("<h1>hi</h1><script src=\"app.js\"></script>"),
    );
    let d = describe_with_view("index.html", "designer.sidebar");
    assert!(check_views(&d, dir.path()).is_empty());
}

#[test]
fn view_entry_missing_is_an_error() {
    let dir = view_project("index.html", None);
    let d = describe_with_view("index.html", "designer.sidebar");
    let v = check_views(&d, dir.path());
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].code, "E_VIEW_ENTRY_MISSING");
}

#[test]
fn view_entry_escaping_its_directory_is_an_error() {
    let dir = view_project("index.html", Some("<h1>hi</h1>"));
    let d = describe_with_view("../../../etc/passwd", "designer.sidebar");
    let v = check_views(&d, dir.path());
    assert!(
        v.iter().any(|x| x.code == "E_VIEW_ENTRY_PATH"),
        "traversal must be reported before the file is looked up: {v:?}"
    );
}

#[test]
fn remote_script_in_the_entry_is_an_error() {
    let dir = view_project(
        "index.html",
        Some("<script src=\"https://cdn.example.com/x.js\"></script>"),
    );
    let d = describe_with_view("index.html", "designer.sidebar");
    let v = check_views(&d, dir.path());
    assert!(
        v.iter().any(|x| x.code == "E_VIEW_REMOTE_ASSET"),
        "manifest integrity is theatre if the page pulls unverified code: {v:?}"
    );
}

#[test]
fn unknown_slot_is_a_warning_not_an_error() {
    let dir = view_project("index.html", Some("<h1>hi</h1>"));
    let d = describe_with_view("index.html", "admin.notARealSlot");
    let v = check_views(&d, dir.path());
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].code, "W_VIEW_SLOT_UNKNOWN");
    assert_eq!(
        v[0].severity,
        Severity::Warning,
        "the SDK's slot list is a snapshot and goes stale by construction — a \
         stale snapshot must never fail a build"
    );
}

#[test]
fn describe_without_views_is_clean() {
    let dir = tempfile::tempdir().expect("tempdir");
    let d = json!({ "contributions": {} });
    assert!(check_views(&d, dir.path()).is_empty());
}

// --- E_VIEW_REMOTE_ASSET is scoped to the tag that owns the attribute ---

#[test]
fn an_ordinary_anchor_with_a_remote_href_lints_clean() {
    let dir = view_project(
        "index.html",
        Some(r#"<a href="https://docs.example.com">Docs</a>"#),
    );
    let d = describe_with_view("index.html", "designer.sidebar");
    assert!(
        check_views(&d, dir.path()).is_empty(),
        "a hyperlink is not a fetched asset the manifest needs to vouch for"
    );
}

#[test]
fn a_remote_stylesheet_link_is_still_an_error() {
    let dir = view_project(
        "index.html",
        Some(r#"<link rel="stylesheet" href="https://cdn.example.com/x.css">"#),
    );
    let d = describe_with_view("index.html", "designer.sidebar");
    let v = check_views(&d, dir.path());
    assert!(
        v.iter().any(|x| x.code == "E_VIEW_REMOTE_ASSET"),
        "a remote <link> stylesheet must still be caught: {v:?}"
    );
}

#[test]
fn a_protocol_relative_single_quoted_script_src_is_still_an_error() {
    let dir = view_project(
        "index.html",
        Some(r"<script src='//cdn.example.com/x.js'></script>"),
    );
    let d = describe_with_view("index.html", "designer.sidebar");
    let v = check_views(&d, dir.path());
    assert!(
        v.iter().any(|x| x.code == "E_VIEW_REMOTE_ASSET"),
        "single-quote protocol-relative src must be as covered as its double-quote twin: {v:?}"
    );
}

// --- E_VIEW_ID_PATTERN ---

fn describe_with_view_id(id: &str) -> serde_json::Value {
    json!({
        "contributions": {
            "views": [{
                "id": id,
                "surface": "designer",
                "title_key": "k",
                "title_fallback": "Hello",
                "entry": "index.html",
                "placement": { "slot": "designer.sidebar" }
            }]
        }
    })
}

#[test]
fn a_valid_view_id_lints_clean() {
    let dir = view_project("index.html", Some("<h1>hi</h1>"));
    // `view_project` writes assets under `assets/views/hello`, matching the
    // id used below, so a clean pass here also exercises the id check
    // running ahead of a real entry lookup rather than short-circuiting it.
    let d = describe_with_view_id("hello");
    assert!(check_views(&d, dir.path()).is_empty());
}

#[test]
fn a_traversal_id_is_rejected_as_an_invalid_id_not_a_missing_entry() {
    let dir = tempfile::tempdir().expect("tempdir");
    let d = describe_with_view_id("../../etc");
    let v = check_views(&d, dir.path());
    assert_eq!(v.len(), 1);
    assert_eq!(
        v[0].code, "E_VIEW_ID_PATTERN",
        "an id that would steer the asset path off the view's own directory \
         must be caught as an invalid id before it is ever joined into a \
         path, not surfaced as E_VIEW_ENTRY_MISSING once the damage is done: {v:?}"
    );
    assert!(
        v.iter().all(|x| x.code != "E_VIEW_ENTRY_MISSING"),
        "must not also report the wrong error: {v:?}"
    );
}
