# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.4] - 2026-04-24

### Fixed
- **Custom file path support**: Now accepts any manifest file path (e.g., `my-app.toml`, `/path/to/Cargo.toml`)
- **Clippy fix**: Moved code after test module with allow attribute
- Content-based detection: parses file content to verify manifest type
- Suffix matching: accepts files ending with known manifest names

### Changed
- Updated version to 0.1.4

## [0.1.3] - 2026-04-24

### Added
- **Ruby Gem Support**:
  - Auto-detects `Gemfile`
  - Checks gem versions via RubyGems.org API
  - Library functions: `check_ruby_gemfile()`, `fetch_ruby()`
- **PHP Composer Support**:
  - Auto-detects `composer.json` 
  - Checks package versions via Packagist.org API
  - Library functions: `check_composer_json()`, `fetch_composer()`
- **Go Modules Support**:
  - Auto-detects `go.mod` files
  - Checks package versions via Go proxy (proxy.golang.org)
- **Docker/OCI Image Support**:
  - Auto-detects `docker-compose.yml` and `docker-compose.yaml`
  - Checks image tags via Docker Hub API
- **Output Formats**:
  - `--format html` - HTML report with dark theme styling
  - `--format step-summary` - GitHub Actions markdown table
  - `--format ndjson` - Newline-delimited JSON for streaming
- **Theme Support**:
  - `--theme auto|dark|light` - Terminal output theming
- **Registry Enhancements**:
  - Timeout configurable in CheckOptions
  - Rate limit retry with exponential backoff

### Changed
- Updated description to include all supported manifest types
- Registry enum now supports: Crates, Npm, PyPI, Go, Docker, Ruby, Composer
- All output formats updated for new registries
- Improved error handling for network requests

## [0.1.2] - 2026-04-15

### Added
- **Output Formats**:
  - `--format github-checks` - GitHub Actions workflow annotations
  - `--format junit` - JUnit XML for CI systems (Jenkins, etc.)
  - `--format sarif` - SARIF for GitHub Advanced Security
- **Configuration File**:
  - `--config <path>` - Custom config file path
  - Auto-detects `.dep-age.toml` or `dep-age.toml`
  - Supports `[tool.dep-age]` section with thresholds, ignore lists, registry URLs
- **Check Mode**:
  - `--check` - Quiet mode, only exit code (no table output)
- **Diff/Trend Tracking**:
  - `--diff` - Show changes since last run
  - Auto-saves results to `.dep-age-history.json`
  - Shows newly stale, improved, and new packages

### Changed
- Improved error handling for network requests
- Added quick-xml dependency for XML generation

## [0.1.1] - 2026-04-13

### Added
- `--format csv` for CSV output (pipeable to spreadsheets)
- `--format json` as alternative to `--json`
- `--sort` flag to sort results by `age` (default), `name`, or `status`
- `--ignore <name>` flag to skip packages by name (repeatable)
- `.dep-age-ignore` file auto-loading — one package name per line, `#` comments supported
- Version gap column in output — shows `version` alongside `latest`
- PyPI support documented: `pyproject.toml` and `requirements.txt`
- `DepAgeSummary::is_all_fresh()` convenience method
- `CheckOptions.ignore_list` field for library consumers
- Homebrew tap: `brew tap Ayyankhan101/dep-age && brew install dep-age`
- CONTRIBUTING.md and SECURITY.md
- `--fail-on stale/aging/any/never` options for finer CI control

### Fixed
- crates.io publish skip check now sends `User-Agent` header (API requires it)
- `cargo publish --token` deprecation replaced with `cargo login`
- Hardcoded user-agent replaced with `env!("CARGO_PKG_VERSION")` constant
- README: PyPI support, `CheckOptions` table, `--cache`, `--fail-on` all documented

## [Unreleased]

## [0.1.0] - 2026-04-12

### Added
- Check dependency age for both `Cargo.toml` (crates.io) and `package.json` (npm)
- Color-coded status output with icons (✓ fresh, ~ aging, ! stale, ✗ ancient)
- Custom thresholds via `--fresh`, `--aging`, `--stale` flags
- `--filter` to show only packages matching a status
- `--json` output for CI pipelines
- `--no-dev` to skip dev dependencies
- `--concurrency` to control parallel registry requests
- Exit code 1 when ancient packages are found (CI-friendly)
- Cargo workspace support
- Library API with `check_cargo_toml`, `check_package_json`, `check_crate`
- File-based registry cache with TTL
- Comprehensive test suite (unit, integration, mocked HTTP)
