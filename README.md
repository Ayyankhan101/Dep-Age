# dep-age

[![Crates.io](https://img.shields.io/crates/v/dep-age.svg)](https://crates.io/crates/dep-age)
[![Crates.io Downloads](https://img.shields.io/crates/d/dep-age.svg)](https://crates.io/crates/dep-age)
[![npm](https://img.shields.io/npm/v/dep-age.svg)](https://www.npmjs.com/package/dep-age)
[![CI](https://github.com/Ayyankhan101/Dep-Age/actions/workflows/ci.yml/badge.svg)](https://github.com/Ayyankhan101/Dep-Age/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

> Check how old your dependencies are — for **Cargo.toml**, **package.json**, **pyproject.toml**, and **requirements.txt**.

See at a glance which packages haven't been updated in months or years. Spot stale and abandoned dependencies before they become a security or compatibility problem.

```
  ✓  tokio                              "1"            3mo       aging    crates
  ✓  serde                              "1"            1mo       fresh    crates
  !  toml                               "0.5"          1.8y      stale    crates
  ✗  time                               "0.1"          3.2y      ancient  crates
```

---

## Install

```bash
cargo install dep-age
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
dep-age = "0.1"
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

---

## CI usage

### GitHub Actions

```yaml
- name: Check dependency ages
  run: dep-age --no-dev --fail-on stale
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

### With cache (faster re-runs)

```bash
# Cache registry responses (1h TTL) for faster re-runs
dep-age --cache --no-dev
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
