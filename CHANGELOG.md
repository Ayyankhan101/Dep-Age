# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `--cache` flag to enable registry response caching (speeds up repeated runs)
- `dep-age cache clear` subcommand to clear all cached entries
- `dep-age cache stats` subcommand to view cache statistics
- Progress bar now ticks in real-time as packages are fetched
- `on_progress` callback in `CheckOptions` for library consumers

### Changed
- Clippy lint fixes for cleaner code

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
