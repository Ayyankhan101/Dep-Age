//! Tests for registry caching functionality

use dep_age::{check_cargo_toml, check_package_json, CheckOptions, RegistryCache};
use serde_json::json;
use std::fs;
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_cache_stores_and_retrieves_response() {
    let mock_server = MockServer::start().await;
    let cache_dir = TempDir::new().unwrap();

    // Mock crates.io response
    Mock::given(method("GET"))
        .and(path("/api/v1/crates/serde"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "crate": {
                "name": "serde",
                "newest_version": "1.0.197",
                "updated_at": "2024-01-15T10:30:00Z"
            }
        })))
        .expect(1) // Should only be called once
        .mount(&mock_server)
        .await;

    let cache = RegistryCache::with_cache_dir(cache_dir.path().to_path_buf())
        .with_ttl(3600);

    let temp_dir = TempDir::new().unwrap();
    fs::write(
        temp_dir.path().join("Cargo.toml"),
        r#"
[package]
name = "test"
version = "0.1.0"

[dependencies]
serde = "1"
"#,
    )
    .unwrap();

    // First call - should hit the mock server
    let opts = CheckOptions {
        crates_base_url: Some(format!("{}/api/v1/crates", mock_server.uri())),
        registry_cache: Some(cache.clone()),
        ..CheckOptions::default()
    };

    let result1 = check_cargo_toml(temp_dir.path().join("Cargo.toml"), &opts).await;
    assert!(result1.is_ok());
    let summary1 = result1.unwrap();
    assert_eq!(summary1.total, 1);

    // Second call - should use cache, not hit mock server
    let opts2 = CheckOptions {
        crates_base_url: Some(format!("{}/api/v1/crates", mock_server.uri())),
        registry_cache: Some(cache.clone()),
        ..CheckOptions::default()
    };

    let result2 = check_cargo_toml(temp_dir.path().join("Cargo.toml"), &opts2).await;
    assert!(result2.is_ok());
    let summary2 = result2.unwrap();
    assert_eq!(summary2.total, 1);
    assert_eq!(summary2.results[0].name, "serde");
}

#[tokio::test]
async fn test_cache_with_disabled_cache() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/v1/crates/tokio"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "crate": {
                "name": "tokio",
                "newest_version": "1.35.1",
                "updated_at": "2024-01-18T09:00:00Z"
            }
        })))
        .expect(2) // Should be called twice since cache is disabled
        .mount(&mock_server)
        .await;

    let temp_dir = TempDir::new().unwrap();
    fs::write(
        temp_dir.path().join("Cargo.toml"),
        r#"
[package]
name = "test"
version = "0.1.0"

[dependencies]
tokio = "1"
"#,
    )
    .unwrap();

    // Call with cache disabled
    let opts = CheckOptions {
        crates_base_url: Some(format!("{}/api/v1/crates", mock_server.uri())),
        registry_cache: None,
        ..CheckOptions::default()
    };

    let result1 = check_cargo_toml(temp_dir.path().join("Cargo.toml"), &opts).await;
    assert!(result1.is_ok());

    let result2 = check_cargo_toml(temp_dir.path().join("Cargo.toml"), &opts).await;
    assert!(result2.is_ok());
}

#[tokio::test]
async fn test_cache_clear() {
    let cache_dir = TempDir::new().unwrap();
    let cache = RegistryCache::with_cache_dir(cache_dir.path().to_path_buf())
        .with_ttl(3600);

    // Add some data to cache
    cache.set("http://example.com/test", b"test data".to_vec());

    // Verify it's there
    let data = cache.get("http://example.com/test");
    assert!(data.is_some());
    assert_eq!(data.unwrap(), b"test data");

    // Clear cache
    cache.clear().unwrap();

    // Verify it's gone
    let data = cache.get("http://example.com/test");
    assert!(data.is_none());
}

#[tokio::test]
async fn test_cache_stats() {
    let cache_dir = TempDir::new().unwrap();
    let cache = RegistryCache::with_cache_dir(cache_dir.path().to_path_buf())
        .with_ttl(3600);

    // Add some entries
    cache.set("http://example.com/1", vec![1; 100]);
    cache.set("http://example.com/2", vec![2; 200]);
    cache.set("http://example.com/3", vec![3; 300]);

    let stats = cache.stats().unwrap();
    assert_eq!(stats.total_entries, 3);
    assert_eq!(stats.valid_entries, 3);
    assert_eq!(stats.expired_entries, 0);
    assert!(stats.total_size_bytes > 0);
}

#[tokio::test]
async fn test_cache_ttl_expiration() {
    let cache_dir = TempDir::new().unwrap();
    
    // Create cache with very short TTL (1 second)
    let cache = RegistryCache::with_cache_dir(cache_dir.path().to_path_buf())
        .with_ttl(1);

    cache.set("http://example.com/expiring", b"test".to_vec());

    // Should be there immediately
    assert!(cache.get("http://example.com/expiring").is_some());

    // Wait for expiration
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Should be expired now
    assert!(cache.get("http://example.com/expiring").is_none());
}

#[tokio::test]
async fn test_npm_caching() {
    let mock_server = MockServer::start().await;
    let cache_dir = TempDir::new().unwrap();

    Mock::given(method("GET"))
        .and(path("/express"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "dist-tags": {
                "latest": "4.18.2"
            },
            "time": {
                "4.18.2": "2024-01-10T12:00:00.000Z"
            }
        })))
        .expect(1) // Only once due to caching
        .mount(&mock_server)
        .await;

    let cache = RegistryCache::with_cache_dir(cache_dir.path().to_path_buf())
        .with_ttl(3600);

    let temp_dir = TempDir::new().unwrap();
    fs::write(
        temp_dir.path().join("package.json"),
        r#"
{
  "name": "test",
  "version": "1.0.0",
  "dependencies": {
    "express": "^4.18.0"
  }
}
"#,
    )
    .unwrap();

    // First call
    let opts = CheckOptions {
        npm_base_url: Some(mock_server.uri()),
        registry_cache: Some(cache.clone()),
        ..CheckOptions::default()
    };

    let result1 = check_package_json(temp_dir.path().join("package.json"), &opts).await;
    assert!(result1.is_ok());

    // Second call - should use cache
    let opts2 = CheckOptions {
        npm_base_url: Some(mock_server.uri()),
        registry_cache: Some(cache.clone()),
        ..CheckOptions::default()
    };

    let result2 = check_package_json(temp_dir.path().join("package.json"), &opts2).await;
    assert!(result2.is_ok());
}

#[tokio::test]
async fn test_multiple_packages_caching() {
    let mock_server = MockServer::start().await;
    let cache_dir = TempDir::new().unwrap();

    Mock::given(method("GET"))
        .and(path("/api/v1/crates/serde"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "crate": {
                "name": "serde",
                "newest_version": "1.0.197",
                "updated_at": "2024-01-15T10:30:00Z"
            }
        })))
        .expect(1)
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
        .expect(1)
        .mount(&mock_server)
        .await;

    let cache = RegistryCache::with_cache_dir(cache_dir.path().to_path_buf())
        .with_ttl(3600);

    let temp_dir = TempDir::new().unwrap();
    fs::write(
        temp_dir.path().join("Cargo.toml"),
        r#"
[package]
name = "test"
version = "0.1.0"

[dependencies]
serde = "1"
tokio = "1"
"#,
    )
    .unwrap();

    // First call - should hit both mocks
    let opts = CheckOptions {
        crates_base_url: Some(format!("{}/api/v1/crates", mock_server.uri())),
        registry_cache: Some(cache.clone()),
        ..CheckOptions::default()
    };

    let result1 = check_cargo_toml(temp_dir.path().join("Cargo.toml"), &opts).await;
    assert!(result1.is_ok());
    assert_eq!(result1.unwrap().total, 2);

    // Second call - should use cache for both
    let opts2 = CheckOptions {
        crates_base_url: Some(format!("{}/api/v1/crates", mock_server.uri())),
        registry_cache: Some(cache.clone()),
        ..CheckOptions::default()
    };

    let result2 = check_cargo_toml(temp_dir.path().join("Cargo.toml"), &opts2).await;
    assert!(result2.is_ok());
    assert_eq!(result2.unwrap().total, 2);
}

#[tokio::test]
async fn test_cache_error_responses_not_cached() {
    let mock_server = MockServer::start().await;
    let cache_dir = TempDir::new().unwrap();

    // Only cache successful responses
    Mock::given(method("GET"))
        .and(path("/api/v1/crates/nonexistent"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&mock_server)
        .await;

    let cache = RegistryCache::with_cache_dir(cache_dir.path().to_path_buf())
        .with_ttl(3600);

    let temp_dir = TempDir::new().unwrap();
    fs::write(
        temp_dir.path().join("Cargo.toml"),
        r#"
[package]
name = "test"
version = "0.1.0"

[dependencies]
nonexistent = "1"
"#,
    )
    .unwrap();

    let opts = CheckOptions {
        crates_base_url: Some(format!("{}/api/v1/crates", mock_server.uri())),
        registry_cache: Some(cache.clone()),
        ..CheckOptions::default()
    };

    let result = check_cargo_toml(temp_dir.path().join("Cargo.toml"), &opts).await;
    assert!(result.is_ok());
    let summary = result.unwrap();
    assert_eq!(summary.errors, 1);
}
