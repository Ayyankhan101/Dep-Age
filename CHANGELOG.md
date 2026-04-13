# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
