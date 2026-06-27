//! Integration tests for Go.mod and Docker Compose parsing
//! These hit real registries — run with `cargo test --test go_docker_tests`

use dep_age::{check_docker_compose, check_go_mod, CheckOptions};
use std::fs;
use tempfile::TempDir;

#[tokio::test]
async fn test_go_mod_block_require_parsing() {
    let tmp = TempDir::new().unwrap();
    let gomod = tmp.path().join("go.mod");
    fs::write(
        &gomod,
        "module example.com/foo\n\nrequire (\n\tgolang.org/x/text v0.14.0\n)\n",
    )
    .unwrap();

    let summary = check_go_mod(&gomod, &CheckOptions::default())
        .await
        .unwrap();
    assert_eq!(summary.total, 1);
    assert_eq!(summary.results[0].name, "golang.org/x/text");
}

#[tokio::test]
async fn test_go_mod_single_require_parsing() {
    let tmp = TempDir::new().unwrap();
    let gomod = tmp.path().join("go.mod");
    fs::write(
        &gomod,
        "module example.com/foo\n\nrequire golang.org/x/text v0.14.0\n",
    )
    .unwrap();

    let summary = check_go_mod(&gomod, &CheckOptions::default())
        .await
        .unwrap();
    assert_eq!(summary.total, 1);
    assert_eq!(summary.results[0].name, "golang.org/x/text");
}

#[tokio::test]
async fn test_go_mod_mixed_require_styles() {
    let tmp = TempDir::new().unwrap();
    let gomod = tmp.path().join("go.mod");
    fs::write(
        &gomod,
        "module example.com/foo\n\nrequire golang.org/x/text v0.14.0\n\nrequire (\n\tgopkg.in/yaml.v3 v3.0.1\n)\n",
    )
    .unwrap();

    let summary = check_go_mod(&gomod, &CheckOptions::default())
        .await
        .unwrap();
    let names: Vec<&str> = summary.results.iter().map(|r| r.name.as_str()).collect();
    assert!(names.contains(&"golang.org/x/text"));
    assert!(names.contains(&"gopkg.in/yaml.v3"));
}

#[tokio::test]
async fn test_docker_compose_yaml_parsing() {
    let tmp = TempDir::new().unwrap();
    let compose = tmp.path().join("docker-compose.yml");
    fs::write(
        &compose,
        "services:\n  web:\n    image: nginx:1.21\n  redis:\n    image: redis:7\n",
    )
    .unwrap();

    let summary = check_docker_compose(&compose, &CheckOptions::default())
        .await
        .unwrap();
    let names: Vec<&str> = summary.results.iter().map(|r| r.name.as_str()).collect();
    assert!(names.contains(&"nginx"));
    assert!(names.contains(&"redis"));
}

#[tokio::test]
async fn test_docker_compose_skips_non_tag() {
    let tmp = TempDir::new().unwrap();
    let compose = tmp.path().join("docker-compose.yml");
    fs::write(
        &compose,
        "services:\n  web:\n    image: localhost:5000/myapp\n",
    )
    .unwrap();

    let summary = check_docker_compose(&compose, &CheckOptions::default())
        .await
        .unwrap();
    assert_eq!(summary.total, 0);
}

#[tokio::test]
async fn test_docker_compose_skips_build_only() {
    let tmp = TempDir::new().unwrap();
    let compose = tmp.path().join("docker-compose.yml");
    fs::write(&compose, "services:\n  web:\n    build: .\n").unwrap();

    let summary = check_docker_compose(&compose, &CheckOptions::default())
        .await
        .unwrap();
    assert_eq!(summary.total, 0);
}
