//! Integration test for `gtdx publish`. Gated behind `GTDX_RUN_BUILD=1`
//! because it requires cargo-component on PATH.

use std::path::PathBuf;
use std::process::Command;

fn gtdx_bin() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push("target/debug/gtdx");
    p
}

fn gate() -> bool {
    std::env::var("GTDX_RUN_BUILD").ok().as_deref() == Some("1")
}

fn run(cmd: &mut Command) -> (bool, String, String) {
    let out = cmd.output().expect("spawn");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn publish_writes_hierarchical_layout_and_receipt() {
    if !gate() {
        eprintln!("skipped: set GTDX_RUN_BUILD=1 to enable (requires cargo-component)");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("demo");
    let home = tmp.path().join("home");

    // scaffold
    let (ok, o, e) = run(Command::new(gtdx_bin())
        .arg("new")
        .arg("demo")
        .arg("--dir")
        .arg(&proj)
        .arg("--author")
        .arg("tester")
        .arg("-y")
        .arg("--no-git"));
    assert!(ok, "gtdx new failed: {o}\n{e}");

    // publish
    let (ok, o, e) = run(Command::new(gtdx_bin())
        .env("GREENTIC_HOME", &home)
        .arg("publish")
        .arg("--manifest")
        .arg(proj.join("Cargo.toml"))
        .arg("--dist")
        .arg(proj.join("dist")));
    assert!(ok, "gtdx publish failed: {o}\n{e}");

    let ver_dir = home.join("registries/local/com.example.demo/0.1.0");
    assert!(ver_dir.join("demo-0.1.0.gtxpack").exists());
    assert!(ver_dir.join("manifest.json").exists());
    assert!(ver_dir.join("artifact.sha256").exists());
    assert!(home.join("registries/local/index.json").exists());
    assert!(
        proj.join("dist/publish-com.example.demo-0.1.0.json")
            .exists()
    );
}

#[test]
fn publish_is_deterministic_sha_across_runs() {
    if !gate() {
        eprintln!("skipped: set GTDX_RUN_BUILD=1 to enable");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("demo");
    let home1 = tmp.path().join("home1");
    let home2 = tmp.path().join("home2");

    assert!(
        run(Command::new(gtdx_bin())
            .arg("new")
            .arg("demo")
            .arg("--dir")
            .arg(&proj)
            .arg("--author")
            .arg("tester")
            .arg("-y")
            .arg("--no-git"))
        .0
    );

    let sha_of = |home: &PathBuf| {
        assert!(
            run(Command::new(gtdx_bin())
                .env("GREENTIC_HOME", home)
                .arg("publish")
                .arg("--manifest")
                .arg(proj.join("Cargo.toml"))
                .arg("--dist")
                .arg(proj.join("dist"))
                .arg("--force"))
            .0
        );
        std::fs::read_to_string(
            home.join("registries/local/com.example.demo/0.1.0/artifact.sha256"),
        )
        .unwrap()
        .trim()
        .to_string()
    };

    let sha_a = sha_of(&home1);
    let sha_b = sha_of(&home2);
    assert_eq!(sha_a, sha_b, "publish must be deterministic");
}

#[test]
fn publish_conflicts_without_force() {
    if !gate() {
        eprintln!("skipped: set GTDX_RUN_BUILD=1 to enable");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("demo");
    let home = tmp.path().join("home");
    assert!(
        run(Command::new(gtdx_bin())
            .arg("new")
            .arg("demo")
            .arg("--dir")
            .arg(&proj)
            .arg("--author")
            .arg("tester")
            .arg("-y")
            .arg("--no-git"))
        .0
    );
    assert!(
        run(Command::new(gtdx_bin())
            .env("GREENTIC_HOME", &home)
            .arg("publish")
            .arg("--manifest")
            .arg(proj.join("Cargo.toml")))
        .0
    );
    let (ok, _o, e) = run(Command::new(gtdx_bin())
        .env("GREENTIC_HOME", &home)
        .arg("publish")
        .arg("--manifest")
        .arg(proj.join("Cargo.toml")));
    assert!(!ok, "second publish without --force must fail");
    assert!(
        e.contains("already exists") || e.contains("VersionExists"),
        "stderr should mention version conflict; got: {e}"
    );
}

#[test]
fn publish_to_local_then_install_round_trip() {
    if !gate() {
        eprintln!("skipped: set GTDX_RUN_BUILD=1 to enable");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("demo");
    let home = tmp.path().join("home");

    assert!(
        run(Command::new(gtdx_bin())
            .arg("new")
            .arg("demo")
            .arg("--dir")
            .arg(&proj)
            .arg("--author")
            .arg("tester")
            .arg("-y")
            .arg("--no-git"))
        .0
    );
    assert!(
        run(Command::new(gtdx_bin())
            .env("GREENTIC_HOME", &home)
            .arg("publish")
            .arg("--manifest")
            .arg(proj.join("Cargo.toml")))
        .0
    );

    // Hierarchical publish wrote .gtxpack under <home>/registries/local/<id>/<version>/
    let pack_path = home.join("registries/local/com.example.demo/0.1.0/demo-0.1.0.gtxpack");
    assert!(
        pack_path.is_file(),
        "publish must write {}",
        pack_path.display()
    );

    // Install from the pack path into a SECOND home — proves round-trip.
    let home2 = tmp.path().join("home2");
    let (ok, o, e) = run(Command::new(gtdx_bin())
        .env("GREENTIC_HOME", &home2)
        .arg("install")
        .arg(pack_path.to_string_lossy().to_string())
        .arg("--trust")
        .arg("loose")
        .arg("-y"));
    assert!(ok, "gtdx install failed: {o}\n{e}");

    let installed = home2.join("extensions/design/com.example.demo-0.1.0");
    assert!(
        installed.exists(),
        "expected install at {}",
        installed.display()
    );
    assert!(installed.join("describe.json").exists());
    assert!(installed.join("extension.wasm").exists());
}

#[test]
fn publish_help_lists_icon_flag() {
    let (ok, out, err) = run(Command::new(gtdx_bin()).arg("publish").arg("--help"));
    assert!(ok, "publish --help failed: {err}");
    assert!(
        out.contains("--icon"),
        "publish --help missing --icon:\n{out}"
    );
}

#[test]
fn publish_icon_patches_describe() {
    if !gate() {
        eprintln!("skipped: set GTDX_RUN_BUILD=1 to enable (requires cargo-component)");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("demo");
    let home = tmp.path().join("home");

    // scaffold a design extension
    let (ok, o, e) = run(Command::new(gtdx_bin())
        .arg("new")
        .arg("demo")
        .arg("--dir")
        .arg(&proj)
        .arg("--author")
        .arg("tester")
        .arg("-y")
        .arg("--no-git"));
    assert!(ok, "scaffold failed: {o}\n{e}");

    let icon = tmp.path().join("logo.svg");
    std::fs::write(&icon, b"<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>").unwrap();

    // publish --dry-run --icon: apply_icon runs before packing; the publish
    // itself may fail later (no registry), but describe.json is patched first.
    let _ = run(Command::new(gtdx_bin())
        .env("GREENTIC_HOME", &home)
        .arg("publish")
        .arg("--dry-run")
        .arg("--manifest")
        .arg(proj.join("Cargo.toml"))
        .arg("--icon")
        .arg(&icon));

    assert!(proj.join("assets/icon.svg").exists());
    let d: serde_json::Value =
        serde_json::from_slice(&std::fs::read(proj.join("describe.json")).unwrap()).unwrap();
    assert_eq!(d["metadata"]["icon"], "assets/icon.svg");
}

/// `gtdx publish --dry-run` is documented as "Build + pack + validate; skip
/// registry write." The skip was real for the registry, but the pack step
/// (`build_pack_with_key` filling `runtime.components.*.gtpack.sha256` from the
/// scaffold's all-zero placeholder) wrote the computed hash straight back into
/// the project's `describe.json` on disk *before* the dry-run bail-out — so a
/// dry run left the working tree dirty. In a real session this made a
/// controller hand a subagent a "clean tree" that was not clean.
///
/// This test needs no toolchain: `--wasm` substitutes a stub component for
/// `cargo component build`, exercising the exact packer code path
/// (`fill_self_contained_hashes`) that mutated describe.json, without the
/// `GTDX_RUN_BUILD` gate the other tests in this file require.
#[test]
fn dry_run_does_not_mutate_describe_json() {
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("demo");
    let home = tmp.path().join("home");

    let (ok, o, e) = run(Command::new(gtdx_bin())
        .arg("new")
        .arg("demo")
        .arg("--dir")
        .arg(&proj)
        .arg("--author")
        .arg("tester")
        .arg("-y")
        .arg("--no-git"));
    assert!(ok, "gtdx new failed: {o}\n{e}");

    let describe_path = proj.join("describe.json");
    let before = std::fs::read(&describe_path).unwrap();
    // Sanity: the scaffold really does ship the all-zero placeholder this test
    // depends on `--dry-run` leaving untouched.
    assert!(
        String::from_utf8_lossy(&before)
            .contains("0000000000000000000000000000000000000000000000000000000000000000"),
        "fixture assumption broken: scaffold no longer ships a placeholder sha256"
    );

    let wasm = tmp.path().join("stub.wasm");
    std::fs::write(&wasm, b"\0asm\x01\0\0\0").unwrap();

    let (ok, stdout, stderr) = run(Command::new(gtdx_bin())
        .env("GREENTIC_HOME", &home)
        .arg("publish")
        .arg("--dry-run")
        .arg("--format")
        .arg("json")
        .arg("--manifest")
        .arg(proj.join("Cargo.toml"))
        .arg("--dist")
        .arg(proj.join("dist"))
        .arg("--wasm")
        .arg(&wasm));
    assert!(ok, "gtdx publish --dry-run failed: {stdout}\n{stderr}");

    let after = std::fs::read(&describe_path).unwrap();
    assert_eq!(
        before, after,
        "publish --dry-run must not mutate describe.json on disk"
    );

    // The registry write really was skipped.
    assert!(
        !home.join("registries/local").exists(),
        "--dry-run must still skip the registry write"
    );

    // The pack + reported sha256 must still be computed for real — that's the
    // point of a dry run — even though the on-disk describe.json is untouched.
    let report: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("dry-run --format json did not print JSON: {e}\n{stdout}"));
    let sha256 = report["sha256"]
        .as_str()
        .expect("dry-run report must include sha256");
    assert_eq!(sha256.len(), 64, "sha256 must be a real 64-hex digest");
    assert_ne!(
        sha256, "0000000000000000000000000000000000000000000000000000000000000000",
        "dry-run must report the real computed sha256, not the placeholder"
    );
}
