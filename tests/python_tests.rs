//! Tests for Python dependency support (pyproject.toml, requirements.txt, PyPI)

use dep_age::{check_pyproject_toml, check_requirements_txt, CheckOptions, Registry};
use serde_json::json;
use std::fs;
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ── parse_python_dep unit tests ──────────────────────────────────────────────

#[test]
fn test_parse_python_dep_no_version() {
    let (name, ver) = dep_age::parse_python_dep_test("requests");
    assert_eq!(name, "requests");
    assert_eq!(ver, "*");
}

#[test]
fn test_parse_python_dep_gte() {
    let (name, ver) = dep_age::parse_python_dep_test("requests>=2.28.0");
    assert_eq!(name, "requests");
    assert_eq!(ver, ">=2.28.0");
}

#[test]
fn test_parse_python_dep_exact() {
    let (name, ver) = dep_age::parse_python_dep_test("flask==2.3.0");
    assert_eq!(name, "flask");
    assert_eq!(ver, "==2.3.0");
}

#[test]
fn test_parse_python_dep_compatible() {
    let (name, ver) = dep_age::parse_python_dep_test("numpy~=1.24.0");
    assert_eq!(name, "numpy");
    assert_eq!(ver, "~=1.24.0");
}

#[test]
fn test_parse_python_dep_compound() {
    let (name, ver) = dep_age::parse_python_dep_test("django>=3.0,<4.0");
    assert_eq!(name, "django");
    assert_eq!(ver, ">=3.0");
}

// ── pyproject.toml tests ─────────────────────────────────────────────────────

#[tokio::test]
async fn test_pyproject_pep621_dependencies() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/pypi/requests/json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "info": { "version": "2.31.0" },
            "releases": {
                "2.28.0": [{ "upload_time": "2024-01-15T10:30:00" }]
            }
        })))
        .mount(&mock_server)
        .await;

    let temp_dir = TempDir::new().unwrap();
    fs::write(
        temp_dir.path().join("pyproject.toml"),
        r#"
[project]
name = "my-project"
version = "0.1.0"
dependencies = [
    "requests>=2.28.0",
]
"#,
    )
    .unwrap();

    let opts = CheckOptions {
        pypi_base_url: Some(mock_server.uri()),
        ..CheckOptions::default()
    };

    let summary = check_pyproject_toml(temp_dir.path().join("pyproject.toml"), &opts)
        .await
        .unwrap();

    assert_eq!(summary.total, 1);
    assert_eq!(summary.results[0].name, "requests");
    assert_eq!(summary.results[0].registry, Registry::PyPI);
}

#[tokio::test]
async fn test_pyproject_poetry_dependencies() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/pypi/fastapi/json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "info": { "version": "0.104.0" },
            "releases": {
                "0.100.0": [{ "upload_time": "2024-02-01T12:00:00" }]
            }
        })))
        .mount(&mock_server)
        .await;

    let temp_dir = TempDir::new().unwrap();
    fs::write(
        temp_dir.path().join("pyproject.toml"),
        r#"
[tool.poetry]
name = "my-poetry-project"
version = "0.1.0"

[tool.poetry.dependencies]
python = "^3.10"
fastapi = "^0.100.0"
"#,
    )
    .unwrap();

    let opts = CheckOptions {
        pypi_base_url: Some(mock_server.uri()),
        ..CheckOptions::default()
    };

    let summary = check_pyproject_toml(temp_dir.path().join("pyproject.toml"), &opts)
        .await
        .unwrap();

    assert_eq!(summary.total, 1);
    assert_eq!(summary.results[0].name, "fastapi");
    assert_eq!(summary.results[0].registry, Registry::PyPI);
}

#[tokio::test]
async fn test_pyproject_empty() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(
        temp_dir.path().join("pyproject.toml"),
        r#"
[project]
name = "empty-project"
version = "0.1.0"
"#,
    )
    .unwrap();

    let opts = CheckOptions::default();
    let summary = check_pyproject_toml(temp_dir.path().join("pyproject.toml"), &opts)
        .await
        .unwrap();

    assert_eq!(summary.total, 0);
}

// ── requirements.txt tests ───────────────────────────────────────────────────

#[tokio::test]
async fn test_requirements_txt_basic() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/pypi/flask/json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "info": { "version": "3.0.0" },
            "releases": {
                "2.3.0": [{ "upload_time": "2024-03-01T08:00:00" }]
            }
        })))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/pypi/numpy/json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "info": { "version": "1.26.0" },
            "releases": {
                "1.24.0": [{ "upload_time": "2024-01-10T06:00:00" }]
            }
        })))
        .mount(&mock_server)
        .await;

    let temp_dir = TempDir::new().unwrap();
    fs::write(
        temp_dir.path().join("requirements.txt"),
        "flask==2.3.0\nnumpy~=1.24.0\n# this is a comment\n\n-e git+https://example.com\n",
    )
    .unwrap();

    let opts = CheckOptions {
        pypi_base_url: Some(mock_server.uri()),
        ..CheckOptions::default()
    };

    let summary = check_requirements_txt(temp_dir.path().join("requirements.txt"), &opts)
        .await
        .unwrap();

    assert_eq!(summary.total, 2);
    let names: Vec<&str> = summary.results.iter().map(|r| r.name.as_str()).collect();
    assert!(names.contains(&"flask"));
    assert!(names.contains(&"numpy"));
}

#[tokio::test]
async fn test_requirements_txt_empty() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(
        temp_dir.path().join("requirements.txt"),
        "# just a comment\n\n",
    )
    .unwrap();

    let opts = CheckOptions::default();
    let summary = check_requirements_txt(temp_dir.path().join("requirements.txt"), &opts)
        .await
        .unwrap();

    assert_eq!(summary.total, 0);
}

#[tokio::test]
async fn test_requirements_txt_file_not_found() {
    let opts = CheckOptions::default();
    let result = check_requirements_txt("/nonexistent/requirements.txt", &opts).await;
    assert!(result.is_err());
}
