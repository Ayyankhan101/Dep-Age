# dep-age

Check how old your dependencies are — for `Cargo.toml`, `package.json`, `pyproject.toml`, and `requirements.txt`.

## Install

```bash
npm install -g dep-age
# or use directly
npx dep-age
```

## Usage

```bash
dep-age              # auto-detect manifest
dep-age --cache      # enable registry caching
dep-age --json       # JSON output for CI
dep-age --fail-on stale  # exit 1 on stale+ packages
```

Built with Rust. [Source on GitHub](https://github.com/Ayyankhan101/Dep-Age)
