use std::process::Command;

use tempfile::TempDir;

fn gtdx_bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_gtdx"))
}

#[test]
fn update_with_nothing_installed_is_a_noop_success() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();

    let output = Command::new(gtdx_bin())
        .arg("--home")
        .arg(&home)
        .arg("update")
        .arg("--all")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.to_lowercase().contains("nothing"),
        "stdout: {stdout}"
    );
}

#[test]
fn update_requires_target_or_all_flag() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();

    let output = Command::new(gtdx_bin())
        .arg("--home")
        .arg(&home)
        .arg("update")
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "expected failure when neither target nor --all is given"
    );
}
