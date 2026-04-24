# dep-age

Check how old your dependencies are — for `Cargo.toml`, `package.json`, `pyproject.toml`, `requirements.txt`, `go.mod`, `docker-compose.yml`, `Gemfile`, and `composer.json`.

## Install

```bash
npm install -g dep-age
# or use directly
npx dep-age
```

## Usage

```bash
dep-age              # auto-detect manifest
dep-age Cargo.toml   # check Cargo.toml
dep-age go.mod       # check Go modules
dep-age docker-compose.yml  # check Docker images
dep-age --cache      # enable registry caching
dep-age --json       # JSON output for CI
dep-age --format ndjson  # NDJSON for streaming
dep-age --fail-on stale  # exit 1 on stale+ packages
```

Built with Rust. [Source on GitHub](https://github.com/Ayyankhan101/Dep-Age)
