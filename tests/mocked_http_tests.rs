//! Mocked HTTP tests using wiremock
//! These tests simulate registry responses without hitting real networks

use dep_age::{check_cargo_toml, check_package_json, CheckOptions, Status};
use serde_json::json;
use std::fs;
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_crates_io_success_response() {
    let mock_server = MockServer::start().await;

    // Mock crates.io API response
    Mock::given(method("GET"))
        .and(path("/api/v1/crates/serde"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "crate": {
                "name": "serde",
                "newest_version": "1.0.197",
                "updated_at": "2024-01-15T10:30:00Z"
            }
        })))
        .mount(&mock_server)
        .await;

    // Create a temporary Cargo.toml
    let temp_dir = TempDir::new().unwrap();
    fs::write(
        temp_dir.path().join("Cargo.toml"),
        r#"
[package]
name = "test-project"
version = "0.1.0"

[dependencies]
serde = "1"
"#,
    )
    .unwrap();

    // Run check with mock server URL
    let opts = CheckOptions {
        crates_base_url: Some(format!("{}/api/v1/crates", mock_server.uri())),
        ..CheckOptions::default()
    };

    let result = check_cargo_toml(temp_dir.path().join("Cargo.toml"), &opts).await;
    assert!(result.is_ok());
    let summary = result.unwrap();

    assert_eq!(summary.total, 1);
    let dep = &summary.results[0];
    assert_eq!(dep.name, "serde");
    assert_eq!(dep.version_spec, "1");
    assert_eq!(dep.latest_version, "1.0.197");
    assert!(dep.published_at.is_some());
    assert!(dep.days_since_publish.is_some());
    assert!(matches!(dep.status, Status::Fresh | Status::Aging | Status::Stale | Status::Ancient));
}

#[tokio::test]
async fn test_crates_io_not_found() {
    let mock_server = MockServer::start().await;

    // Mock 404 response
    Mock::given(method("GET"))
        .and(path("/api/v1/crates/nonexistent-crate"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&mock_server)
        .await;

    let temp_dir = TempDir::new().unwrap();
    fs::write(
        temp_dir.path().join("Cargo.toml"),
        r#"
[package]
name = "test-project"
version = "0.1.0"

[dependencies]
nonexistent-crate = "1"
"#,
    )
    .unwrap();

    let opts = CheckOptions {
        crates_base_url: Some(format!("{}/api/v1/crates", mock_server.uri())),
        ..CheckOptions::default()
    };

    let result = check_cargo_toml(temp_dir.path().join("Cargo.toml"), &opts).await;
    assert!(result.is_ok());
    let summary = result.unwrap();

    assert_eq!(summary.total, 1);
    let dep = &summary.results[0];
    assert_eq!(dep.name, "nonexistent-crate");
    assert!(matches!(dep.status, Status::Error(_)));
}

#[tokio::test]
async fn test_crates_io_server_error() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/v1/crates/broken-crate"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock_server)
        .await;

    let temp_dir = TempDir::new().unwrap();
    fs::write(
        temp_dir.path().join("Cargo.toml"),
        r#"
[package]
name = "test-project"
version = "0.1.0"

[dependencies]
broken-crate = "1"
"#,
    )
    .unwrap();

    let opts = CheckOptions {
        crates_base_url: Some(format!("{}/api/v1/crates", mock_server.uri())),
        ..CheckOptions::default()
    };

    let result = check_cargo_toml(temp_dir.path().join("Cargo.toml"), &opts).await;
    assert!(result.is_ok());
    let summary = result.unwrap();
    assert_eq!(summary.errors, 1);
}

#[tokio::test]
async fn test_npm_success_response() {
    let mock_server = MockServer::start().await;

    // Mock npm registry response
    Mock::given(method("GET"))
        .and(path("/express"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "dist-tags": {
                "latest": "4.18.2"
            },
            "time": {
                "4.18.2": "2024-01-10T12:00:00.000Z",
                "4.18.0": "2023-10-05T08:00:00.000Z"
            }
        })))
        .mount(&mock_server)
        .await;

    let temp_dir = TempDir::new().unwrap();
    fs::write(
        temp_dir.path().join("package.json"),
        r#"
{
  "name": "test-project",
  "version": "1.0.0",
  "dependencies": {
    "express": "^4.18.0"
  }
}
"#,
    )
    .unwrap();

    let opts = CheckOptions {
        npm_base_url: Some(mock_server.uri()),
        ..CheckOptions::default()
    };

    let result = check_package_json(temp_dir.path().join("package.json"), &opts).await;
    assert!(result.is_ok());
    let summary = result.unwrap();

    assert_eq!(summary.total, 1);
    let dep = &summary.results[0];
    assert_eq!(dep.name, "express");
    assert_eq!(dep.version_spec, "^4.18.0");
    assert_eq!(dep.latest_version, "4.18.2");
    assert!(dep.published_at.is_some());
    assert!(dep.days_since_publish.is_some());
}

#[tokio::test]
async fn test_npm_package_not_found() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/nonexistent-package"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&mock_server)
        .await;

    let temp_dir = TempDir::new().unwrap();
    fs::write(
        temp_dir.path().join("package.json"),
        r#"
{
  "name": "test-project",
  "version": "1.0.0",
  "dependencies": {
    "nonexistent-package": "1.0.0"
  }
}
"#,
    )
    .unwrap();

    let opts = CheckOptions {
        npm_base_url: Some(mock_server.uri()),
        ..CheckOptions::default()
    };

    let result = check_package_json(temp_dir.path().join("package.json"), &opts).await;
    assert!(result.is_ok());
    let summary = result.unwrap();
    assert_eq!(summary.errors, 1);
}

#[tokio::test]
async fn test_npm_scoped_package() {
    let mock_server = MockServer::start().await;

    // Scoped packages need URL encoding
    Mock::given(method("GET"))
        .and(path("/@babel%2Fcore"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "dist-tags": {
                "latest": "7.23.9"
            },
            "time": {
                "7.23.9": "2024-01-20T14:00:00.000Z"
            }
        })))
        .mount(&mock_server)
        .await;

    let temp_dir = TempDir::new().unwrap();
    fs::write(
        temp_dir.path().join("package.json"),
        r#"
{
  "name": "test-project",
  "version": "1.0.0",
  "dependencies": {
    "@babel/core": "^7.23.0"
  }
}
"#,
    )
    .unwrap();

    let opts = CheckOptions {
        npm_base_url: Some(mock_server.uri()),
        ..CheckOptions::default()
    };

    let result = check_package_json(temp_dir.path().join("package.json"), &opts).await;
    assert!(result.is_ok());
    let summary = result.unwrap();
    assert_eq!(summary.total, 1);
    assert_eq!(summary.results[0].name, "@babel/core");
}

#[tokio::test]
async fn test_multiple_crates_parallel_requests() {
    let mock_server = MockServer::start().await;

    // Mock multiple crate responses
    Mock::given(method("GET"))
        .and(path("/api/v1/crates/serde"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "crate": {
                "name": "serde",
                "newest_version": "1.0.197",
                "updated_at": "2024-01-15T10:30:00Z"
            }
        })))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/v1/crates/tokio"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "crate": {
                "name": "tokio",
                "newest_version": "1.35.1",
                "updated_at": "2024-01-18T09:00:00Z"
            }
        })))
        .mount(&mock_server)
        .await;

    let temp_dir = TempDir::new().unwrap();
    fs::write(
        temp_dir.path().join("Cargo.toml"),
        r#"
[package]
name = "test-project"
version = "0.1.0"

[dependencies]
serde = "1"
tokio = "1"
"#,
    )
    .unwrap();

    let opts = CheckOptions {
        crates_base_url: Some(format!("{}/api/v1/crates", mock_server.uri())),
        concurrency: 2,
        ..CheckOptions::default()
    };

    let result = check_cargo_toml(temp_dir.path().join("Cargo.toml"), &opts).await;
    assert!(result.is_ok());
    let summary = result.unwrap();

    assert_eq!(summary.total, 2);
    assert_eq!(summary.errors, 0);

    let names: Vec<&str> = summary.results.iter().map(|r| r.name.as_str()).collect();
    assert!(names.contains(&"serde"));
    assert!(names.contains(&"tokio"));
}

#[tokio::test]
async fn test_mixed_success_and_errors() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/v1/crates/good-crate"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "crate": {
                "name": "good-crate",
                "newest_version": "1.0.0",
                "updated_at": "2024-01-15T10:30:00Z"
            }
        })))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/v1/crates/bad-crate"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock_server)
        .await;

    let temp_dir = TempDir::new().unwrap();
    fs::write(
        temp_dir.path().join("Cargo.toml"),
        r#"
[package]
name = "test-project"
version = "0.1.0"

[dependencies]
good-crate = "1"
bad-crate = "1"
"#,
    )
    .unwrap();

    let opts = CheckOptions {
        crates_base_url: Some(format!("{}/api/v1/crates", mock_server.uri())),
        ..CheckOptions::default()
    };

    let result = check_cargo_toml(temp_dir.path().join("Cargo.toml"), &opts).await;
    assert!(result.is_ok());
    let summary = result.unwrap();

    assert_eq!(summary.total, 2);
    assert_eq!(summary.errors, 1);
    assert!(summary.fresh + summary.aging + summary.stale + summary.ancient == 1);
}

#[tokio::test]
async fn test_crates_io_invalid_json_response() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/v1/crates/broken-response"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("this is not valid json {{{"),
        )
        .mount(&mock_server)
        .await;

    let temp_dir = TempDir::new().unwrap();
    fs::write(
        temp_dir.path().join("Cargo.toml"),
        r#"
[package]
name = "test-project"
version = "0.1.0"

[dependencies]
broken-response = "1"
"#,
    )
    .unwrap();

    let opts = CheckOptions {
        crates_base_url: Some(format!("{}/api/v1/crates", mock_server.uri())),
        ..CheckOptions::default()
    };

    let result = check_cargo_toml(temp_dir.path().join("Cargo.toml"), &opts).await;
    assert!(result.is_ok());
    let summary = result.unwrap();
    assert_eq!(summary.errors, 1);
    assert!(matches!(summary.results[0].status, Status::Error(_)));
}

#[tokio::test]
async fn test_npm_missing_dist_tags() {
    let mock_server = MockServer::start().await;

    // Response without dist-tags
    Mock::given(method("GET"))
        .and(path("/no-tags-package"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "time": {
                "1.0.0": "2024-01-10T12:00:00.000Z"
            }
        })))
        .mount(&mock_server)
        .await;

    let temp_dir = TempDir::new().unwrap();
    fs::write(
        temp_dir.path().join("package.json"),
        r#"
{
  "name": "test-project",
  "version": "1.0.0",
  "dependencies": {
    "no-tags-package": "1.0.0"
  }
}
"#,
    )
    .unwrap();

    let opts = CheckOptions {
        npm_base_url: Some(mock_server.uri()),
        ..CheckOptions::default()
    };

    let result = check_package_json(temp_dir.path().join("package.json"), &opts).await;
    assert!(result.is_ok());
    let summary = result.unwrap();
    assert_eq!(summary.total, 1);
    // Should still work by falling back to time map
    assert!(summary.results[0].latest_version == "unknown" || summary.results[0].days_since_publish.is_some());
}
