//! CLI integration tests

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn bin_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("target");
    // Pick the right binary for the test profile
    if cfg!(debug_assertions) {
        path.push("debug");
    } else {
        path.push("release");
    }
    path.push("dep-age");
    path
}

fn create_temp_cargo_toml(dir: &std::path::Path, content: &str) {
    fs::write(dir.join("Cargo.toml"), content).unwrap();
}

fn create_temp_package_json(dir: &std::path::Path, content: &str) {
    fs::write(dir.join("package.json"), content).unwrap();
}

#[test]
fn test_cli_no_manifest_found() {
    let temp_dir = tempfile::tempdir().unwrap();
    let output = Command::new(bin_path())
        .current_dir(&temp_dir)
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("No Cargo.toml, package.json, pyproject.toml, or requirements.txt found")
    );
    assert!(!output.status.success());
}

#[test]
fn test_cli_invalid_file() {
    let temp_dir = tempfile::tempdir().unwrap();
    fs::write(temp_dir.path().join("other.txt"), "content").unwrap();

    let output = Command::new(bin_path())
        .current_dir(&temp_dir)
        .arg("other.txt")
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Unrecognised file"));
    assert!(!output.status.success());
}

#[test]
fn test_cli_cargo_toml_detection() {
    let temp_dir = tempfile::tempdir().unwrap();
    create_temp_cargo_toml(
        temp_dir.path(),
        r#"
[package]
name = "test"
version = "0.1.0"

[dependencies]
"#,
    );

    let output = Command::new(bin_path())
        .current_dir(&temp_dir)
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("dep-age"));
    assert!(stdout.contains("crates.io"));
    assert!(output.status.success());
}

#[test]
fn test_cli_package_json_detection() {
    let temp_dir = tempfile::tempdir().unwrap();
    create_temp_package_json(
        temp_dir.path(),
        r#"
{
  "name": "test",
  "version": "1.0.0"
}
"#,
    );

    let output = Command::new(bin_path())
        .current_dir(&temp_dir)
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("dep-age"));
    assert!(stdout.contains("npm"));
    assert!(output.status.success());
}

#[test]
fn test_cli_json_output() {
    let temp_dir = tempfile::tempdir().unwrap();
    create_temp_cargo_toml(
        temp_dir.path(),
        r#"
[package]
name = "test"
version = "0.1.0"

[dependencies]
"#,
    );

    let output = Command::new(bin_path())
        .current_dir(&temp_dir)
        .arg("--json")
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should be valid JSON
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("Should be valid JSON");
    assert!(json.get("total").is_some());
    assert!(json.get("fresh").is_some());
    assert!(json.get("checkedAt").is_some());
}

#[test]
fn test_cli_help() {
    let output = Command::new(bin_path())
        .arg("--help")
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("dep-age"));
    assert!(stdout.contains("--json"));
    assert!(stdout.contains("--no-dev"));
    assert!(stdout.contains("--filter"));
    assert!(output.status.success());
}

#[test]
fn test_cli_version() {
    let output = Command::new(bin_path())
        .arg("--version")
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("0.1.0"));
    assert!(output.status.success());
}

#[test]
fn test_cli_fail_on_ancient_no_ancient_packages() {
    let temp_dir = tempfile::tempdir().unwrap();
    create_temp_cargo_toml(
        temp_dir.path(),
        r#"
[package]
name = "test"
version = "0.1.0"

[dependencies]
"#,
    );

    let output = Command::new(bin_path())
        .current_dir(&temp_dir)
        .arg("--fail-on")
        .arg("ancient")
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
}

#[test]
fn test_cli_fail_on_never_always_succeeds() {
    let temp_dir = tempfile::tempdir().unwrap();
    create_temp_cargo_toml(
        temp_dir.path(),
        r#"
[package]
name = "test"
version = "0.1.0"

[dependencies]
"#,
    );

    let output = Command::new(bin_path())
        .current_dir(&temp_dir)
        .arg("--fail-on")
        .arg("never")
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
}

#[test]
fn test_cli_fail_on_help_shows_all_options() {
    let output = Command::new(bin_path())
        .arg("--help")
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--fail-on"));
    assert!(stdout.contains("ancient"));
    assert!(stdout.contains("stale"));
    assert!(stdout.contains("aging"));
    assert!(stdout.contains("any"));
    assert!(stdout.contains("never"));
}

#[test]
fn test_cli_cache_flag_accepted() {
    let temp_dir = tempfile::tempdir().unwrap();
    create_temp_cargo_toml(
        temp_dir.path(),
        r#"
[package]
name = "test"
version = "0.1.0"

[dependencies]
"#,
    );

    let output = Command::new(bin_path())
        .current_dir(&temp_dir)
        .arg("--cache")
        .output()
        .expect("Failed to execute command");

    // Should succeed (cache created and used)
    assert!(output.status.success());
}

#[test]
fn test_cli_cache_clear_subcommand() {
    let output = Command::new(bin_path())
        .arg("cache")
        .arg("clear")
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("cache"));
    assert!(output.status.success());
}

#[test]
fn test_cli_cache_stats_subcommand() {
    let output = Command::new(bin_path())
        .arg("cache")
        .arg("stats")
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Cache Statistics"));
    assert!(output.status.success());
}
