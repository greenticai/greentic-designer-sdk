use super::*;
use rules::is_breaking_bump;
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
