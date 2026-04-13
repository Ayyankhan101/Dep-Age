# Contributing to dep-age

Thanks for your interest in contributing! All contributions are welcome.

## Development

### Prerequisites

- [Rust](https://rust-lang.org) (latest stable)
- [cargo](https://doc.rust-lang.org/cargo/)

### Setup

```bash
git clone https://github.com/Ayyankhan101/Dep-Age.git
cd Dep-Age
```

### Before submitting a PR

All PRs must pass the following checks:

```bash
cargo fmt              # Format code
cargo clippy -- -D warnings  # Zero lint warnings
cargo test             # All tests pass
```

### Adding tests

- Unit tests go in `tests/unit_tests.rs`
- Integration tests with mocked HTTP go in `tests/mocked_http_tests.rs`
- CLI tests go in `tests/cli_tests.rs`

### Commit style

Use [conventional commits](https://www.conventionalcommits.org/):

```
feat: add --ignore flag
fix: handle empty pyproject.toml gracefully
docs: update README with PyPI examples
```

## Reporting issues

- Check existing issues before opening a new one
- Include the version (`dep-age --version`), OS, and steps to reproduce
- For bugs, include the smallest possible example that triggers the issue
