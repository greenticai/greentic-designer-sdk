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
    // Installs land at `<id>-<version>` (flat layout), never at a bare
    // `<id>/` — mirror that here so the rule under test actually finds it.
    let version = describe
        .pointer("/metadata/version")
        .and_then(|v| v.as_str())
        .expect("fixture describe must set metadata.version");
    let dir = home
        .join("extensions")
        .join("design")
        .join(format!("{id}-{version}"));
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

/// The rule reads the flat install layout (`<id>-<version>`), not a bare
/// `<id>/` directory — nothing installs at the bare path, and prior to this
/// fix the rule always read that nonexistent path, which is why it never
/// fired for any kind.
#[test]
fn describe_diff_ignores_a_bare_id_directory_with_no_version_suffix() {
    let home = empty_home();
    let bare_dir = home.path().join("extensions/design/x");
    std::fs::create_dir_all(&bare_dir).unwrap();
    std::fs::write(
        bare_dir.join("describe.json"),
        serde_json::to_vec(&json!({
            "metadata": {"id": "x", "version": "1.0.0"},
            "kind": "DesignExtension",
            "contributions": {"tools": [{"name": "t1"}]}
        }))
        .unwrap(),
    )
    .unwrap();
    let current = json!({
        "metadata": {"id": "x", "version": "1.0.0"},
        "kind": "DesignExtension",
        "contributions": {"tools": []}
    });
    assert!(
        check_describe_diff_breaking(&current, home.path()).is_empty(),
        "a bare <id>/ dir (no -<version> suffix) must not be treated as an installed copy"
    );
}

/// When multiple versions of the same extension are installed, the rule must
/// diff against the highest one, not an arbitrary directory-listing order.
#[test]
fn describe_diff_diffs_against_the_highest_installed_version() {
    let home = empty_home();
    let older = json!({
        "metadata": {"id": "x", "version": "1.0.0"},
        "kind": "DesignExtension",
        "contributions": {"tools": [{"name": "only_in_old"}]}
    });
    let newer = json!({
        "metadata": {"id": "x", "version": "1.5.0"},
        "kind": "DesignExtension",
        "contributions": {"tools": [{"name": "t1"}]}
    });
    write_installed(home.path(), "x", &older);
    write_installed(home.path(), "x", &newer);
    let current = json!({
        "metadata": {"id": "x", "version": "1.5.0"},
        "kind": "DesignExtension",
        "contributions": {"tools": [{"name": "t1"}]}
    });
    // Nothing removed vs the newer (1.5.0) install even though the older
    // (1.0.0) install's tool set differs — proves 1.5.0 was the one diffed.
    assert!(
        check_describe_diff_breaking(&current, home.path()).is_empty(),
        "must diff against the highest installed version (1.5.0), not the lowest"
    );
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
    let installed_dir = home.path().join("extensions/design/com.example.x-0.1.0");
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
    let installed_dir = home.path().join("extensions/design/com.example.x-0.1.0");
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
    let installed_dir = home.path().join("extensions/design/com.example.y-0.1.0");
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
        "3aigent.designer",
        "greentic.telco_x",
        // A digit-led word becomes an invalid WIT label: 1.2.16 let these
        // through and they scaffolded projects `cargo component build` refused.
        "greentic.3aigent-designer",
        "greentic.provider-3aigent",
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
        // The namespace is the author's to choose — this was rejected before
        // 1.2.16, in disagreement with `describe-v2.json`.
        "com.acme.my-ext",
        // Digits are fine once a word has started.
        "greentic.aigent3-designer",
        "io.github.someone.viewer3d",
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

/// `E_ID_PATTERN` used to print only the regex, leaving the author to diff
/// their id against it by eye.
#[test]
fn id_pattern_message_names_the_offending_part() {
    let d = json!({ "metadata": { "id": "greentic.Sorla" } });
    let v = check_id_pattern(&d);
    assert_eq!(v.len(), 1);
    assert!(v[0].message.contains("Sorla"), "{}", v[0].message);
    assert!(v[0].message.contains("lowercase"), "{}", v[0].message);
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
/// `gtdx new` defaulted to `com.example.<name>` while `E_ID_PATTERN` required
/// the `greentic.` namespace, so an untouched scaffold failed `gtdx lint` with
/// exit 1 for every kind — the tool's own output rejected by its own governance
/// rule, with nothing anywhere connecting the two. Both sides were deliberate
/// and neither knew about the other.
///
/// The namespace requirement is gone, but the pairing still needs asserting:
/// the scaffold default must satisfy the linter shipped beside it, whatever
/// either one becomes. `a1` and `viewer3d` cover the digits that the pre-1.2.16
/// rule choked on — a name whose *word* starts with a digit is rejected as a
/// name, so it never reaches the id rule.
#[test]
fn the_scaffold_default_id_passes_the_id_rule() {
    for name in ["demo", "telco-x", "a1", "viewer3d", "aigent3-designer"] {
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

/// `kind_dir_name` re-implemented `dir_name()` from the wire string and
/// omitted `wasix:mcp/router`, so `W_DESCRIBE_DIFF_BREAKING` silently skipped
/// every MCP router. A lint that reports nothing is indistinguishable from a
/// lint that found nothing.
#[test]
fn kind_dir_name_resolves_every_kind() {
    use greentic_extension_sdk_contract::ExtensionKind;

    for kind in ExtensionKind::ALL.iter().copied() {
        assert_eq!(
            super::rules::kind_dir_name(kind.wire_name()),
            Some(kind.dir_name()),
            "kind_dir_name failed for wire name {}",
            kind.wire_name()
        );
    }
}

#[test]
fn kind_dir_name_rejects_an_unknown_wire_name() {
    // `AddonExtension` used to be the canonical "not a kind yet" example
    // here; now that `ExtensionKind::Addon` exists (wire name
    // `AddonExtension`), `BogusExtension` fills that role instead.
    assert_eq!(super::rules::kind_dir_name("BogusExtension"), None);
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

// --- contributions.addons ---

use rules_addons::check_addons;

fn describe_with_addon(addon: &serde_json::Value) -> serde_json::Value {
    json!({ "contributions": { "addons": [addon] } })
}

fn base_addon() -> serde_json::Value {
    json!({
        "id": "qdrant",
        "family": "vector-db",
        "display_name": "Qdrant",
        "description": "Vector database.",
        "config_schema": "{\"type\":\"object\"}",
        "desired_state_schema": "{\"type\":\"object\",\"properties\":{\"collections\":{\"type\":\"array\"}}}",
        "outputs": [{ "name": "QDRANT_URL", "type": "text" }]
    })
}

#[test]
fn a_well_formed_addon_produces_no_violations() {
    let v = check_addons(
        &describe_with_addon(&base_addon()),
        tempfile::tempdir().expect("tempdir").path(),
    );
    assert!(v.is_empty(), "expected no violations, got: {v:?}");
}

#[test]
fn an_id_with_uppercase_or_underscores_is_an_error() {
    for bad in ["Qdrant", "qdrant_db", "-qdrant", ""] {
        let mut a = base_addon();
        a["id"] = json!(bad);
        let v = check_addons(
            &describe_with_addon(&a),
            tempfile::tempdir().expect("tempdir").path(),
        );
        assert!(
            v.iter().any(|x| x.code == "E_ADDON_ID_PATTERN"),
            "id {bad:?} should be rejected, got: {v:?}"
        );
    }
}

/// Output names become environment variables on the consuming service, so a
/// name that is not a valid identifier breaks at injection time.
#[test]
fn an_output_name_that_is_not_env_var_safe_is_an_error() {
    for bad in ["qdrant-url", "1url", "url!", ""] {
        let mut a = base_addon();
        a["outputs"] = json!([{ "name": bad, "type": "text" }]);
        let v = check_addons(
            &describe_with_addon(&a),
            tempfile::tempdir().expect("tempdir").path(),
        );
        assert!(
            v.iter().any(|x| x.code == "E_ADDON_OUTPUT_NAME"),
            "output name {bad:?} should be rejected, got: {v:?}"
        );
    }
}

/// Spec D16. A credential in desired state can never be read back by
/// `observe`, so it diffs forever and no plan is ever clean. Catching it here
/// is cheaper than discovering it when the first reconcile never converges.
#[test]
fn a_secret_looking_property_in_desired_state_is_an_error() {
    for bad in [
        "password",
        "apiKey",
        "api_key",
        "auth_token",
        "clientSecret",
        "credentials",
        "token",
        "authToken",
        "refresh-token",
    ] {
        let mut a = base_addon();
        a["desired_state_schema"] = json!(format!(
            r#"{{"type":"object","properties":{{"{bad}":{{"type":"string"}}}}}}"#
        ));
        let v = check_addons(
            &describe_with_addon(&a),
            tempfile::tempdir().expect("tempdir").path(),
        );
        assert!(
            v.iter()
                .any(|x| x.code == "E_ADDON_SECRET_IN_DESIRED_STATE"),
            "desired-state property {bad:?} should be rejected, got: {v:?}"
        );
    }
}

/// The same word in `config_schema` is fine — config is not reconciled
/// against observed state, so it does not diff forever.
#[test]
fn a_secret_looking_property_in_config_schema_is_not_flagged() {
    let mut a = base_addon();
    a["config_schema"] = json!(r#"{"type":"object","properties":{"password":{"type":"string"}}}"#);
    let v = check_addons(
        &describe_with_addon(&a),
        tempfile::tempdir().expect("tempdir").path(),
    );
    assert!(
        !v.iter()
            .any(|x| x.code == "E_ADDON_SECRET_IN_DESIRED_STATE"),
        "config_schema must not be flagged, got: {v:?}"
    );
}

/// `token` only names a credential when it is the final segment of the
/// property name — the head noun, not a modifier earlier in the name.
/// `max_tokens` and `token_limit` are about tokens (a count, a limit), not
/// a token itself, so they must not be flagged. Note `max_tokens` is
/// plural: the last segment is `tokens`, a different segment than `token`,
/// which is exactly why this can't be loosened back to a substring check.
#[test]
fn properties_where_token_is_a_modifier_not_the_head_noun_are_not_flagged() {
    for ok in ["max_tokens", "token_limit", "tokenizer"] {
        let mut a = base_addon();
        a["desired_state_schema"] = json!(format!(
            r#"{{"type":"object","properties":{{"{ok}":{{"type":"string"}}}}}}"#
        ));
        let v = check_addons(
            &describe_with_addon(&a),
            tempfile::tempdir().expect("tempdir").path(),
        );
        assert!(
            !v.iter()
                .any(|x| x.code == "E_ADDON_SECRET_IN_DESIRED_STATE"),
            "desired-state property {ok:?} must not be flagged, got: {v:?}"
        );
    }
}

#[test]
fn an_unfamiliar_family_is_a_warning_not_an_error() {
    let mut a = base_addon();
    a["family"] = json!("quantum-db");
    let v = check_addons(
        &describe_with_addon(&a),
        tempfile::tempdir().expect("tempdir").path(),
    );
    let hit = v
        .iter()
        .find(|x| x.code == "W_ADDON_FAMILY_UNKNOWN")
        .unwrap_or_else(|| panic!("expected W_ADDON_FAMILY_UNKNOWN, got: {v:?}"));
    assert!(
        matches!(hit.severity, Severity::Warning),
        "an unfamiliar family must warn, not fail the run: {hit:?}"
    );
}

#[test]
fn a_describe_with_no_addons_produces_no_violations() {
    let v = check_addons(
        &json!({ "contributions": {} }),
        tempfile::tempdir().expect("tempdir").path(),
    );
    assert!(v.is_empty(), "expected no violations, got: {v:?}");
}

/// D16 cites `rediscloud_acl_user` by name - Redis ACL users, a list of
/// managed objects each carrying a `password`. The old top-level-only check
/// never looked inside `items.properties` and so never fired on the exact
/// case it was written for.
#[test]
fn a_secret_nested_under_items_properties_is_caught_with_a_path() {
    let mut a = base_addon();
    a["desired_state_schema"] = json!(
        r#"{"type":"object","properties":{
            "acl_users":{"type":"array","items":{"type":"object",
                "properties":{"username":{"type":"string"},"password":{"type":"string"}}}}}}"#
    );
    let v = check_addons(
        &describe_with_addon(&a),
        tempfile::tempdir().expect("tempdir").path(),
    );
    let hit = v
        .iter()
        .find(|x| x.code == "E_ADDON_SECRET_IN_DESIRED_STATE")
        .unwrap_or_else(|| panic!("expected E_ADDON_SECRET_IN_DESIRED_STATE, got: {v:?}"));
    assert!(
        hit.message.contains("acl_users[].password"),
        "message should carry the full path to the offending property: {hit:?}"
    );
    assert!(
        !v.iter().any(|x| x.message.contains("username")),
        "username is not a secret and must not be flagged: {v:?}"
    );
}

/// A secret defined inside `$defs` (reachable only by `$ref` elsewhere in
/// the schema) must still be caught - the walk covers `$defs` unconditionally
/// rather than trying to resolve `$ref`.
#[test]
fn a_secret_reachable_only_through_defs_is_caught() {
    let mut a = base_addon();
    a["desired_state_schema"] = json!(
        r##"{"type":"object","$defs":{"credentials":{"type":"object",
            "properties":{"password":{"type":"string"}}}},
            "properties":{"admin":{"$ref":"#/$defs/credentials"}}}"##
    );
    let v = check_addons(
        &describe_with_addon(&a),
        tempfile::tempdir().expect("tempdir").path(),
    );
    assert!(
        v.iter()
            .any(|x| x.code == "E_ADDON_SECRET_IN_DESIRED_STATE"),
        "a secret nested inside $defs must be caught: {v:?}"
    );
}

/// A secret defined inside an `allOf` branch must be caught.
#[test]
fn a_secret_inside_an_all_of_branch_is_caught() {
    let mut a = base_addon();
    a["desired_state_schema"] = json!(
        r#"{"type":"object","allOf":[{"type":"object",
            "properties":{"admin_password":{"type":"string"}}}]}"#
    );
    let v = check_addons(
        &describe_with_addon(&a),
        tempfile::tempdir().expect("tempdir").path(),
    );
    assert!(
        v.iter()
            .any(|x| x.code == "E_ADDON_SECRET_IN_DESIRED_STATE"),
        "a secret nested inside an allOf branch must be caught: {v:?}"
    );
}

/// Draft 2020-12 declares tuples with `prefixItems`, not array-form `items`
/// (which is deprecated there). `describe-v2.json` is a 2020-12 schema, so
/// this is the form addon authors actually write.
#[test]
fn a_secret_inside_prefix_items_is_caught_with_an_array_path() {
    let mut a = base_addon();
    a["desired_state_schema"] = json!(
        r#"{"type":"object","properties":{"pairs":{"type":"array",
            "prefixItems":[{"type":"object",
                "properties":{"password":{"type":"string"}}}]}}}"#
    );
    let v = check_addons(
        &describe_with_addon(&a),
        tempfile::tempdir().expect("tempdir").path(),
    );
    let hit = v
        .iter()
        .find(|x| x.code == "E_ADDON_SECRET_IN_DESIRED_STATE")
        .unwrap_or_else(|| panic!("expected E_ADDON_SECRET_IN_DESIRED_STATE, got: {v:?}"));
    assert!(
        hit.message.contains("pairs[].password"),
        "message should carry the array path to the offending property: {hit:?}"
    );
}

/// `contains` is the non-tuple counterpart of `items` - a schema at least
/// one array element must match.
#[test]
fn a_secret_inside_contains_is_caught() {
    let mut a = base_addon();
    a["desired_state_schema"] = json!(
        r#"{"type":"object","properties":{"nodes":{"type":"array",
            "contains":{"type":"object",
                "properties":{"admin_password":{"type":"string"}}}}}}"#
    );
    let v = check_addons(
        &describe_with_addon(&a),
        tempfile::tempdir().expect("tempdir").path(),
    );
    assert!(
        v.iter()
            .any(|x| x.code == "E_ADDON_SECRET_IN_DESIRED_STATE"),
        "a secret nested inside contains must be caught: {v:?}"
    );
}

/// `if`, `then`, `else` all constrain the same data position as their
/// parent and can carry a nested `properties` map, exactly like `allOf`.
/// Data can genuinely match an `if` before `then` applies, so these are
/// real positions, unlike `not` below.
#[test]
fn a_secret_inside_if_then_or_else_is_caught() {
    for wrapper in ["if", "then", "else"] {
        let mut a = base_addon();
        a["desired_state_schema"] = json!(format!(
            r#"{{"type":"object","{wrapper}":{{"type":"object",
                "properties":{{"admin_password":{{"type":"string"}}}}}}}}"#
        ));
        let v = check_addons(
            &describe_with_addon(&a),
            tempfile::tempdir().expect("tempdir").path(),
        );
        assert!(
            v.iter()
                .any(|x| x.code == "E_ADDON_SECRET_IN_DESIRED_STATE"),
            "a secret nested inside {wrapper:?} must be caught: {v:?}"
        );
    }
}

/// `not: {"properties":{"admin_password":...}}` means the instance must NOT
/// have `admin_password` in that shape - the author is forbidding the
/// credential, not declaring one. A name reachable only through `not` must
/// not be flagged: doing so would invert the schema's meaning and punish an
/// author for writing the exact prohibition D16 recommends.
#[test]
fn a_secret_reachable_only_through_not_is_not_flagged() {
    let mut a = base_addon();
    a["desired_state_schema"] = json!(
        r#"{"type":"object","not":{"type":"object",
            "properties":{"admin_password":{"type":"string"}}}}"#
    );
    let v = check_addons(
        &describe_with_addon(&a),
        tempfile::tempdir().expect("tempdir").path(),
    );
    assert!(
        v.is_empty(),
        "a property forbidden by `not` must not be flagged as declared: {v:?}"
    );
}

/// `unevaluatedProperties` is 2020-12's successor to `additionalProperties`
/// for properties left over after composition - it takes a schema, not
/// only a boolean, and that schema is a real leftover-property data
/// position, so a secret hiding inside it must be caught.
#[test]
fn a_secret_inside_unevaluated_properties_is_caught() {
    let mut a = base_addon();
    a["desired_state_schema"] = json!(
        r#"{"type":"object","properties":{"known":{"type":"string"}},
            "unevaluatedProperties":{"type":"object",
                "properties":{"leftover_password":{"type":"string"}}}}"#
    );
    let v = check_addons(
        &describe_with_addon(&a),
        tempfile::tempdir().expect("tempdir").path(),
    );
    assert!(
        v.iter().any(|x| x.code == "E_ADDON_SECRET_IN_DESIRED_STATE"
            && x.message.contains("leftover_password")),
        "a secret nested inside unevaluatedProperties must be caught: {v:?}"
    );
}

/// `dependentSchemas` values are full schemas applied to the same object,
/// so a secret hiding inside one must be caught.
#[test]
fn a_secret_inside_dependent_schemas_is_caught() {
    let mut a = base_addon();
    a["desired_state_schema"] = json!(
        r#"{"type":"object","properties":{"credit_card":{"type":"string"}},
            "dependentSchemas":{"credit_card":{"type":"object",
                "properties":{"cvv_secret":{"type":"string"}}}}}"#
    );
    let v = check_addons(
        &describe_with_addon(&a),
        tempfile::tempdir().expect("tempdir").path(),
    );
    assert!(
        v.iter().any(
            |x| x.code == "E_ADDON_SECRET_IN_DESIRED_STATE" && x.message.contains("cvv_secret")
        ),
        "a secret nested inside a dependentSchemas value must be caught: {v:?}"
    );
}

/// `propertyNames` validates property *names* as strings, never the object
/// itself - a `properties` map placed inside it is schema-legal but dead,
/// since it can never apply to any actual data. It must not be walked.
#[test]
fn a_properties_map_inside_property_names_is_not_walked() {
    let mut a = base_addon();
    a["desired_state_schema"] = json!(
        r#"{"type":"object","propertyNames":{"type":"string",
            "properties":{"password":{"type":"string"}}}}"#
    );
    let v = check_addons(
        &describe_with_addon(&a),
        tempfile::tempdir().expect("tempdir").path(),
    );
    assert!(
        v.is_empty(),
        "propertyNames must not be walked for nested properties: {v:?}"
    );
}

/// A `$defs` nested under a real data position must not report a path that
/// looks like it lives there. `foo.password` would send the author looking
/// for a `password` property directly under `foo`, when none exists - the
/// `password` here only exists inside a definition, reachable (if at all)
/// through a `$ref` this walk never resolves.
#[test]
fn a_secret_inside_nested_defs_reports_a_defs_marker_not_a_fake_data_path() {
    let mut a = base_addon();
    a["desired_state_schema"] = json!(
        r#"{"type":"object","properties":{"foo":{"type":"object",
            "$defs":{"credentials":{"type":"object",
                "properties":{"password":{"type":"string"}}}}}}}"#
    );
    let v = check_addons(
        &describe_with_addon(&a),
        tempfile::tempdir().expect("tempdir").path(),
    );
    let hit = v
        .iter()
        .find(|x| x.code == "E_ADDON_SECRET_IN_DESIRED_STATE")
        .unwrap_or_else(|| panic!("expected E_ADDON_SECRET_IN_DESIRED_STATE, got: {v:?}"));
    assert!(
        hit.message.contains("foo.$defs/credentials.password"),
        "path should carry a $defs marker rather than a fake data position: {hit:?}"
    );
    assert!(
        !hit.message.contains("\"foo.password\""),
        "must not report the nonexistent data position foo.password: {hit:?}"
    );
}

/// The same fix at the root: `$defs` there already had a usable path before
/// this change (a bare `password`), but the marker format must still apply
/// consistently rather than only kicking in once nesting is involved.
#[test]
fn a_secret_inside_root_defs_also_carries_the_defs_marker() {
    let mut a = base_addon();
    a["desired_state_schema"] = json!(
        r##"{"type":"object","$defs":{"credentials":{"type":"object",
            "properties":{"password":{"type":"string"}}}},
            "properties":{"admin":{"$ref":"#/$defs/credentials"}}}"##
    );
    let v = check_addons(
        &describe_with_addon(&a),
        tempfile::tempdir().expect("tempdir").path(),
    );
    let hit = v
        .iter()
        .find(|x| x.code == "E_ADDON_SECRET_IN_DESIRED_STATE")
        .unwrap_or_else(|| panic!("expected E_ADDON_SECRET_IN_DESIRED_STATE, got: {v:?}"));
    assert!(
        hit.message.contains("$defs/credentials.password"),
        "path should carry the $defs marker: {hit:?}"
    );
}

/// A JSON Schema may legally have a property literally named `properties`.
/// The keyword itself must never be treated as a candidate name - only
/// values that appear as a key *inside* a `properties` map are.
#[test]
fn a_property_literally_named_properties_is_evaluated_as_a_property_not_a_keyword() {
    let mut a = base_addon();
    a["desired_state_schema"] =
        json!(r#"{"type":"object","properties":{"properties":{"type":"string"}}}"#);
    let v = check_addons(
        &describe_with_addon(&a),
        tempfile::tempdir().expect("tempdir").path(),
    );
    assert!(
        v.is_empty(),
        "a property literally named `properties` is not itself a secret: {v:?}"
    );
}
/// Every row of the review table in item 5: none of these are credentials,
/// and every one of them must NOT be flagged.
#[test]
fn legitimate_day_2_properties_are_not_flagged() {
    for ok in [
        "password_encryption",
        "scram_password_iterations",
        "password_policy",
        "min_password_length",
        "require_password",
        "api_key_id",
        "secret_ref",
        "secret_name",
        "secretKeyRef",
        "admin_secret_ref",
        "credential_rotation_days",
        "allow_credentials",
        "secrets_backend",
    ] {
        let mut a = base_addon();
        a["desired_state_schema"] = json!(format!(
            r#"{{"type":"object","properties":{{"{ok}":{{"type":"string"}}}}}}"#
        ));
        let v = check_addons(
            &describe_with_addon(&a),
            tempfile::tempdir().expect("tempdir").path(),
        );
        assert!(
            !v.iter()
                .any(|x| x.code == "E_ADDON_SECRET_IN_DESIRED_STATE"),
            "legitimate property {ok:?} must not be flagged, got: {v:?}"
        );
    }
}

/// The narrowing in item 5 must not weaken the positives item 5 explicitly
/// says to keep firing.
#[test]
fn narrowing_does_not_weaken_the_existing_positives() {
    for bad in [
        "password",
        "admin_password",
        "apiKey",
        "clientSecret",
        "auth_token",
    ] {
        let mut a = base_addon();
        a["desired_state_schema"] = json!(format!(
            r#"{{"type":"object","properties":{{"{bad}":{{"type":"string"}}}}}}"#
        ));
        let v = check_addons(
            &describe_with_addon(&a),
            tempfile::tempdir().expect("tempdir").path(),
        );
        assert!(
            v.iter()
                .any(|x| x.code == "E_ADDON_SECRET_IN_DESIRED_STATE"),
            "{bad:?} must still be flagged, got: {v:?}"
        );
    }
}

/// Pins the second safety assumption documented on `walk_schema_properties`:
/// that function recurses with no depth guard of its own, which is safe only
/// because `serde_json::from_str` refuses to *parse* anything nested past its
/// default 128-frame `remaining_depth` limit in the first place. If this
/// crate ever enables `serde_json`'s `unbounded_depth` feature, this test
/// starts failing - which is the signal that `walk_schema_properties` now
/// needs an explicit depth guard of its own.
#[test]
fn a_desired_state_schema_nested_past_serde_json_depth_limit_fails_to_parse() {
    // 200 nested arrays comfortably clears serde_json's 128-frame default
    // depth limit regardless of exactly where the off-by-one boundary falls.
    let depth = 200;
    let nested: String = "[".repeat(depth) + &"]".repeat(depth);

    // The assumption itself: serde_json rejects this at parse time.
    assert!(
        serde_json::from_str::<serde_json::Value>(&nested).is_err(),
        "expected serde_json to refuse to parse {depth} levels of nesting - \
         if it now succeeds, `unbounded_depth` may have been enabled \
         somewhere in the dependency graph, and \
         `walk_schema_properties`'s lack of a depth guard is no longer safe"
    );

    // The consequence relied on by `check_addons`: since parsing fails, the
    // `if let Ok(parsed) = ...` around the walk skips this schema entirely -
    // `walk_schema_properties` never sees it, and no violation is reported.
    let mut a = base_addon();
    a["desired_state_schema"] = json!(nested);
    let v = check_addons(
        &describe_with_addon(&a),
        tempfile::tempdir().expect("tempdir").path(),
    );
    assert!(
        v.is_empty(),
        "an unparsable desired_state_schema must be silently skipped, not \
         reach the walk: {v:?}"
    );
}

// --- supports_backup vs wit/world.wit ---

/// Writes `wit/world.wit` under a fresh tempdir and returns the dir, so
/// `check_addons` can read it back exactly the way it reads a real
/// extension's source tree.
fn dir_with_world_wit(source: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(dir.path().join("wit")).expect("mkdir wit");
    std::fs::write(dir.path().join("wit/world.wit"), source).expect("write world.wit");
    dir
}

/// What `gtdx new --kind addon` actually writes: `validation`, `workload`
/// and `reconciler` exported, `backup` deliberately absent.
const WORLD_WITHOUT_BACKUP: &str = r"
package greentic:example-cache;

world extension {
  import greentic:extension-base/types@0.3.0;
  import greentic:extension-host/logging@0.1.0;

  export greentic:extension-base/manifest@0.3.0;
  export greentic:extension-base/lifecycle@0.3.0;
  export greentic:extension-addon/validation@0.1.0;
  export greentic:extension-addon/workload@0.1.0;
  export greentic:extension-addon/reconciler@0.1.0;
}
";

/// The same world, with `backup` genuinely exported.
const WORLD_WITH_BACKUP: &str = r"
package greentic:example-cache;

world extension {
  import greentic:extension-base/types@0.3.0;
  import greentic:extension-host/logging@0.1.0;

  export greentic:extension-base/manifest@0.3.0;
  export greentic:extension-base/lifecycle@0.3.0;
  export greentic:extension-addon/validation@0.1.0;
  export greentic:extension-addon/workload@0.1.0;
  export greentic:extension-addon/reconciler@0.1.0;
  export greentic:extension-addon/backup@0.1.0;
}
";

/// The exact trap: `gtdx new --kind addon`'s own `wit/world.wit.tmpl` ships
/// this comment block, which mentions `addon-extension-with-backup` and
/// `backup` by name while the world it documents exports neither. A raw
/// `contains("backup")` over this file would report the opposite of the
/// truth.
const WORLD_WITH_BACKUP_MENTIONED_ONLY_IN_A_COMMENT: &str = r"
package greentic:example-cache;

// Mirrors `addon-extension` from `greentic:extension-addon@0.1.0` —
// deliberately the world WITHOUT `backup`. Selecting
// `addon-extension-with-backup` here (and exporting `backup`) would claim a
// snapshot capability this scaffold does not implement; `describe.json`'s
// `supports_backup: false` and this world's missing `backup` export are the
// same claim made twice, in two places the platform can each check
// independently.
world extension {
  import greentic:extension-base/types@0.3.0;

  export greentic:extension-base/manifest@0.3.0;
  export greentic:extension-base/lifecycle@0.3.0;
  export greentic:extension-addon/validation@0.1.0;
  export greentic:extension-addon/workload@0.1.0;
  export greentic:extension-addon/reconciler@0.1.0;
}
";

fn addon_claiming_backup() -> serde_json::Value {
    let mut a = base_addon();
    a["supports_backup"] = json!(true);
    a
}

/// `gtdx lint --dir` is pointed at packed and installed extensions too,
/// where the source tree - including `wit/` - is legitimately absent. Both
/// backup rules must stay completely silent rather than treating a missing
/// file as either a pass or a failure.
#[test]
fn a_missing_world_wit_keeps_both_backup_rules_silent() {
    let dir = tempfile::tempdir().expect("tempdir");
    let v = check_addons(&describe_with_addon(&addon_claiming_backup()), dir.path());
    assert!(
        v.is_empty(),
        "a missing wit/world.wit must not trigger either backup rule: {v:?}"
    );
}

/// The scaffold's clean baseline: `supports_backup: false`, no `backup`
/// export. Must lint clean.
#[test]
fn a_world_without_backup_and_no_addon_claiming_it_lints_clean() {
    let dir = dir_with_world_wit(WORLD_WITHOUT_BACKUP);
    let v = check_addons(&describe_with_addon(&base_addon()), dir.path());
    assert!(v.is_empty(), "expected no violations, got: {v:?}");
}

/// The failing case this rule exists for: an addon claims `supports_backup:
/// true` but the world does not export `backup`. The platform would offer a
/// snapshot it cannot take.
#[test]
fn an_addon_claiming_backup_without_the_export_is_an_error() {
    let dir = dir_with_world_wit(WORLD_WITHOUT_BACKUP);
    let v = check_addons(&describe_with_addon(&addon_claiming_backup()), dir.path());
    let hit = v
        .iter()
        .find(|x| x.code == "E_ADDON_BACKUP_NOT_EXPORTED")
        .unwrap_or_else(|| panic!("expected E_ADDON_BACKUP_NOT_EXPORTED, got: {v:?}"));
    assert!(
        matches!(hit.severity, Severity::Error),
        "a claimed-but-unexported backup must fail the run: {hit:?}"
    );
    assert!(
        hit.message.contains("\"qdrant\""),
        "message must name the offending addon id: {hit:?}"
    );
}

/// Once the world actually exports `backup`, the same claim goes quiet.
#[test]
fn an_addon_claiming_backup_with_the_export_is_clean() {
    let dir = dir_with_world_wit(WORLD_WITH_BACKUP);
    let v = check_addons(&describe_with_addon(&addon_claiming_backup()), dir.path());
    assert!(
        !v.iter().any(|x| x.code == "E_ADDON_BACKUP_NOT_EXPORTED"),
        "an addon claiming backup with a genuine export must not be flagged: {v:?}"
    );
}

/// The trap: a comment mentioning `backup` (and `addon-extension-with-backup`)
/// must not be mistaken for an actual export. This is the scaffold's own
/// `wit/world.wit.tmpl` comment, verbatim.
#[test]
fn a_backup_mention_inside_a_comment_does_not_count_as_an_export() {
    let dir = dir_with_world_wit(WORLD_WITH_BACKUP_MENTIONED_ONLY_IN_A_COMMENT);
    let v = check_addons(&describe_with_addon(&addon_claiming_backup()), dir.path());
    assert!(
        v.iter().any(|x| x.code == "E_ADDON_BACKUP_NOT_EXPORTED"),
        "a comment mentioning backup must not be read as an export - a naive \
         `contains(\"backup\")` would wrongly clear this addon: {v:?}"
    );
}

/// A `/* */` block comment mentioning backup must be stripped the same way
/// a `//` line comment is.
#[test]
fn a_backup_mention_inside_a_block_comment_does_not_count_as_an_export() {
    let world = r"
package greentic:example-cache;

/* export greentic:extension-addon/backup@0.1.0; -- not really, this is
   commented out on purpose */
world extension {
  export greentic:extension-addon/validation@0.1.0;
  export greentic:extension-addon/workload@0.1.0;
  export greentic:extension-addon/reconciler@0.1.0;
}
";
    let dir = dir_with_world_wit(world);
    let v = check_addons(&describe_with_addon(&addon_claiming_backup()), dir.path());
    assert!(
        v.iter().any(|x| x.code == "E_ADDON_BACKUP_NOT_EXPORTED"),
        "a block-commented-out export must not count as real: {v:?}"
    );
}

/// The reverse case: the world genuinely exports `backup` but no addon in
/// the catalogue says so. Drift, not a lie - a warning, not an error.
#[test]
fn a_world_exporting_backup_with_no_addon_declaring_it_is_a_warning() {
    let dir = dir_with_world_wit(WORLD_WITH_BACKUP);
    let v = check_addons(&describe_with_addon(&base_addon()), dir.path());
    let hit = v
        .iter()
        .find(|x| x.code == "W_ADDON_BACKUP_UNDECLARED")
        .unwrap_or_else(|| panic!("expected W_ADDON_BACKUP_UNDECLARED, got: {v:?}"));
    assert!(
        matches!(hit.severity, Severity::Warning),
        "undeclared-but-implemented backup must warn, not fail the run: {hit:?}"
    );
}

/// Once some addon in the catalogue does declare `supports_backup: true`,
/// the drift warning must not also fire.
#[test]
fn a_world_exporting_backup_with_an_addon_declaring_it_has_no_warning() {
    let dir = dir_with_world_wit(WORLD_WITH_BACKUP);
    let v = check_addons(&describe_with_addon(&addon_claiming_backup()), dir.path());
    assert!(
        !v.iter().any(|x| x.code == "W_ADDON_BACKUP_UNDECLARED"),
        "a correctly-advertised backup must not also warn: {v:?}"
    );
}
