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
