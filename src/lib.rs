//! # dep-age
//!
//! Check how old your dependencies are.
//!
//! Supports `Cargo.toml` (crates.io), `package.json` (npm),
//! `pyproject.toml` and `requirements.txt` (PyPI).
//!
//! ## Example
//!
//! ```rust,no_run
//! use dep_age::{check_cargo_toml, check_package_json, CheckOptions};
//!
//! #[tokio::main]
//! async fn main() {
//!     let opts = CheckOptions::default();
//!
//!     // Check a Cargo.toml
//!     let summary = check_cargo_toml("Cargo.toml", &opts).await.unwrap();
//!     println!("Ancient packages: {}", summary.ancient);
//!
//!     // Check a package.json
//!     let summary = check_package_json("package.json", &opts).await.unwrap();
//!     println!("Stale packages: {}", summary.stale);
//! }
//! ```

use chrono::{DateTime, Utc};
use futures::stream::{self, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};

pub mod config;
pub mod diff;
pub mod output;
use std::collections::HashMap;

/// User-Agent header for all registry requests.
const USER_AGENT: &str = concat!(
    "dep-age/",
    env!("CARGO_PKG_VERSION"),
    " (https://crates.io/crates/dep-age)"
);
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;

// ── Error type ───────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum DepAgeError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML parse error: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("File not found: {0}")]
    FileNotFound(String),
}

// ── Caching infrastructure ───────────────────────────────────────────────────

/// Cache entry with TTL (time-to-live)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheEntry {
    /// The cached response data
    data: Vec<u8>,
    /// When this entry was created
    created_at: DateTime<Utc>,
    /// Time-to-live in seconds
    ttl_seconds: i64,
}

impl CacheEntry {
    fn is_expired(&self) -> bool {
        let age = (Utc::now() - self.created_at).num_seconds();
        age >= self.ttl_seconds
    }
}

/// File-based cache for registry responses
#[derive(Debug, Clone)]
pub struct RegistryCache {
    /// Cache directory path
    cache_dir: std::path::PathBuf,
    /// Default TTL in seconds (default: 1 hour)
    ttl_seconds: i64,
    /// Whether caching is enabled
    enabled: bool,
}

impl RegistryCache {
    /// Create a new cache with default settings
    pub fn new() -> Result<Self, DepAgeError> {
        let cache_dir = Self::default_cache_dir()?;
        Ok(Self {
            cache_dir,
            ttl_seconds: 3600, // 1 hour
            enabled: true,
        })
    }

    /// Create a cache with custom directory
    pub fn with_cache_dir(cache_dir: std::path::PathBuf) -> Self {
        Self {
            cache_dir,
            ttl_seconds: 3600,
            enabled: true,
        }
    }

    /// Set custom TTL
    pub fn with_ttl(mut self, ttl_seconds: i64) -> Self {
        self.ttl_seconds = ttl_seconds;
        self
    }

    /// Enable or disable caching
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Get default cache directory
    fn default_cache_dir() -> Result<std::path::PathBuf, DepAgeError> {
        let project_dir = dirs::cache_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
            .join("dep-age");
        Ok(project_dir)
    }

    /// Initialize cache directory
    fn ensure_cache_dir(&self) -> Result<(), DepAgeError> {
        if !self.cache_dir.exists() {
            std::fs::create_dir_all(&self.cache_dir)?;
        }
        Ok(())
    }

    /// Generate cache key from URL
    fn cache_key(&self, url: &str) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(url.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Get cached response if available and not expired
    pub fn get(&self, url: &str) -> Option<Vec<u8>> {
        if !self.enabled {
            return None;
        }

        let key = self.cache_key(url);
        let cache_file = self.cache_dir.join(format!("{}.cache", key));

        if !cache_file.exists() {
            return None;
        }

        let content = match std::fs::read(&cache_file) {
            Ok(c) => c,
            Err(_) => return None,
        };

        let entry: CacheEntry = match serde_json::from_slice(&content) {
            Ok(e) => e,
            Err(_) => return None,
        };

        if entry.is_expired() {
            // Remove expired entry
            let _ = std::fs::remove_file(&cache_file);
            return None;
        }

        Some(entry.data)
    }

    /// Store response in cache
    pub fn set(&self, url: &str, data: Vec<u8>) {
        if !self.enabled {
            return;
        }

        if let Err(e) = self.ensure_cache_dir() {
            eprintln!("Warning: Failed to create cache directory: {}", e);
            return;
        }

        let key = self.cache_key(url);
        let cache_file = self.cache_dir.join(format!("{}.cache", key));

        let entry = CacheEntry {
            data,
            created_at: Utc::now(),
            ttl_seconds: self.ttl_seconds,
        };

        if let Ok(json) = serde_json::to_string(&entry) {
            if let Err(e) = std::fs::write(&cache_file, json) {
                eprintln!("Warning: Failed to write cache: {}", e);
            }
        }
    }

    /// Clear all cached entries
    pub fn clear(&self) -> Result<(), DepAgeError> {
        if self.cache_dir.exists() {
            for entry in std::fs::read_dir(&self.cache_dir)?.flatten() {
                if entry.path().extension().is_some_and(|ext| ext == "cache") {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
        Ok(())
    }

    /// Get cache statistics
    pub fn stats(&self) -> Result<CacheStats, DepAgeError> {
        let mut total_entries = 0;
        let mut expired_entries = 0;
        let mut total_size = 0u64;

        if self.cache_dir.exists() {
            for entry in std::fs::read_dir(&self.cache_dir)?.flatten() {
                if entry.path().extension().is_some_and(|ext| ext == "cache") {
                    total_entries += 1;
                    total_size += entry.metadata().map(|m| m.len()).unwrap_or(0);

                    if let Ok(content) = std::fs::read(entry.path()) {
                        if let Ok(cache_entry) = serde_json::from_slice::<CacheEntry>(&content) {
                            if cache_entry.is_expired() {
                                expired_entries += 1;
                            }
                        }
                    }
                }
            }
        }

        Ok(CacheStats {
            total_entries,
            expired_entries,
            valid_entries: total_entries - expired_entries,
            total_size_bytes: total_size,
        })
    }
}

impl Default for RegistryCache {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| Self {
            cache_dir: std::path::PathBuf::from("/tmp/dep-age"),
            ttl_seconds: 3600,
            enabled: true,
        })
    }
}

/// Cache statistics
#[derive(Debug)]
pub struct CacheStats {
    pub total_entries: usize,
    pub expired_entries: usize,
    pub valid_entries: usize,
    pub total_size_bytes: u64,
}

// ── Public types ─────────────────────────────────────────────────────────────

/// The status of a single dependency based on how old it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// Published less than 90 days ago (configurable).
    Fresh,
    /// Published 90 days – 1 year ago.
    Aging,
    /// Published 1–2 years ago.
    Stale,
    /// Published more than 2 years ago.
    Ancient,
    /// Could not be fetched from the registry.
    Error(String),
}

impl Status {
    pub fn as_str(&self) -> &str {
        match self {
            Status::Fresh => "fresh",
            Status::Aging => "aging",
            Status::Stale => "stale",
            Status::Ancient => "ancient",
            Status::Error(_) => "error",
        }
    }
}

/// Result for a single dependency.
#[derive(Debug, Clone)]
pub struct DepResult {
    pub name: String,
    pub version_spec: String,
    pub latest_version: String,
    pub published_at: Option<DateTime<Utc>>,
    pub days_since_publish: Option<i64>,
    pub status: Status,
    pub registry: Registry,
}

/// Which package registry was queried.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Registry {
    Crates,
    Npm,
    PyPI,
    Go,
    Docker,
}

/// Aggregated summary of all dependencies checked.
#[derive(Debug)]
pub struct DepAgeSummary {
    pub results: Vec<DepResult>,
    pub total: usize,
    pub fresh: usize,
    pub aging: usize,
    pub stale: usize,
    pub ancient: usize,
    pub errors: usize,
    pub oldest: Option<DepResult>,
    pub checked_at: DateTime<Utc>,
}

impl DepAgeSummary {
    /// Returns true if all packages are fresh with no errors.
    pub fn is_all_fresh(&self) -> bool {
        self.aging == 0 && self.stale == 0 && self.ancient == 0 && self.errors == 0
    }
}

/// Options for customising the check.
pub struct CheckOptions {
    /// Include dev dependencies (default: true).
    pub include_dev: bool,
    /// Maximum concurrent registry requests (default: 10).
    pub concurrency: usize,
    /// Days threshold for "fresh" status (default: 90).
    pub threshold_fresh: i64,
    /// Days threshold for "aging" status (default: 365).
    pub threshold_aging: i64,
    /// Days threshold for "stale" status (default: 730).
    pub threshold_stale: i64,
    /// Package names to skip from the check.
    pub ignore_list: Vec<String>,
    /// Optional custom crates.io base URL (for testing).
    pub crates_base_url: Option<String>,
    /// Optional custom npm registry base URL (for testing).
    pub npm_base_url: Option<String>,
    /// Optional custom PyPI base URL (for testing).
    pub pypi_base_url: Option<String>,
    /// Optional registry cache (for caching HTTP responses).
    pub registry_cache: Option<RegistryCache>,
    /// Optional progress callback called after each package is fetched.
    pub on_progress: Option<Arc<dyn Fn(usize) + Send + Sync>>,
}

impl Default for CheckOptions {
    fn default() -> Self {
        Self {
            include_dev: true,
            concurrency: 10,
            threshold_fresh: 90,
            threshold_aging: 365,
            threshold_stale: 730,
            ignore_list: vec![],
            crates_base_url: None,
            npm_base_url: None,
            pypi_base_url: None,
            registry_cache: None,
            on_progress: None,
        }
    }
}

impl Clone for CheckOptions {
    fn clone(&self) -> Self {
        Self {
            include_dev: self.include_dev,
            concurrency: self.concurrency,
            threshold_fresh: self.threshold_fresh,
            threshold_aging: self.threshold_aging,
            threshold_stale: self.threshold_stale,
            ignore_list: self.ignore_list.clone(),
            crates_base_url: self.crates_base_url.clone(),
            npm_base_url: self.npm_base_url.clone(),
            pypi_base_url: self.pypi_base_url.clone(),
            registry_cache: self.registry_cache.clone(),
            on_progress: self.on_progress.clone(),
        }
    }
}

impl std::fmt::Debug for CheckOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CheckOptions")
            .field("include_dev", &self.include_dev)
            .field("concurrency", &self.concurrency)
            .field("threshold_fresh", &self.threshold_fresh)
            .field("threshold_aging", &self.threshold_aging)
            .field("threshold_stale", &self.threshold_stale)
            .field("ignore_list", &self.ignore_list)
            .field("crates_base_url", &self.crates_base_url)
            .field("npm_base_url", &self.npm_base_url)
            .field("pypi_base_url", &self.pypi_base_url)
            .field("registry_cache", &self.registry_cache)
            .field(
                "on_progress",
                &self.on_progress.as_ref().map(|_| "Some(callback)"),
            )
            .finish()
    }
}

impl CheckOptions {
    /// Create new options with modified concurrency
    pub fn with_concurrency(self, concurrency: usize) -> Self {
        Self {
            concurrency,
            ..self
        }
    }
}

// ── Internal registry response types ─────────────────────────────────────────

#[derive(Deserialize, Serialize)]
struct CratesApiCrate {
    newest_version: String,
    updated_at: Option<String>,
}

#[derive(Deserialize, Serialize)]
struct CratesApiResponse {
    #[serde(rename = "crate")]
    krate: CratesApiCrate,
}

#[derive(Deserialize, Serialize)]
struct NpmDistTags {
    latest: Option<String>,
}

#[derive(Deserialize, Serialize)]
struct NpmApiResponse {
    #[serde(rename = "dist-tags")]
    dist_tags: Option<NpmDistTags>,
    time: Option<HashMap<String, String>>,
}

// ── PyPI API response types ──────────────────────────────────────────────────

#[derive(Deserialize, Serialize)]
struct PyPiInfo {
    version: String,
}

#[derive(Deserialize, Serialize)]
struct PyPiReleaseFile {
    upload_time: Option<String>,
}

#[derive(Deserialize, Serialize)]
struct PyPiApiResponse {
    info: PyPiInfo,
    releases: HashMap<String, Vec<PyPiReleaseFile>>,
}

// ── Go Module API response types ─────────────────────────────────────────────────

#[derive(Deserialize, Serialize)]
#[allow(dead_code, non_snake_case)]
struct GoVersionInfo {
    Version: String,
    Time: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[allow(dead_code, non_snake_case)]
struct GoApiResponse {
    Version: String,
    Time: String,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct GoModFile {
    #[serde(rename = "Module")]
    module: Option<GoModule>,
    #[serde(rename = "Require")]
    require: Option<Vec<GoRequire>>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct GoModule {
    #[serde(rename = "Path")]
    path: String,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct GoRequire {
    #[serde(rename = "Path")]
    path: String,
    #[serde(rename = "Version")]
    version: Option<String>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct GoSumEntry {
    #[serde(rename = "Path")]
    path: String,
    #[serde(rename = "Version")]
    version: String,
    #[serde(rename = "Info")]
    info: Option<String>,
    #[serde(rename = "Zip")]
    zip: Option<String>,
    #[serde(rename = "Sum")]
    sum: Option<String>,
}

// ── TOML manifest types ───────────────────────────────────────────────────────

#[derive(Deserialize)]
struct CargoManifest {
    dependencies: Option<HashMap<String, toml::Value>>,
    #[serde(rename = "dev-dependencies")]
    dev_dependencies: Option<HashMap<String, toml::Value>>,
    #[serde(rename = "build-dependencies")]
    build_dependencies: Option<HashMap<String, toml::Value>>,
    #[serde(rename = "workspace")]
    workspace: Option<CargoWorkspace>,
}

#[derive(Deserialize)]
struct CargoWorkspace {
    members: Option<Vec<String>>,
}

/// Progress callback type
pub type ProgressFn = Box<dyn Fn(usize, usize) + Send + Sync>;

// ── Core logic ────────────────────────────────────────────────────────────────

/// Classify a dependency based on its age.
pub fn classify(days: i64, opts: &CheckOptions) -> Status {
    if days < opts.threshold_fresh {
        Status::Fresh
    } else if days < opts.threshold_aging {
        Status::Aging
    } else if days < opts.threshold_stale {
        Status::Stale
    } else {
        Status::Ancient
    }
}

fn extract_version_str(val: &toml::Value) -> String {
    match val {
        toml::Value::String(s) => s.clone(),
        toml::Value::Table(t) => t
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("*")
            .to_string(),
        _ => "*".to_string(),
    }
}

/// Build a DepResult from crates.io API response
fn build_crate_result(
    name: &str,
    version: &str,
    data: CratesApiResponse,
    opts: &CheckOptions,
) -> DepResult {
    let published_str = data.krate.updated_at.unwrap_or_default();
    match DateTime::parse_from_rfc3339(&published_str) {
        Ok(dt) => {
            let published_at: DateTime<Utc> = dt.with_timezone(&Utc);
            let days = (Utc::now() - published_at).num_days();
            DepResult {
                name: name.to_string(),
                version_spec: version.to_string(),
                latest_version: data.krate.newest_version,
                published_at: Some(published_at),
                days_since_publish: Some(days),
                status: classify(days, opts),
                registry: Registry::Crates,
            }
        }
        Err(e) => DepResult {
            name: name.to_string(),
            version_spec: version.to_string(),
            latest_version: "unknown".to_string(),
            published_at: None,
            days_since_publish: None,
            status: Status::Error(e.to_string()),
            registry: Registry::Crates,
        },
    }
}

async fn fetch_crate(client: &Client, name: &str, version: &str, opts: &CheckOptions) -> DepResult {
    let base_url = opts
        .crates_base_url
        .as_deref()
        .unwrap_or("https://crates.io/api/v1/crates");
    let url = format!("{}/{}", base_url, name);

    // Check cache first
    if let Some(cache) = &opts.registry_cache {
        if let Some(cached_data) = cache.get(&url) {
            if let Ok(data) = serde_json::from_slice::<CratesApiResponse>(&cached_data) {
                return build_crate_result(name, version, data, opts);
            }
        }
    }

    match client.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => {
            match resp.json::<CratesApiResponse>().await {
                Ok(data) => {
                    // Cache the response
                    if let Some(cache) = &opts.registry_cache {
                        if let Ok(json) = serde_json::to_vec(&data) {
                            cache.set(&url, json);
                        }
                    }
                    build_crate_result(name, version, data, opts)
                }
                Err(e) => DepResult {
                    name: name.to_string(),
                    version_spec: version.to_string(),
                    latest_version: "unknown".to_string(),
                    published_at: None,
                    days_since_publish: None,
                    status: Status::Error(e.to_string()),
                    registry: Registry::Crates,
                },
            }
        }
        Ok(resp) => DepResult {
            name: name.to_string(),
            version_spec: version.to_string(),
            latest_version: "unknown".to_string(),
            published_at: None,
            days_since_publish: None,
            status: Status::Error(format!("Registry returned {}", resp.status())),
            registry: Registry::Crates,
        },
        Err(e) => DepResult {
            name: name.to_string(),
            version_spec: version.to_string(),
            latest_version: "unknown".to_string(),
            published_at: None,
            days_since_publish: None,
            status: Status::Error(e.to_string()),
            registry: Registry::Crates,
        },
    }
}

/// Build a DepResult from npm API response
fn build_npm_result(
    name: &str,
    version: &str,
    data: NpmApiResponse,
    opts: &CheckOptions,
) -> DepResult {
    let latest = data
        .dist_tags
        .as_ref()
        .and_then(|d| d.latest.clone())
        .unwrap_or_else(|| "unknown".to_string());

    let time_map = data.time.unwrap_or_default();
    // Try exact version first, fall back to latest
    let clean_ver = version.trim_start_matches(['^', '~', '=', '>', '<', ' ']);
    let publish_str = time_map
        .get(clean_ver)
        .or_else(|| time_map.get(&latest))
        .cloned();

    match publish_str.and_then(|s| DateTime::parse_from_rfc3339(&s).ok()) {
        Some(dt) => {
            let published_at: DateTime<Utc> = dt.with_timezone(&Utc);
            let days = (Utc::now() - published_at).num_days();
            DepResult {
                name: name.to_string(),
                version_spec: version.to_string(),
                latest_version: latest,
                published_at: Some(published_at),
                days_since_publish: Some(days),
                status: classify(days, opts),
                registry: Registry::Npm,
            }
        }
        None => DepResult {
            name: name.to_string(),
            version_spec: version.to_string(),
            latest_version: latest,
            published_at: None,
            days_since_publish: None,
            status: Status::Error("No publish time found".to_string()),
            registry: Registry::Npm,
        },
    }
}

async fn fetch_npm(client: &Client, name: &str, version: &str, opts: &CheckOptions) -> DepResult {
    let encoded = name.replace('/', "%2F");
    let base_url = opts
        .npm_base_url
        .as_deref()
        .unwrap_or("https://registry.npmjs.org");
    let url = format!("{}/{}", base_url, encoded);

    // Check cache first
    if let Some(cache) = &opts.registry_cache {
        if let Some(cached_data) = cache.get(&url) {
            if let Ok(data) = serde_json::from_slice::<NpmApiResponse>(&cached_data) {
                return build_npm_result(name, version, data, opts);
            }
        }
    }

    match client.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => {
            match resp.json::<NpmApiResponse>().await {
                Ok(data) => {
                    // Cache the response
                    if let Some(cache) = &opts.registry_cache {
                        if let Ok(json) = serde_json::to_vec(&data) {
                            cache.set(&url, json);
                        }
                    }
                    build_npm_result(name, version, data, opts)
                }
                Err(e) => DepResult {
                    name: name.to_string(),
                    version_spec: version.to_string(),
                    latest_version: "unknown".to_string(),
                    published_at: None,
                    days_since_publish: None,
                    status: Status::Error(e.to_string()),
                    registry: Registry::Npm,
                },
            }
        }
        Ok(resp) => DepResult {
            name: name.to_string(),
            version_spec: version.to_string(),
            latest_version: "unknown".to_string(),
            published_at: None,
            days_since_publish: None,
            status: Status::Error(format!("Registry returned {}", resp.status())),
            registry: Registry::Npm,
        },
        Err(e) => DepResult {
            name: name.to_string(),
            version_spec: version.to_string(),
            latest_version: "unknown".to_string(),
            published_at: None,
            days_since_publish: None,
            status: Status::Error(e.to_string()),
            registry: Registry::Npm,
        },
    }
}

// ── PyPI fetch logic ─────────────────────────────────────────────────────────

/// Build a DepResult from PyPI API response
fn build_pypi_result(
    name: &str,
    version: &str,
    data: PyPiApiResponse,
    opts: &CheckOptions,
) -> DepResult {
    let latest = data.info.version;

    // Try to find the upload time for the requested version, fall back to latest
    let clean_ver = version
        .trim_start_matches(['=', '>', '<', '~', '!'])
        .split(',')
        .next()
        .unwrap_or(version)
        .trim();

    let publish_str = data
        .releases
        .get(clean_ver)
        .or_else(|| data.releases.get(&latest))
        .and_then(|files| files.first())
        .and_then(|f| f.upload_time.clone());

    match publish_str.and_then(|s| DateTime::parse_from_rfc3339(&s).ok()) {
        Some(dt) => {
            let published_at: DateTime<Utc> = dt.with_timezone(&Utc);
            let days = (Utc::now() - published_at).num_days();
            DepResult {
                name: name.to_string(),
                version_spec: version.to_string(),
                latest_version: latest,
                published_at: Some(published_at),
                days_since_publish: Some(days),
                status: classify(days, opts),
                registry: Registry::PyPI,
            }
        }
        None => DepResult {
            name: name.to_string(),
            version_spec: version.to_string(),
            latest_version: latest,
            published_at: None,
            days_since_publish: None,
            status: Status::Error("No publish time found".to_string()),
            registry: Registry::PyPI,
        },
    }
}

async fn fetch_pypi(client: &Client, name: &str, version: &str, opts: &CheckOptions) -> DepResult {
    let base_url = opts
        .pypi_base_url
        .as_deref()
        .unwrap_or("https://pypi.org/pypi");
    let url = format!("{}/{}/json", base_url, name);

    // Check cache first
    if let Some(cache) = &opts.registry_cache {
        if let Some(cached_data) = cache.get(&url) {
            if let Ok(data) = serde_json::from_slice::<PyPiApiResponse>(&cached_data) {
                return build_pypi_result(name, version, data, opts);
            }
        }
    }

    match client.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => {
            match resp.json::<PyPiApiResponse>().await {
                Ok(data) => {
                    // Cache the response
                    if let Some(cache) = &opts.registry_cache {
                        if let Ok(json) = serde_json::to_vec(&data) {
                            cache.set(&url, json);
                        }
                    }
                    build_pypi_result(name, version, data, opts)
                }
                Err(e) => DepResult {
                    name: name.to_string(),
                    version_spec: version.to_string(),
                    latest_version: "unknown".to_string(),
                    published_at: None,
                    days_since_publish: None,
                    status: Status::Error(e.to_string()),
                    registry: Registry::PyPI,
                },
            }
        }
        Ok(resp) => DepResult {
            name: name.to_string(),
            version_spec: version.to_string(),
            latest_version: "unknown".to_string(),
            published_at: None,
            days_since_publish: None,
            status: Status::Error(format!("Registry returned {}", resp.status())),
            registry: Registry::PyPI,
        },
        Err(e) => DepResult {
            name: name.to_string(),
            version_spec: version.to_string(),
            latest_version: "unknown".to_string(),
            published_at: None,
            days_since_publish: None,
            status: Status::Error(e.to_string()),
            registry: Registry::PyPI,
        },
    }
}

// ── Go Module fetch logic ──────────────────────────────────────────────────────

fn build_go_result(
    name: &str,
    version: &str,
    data: GoApiResponse,
    opts: &CheckOptions,
) -> DepResult {
    let publish_str = &data.Time;
    match DateTime::parse_from_rfc3339(publish_str) {
        Ok(dt) => {
            let published_at: DateTime<Utc> = dt.with_timezone(&Utc);
            let days = (Utc::now() - published_at).num_days();
            DepResult {
                name: name.to_string(),
                version_spec: version.to_string(),
                latest_version: data.Version.clone(),
                published_at: Some(published_at),
                days_since_publish: Some(days),
                status: classify(days, opts),
                registry: Registry::Go,
            }
        }
        Err(e) => DepResult {
            name: name.to_string(),
            version_spec: version.to_string(),
            latest_version: data.Version,
            published_at: None,
            days_since_publish: None,
            status: Status::Error(e.to_string()),
            registry: Registry::Go,
        },
    }
}

async fn fetch_go_module(
    client: &Client,
    name: &str,
    version: &str,
    opts: &CheckOptions,
) -> DepResult {
    let base_url = "https://proxy.golang.org";
    let clean_ver = version.trim_start_matches(['v', '^']);
    let url = format!("{}/{}/@v/{}.info", base_url, name, clean_ver);

    if let Some(cache) = &opts.registry_cache {
        if let Some(cached_data) = cache.get(&url) {
            if let Ok(data) = serde_json::from_slice::<GoApiResponse>(&cached_data) {
                return build_go_result(name, version, data, opts);
            }
        }
    }

    match client.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => match resp.text().await {
            Ok(text) => {
                let data = GoApiResponse {
                    Version: clean_ver.to_string(),
                    Time: text,
                };
                if let Some(cache) = &opts.registry_cache {
                    if let Ok(json) = serde_json::to_vec(&data) {
                        cache.set(&url, json);
                    }
                }
                build_go_result(name, version, data, opts)
            }
            Err(e) => DepResult {
                name: name.to_string(),
                version_spec: version.to_string(),
                latest_version: "unknown".to_string(),
                published_at: None,
                days_since_publish: None,
                status: Status::Error(e.to_string()),
                registry: Registry::Go,
            },
        },
        Ok(resp) => DepResult {
            name: name.to_string(),
            version_spec: version.to_string(),
            latest_version: "unknown".to_string(),
            published_at: None,
            days_since_publish: None,
            status: Status::Error(format!("Registry returned {}", resp.status())),
            registry: Registry::Go,
        },
        Err(e) => DepResult {
            name: name.to_string(),
            version_spec: version.to_string(),
            latest_version: "unknown".to_string(),
            published_at: None,
            days_since_publish: None,
            status: Status::Error(e.to_string()),
            registry: Registry::Go,
        },
    }
}

// ── Docker/OCI Image fetch logic ───────────────────────────────────────────────

#[derive(Deserialize, Serialize)]
struct DockerHubResponse {
    name: String,
    tags: Vec<DockerTag>,
}

#[derive(Deserialize, Serialize)]
struct DockerTag {
    name: String,
    last_updated: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[allow(dead_code)]
struct DockerHubTokenResponse {
    token: String,
}

fn build_docker_result(
    name: &str,
    version: &str,
    last_updated: Option<String>,
    opts: &CheckOptions,
) -> DepResult {
    match last_updated.and_then(|s| DateTime::parse_from_rfc3339(&s).ok()) {
        Some(dt) => {
            let published_at: DateTime<Utc> = dt.with_timezone(&Utc);
            let days = (Utc::now() - published_at).num_days();
            DepResult {
                name: name.to_string(),
                version_spec: version.to_string(),
                latest_version: version.to_string(),
                published_at: Some(published_at),
                days_since_publish: Some(days),
                status: classify(days, opts),
                registry: Registry::Docker,
            }
        }
        None => DepResult {
            name: name.to_string(),
            version_spec: version.to_string(),
            latest_version: version.to_string(),
            published_at: None,
            days_since_publish: None,
            status: Status::Error("No publish time found".to_string()),
            registry: Registry::Docker,
        },
    }
}

async fn fetch_docker_image(
    client: &Client,
    name: &str,
    version: &str,
    opts: &CheckOptions,
) -> DepResult {
    let (image, tag) = if version.contains(':') {
        let parts: Vec<&str> = version.splitn(2, ':').collect();
        (parts[0], parts[1])
    } else {
        (name, version)
    };

    let url = format!("https://registry.hub.docker.com/v2/repositories/{}", image);

    if let Some(cache) = &opts.registry_cache {
        if let Some(cached_data) = cache.get(&url) {
            if let Ok(data) = serde_json::from_slice::<DockerHubResponse>(&cached_data) {
                let last_updated = data
                    .tags
                    .iter()
                    .find(|t| t.name == tag)
                    .and_then(|t| t.last_updated.clone());
                return build_docker_result(name, version, last_updated, opts);
            }
        }
    }

    match client.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => match resp.json::<DockerHubResponse>().await {
            Ok(data) => {
                if let Some(cache) = &opts.registry_cache {
                    if let Ok(json) = serde_json::to_vec(&data) {
                        cache.set(&url, json);
                    }
                }
                let last_updated = data
                    .tags
                    .iter()
                    .find(|t| t.name == tag)
                    .and_then(|t| t.last_updated.clone());
                build_docker_result(name, version, last_updated, opts)
            }
            Err(e) => DepResult {
                name: name.to_string(),
                version_spec: version.to_string(),
                latest_version: "unknown".to_string(),
                published_at: None,
                days_since_publish: None,
                status: Status::Error(e.to_string()),
                registry: Registry::Docker,
            },
        },
        Ok(resp) => DepResult {
            name: name.to_string(),
            version_spec: version.to_string(),
            latest_version: "unknown".to_string(),
            published_at: None,
            days_since_publish: None,
            status: Status::Error(format!("Registry returned {}", resp.status())),
            registry: Registry::Docker,
        },
        Err(e) => DepResult {
            name: name.to_string(),
            version_spec: version.to_string(),
            latest_version: "unknown".to_string(),
            published_at: None,
            days_since_publish: None,
            status: Status::Error(e.to_string()),
            registry: Registry::Docker,
        },
    }
}

fn build_summary(results: Vec<DepResult>) -> DepAgeSummary {
    let oldest = results
        .iter()
        .filter(|r| r.days_since_publish.is_some())
        .max_by_key(|r| r.days_since_publish.unwrap_or(0))
        .cloned();

    DepAgeSummary {
        fresh: results.iter().filter(|r| r.status == Status::Fresh).count(),
        aging: results.iter().filter(|r| r.status == Status::Aging).count(),
        stale: results.iter().filter(|r| r.status == Status::Stale).count(),
        ancient: results
            .iter()
            .filter(|r| r.status == Status::Ancient)
            .count(),
        errors: results
            .iter()
            .filter(|r| matches!(r.status, Status::Error(_)))
            .count(),
        total: results.len(),
        oldest,
        checked_at: Utc::now(),
        results,
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Check all dependencies listed in a `Cargo.toml` file.
///
/// # Example
/// ```rust,no_run
/// # use dep_age::{check_cargo_toml, CheckOptions};
/// # #[tokio::main] async fn main() {
/// let summary = check_cargo_toml("Cargo.toml", &CheckOptions::default()).await.unwrap();
/// println!("{} ancient packages", summary.ancient);
/// # }
/// ```
pub async fn check_cargo_toml(
    path: impl AsRef<Path>,
    opts: &CheckOptions,
) -> Result<DepAgeSummary, DepAgeError> {
    let path = path.as_ref();
    if !path.exists() {
        return Err(DepAgeError::FileNotFound(path.display().to_string()));
    }

    let content = std::fs::read_to_string(path)?;
    let manifest: CargoManifest = toml::from_str(&content)?;

    let mut deps: HashMap<String, String> = HashMap::new();
    if let Some(d) = manifest.dependencies {
        for (k, v) in d {
            deps.insert(k, extract_version_str(&v));
        }
    }
    if opts.include_dev {
        if let Some(d) = manifest.dev_dependencies {
            for (k, v) in d {
                deps.insert(k, extract_version_str(&v));
            }
        }
    }
    if let Some(d) = manifest.build_dependencies {
        for (k, v) in d {
            deps.insert(k, extract_version_str(&v));
        }
    }

    let client = Client::builder().user_agent(USER_AGENT).build()?;

    let entries: Vec<(String, String)> = deps
        .into_iter()
        .filter(|(name, _)| {
            !opts
                .ignore_list
                .iter()
                .any(|i| i.eq_ignore_ascii_case(name))
        })
        .collect();
    let on_progress = opts.on_progress.clone();
    let results = stream::iter(entries)
        .enumerate()
        .map(|(i, (name, ver))| {
            let client = client.clone();
            let opts = opts.clone();
            let on_progress = on_progress.clone();
            async move {
                let result = fetch_crate(&client, &name, &ver, &opts).await;
                if let Some(cb) = on_progress {
                    cb(i + 1);
                }
                result
            }
        })
        .buffer_unordered(opts.concurrency)
        .collect::<Vec<_>>()
        .await;

    Ok(build_summary(results))
}

/// Check all dependencies listed in a `package.json` file.
///
/// # Example
/// ```rust,no_run
/// # use dep_age::{check_package_json, CheckOptions};
/// # #[tokio::main] async fn main() {
/// let summary = check_package_json("package.json", &CheckOptions::default()).await.unwrap();
/// println!("{} stale packages", summary.stale);
/// # }
/// ```
pub async fn check_package_json(
    path: impl AsRef<Path>,
    opts: &CheckOptions,
) -> Result<DepAgeSummary, DepAgeError> {
    let path = path.as_ref();
    if !path.exists() {
        return Err(DepAgeError::FileNotFound(path.display().to_string()));
    }

    let content = std::fs::read_to_string(path)?;
    let pkg: serde_json::Value = serde_json::from_str(&content)?;

    let mut deps: HashMap<String, String> = HashMap::new();

    if let Some(d) = pkg.get("dependencies").and_then(|v| v.as_object()) {
        for (k, v) in d {
            deps.insert(k.clone(), v.as_str().unwrap_or("*").to_string());
        }
    }
    if opts.include_dev {
        if let Some(d) = pkg.get("devDependencies").and_then(|v| v.as_object()) {
            for (k, v) in d {
                deps.insert(k.clone(), v.as_str().unwrap_or("*").to_string());
            }
        }
    }

    let client = Client::builder().user_agent(USER_AGENT).build()?;

    let entries: Vec<(String, String)> = deps
        .into_iter()
        .filter(|(name, _)| {
            !opts
                .ignore_list
                .iter()
                .any(|i| i.eq_ignore_ascii_case(name))
        })
        .collect();
    let on_progress = opts.on_progress.clone();
    let results = stream::iter(entries)
        .enumerate()
        .map(|(i, (name, ver))| {
            let client = client.clone();
            let opts = opts.clone();
            let on_progress = on_progress.clone();
            async move {
                let result = fetch_npm(&client, &name, &ver, &opts).await;
                if let Some(cb) = on_progress {
                    cb(i + 1);
                }
                result
            }
        })
        .buffer_unordered(opts.concurrency)
        .collect::<Vec<_>>()
        .await;

    Ok(build_summary(results))
}

/// Check a single crate by name.
pub async fn check_crate(name: &str, version: &str, opts: &CheckOptions) -> DepResult {
    let client = Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .expect("failed to build HTTP client");
    fetch_crate(&client, name, version, opts).await
}

/// Check a single npm package by name.
pub async fn check_npm_package(name: &str, version: &str, opts: &CheckOptions) -> DepResult {
    let client = Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .expect("failed to build HTTP client");
    fetch_npm(&client, name, version, opts).await
}

/// Check all dependencies listed in a `pyproject.toml` file.
///
/// Supports both `[project].dependencies` (PEP 621) and `[tool.poetry.dependencies]`
/// as well as `[project.optional-dependencies]` and `[tool.poetry.group.*.dependencies]`.
///
/// # Example
/// ```rust,no_run
/// # use dep_age::{check_pyproject_toml, CheckOptions};
/// # #[tokio::main] async fn main() {
/// let summary = check_pyproject_toml("pyproject.toml", &CheckOptions::default()).await.unwrap();
/// println!("{} stale packages", summary.stale);
/// # }
/// ```
pub async fn check_pyproject_toml(
    path: impl AsRef<Path>,
    opts: &CheckOptions,
) -> Result<DepAgeSummary, DepAgeError> {
    let path = path.as_ref();
    if !path.exists() {
        return Err(DepAgeError::FileNotFound(path.display().to_string()));
    }

    let content = std::fs::read_to_string(path)?;
    let manifest: toml::Value = toml::from_str(&content)?;

    let mut deps: HashMap<String, String> = HashMap::new();

    // PEP 621: [project].dependencies (array of "name>=version" strings)
    if let Some(proj_deps) = manifest
        .get("project")
        .and_then(|p| p.get("dependencies"))
        .and_then(|d| d.as_array())
    {
        for dep_str in proj_deps {
            if let Some(s) = dep_str.as_str() {
                let (name, ver) = parse_python_dep(s);
                deps.insert(name, ver);
            }
        }
    }

    // PEP 621: [project].optional-dependencies (dict of arrays)
    if let Some(opt_deps) = manifest
        .get("project")
        .and_then(|p| p.get("optional-dependencies"))
        .and_then(|d| d.as_table())
    {
        for (_group, group_deps) in opt_deps {
            if let Some(arr) = group_deps.as_array() {
                for dep_str in arr {
                    if let Some(s) = dep_str.as_str() {
                        let (name, ver) = parse_python_dep(s);
                        deps.insert(name, ver);
                    }
                }
            }
        }
    }

    // Poetry: [tool.poetry.dependencies]
    if let Some(poetry_deps) = manifest
        .get("tool")
        .and_then(|t| t.get("poetry"))
        .and_then(|p| p.get("dependencies"))
        .and_then(|d| d.as_table())
    {
        for (name, val) in poetry_deps {
            // Skip "python" itself
            if name.to_lowercase() == "python" {
                continue;
            }
            let ver = extract_python_version_str(val);
            deps.insert(name.clone(), ver);
        }
    }

    // Poetry groups: [tool.poetry.group.*.dependencies]
    if let Some(groups) = manifest
        .get("tool")
        .and_then(|t| t.get("poetry"))
        .and_then(|p| p.get("group"))
        .and_then(|g| g.as_table())
    {
        for (_group_name, group_table) in groups {
            if let Some(group_deps) = group_table.get("dependencies").and_then(|d| d.as_table()) {
                for (name, val) in group_deps {
                    if name.to_lowercase() == "python" {
                        continue;
                    }
                    let ver = extract_python_version_str(val);
                    deps.insert(name.clone(), ver);
                }
            }
        }
    }

    if deps.is_empty() {
        return Ok(build_summary(vec![]));
    }

    let client = Client::builder().user_agent(USER_AGENT).build()?;

    let entries: Vec<(String, String)> = deps
        .into_iter()
        .filter(|(name, _)| {
            !opts
                .ignore_list
                .iter()
                .any(|i| i.eq_ignore_ascii_case(name))
        })
        .collect();
    let on_progress = opts.on_progress.clone();
    let results = stream::iter(entries)
        .enumerate()
        .map(|(i, (name, ver))| {
            let client = client.clone();
            let opts = opts.clone();
            let on_progress = on_progress.clone();
            async move {
                let result = fetch_pypi(&client, &name, &ver, &opts).await;
                if let Some(cb) = on_progress {
                    cb(i + 1);
                }
                result
            }
        })
        .buffer_unordered(opts.concurrency)
        .collect::<Vec<_>>()
        .await;

    Ok(build_summary(results))
}

/// Check all dependencies listed in a `requirements.txt` file.
///
/// Supports standard pip requirements format:
/// ```text
/// requests>=2.28.0
/// flask==2.3.0
/// numpy~=1.24.0
/// ```
///
/// # Example
/// ```rust,no_run
/// # use dep_age::{check_requirements_txt, CheckOptions};
/// # #[tokio::main] async fn main() {
/// let summary = check_requirements_txt("requirements.txt", &CheckOptions::default()).await.unwrap();
/// println!("{} stale packages", summary.stale);
/// # }
/// ```
pub async fn check_requirements_txt(
    path: impl AsRef<Path>,
    opts: &CheckOptions,
) -> Result<DepAgeSummary, DepAgeError> {
    let path = path.as_ref();
    if !path.exists() {
        return Err(DepAgeError::FileNotFound(path.display().to_string()));
    }

    let content = std::fs::read_to_string(path)?;
    let mut deps: HashMap<String, String> = HashMap::new();

    for line in content.lines() {
        let line = line.trim();
        // Skip empty lines, comments, options, and -r includes
        if line.is_empty()
            || line.starts_with('#')
            || line.starts_with('-')
            || line.starts_with('#')
        {
            continue;
        }
        let (name, ver) = parse_python_dep(line);
        if !name.is_empty() {
            deps.insert(name, ver);
        }
    }

    if deps.is_empty() {
        return Ok(build_summary(vec![]));
    }

    let client = Client::builder().user_agent(USER_AGENT).build()?;

    let entries: Vec<(String, String)> = deps
        .into_iter()
        .filter(|(name, _)| {
            !opts
                .ignore_list
                .iter()
                .any(|i| i.eq_ignore_ascii_case(name))
        })
        .collect();
    let on_progress = opts.on_progress.clone();
    let results = stream::iter(entries)
        .enumerate()
        .map(|(i, (name, ver))| {
            let client = client.clone();
            let opts = opts.clone();
            let on_progress = on_progress.clone();
            async move {
                let result = fetch_pypi(&client, &name, &ver, &opts).await;
                if let Some(cb) = on_progress {
                    cb(i + 1);
                }
                result
            }
        })
        .buffer_unordered(opts.concurrency)
        .collect::<Vec<_>>()
        .await;

    Ok(build_summary(results))
}

/// Parse a Python dependency string into (name, version_spec).
/// Handles: "requests>=2.28.0", "flask==2.3.0", "numpy~=1.24", "pandas"
fn parse_python_dep(s: &str) -> (String, String) {
    let s = s.trim();
    // Find the first version operator
    for (i, c) in s.char_indices() {
        if matches!(c, '>' | '<' | '=' | '~' | '!' | ';') {
            let name = s[..i].trim().to_string();
            let version = s[i..].trim().to_string();
            // For compound specs like ">=1.0,<2.0", take the first constraint
            let version = version
                .split(',')
                .next()
                .unwrap_or(&version)
                .trim()
                .to_string();
            return (name, version);
        }
    }
    // No version specified
    (s.to_string(), "*".to_string())
}

/// Extract version string from a pyproject.toml value (handles both string and table forms).
fn extract_python_version_str(val: &toml::Value) -> String {
    match val {
        toml::Value::String(s) => {
            if s == "*" {
                "*".to_string()
            } else {
                // Poetry uses "^1.0.0" style
                s.clone()
            }
        }
        toml::Value::Table(t) => t
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("*")
            .to_string(),
        _ => "*".to_string(),
    }
}

/// Test helper — exposes parse_python_dep for unit tests.
#[doc(hidden)]
pub fn parse_python_dep_test(s: &str) -> (String, String) {
    parse_python_dep(s)
}

/// Check all dependencies in a Cargo workspace.
///
/// This function detects if the Cargo.toml is a workspace root and checks
/// all workspace member packages.
///
/// # Example
/// ```rust,no_run
/// # use dep_age::{check_cargo_workspace, CheckOptions};
/// # #[tokio::main] async fn main() {
/// let summary = check_cargo_workspace("Cargo.toml", &CheckOptions::default()).await.unwrap();
/// println!("Total packages: {}", summary.total);
/// # }
/// ```
pub async fn check_cargo_workspace(
    path: impl AsRef<Path>,
    opts: &CheckOptions,
) -> Result<DepAgeSummary, DepAgeError> {
    let path = path.as_ref();
    if !path.exists() {
        return Err(DepAgeError::FileNotFound(path.display().to_string()));
    }

    let content = std::fs::read_to_string(path)?;
    let manifest: CargoManifest = toml::from_str(&content)?;

    // Check if this is a workspace
    if let Some(workspace) = &manifest.workspace {
        if let Some(members) = &workspace.members {
            // This is a workspace root - collect all member dependencies
            let workspace_root = path.parent().unwrap_or(Path::new("."));
            let mut all_deps: HashMap<String, String> = HashMap::new();

            for member_pattern in members {
                // Handle glob patterns like "crates/*"
                if member_pattern.contains('*') {
                    // Get the parent directory of the glob pattern
                    let glob_parent = workspace_root.join(member_pattern.trim_end_matches("/*"));

                    if let Ok(entries) = std::fs::read_dir(&glob_parent) {
                        for entry in entries.filter_map(|e| e.ok()) {
                            let entry_path = entry.path();
                            if entry_path.is_dir() && entry_path.join("Cargo.toml").exists() {
                                if let Ok(member_deps) =
                                    extract_deps_from_manifest(&entry_path.join("Cargo.toml"), opts)
                                {
                                    all_deps.extend(member_deps);
                                }
                            }
                        }
                    }
                } else {
                    // Direct path
                    let member_path = workspace_root.join(member_pattern);
                    let cargo_toml = if member_path.is_dir() {
                        member_path.join("Cargo.toml")
                    } else {
                        member_path
                    };

                    if cargo_toml.exists() {
                        if let Ok(member_deps) = extract_deps_from_manifest(&cargo_toml, opts) {
                            all_deps.extend(member_deps);
                        }
                    }
                }
            }

            // Fetch all dependencies from registry
            let client = Client::builder().user_agent(USER_AGENT).build()?;

            let entries: Vec<(String, String)> = all_deps
                .into_iter()
                .filter(|(name, _)| {
                    !opts
                        .ignore_list
                        .iter()
                        .any(|i| i.eq_ignore_ascii_case(name))
                })
                .collect();
            let on_progress = opts.on_progress.clone();
            let results = stream::iter(entries)
                .enumerate()
                .map(|(i, (name, ver))| {
                    let client = client.clone();
                    let opts = opts.clone();
                    let on_progress = on_progress.clone();
                    async move {
                        let result = fetch_crate(&client, &name, &ver, &opts).await;
                        if let Some(cb) = on_progress {
                            cb(i + 1);
                        }
                        result
                    }
                })
                .buffer_unordered(opts.concurrency)
                .collect::<Vec<_>>()
                .await;

            return Ok(build_summary(results));
        }
    }

    // Not a workspace - fall back to single manifest check
    check_cargo_toml(path, opts).await
}

/// Extract dependencies from a Cargo.toml manifest
fn extract_deps_from_manifest(
    path: &Path,
    opts: &CheckOptions,
) -> Result<HashMap<String, String>, DepAgeError> {
    let content = std::fs::read_to_string(path)?;
    let manifest: CargoManifest = toml::from_str(&content)?;

    let mut deps: HashMap<String, String> = HashMap::new();
    if let Some(d) = manifest.dependencies {
        for (k, v) in d {
            deps.insert(k, extract_version_str(&v));
        }
    }
    if opts.include_dev {
        if let Some(d) = manifest.dev_dependencies {
            for (k, v) in d {
                deps.insert(k, extract_version_str(&v));
            }
        }
    }
    if let Some(d) = manifest.build_dependencies {
        for (k, v) in d {
            deps.insert(k, extract_version_str(&v));
        }
    }

    Ok(deps)
}

/// Check a single Go module by name.
pub async fn check_go_module(name: &str, version: &str, opts: &CheckOptions) -> DepResult {
    let client = Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .expect("failed to build HTTP client");
    fetch_go_module(&client, name, version, opts).await
}

/// Check all dependencies listed in a `go.mod` file.
///
/// # Example
/// ```rust,no_run
/// # use dep_age::{check_go_mod, CheckOptions};
/// # #[tokio::main] async fn main() {
/// let summary = check_go_mod("go.mod", &CheckOptions::default()).await.unwrap();
/// println!("{} stale packages", summary.stale);
/// # }
/// ```
pub async fn check_go_mod(
    path: impl AsRef<Path>,
    opts: &CheckOptions,
) -> Result<DepAgeSummary, DepAgeError> {
    let path = path.as_ref();
    if !path.exists() {
        return Err(DepAgeError::FileNotFound(path.display().to_string()));
    }

    let content = std::fs::read_to_string(path)?;
    let mut deps: HashMap<String, String> = HashMap::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty()
            || line.starts_with("//")
            || line.starts_with("module ")
            || line.starts_with("go ")
        {
            continue;
        }
        if line.starts_with("require (") {
            continue;
        }
        if line == ")" {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let name = parts[0].to_string();
            let version = parts[1].to_string();
            deps.insert(name, version);
        }
    }

    if deps.is_empty() {
        return Ok(build_summary(vec![]));
    }

    let client = Client::builder().user_agent(USER_AGENT).build()?;

    let entries: Vec<(String, String)> = deps
        .into_iter()
        .filter(|(name, _)| {
            !opts
                .ignore_list
                .iter()
                .any(|i| i.eq_ignore_ascii_case(name))
        })
        .collect();
    let on_progress = opts.on_progress.clone();
    let results = stream::iter(entries)
        .enumerate()
        .map(|(i, (name, ver))| {
            let client = client.clone();
            let opts = opts.clone();
            let on_progress = on_progress.clone();
            async move {
                let result = fetch_go_module(&client, &name, &ver, &opts).await;
                if let Some(cb) = on_progress {
                    cb(i + 1);
                }
                result
            }
        })
        .buffer_unordered(opts.concurrency)
        .collect::<Vec<_>>()
        .await;

    Ok(build_summary(results))
}

/// Check a single Docker image by name.
pub async fn check_docker_image(name: &str, version: &str, opts: &CheckOptions) -> DepResult {
    let client = Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .expect("failed to build HTTP client");
    fetch_docker_image(&client, name, version, opts).await
}

/// Check Docker images listed in a Docker Compose file.
///
/// # Example
/// ```rust,no_run
/// # use dep_age::{check_docker_compose, CheckOptions};
/// # #[tokio::main] async fn main() {
/// let summary = check_docker_compose("docker-compose.yml", &CheckOptions::default()).await.unwrap();
/// println!("{} stale images", summary.stale);
/// # }
/// ```
pub async fn check_docker_compose(
    path: impl AsRef<Path>,
    opts: &CheckOptions,
) -> Result<DepAgeSummary, DepAgeError> {
    let path = path.as_ref();
    if !path.exists() {
        return Err(DepAgeError::FileNotFound(path.display().to_string()));
    }

    let content = std::fs::read_to_string(path)?;
    let mut deps: HashMap<String, String> = HashMap::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with("image:") {
            let image = line.trim_start_matches("image:").trim();
            if !image.is_empty() {
                let parts: Vec<&str> = image.splitn(2, ':').collect();
                let name = parts[0].to_string();
                let version = parts.get(1).unwrap_or(&"latest").to_string();
                deps.insert(name, version);
            }
        }
    }

    if deps.is_empty() {
        return Ok(build_summary(vec![]));
    }

    let client = Client::builder().user_agent(USER_AGENT).build()?;

    let entries: Vec<(String, String)> = deps
        .into_iter()
        .filter(|(name, _)| {
            !opts
                .ignore_list
                .iter()
                .any(|i| i.eq_ignore_ascii_case(name))
        })
        .collect();
    let on_progress = opts.on_progress.clone();
    let results = stream::iter(entries)
        .enumerate()
        .map(|(i, (name, ver))| {
            let client = client.clone();
            let opts = opts.clone();
            let on_progress = on_progress.clone();
            async move {
                let result = fetch_docker_image(&client, &name, &ver, &opts).await;
                if let Some(cb) = on_progress {
                    cb(i + 1);
                }
                result
            }
        })
        .buffer_unordered(opts.concurrency)
        .collect::<Vec<_>>()
        .await;

    Ok(build_summary(results))
}
