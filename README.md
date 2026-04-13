# dep-age

> Check how old your dependencies are — for both **Cargo.toml** and **package.json**.

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

# Skip dev-dependencies
dep-age --no-dev

# Only show stale and ancient packages
dep-age --filter ancient
dep-age --filter stale

# Output raw JSON (great for CI)
dep-age --json

# Custom age thresholds (days)
dep-age --fresh 60 --aging 180 --stale 540
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
use dep_age::{check_crate, check_npm_package, CheckOptions};

#[tokio::main]
async fn main() {
    let opts = CheckOptions::default();

    let result = check_crate("serde", "1", &opts).await;
    println!("{}: {} days old", result.name, result.days_since_publish.unwrap_or(0));

    let result = check_npm_package("lodash", "^4.17.21", &opts).await;
    println!("{}: {}", result.name, result.status.as_str());
}
```

### `CheckOptions`

| Field              | Type    | Default | Description                            |
|--------------------|---------|---------|----------------------------------------|
| `include_dev`      | `bool`  | `true`  | Include dev-dependencies               |
| `concurrency`      | `usize` | `10`    | Parallel registry requests             |
| `threshold_fresh`  | `i64`   | `90`    | Days below which = fresh               |
| `threshold_aging`  | `i64`   | `365`   | Days below which = aging               |
| `threshold_stale`  | `i64`   | `730`   | Days below which = stale               |

### `DepResult`

```rust
pub struct DepResult {
    pub name: String,
    pub version_spec: String,
    pub latest_version: String,
    pub published_at: Option<DateTime<Utc>>,
    pub days_since_publish: Option<i64>,
    pub status: Status,     // Fresh | Aging | Stale | Ancient | Error(String)
    pub registry: Registry, // Crates | Npm
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

Use `--json` and pipe to `jq`:

```bash
# Fail if any packages are ancient
dep-age --json | jq 'if .ancient > 0 then error("ancient packages found") else . end'

# Or just let dep-age exit 1 automatically
dep-age --no-dev && echo "all good"
```

---

## License

MIT
