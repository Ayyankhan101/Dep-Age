# dep-age

[![Crates.io](https://img.shields.io/crates/v/dep-age.svg)](https://crates.io/crates/dep-age)
[![Crates.io Downloads](https://img.shields.io/crates/d/dep-age.svg)](https://crates.io/crates/dep-age)
[![npm](https://img.shields.io/npm/v/dep-age.svg)](https://www.npmjs.com/package/dep-age)
[![CI](https://github.com/Ayyankhan101/Dep-Age/actions/workflows/ci.yml/badge.svg)](https://github.com/Ayyankhan101/Dep-Age/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

> Check how old your dependencies are — for **Cargo.toml**, **package.json**, **pyproject.toml**, **requirements.txt**, **go.mod**, and **docker-compose.yml**.

See at a glance which packages haven't been updated in months or years. Spot stale and abandoned dependencies before they become a security or compatibility problem.

```
  ✓  tokio                              "1"            3mo       aging    crates
  ✓  serde                              "1"            1mo       fresh    crates
  !  toml                               "0.5"          1.8y      stale    crates
  ✗  time                               "0.1"          3.2y      ancient  crates
```

---

## Install

### Cargo

```bash
cargo install dep-age
```

### Homebrew

```bash
brew tap Ayyankhan101/dep-age
brew install dep-age
```

### npm

```bash
npm install -g dep-age
```

---

## CLI usage

```bash
# Auto-detect Cargo.toml or package.json in current directory
dep-age

# Check a specific file
dep-age Cargo.toml
dep-age path/to/package.json
dep-age path/to/pyproject.toml
dep-age path/to/requirements.txt
dep-age path/to/go.mod
dep-age path/to/docker-compose.yml

# Skip dev-dependencies
dep-age --no-dev

# Only show specific statuses
dep-age --filter ancient
dep-age --filter stale
dep-age --filter aging

# Output raw JSON (great for CI)
dep-age --json

# Fail only on stale+ancient (default: ancient)
dep-age --fail-on stale

# Use cached registry responses (1h TTL)
dep-age --cache

# Custom age thresholds (days)
dep-age --fresh 60 --aging 180 --stale 540

# Ignore specific packages
dep-age --ignore time --ignore old-crate

# Sort results
dep-age --sort age        # by age (default)
dep-age --sort name       # alphabetically
dep-age --sort status     # grouped by status

# Output formats
dep-age --format csv      # CSV output (pipeable to spreadsheets)
dep-age --format json     # JSON output (alternative to --json)
dep-age --format ndjson   # Newline-delimited JSON (streaming-friendly)
dep-age --format github-checks  # GitHub Actions annotations
dep-age --format junit          # JUnit XML for CI systems
dep-age --format sarif         # SARIF for GitHub Advanced Security

# Config file (create .dep-age.toml in project root)
# [tool.dep-age]
# fresh = 60
# aging = 180
# stale = 540
# no-dev = true
# fail-on = "stale"
# ignore = ["time", "old-crate"]

# Custom config file
dep-age --config path/to/dep-age.toml

# Check mode - quiet, exit code only
dep-age --check

# Diff - show changes since last run
dep-age --diff

# Ignore file: create a `.dep-age-ignore` in your project root
# One package name per line, # comments supported

# Control parallelism (default: 10)
dep-age --concurrency 20
```

### Status thresholds

| Icon | Status    | Age             |
|------|-----------|-----------------|
| ✓    | `fresh`   | < 90 days       |
| ~    | `aging`   | 90 days – 1 yr  |
| !    | `stale`   | 1 yr – 2 yrs    |
| ✗    | `ancient` | 2+ years        |

The CLI exits with code `1` if any `ancient` packages are found — useful for CI pipelines.

---

## Library usage

Add to your `Cargo.toml`:

```toml
[dependencies]
dep-age = "0.1.3"
tokio = { version = "1", features = ["full"] }
```

### Check a `Cargo.toml`

```rust
use dep_age::{check_cargo_toml, CheckOptions};

#[tokio::main]
async fn main() {
    let opts = CheckOptions::default();
    let summary = check_cargo_toml("Cargo.toml", &opts).await.unwrap();

    println!("Total packages: {}", summary.total);
    println!("Ancient:        {}", summary.ancient);
    println!("Stale:          {}", summary.stale);

    if let Some(oldest) = &summary.oldest {
        println!("Oldest: {} ({} days)", oldest.name, oldest.days_since_publish.unwrap_or(0));
    }

    for result in &summary.results {
        println!("{} → {} ({})", result.name, result.status.as_str(),
            result.days_since_publish.map(|d| format!("{}d", d)).unwrap_or_default()
        );
    }
}
```

### Check a `package.json`

```rust
use dep_age::{check_package_json, CheckOptions};

#[tokio::main]
async fn main() {
    let opts = CheckOptions {
        include_dev: false,   // skip devDependencies
        concurrency: 20,      // more parallel requests
        ..CheckOptions::default()
    };

    let summary = check_package_json("package.json", &opts).await.unwrap();
    println!("Stale npm packages: {}", summary.stale);
}
```

### Check a single package

```rust
use dep_age::{check_crate, check_npm_package, check_pypi_package, CheckOptions};

#[tokio::main]
async fn main() {
    let opts = CheckOptions::default();

    let result = check_crate("serde", "1", &opts).await;
    println!("{}: {} days old", result.name, result.days_since_publish.unwrap_or(0));

    let result = check_npm_package("lodash", "^4.17.21", &opts).await;
    println!("{}: {}", result.name, result.status.as_str());

    let result = check_pypi_package("requests", ">=2.28", &opts).await;
    println!("{}: {}", result.name, result.status.as_str());
}
```

### Check a `pyproject.toml` or `requirements.txt`

```rust
use dep_age::{check_pyproject_toml, check_requirements_txt, CheckOptions};

#[tokio::main]
async fn main() {
    let opts = CheckOptions::default();

    let summary = check_pyproject_toml("pyproject.toml", &opts).await.unwrap();
    println!("Total Python deps: {}", summary.total);

    let summary = check_requirements_txt("requirements.txt", &opts).await.unwrap();
    println!("Ancient Python packages: {}", summary.ancient);
}
```

### Check a `go.mod`

```rust
use dep_age::{check_go_mod, CheckOptions};

#[tokio::main]
async fn main() {
    let opts = CheckOptions::default();

    let summary = check_go_mod("go.mod", &opts).await.unwrap();
    println!("Total Go modules: {}", summary.total);
    println!("Stale: {}", summary.stale);
}
```

### Check a Docker Compose file

```rust
use dep_age::{check_docker_compose, CheckOptions};

#[tokio::main]
async fn main() {
    let opts = CheckOptions::default();

    let summary = check_docker_compose("docker-compose.yml", &opts).await.unwrap();
    println!("Total Docker images: {}", summary.total);
    println!("Ancient images: {}", summary.ancient);
}
```

### `CheckOptions`

| Field              | Type                  | Default | Description                            |
|--------------------|-----------------------|---------|----------------------------------------|
| `include_dev`      | `bool`                | `true`  | Include dev-dependencies               |
| `concurrency`      | `usize`               | `10`    | Parallel registry requests             |
| `threshold_fresh`  | `i64`                 | `90`    | Days below which = fresh               |
| `threshold_aging`  | `i64`                 | `365`   | Days below which = aging               |
| `threshold_stale`  | `i64`                 | `730`   | Days below which = stale               |
| `ignore_list`      | `Vec<String>`         | `[]`    | Package names to skip                  |
| `crates_base_url`  | `Option<String>`      | `None`  | Override crates.io API URL             |
| `npm_base_url`     | `Option<String>`      | `None`  | Override npm registry URL              |
| `pypi_base_url`    | `Option<String>`      | `None`  | Override PyPI API URL                 |
| `registry_cache`   | `Option<RegistryCache>` | `None` | Use cached registry responses        |
| `on_progress`      | `Arc<dyn Fn(DepResult)>` | `None` | Callback per-package results          |

### `DepResult`

```rust
pub struct DepResult {
    pub name: String,
    pub version_spec: String,
    pub latest_version: String,
    pub published_at: Option<DateTime<Utc>>,
    pub days_since_publish: Option<i64>,
    pub status: Status,     // Fresh | Aging | Stale | Ancient | Error(String)
    pub registry: Registry, // Crates | Npm | Pypi
}
```

### `DepAgeSummary`

```rust
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
```

#### Convenience methods

```rust
// Check if all packages are fresh (no aging, stale, ancient, or errors)
if summary.is_all_fresh() {
    println!("All dependencies are up to date!");
}
```

---

## CI usage

### GitHub Actions

```yaml
- name: Check dependency ages
  run: dep-age --no-dev --fail-on stale
```

### GitHub Actions with SARIF (Advanced Security)

```yaml
- name: Check dependency ages
  run: dep-age --format sarif --output dep-age.sarif

- name: Upload SARIF
  uses: github/codeql-action/upload-sarif@v3
  with:
    sarif_file: dep-age.sarif
```

### GitHub Actions with GitHub Checks

```yaml
- name: Check dependency ages
  run: dep-age --format github-checks >> $GITHUB_OUTPUT
```

### JSON output + jq

```bash
# Fail if any packages are ancient
dep-age --json | jq 'if .ancient > 0 then error("ancient packages found") else . end'

# Or just let dep-age exit 1 automatically
dep-age --no-dev && echo "all good"

# Save a report
dep-age --json > dep-age-report.json
```

### JUnit XML (Jenkins, etc.)

```bash
dep-age --format junit > test-results.xml
```

### With cache (faster re-runs)

```bash
# Cache registry responses (1h TTL) for faster re-runs
dep-age --cache --no-dev
```

### Diff tracking

```bash
# First run - establishes baseline
dep-age --no-dev

# Second run - shows changes
dep-age --no-dev --diff
```

---

## Contributing

1. Fork and clone the repo
2. Run `cargo fmt && cargo clippy -- -D warnings && cargo test`
3. Open a PR with a clear description of the change

All PRs require green CI (formatting, clippy, tests) before merge.

---

## Security

To report a security vulnerability, please email the maintainer. Do not open a public issue.

---

## License

MIT
