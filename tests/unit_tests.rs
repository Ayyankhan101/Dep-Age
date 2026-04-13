// Unit tests for dep-age library

use dep_age::{classify, CheckOptions, Registry, Status};

#[test]
fn test_classify_fresh() {
    let opts = CheckOptions::default();
    assert_eq!(classify(30, &opts), Status::Fresh);
    assert_eq!(classify(89, &opts), Status::Fresh);
    assert_eq!(classify(0, &opts), Status::Fresh);
}

#[test]
fn test_classify_aging() {
    let opts = CheckOptions::default();
    assert_eq!(classify(90, &opts), Status::Aging);
    assert_eq!(classify(180, &opts), Status::Aging);
    assert_eq!(classify(364, &opts), Status::Aging);
}

#[test]
fn test_classify_stale() {
    let opts = CheckOptions::default();
    assert_eq!(classify(365, &opts), Status::Stale);
    assert_eq!(classify(500, &opts), Status::Stale);
    assert_eq!(classify(729, &opts), Status::Stale);
}

#[test]
fn test_classify_ancient() {
    let opts = CheckOptions::default();
    assert_eq!(classify(730, &opts), Status::Ancient);
    assert_eq!(classify(1000, &opts), Status::Ancient);
}

#[test]
fn test_classify_custom_thresholds() {
    let opts = CheckOptions {
        threshold_fresh: 60,
        threshold_aging: 180,
        threshold_stale: 365,
        ..CheckOptions::default()
    };
    assert_eq!(classify(30, &opts), Status::Fresh);
    assert_eq!(classify(100, &opts), Status::Aging);
    assert_eq!(classify(200, &opts), Status::Stale);
    assert_eq!(classify(400, &opts), Status::Ancient);
}

#[test]
fn test_status_as_str() {
    assert_eq!(Status::Fresh.as_str(), "fresh");
    assert_eq!(Status::Aging.as_str(), "aging");
    assert_eq!(Status::Stale.as_str(), "stale");
    assert_eq!(Status::Ancient.as_str(), "ancient");
    assert_eq!(Status::Error("test".to_string()).as_str(), "error");
}

#[test]
fn test_status_equality() {
    assert_eq!(Status::Fresh, Status::Fresh);
    assert_eq!(Status::Ancient, Status::Ancient);
    assert_ne!(Status::Fresh, Status::Stale);
    assert_eq!(
        Status::Error("foo".to_string()),
        Status::Error("foo".to_string())
    );
}

#[test]
fn test_registry_equality() {
    assert_eq!(Registry::Crates, Registry::Crates);
    assert_eq!(Registry::Npm, Registry::Npm);
    assert_ne!(Registry::Crates, Registry::Npm);
}

#[test]
fn test_check_options_default() {
    let opts = CheckOptions::default();
    assert!(opts.include_dev);
    assert_eq!(opts.concurrency, 10);
    assert_eq!(opts.threshold_fresh, 90);
    assert_eq!(opts.threshold_aging, 365);
    assert_eq!(opts.threshold_stale, 730);
}

#[test]
fn test_check_options_clone() {
    let opts = CheckOptions::default();
    let cloned = opts.clone();
    assert_eq!(opts.include_dev, cloned.include_dev);
    assert_eq!(opts.concurrency, cloned.concurrency);
}

// ── TOML Parsing Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod toml_parsing {
    use dep_age::{check_cargo_toml, CheckOptions};
    use std::fs;
    use tempfile::TempDir;

    fn create_temp_cargo_toml(dir: &TempDir, content: &str) {
        fs::write(dir.path().join("Cargo.toml"), content).unwrap();
    }

    #[tokio::test]
    async fn test_empty_dependencies() {
        let temp_dir = TempDir::new().unwrap();
        create_temp_cargo_toml(
            &temp_dir,
            r#"
[package]
name = "test-project"
version = "0.1.0"

[dependencies]
"#,
        );

        let result =
            check_cargo_toml(temp_dir.path().join("Cargo.toml"), &CheckOptions::default()).await;
        assert!(result.is_ok());
        let summary = result.unwrap();
        assert_eq!(summary.total, 0);
        assert_eq!(summary.fresh, 0);
        assert_eq!(summary.ancient, 0);
    }

    #[tokio::test]
    async fn test_simple_string_dependencies() {
        let temp_dir = TempDir::new().unwrap();
        create_temp_cargo_toml(
            &temp_dir,
            r#"
[package]
name = "test-project"
version = "0.1.0"

[dependencies]
serde = "1"
tokio = "1"
"#,
        );

        let result =
            check_cargo_toml(temp_dir.path().join("Cargo.toml"), &CheckOptions::default()).await;
        assert!(result.is_ok());
        let summary = result.unwrap();
        assert_eq!(summary.total, 2);
        // Verify dependency names were extracted
        let names: Vec<&str> = summary.results.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"serde"));
        assert!(names.contains(&"tokio"));
    }

    #[tokio::test]
    async fn test_table_style_dependencies() {
        let temp_dir = TempDir::new().unwrap();
        create_temp_cargo_toml(
            &temp_dir,
            r#"
[package]
name = "test-project"
version = "0.1.0"

[dependencies.serde]
version = "1"
features = ["derive"]

[dependencies.tokio]
version = "1"
features = ["full"]
"#,
        );

        let result =
            check_cargo_toml(temp_dir.path().join("Cargo.toml"), &CheckOptions::default()).await;
        assert!(result.is_ok());
        let summary = result.unwrap();
        assert_eq!(summary.total, 2);
    }

    #[tokio::test]
    async fn test_dev_dependencies_included() {
        let temp_dir = TempDir::new().unwrap();
        create_temp_cargo_toml(
            &temp_dir,
            r#"
[package]
name = "test-project"
version = "0.1.0"

[dependencies]
serde = "1"

[dev-dependencies]
tokio = { version = "1", features = ["macros"] }
"#,
        );

        let opts = CheckOptions {
            include_dev: true,
            ..CheckOptions::default()
        };
        let result = check_cargo_toml(temp_dir.path().join("Cargo.toml"), &opts).await;
        assert!(result.is_ok());
        let summary = result.unwrap();
        assert_eq!(summary.total, 2);
    }

    #[tokio::test]
    async fn test_dev_dependencies_excluded() {
        let temp_dir = TempDir::new().unwrap();
        create_temp_cargo_toml(
            &temp_dir,
            r#"
[package]
name = "test-project"
version = "0.1.0"

[dependencies]
serde = "1"

[dev-dependencies]
tokio = "1"
"#,
        );

        let opts = CheckOptions {
            include_dev: false,
            ..CheckOptions::default()
        };
        let result = check_cargo_toml(temp_dir.path().join("Cargo.toml"), &opts).await;
        assert!(result.is_ok());
        let summary = result.unwrap();
        assert_eq!(summary.total, 1);
        let names: Vec<&str> = summary.results.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"serde"));
        assert!(!names.contains(&"tokio"));
    }

    #[tokio::test]
    async fn test_build_dependencies() {
        let temp_dir = TempDir::new().unwrap();
        create_temp_cargo_toml(
            &temp_dir,
            r#"
[package]
name = "test-project"
version = "0.1.0"

[dependencies]
serde = "1"

[build-dependencies]
cc = "1"
"#,
        );

        let result =
            check_cargo_toml(temp_dir.path().join("Cargo.toml"), &CheckOptions::default()).await;
        assert!(result.is_ok());
        let summary = result.unwrap();
        assert_eq!(summary.total, 2);
        let names: Vec<&str> = summary.results.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"cc"));
    }

    #[tokio::test]
    async fn test_file_not_found_error() {
        let result =
            check_cargo_toml("/nonexistent/path/Cargo.toml", &CheckOptions::default()).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("File not found"));
    }

    #[tokio::test]
    async fn test_invalid_toml_error() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(
            temp_dir.path().join("Cargo.toml"),
            "this is not valid toml {{{",
        )
        .unwrap();

        let result =
            check_cargo_toml(temp_dir.path().join("Cargo.toml"), &CheckOptions::default()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mixed_dependency_formats() {
        let temp_dir = TempDir::new().unwrap();
        create_temp_cargo_toml(
            &temp_dir,
            r#"
[package]
name = "test-project"
version = "0.1.0"

[dependencies]
serde = "1"
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.12" }

[dev-dependencies]
tempfile = "3"

[build-dependencies]
cc = "1"
"#,
        );

        let result =
            check_cargo_toml(temp_dir.path().join("Cargo.toml"), &CheckOptions::default()).await;
        assert!(result.is_ok());
        let summary = result.unwrap();
        assert_eq!(summary.total, 5);
    }

    #[tokio::test]
    async fn test_version_extraction() {
        let temp_dir = TempDir::new().unwrap();
        create_temp_cargo_toml(
            &temp_dir,
            r#"
[package]
name = "test-project"
version = "0.1.0"

[dependencies]
serde = "1"
tokio = { version = "^1.35" }
reqwest = { version = "~0.11" }
"#,
        );

        let result =
            check_cargo_toml(temp_dir.path().join("Cargo.toml"), &CheckOptions::default()).await;
        assert!(result.is_ok());
        let summary = result.unwrap();

        // Check version specs were extracted correctly
        let serde_result = summary.results.iter().find(|r| r.name == "serde");
        let tokio_result = summary.results.iter().find(|r| r.name == "tokio");
        let reqwest_result = summary.results.iter().find(|r| r.name == "reqwest");

        assert!(serde_result.is_some());
        assert_eq!(serde_result.unwrap().version_spec, "1");

        assert!(tokio_result.is_some());
        assert_eq!(tokio_result.unwrap().version_spec, "^1.35");

        assert!(reqwest_result.is_some());
        assert_eq!(reqwest_result.unwrap().version_spec, "~0.11");
    }
}

// ── JSON Parsing Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod json_parsing {
    use dep_age::{check_package_json, CheckOptions};
    use std::fs;
    use tempfile::TempDir;

    fn create_temp_package_json(dir: &TempDir, content: &str) {
        fs::write(dir.path().join("package.json"), content).unwrap();
    }

    #[tokio::test]
    async fn test_empty_dependencies() {
        let temp_dir = TempDir::new().unwrap();
        create_temp_package_json(
            &temp_dir,
            r#"
{
  "name": "test-project",
  "version": "1.0.0",
  "dependencies": {}
}
"#,
        );

        let result = check_package_json(
            temp_dir.path().join("package.json"),
            &CheckOptions::default(),
        )
        .await;
        assert!(result.is_ok());
        let summary = result.unwrap();
        assert_eq!(summary.total, 0);
    }

    #[tokio::test]
    async fn test_dependencies_extracted() {
        let temp_dir = TempDir::new().unwrap();
        create_temp_package_json(
            &temp_dir,
            r#"
{
  "name": "test-project",
  "version": "1.0.0",
  "dependencies": {
    "express": "^4.18.0",
    "lodash": "^4.17.21"
  }
}
"#,
        );

        let result = check_package_json(
            temp_dir.path().join("package.json"),
            &CheckOptions::default(),
        )
        .await;
        assert!(result.is_ok());
        let summary = result.unwrap();
        assert_eq!(summary.total, 2);
        let names: Vec<&str> = summary.results.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"express"));
        assert!(names.contains(&"lodash"));
    }

    #[tokio::test]
    async fn test_dev_dependencies_included() {
        let temp_dir = TempDir::new().unwrap();
        create_temp_package_json(
            &temp_dir,
            r#"
{
  "name": "test-project",
  "version": "1.0.0",
  "dependencies": {
    "express": "^4.18.0"
  },
  "devDependencies": {
    "jest": "^29.0.0",
    "typescript": "^5.0.0"
  }
}
"#,
        );

        let opts = CheckOptions {
            include_dev: true,
            ..CheckOptions::default()
        };
        let result = check_package_json(temp_dir.path().join("package.json"), &opts).await;
        assert!(result.is_ok());
        let summary = result.unwrap();
        assert_eq!(summary.total, 3);
    }

    #[tokio::test]
    async fn test_dev_dependencies_excluded() {
        let temp_dir = TempDir::new().unwrap();
        create_temp_package_json(
            &temp_dir,
            r#"
{
  "name": "test-project",
  "version": "1.0.0",
  "dependencies": {
    "express": "^4.18.0"
  },
  "devDependencies": {
    "jest": "^29.0.0"
  }
}
"#,
        );

        let opts = CheckOptions {
            include_dev: false,
            ..CheckOptions::default()
        };
        let result = check_package_json(temp_dir.path().join("package.json"), &opts).await;
        assert!(result.is_ok());
        let summary = result.unwrap();
        assert_eq!(summary.total, 1);
        let names: Vec<&str> = summary.results.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"express"));
        assert!(!names.contains(&"jest"));
    }

    #[tokio::test]
    async fn test_file_not_found_error() {
        let result =
            check_package_json("/nonexistent/path/package.json", &CheckOptions::default()).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("File not found"));
    }

    #[tokio::test]
    async fn test_invalid_json_error() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(
            temp_dir.path().join("package.json"),
            "{ this is not valid json",
        )
        .unwrap();

        let result = check_package_json(
            temp_dir.path().join("package.json"),
            &CheckOptions::default(),
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_version_specs_preserved() {
        let temp_dir = TempDir::new().unwrap();
        create_temp_package_json(
            &temp_dir,
            r#"
{
  "name": "test-project",
  "version": "1.0.0",
  "dependencies": {
    "react": "^18.2.0",
    "vue": "~3.3.0",
    "angular": ">=15.0.0"
  }
}
"#,
        );

        let result = check_package_json(
            temp_dir.path().join("package.json"),
            &CheckOptions::default(),
        )
        .await;
        assert!(result.is_ok());
        let summary = result.unwrap();

        let react_result = summary.results.iter().find(|r| r.name == "react").unwrap();
        let vue_result = summary.results.iter().find(|r| r.name == "vue").unwrap();
        let angular_result = summary
            .results
            .iter()
            .find(|r| r.name == "angular")
            .unwrap();

        assert_eq!(react_result.version_spec, "^18.2.0");
        assert_eq!(vue_result.version_spec, "~3.3.0");
        assert_eq!(angular_result.version_spec, ">=15.0.0");
    }

    #[tokio::test]
    async fn test_no_dependencies_key() {
        let temp_dir = TempDir::new().unwrap();
        create_temp_package_json(
            &temp_dir,
            r#"
{
  "name": "test-project",
  "version": "1.0.0"
}
"#,
        );

        let result = check_package_json(
            temp_dir.path().join("package.json"),
            &CheckOptions::default(),
        )
        .await;
        assert!(result.is_ok());
        let summary = result.unwrap();
        assert_eq!(summary.total, 0);
    }
}

// ── Workspace Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod workspace_tests {
    use dep_age::{check_cargo_workspace, CheckOptions};
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_workspace_with_direct_members() {
        let temp_dir = TempDir::new().unwrap();

        // Create workspace root
        fs::write(
            temp_dir.path().join("Cargo.toml"),
            r#"
[workspace]
members = ["crate-a", "crate-b"]
"#,
        )
        .unwrap();

        // Create crate-a
        let crate_a_dir = temp_dir.path().join("crate-a");
        fs::create_dir_all(&crate_a_dir).unwrap();
        fs::write(
            crate_a_dir.join("Cargo.toml"),
            r#"
[package]
name = "crate-a"
version = "0.1.0"

[dependencies]
serde = "1"
"#,
        )
        .unwrap();

        // Create crate-b
        let crate_b_dir = temp_dir.path().join("crate-b");
        fs::create_dir_all(&crate_b_dir).unwrap();
        fs::write(
            crate_b_dir.join("Cargo.toml"),
            r#"
[package]
name = "crate-b"
version = "0.1.0"

[dependencies]
tokio = "1"
"#,
        )
        .unwrap();

        let result =
            check_cargo_workspace(temp_dir.path().join("Cargo.toml"), &CheckOptions::default())
                .await;
        assert!(result.is_ok());
        let summary = result.unwrap();
        assert_eq!(summary.total, 2);
        let names: Vec<&str> = summary.results.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"serde"));
        assert!(names.contains(&"tokio"));
    }

    #[tokio::test]
    async fn test_workspace_with_glob_pattern() {
        let temp_dir = TempDir::new().unwrap();

        // Create workspace root with glob pattern
        fs::write(
            temp_dir.path().join("Cargo.toml"),
            r#"
[workspace]
members = ["crates/*"]
"#,
        )
        .unwrap();

        // Create crates directory
        let crates_dir = temp_dir.path().join("crates");
        fs::create_dir_all(&crates_dir).unwrap();

        // Create crates/core
        let core_dir = crates_dir.join("core");
        fs::create_dir_all(&core_dir).unwrap();
        fs::write(
            core_dir.join("Cargo.toml"),
            r#"
[package]
name = "core-crate"
version = "0.1.0"

[dependencies]
serde = "1"
"#,
        )
        .unwrap();

        // Create crates/utils
        let utils_dir = crates_dir.join("utils");
        fs::create_dir_all(&utils_dir).unwrap();
        fs::write(
            utils_dir.join("Cargo.toml"),
            r#"
[package]
name = "utils-crate"
version = "0.1.0"

[dependencies]
tokio = "1"
"#,
        )
        .unwrap();

        let result =
            check_cargo_workspace(temp_dir.path().join("Cargo.toml"), &CheckOptions::default())
                .await;
        assert!(result.is_ok());
        let summary = result.unwrap();
        assert_eq!(summary.total, 2);
    }

    #[tokio::test]
    async fn test_workspace_falls_back_to_single_manifest() {
        let temp_dir = TempDir::new().unwrap();

        // Create a regular Cargo.toml (not a workspace)
        fs::write(
            temp_dir.path().join("Cargo.toml"),
            r#"
[package]
name = "single-project"
version = "0.1.0"

[dependencies]
serde = "1"
"#,
        )
        .unwrap();

        // Should work like check_cargo_toml
        let result =
            check_cargo_workspace(temp_dir.path().join("Cargo.toml"), &CheckOptions::default())
                .await;
        assert!(result.is_ok());
        let summary = result.unwrap();
        assert_eq!(summary.total, 1);
        assert_eq!(summary.results[0].name, "serde");
    }

    #[tokio::test]
    async fn test_workspace_empty_members() {
        let temp_dir = TempDir::new().unwrap();

        // Create workspace root with no members
        fs::write(
            temp_dir.path().join("Cargo.toml"),
            r#"
[workspace]
members = []
"#,
        )
        .unwrap();

        let result =
            check_cargo_workspace(temp_dir.path().join("Cargo.toml"), &CheckOptions::default())
                .await;
        assert!(result.is_ok());
        let summary = result.unwrap();
        assert_eq!(summary.total, 0);
    }

    #[tokio::test]
    async fn test_workspace_with_dev_dependencies() {
        let temp_dir = TempDir::new().unwrap();

        // Create workspace root
        fs::write(
            temp_dir.path().join("Cargo.toml"),
            r#"
[workspace]
members = ["lib"]
"#,
        )
        .unwrap();

        // Create lib with dev dependencies
        let lib_dir = temp_dir.path().join("lib");
        fs::create_dir_all(&lib_dir).unwrap();
        fs::write(
            lib_dir.join("Cargo.toml"),
            r#"
[package]
name = "lib"
version = "0.1.0"

[dependencies]
serde = "1"

[dev-dependencies]
tokio = "1"
"#,
        )
        .unwrap();

        // Include dev dependencies
        let opts = CheckOptions {
            include_dev: true,
            ..CheckOptions::default()
        };
        let result = check_cargo_workspace(temp_dir.path().join("Cargo.toml"), &opts).await;
        assert!(result.is_ok());
        let summary = result.unwrap();
        assert_eq!(summary.total, 2);

        // Exclude dev dependencies
        let opts = CheckOptions {
            include_dev: false,
            ..CheckOptions::default()
        };
        let result = check_cargo_workspace(temp_dir.path().join("Cargo.toml"), &opts).await;
        assert!(result.is_ok());
        let summary = result.unwrap();
        assert_eq!(summary.total, 1);
        assert_eq!(summary.results[0].name, "serde");
    }
}
