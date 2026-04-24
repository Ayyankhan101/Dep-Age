use clap::{Parser, Subcommand, ValueEnum};
use colored::*;
use dep_age::config::ToolConfig;
use dep_age::diff::{compute_diff, format_diff, PreviousRun};
use dep_age::output::{format_github_checks, format_junit, format_sarif};
use dep_age::{
    check_cargo_toml, check_package_json, CheckOptions, DepResult, Registry, RegistryCache, Status,
};
use indicatif::ProgressBar;
use std::path::PathBuf;

// ── CLI definition ─────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(
    name = "dep-age",
    about = "Check how old your dependencies are",
    long_about = "Checks crates.io or npm registry to show when each dependency was last published.\nSupports Cargo.toml and package.json.",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Path to Cargo.toml or package.json (default: auto-detect in current directory)
    #[arg(value_name = "FILE")]
    file: Option<PathBuf>,

    /// Skip dev-dependencies
    #[arg(long = "no-dev", default_value_t = false)]
    no_dev: bool,

    /// Only show packages matching this status
    #[arg(long, value_enum)]
    filter: Option<FilterArg>,

    /// Output format: pretty (default), json, csv
    #[arg(long, default_value = "pretty")]
    format: OutputFormat,

    /// Output raw JSON (shorthand for --format json)
    #[arg(long, conflicts_with = "format")]
    json: bool,

    /// Number of parallel registry requests
    #[arg(long, default_value_t = 10)]
    concurrency: usize,

    /// Days threshold for "fresh" (default: 90)
    #[arg(long, default_value_t = 90)]
    fresh: i64,

    /// Days threshold for "aging" (default: 365)
    #[arg(long, default_value_t = 365)]
    aging: i64,

    /// Days threshold for "stale" (default: 730)
    #[arg(long, default_value_t = 730)]
    stale: i64,

    /// Enable registry caching (speeds up repeated runs)
    #[arg(long, default_value_t = false)]
    cache: bool,

    /// Ignore packages by name (repeatable)
    #[arg(long)]
    ignore: Vec<String>,

    /// Sort results by: age (default), name, status
    #[arg(long, default_value = "age")]
    sort: SortArg,

    /// Exit code 1 when packages match this status or worse
    #[arg(long, value_enum, default_value = "ancient")]
    fail_on: FailOnArg,

    /// Quiet mode: only output exit code (no table output)
    #[arg(long, default_value_t = false)]
    check: bool,

    /// Path to config file (default: .dep-age.toml)
    #[arg(long)]
    config: Option<PathBuf>,

    /// Show diff compared to last run
    #[arg(long, default_value_t = false)]
    diff: bool,

    /// Output theme: auto (default), dark, light
    #[arg(long, default_value = "auto")]
    theme: ThemeArg,
    // TODO: Interactive TUI mode with live updates
    // #[arg(long, default_value_t = false)]
    // tui: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Manage the registry cache
    Cache {
        #[command(subcommand)]
        action: CacheAction,
    },
}

#[derive(Subcommand)]
enum CacheAction {
    /// Clear all cached registry responses
    Clear,
    /// Show cache statistics (entries, size, hit rate)
    Stats,
}

#[derive(Clone, ValueEnum)]
enum FilterArg {
    Fresh,
    Aging,
    Stale,
    Ancient,
    Error,
}

#[derive(Clone, ValueEnum)]
enum ThemeArg {
    Auto,
    Dark,
    Light,
}

#[derive(Clone, Default, ValueEnum)]
enum OutputFormat {
    /// Human-readable table (default)
    #[default]
    Pretty,
    /// JSON array with summary
    Json,
    /// CSV with header row
    Csv,
    /// GitHub Actions workflow annotations
    GithubChecks,
    /// JUnit XML for CI systems
    Junit,
    /// SARIF for GitHub Advanced Security
    Sarif,
    /// Newline-delimited JSON (streaming-friendly)
    Ndjson,
    /// HTML report
    Html,
    /// GitHub Actions step summary
    StepSummary,
}

#[derive(Clone, ValueEnum)]
enum FailOnArg {
    /// Fail only if ancient packages exist (default, current behavior)
    Ancient,
    /// Fail if stale or ancient packages exist
    Stale,
    /// Fail if any aging, stale, or ancient packages exist
    Aging,
    /// Fail on any non-fresh package (aging/stale/ancient)
    Any,
    /// Never fail (exit 0 always, unless parse errors)
    Never,
}

#[derive(Clone, PartialEq, ValueEnum)]
enum SortArg {
    /// Sort by age (oldest first, default)
    Age,
    /// Sort alphabetically by package name
    Name,
    /// Sort by status (ancient first, then stale, aging, fresh)
    Status,
}

// ── Display helpers ─────────────────────────────────────────────────────────

pub struct Theme {
    pub fresh: ColoredString,
    pub aging: ColoredString,
    pub stale: ColoredString,
    pub ancient: ColoredString,
    pub error: ColoredString,
    pub check: ColoredString,
    pub dim: ColoredString,
    pub icon_ok: ColoredString,
    pub icon_warn: ColoredString,
    pub icon_bad: ColoredString,
    pub icon_err: ColoredString,
}

impl Theme {
    pub fn dark() -> Self {
        Theme {
            fresh: "fresh".green(),
            aging: "aging".yellow(),
            stale: "stale".red(),
            ancient: "ancient".magenta(),
            error: "error".dimmed(),
            check: "check".cyan(),
            dim: "dim".dimmed(),
            icon_ok: "✓".green(),
            icon_warn: "~".yellow(),
            icon_bad: "!".red(),
            icon_err: "?".dimmed(),
        }
    }

    pub fn light() -> Self {
        Theme {
            fresh: "fresh".green().bold(),
            aging: "aging".yellow().bold(),
            stale: "stale".red().bold(),
            ancient: "ancient".red().bold(),
            error: "error".black().on_yellow(),
            check: "check".blue().bold(),
            dim: "dim".black(),
            icon_ok: "✓".green().bold(),
            icon_warn: "~".yellow().bold(),
            icon_bad: "!".red().bold(),
            icon_err: "x".yellow().bold(),
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

#[allow(dead_code)]
fn get_theme(theme_arg: &ThemeArg) -> Theme {
    match theme_arg {
        ThemeArg::Auto => Theme::dark(),
        ThemeArg::Dark => Theme::dark(),
        ThemeArg::Light => Theme::light(),
    }
}

fn status_color(r: &DepResult) -> ColoredString {
    match &r.status {
        Status::Fresh => "fresh".green(),
        Status::Aging => "aging".yellow(),
        Status::Stale => "stale".red(),
        Status::Ancient => "ancient".magenta(),
        Status::Error(_) => "error".dimmed(),
    }
}

fn status_icon(r: &DepResult) -> ColoredString {
    match &r.status {
        Status::Fresh => "✓".green(),
        Status::Aging => "~".yellow(),
        Status::Stale => "!".red(),
        Status::Ancient => "✗".magenta(),
        Status::Error(_) => "?".dimmed(),
    }
}

fn format_age(days: Option<i64>) -> String {
    match days {
        None => "unknown".to_string(),
        Some(d) if d < 30 => format!("{}d", d),
        Some(d) if d < 365 => format!("{}mo", d / 30),
        Some(d) => format!("{:.1}y", d as f64 / 365.0),
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

fn print_row(r: &DepResult) {
    let icon = status_icon(r);
    let name = format!("{:<30}", r.name);
    let ver = format!("{:<12}", r.version_spec);
    let latest = format!("{:<10}", r.latest_version);
    let age = format_age(r.days_since_publish);
    let age_col = match &r.status {
        Status::Fresh => format!("{:<10}", age).green(),
        Status::Aging => format!("{:<10}", age).yellow(),
        Status::Stale => format!("{:<10}", age).red(),
        Status::Ancient => format!("{:<10}", age).magenta(),
        Status::Error(e) => format!("{:<10}", e.chars().take(18).collect::<String>()).dimmed(),
    };
    let reg = match r.registry {
        Registry::Crates => "crates".dimmed(),
        Registry::Npm => "npm".dimmed(),
        Registry::PyPI => "pypi".dimmed(),
        Registry::Go => "go".dimmed(),
        Registry::Docker => "docker".dimmed(),
        Registry::Ruby => "ruby".dimmed(),
        Registry::Composer => "packagist".dimmed(),
    };

    println!(
        "  {}  {} {} {} {}  {}  {}",
        icon,
        name.bold(),
        ver.dimmed(),
        latest.dimmed(),
        age_col,
        status_color(r),
        reg
    );
}

fn print_legend() {
    println!(
        "  {}  {} {} {}",
        "✓ fresh <90d".green(),
        "~ aging <1y".yellow(),
        "! stale <2y".red(),
        "✗ ancient 2y+".magenta()
    );
    println!();
}

fn should_show(r: &DepResult, filter: &Option<FilterArg>) -> bool {
    match filter {
        None => true,
        Some(FilterArg::Fresh) => r.status == Status::Fresh,
        Some(FilterArg::Aging) => r.status == Status::Aging,
        Some(FilterArg::Stale) => r.status == Status::Stale,
        Some(FilterArg::Ancient) => r.status == Status::Ancient,
        Some(FilterArg::Error) => matches!(r.status, Status::Error(_)),
    }
}

fn should_fail(summary: &dep_age::DepAgeSummary, fail_on: &FailOnArg) -> bool {
    match fail_on {
        FailOnArg::Ancient => summary.ancient > 0,
        FailOnArg::Stale => summary.stale > 0 || summary.ancient > 0,
        FailOnArg::Aging => summary.aging > 0 || summary.stale > 0 || summary.ancient > 0,
        FailOnArg::Any => summary.aging > 0 || summary.stale > 0 || summary.ancient > 0,
        FailOnArg::Never => false,
    }
}

fn sort_key(s: &Status) -> u8 {
    match s {
        Status::Ancient => 0,
        Status::Stale => 1,
        Status::Aging => 2,
        Status::Fresh => 3,
        Status::Error(_) => 4,
    }
}

// ── JSON output ─────────────────────────────────────────────────────────────

fn print_json(summary: &dep_age::DepAgeSummary) {
    // Hand-roll compact JSON to avoid adding serde-json feature flags
    // (serde_json is already a dep via lib.rs, so this is fine)
    let results: Vec<serde_json::Value> = summary
        .results
        .iter()
        .map(|r| {
            serde_json::json!({
                "name": r.name,
                "version": r.version_spec,
                "latestVersion": r.latest_version,
                "publishedAt": r.published_at.map(|d| d.to_rfc3339()),
                "daysSincePublish": r.days_since_publish,
                "status": r.status.as_str(),
                "registry": match r.registry { Registry::Crates => "crates", Registry::Npm => "npm", Registry::PyPI => "pypi", Registry::Go => "go", Registry::Docker => "docker", Registry::Ruby => "ruby", Registry::Composer => "packagist" },
            })
        })
        .collect();

    let output = serde_json::json!({
        "total": summary.total,
        "fresh": summary.fresh,
        "aging": summary.aging,
        "stale": summary.stale,
        "ancient": summary.ancient,
        "errors": summary.errors,
        "checkedAt": summary.checked_at.to_rfc3339(),
        "oldestPackage": summary.oldest.as_ref().map(|r| r.name.clone()),
        "results": results,
    });

    println!("{}", serde_json::to_string_pretty(&output).unwrap());
}

fn print_csv(summary: &dep_age::DepAgeSummary) {
    println!("name,version,latest,published_at,days_since_publish,status,registry");
    for r in &summary.results {
        let published = r.published_at.map(|d| d.to_rfc3339()).unwrap_or_default();
        let days = r
            .days_since_publish
            .map(|d| d.to_string())
            .unwrap_or_default();
        let registry = match r.registry {
            Registry::Crates => "crates",
            Registry::Npm => "npm",
            Registry::PyPI => "pypi",
            Registry::Go => "go",
            Registry::Docker => "docker",
            Registry::Ruby => "ruby",
            Registry::Composer => "packagist",
        };
        println!(
            "{},{},{},{},{},{},{}",
            r.name,
            r.version_spec,
            r.latest_version,
            published,
            days,
            r.status.as_str(),
            registry
        );
    }
}

fn print_html(summary: &dep_age::DepAgeSummary) {
    println!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>dep-age report</title>
    <style>
        body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; margin: 40px; background: #1e1e1e; color: #e0e0e0; }}
        h1 {{ color: #58a6ff; }}
        table {{ border-collapse: collapse; width: 100%; max-width: 1000px; }}
        th, td {{ padding: 12px; text-align: left; border-bottom: 1px solid #333; }}
        th {{ background: #2d2d2d; color: #fff; }}
        .fresh {{ color: #3fb950; }}
        .aging {{ color: #d29922; }}
        .stale {{ color: #f85149; }}
        .ancient {{ color: #a371f7; }}
        .error {{ color: #8b949e; }}
        .summary {{ margin: 20px 0; padding: 20px; background: #2d2d2d; border-radius: 8px; }}
        .summary-item {{ display: inline-block; margin: 10px 20px 10px 0; }}
        .summary-label {{ color: #8b949e; }}
        .summary-value {{ font-size: 24px; font-weight: bold; margin-left: 8px; }}
    </style>
</head>
<body>
    <h1>dep-age report</h1>
    <div class="summary">
        <div class="summary-item"><span class="summary-label">Total:</span><span class="summary-value">{}</span></div>
        <div class="summary-item"><span class="summary-label fresh">Fresh:</span><span class="summary-value fresh">{}</span></div>
        <div class="summary-item"><span class="summary-label aging">Aging:</span><span class="summary-value aging">{}</span></div>
        <div class="summary-item"><span class="summary-label stale">Stale:</span><span class="summary-value stale">{}</span></div>
        <div class="summary-item"><span class="summary-label ancient">Ancient:</span><span class="summary-value ancient">{}</span></div>
        <div class="summary-item"><span class="summary-label error">Errors:</span><span class="summary-value error">{}</span></div>
    </div>
    <table>
        <thead>
            <tr><th>Package</th><th>Version</th><th>Latest</th><th>Age</th><th>Status</th><th>Registry</th></tr>
        </thead>
        <tbody>"#,
        summary.total, summary.fresh, summary.aging, summary.stale, summary.ancient, summary.errors
    );

    for r in &summary.results {
        let status_class = match r.status {
            Status::Fresh => "fresh",
            Status::Aging => "aging",
            Status::Stale => "stale",
            Status::Ancient => "ancient",
            Status::Error(_) => "error",
        };
        let status_text = r.status.as_str();
        let age = r
            .days_since_publish
            .map(|d| d.to_string())
            .unwrap_or_else(|| "?".to_string());
        let registry = match r.registry {
            Registry::Crates => "crates",
            Registry::Npm => "npm",
            Registry::PyPI => "pypi",
            Registry::Go => "go",
            Registry::Docker => "docker",
            Registry::Ruby => "ruby",
            Registry::Composer => "packagist",
        };
        println!(
            r#"            <tr>
                <td>{}</td>
                <td>{}</td>
                <td>{}</td>
                <td>{} days</td>
                <td class="{}">{}</td>
                <td>{}</td>
            </tr>"#,
            r.name, r.version_spec, r.latest_version, age, status_class, status_text, registry
        );
    }

    println!(
        r#"        </tbody>
    </table>
</body>
</html>"#
    );
}

fn print_step_summary(summary: &dep_age::DepAgeSummary) {
    println!("## Dependency Age Report");
    println!();
    println!("| Status | Count |");
    println!("|--------|-------|");
    if summary.fresh > 0 {
        println!(
            "| :white_check_mark: Fresh (<90d) | **{}** |",
            summary.fresh
        );
    }
    if summary.aging > 0 {
        println!("| :warning: Aging (90d-1y) | **{}** |", summary.aging);
    }
    if summary.stale > 0 {
        println!("| :x: Stale (1-2y) | **{}** |", summary.stale);
    }
    if summary.ancient > 0 {
        println!("| :no_entry: Ancient (>2y) | **{}** |", summary.ancient);
    }
    if summary.errors > 0 {
        println!("| :question: Errors | **{}** |", summary.errors);
    }
    println!();
    println!("**Total:** {} packages checked", summary.total);

    if summary.stale > 0 || summary.ancient > 0 {
        println!();
        println!("### Stale Dependencies");
        println!();
        println!("| Package | Version | Age | Published |");
        println!("|---------|----------|-----|-----------|");
        for r in &summary.results {
            if matches!(r.status, Status::Stale | Status::Ancient) {
                let age = r
                    .days_since_publish
                    .map(|d| format!("{}d", d))
                    .unwrap_or_else(|| "?".to_string());
                let published = r
                    .published_at
                    .map(|d| d.format("%Y-%m-%d").to_string())
                    .unwrap_or_else(|| "?".to_string());
                println!(
                    "| {} | {} | {} | {} |",
                    r.name, r.version_spec, age, published
                );
            }
        }
    }
}

// ── Helper functions to count dependencies ───────────────────────────────────

fn get_cargo_dep_count(path: &PathBuf) -> usize {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return 0,
    };

    let manifest: toml::Value = match toml::from_str(&content) {
        Ok(m) => m,
        Err(_) => return 0,
    };

    let mut count = 0;
    if let Some(deps) = manifest.get("dependencies").and_then(|v| v.as_table()) {
        count += deps.len();
    }
    if let Some(deps) = manifest.get("dev-dependencies").and_then(|v| v.as_table()) {
        count += deps.len();
    }
    if let Some(deps) = manifest
        .get("build-dependencies")
        .and_then(|v| v.as_table())
    {
        count += deps.len();
    }
    count
}

fn get_package_json_dep_count(path: &PathBuf) -> usize {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return 0,
    };

    let pkg: serde_json::Value = match serde_json::from_str(&content) {
        Ok(p) => p,
        Err(_) => return 0,
    };

    let mut count = 0;
    if let Some(deps) = pkg.get("dependencies").and_then(|v| v.as_object()) {
        count += deps.len();
    }
    if let Some(deps) = pkg.get("devDependencies").and_then(|v| v.as_object()) {
        count += deps.len();
    }
    count
}

fn get_pyproject_dep_count(path: &PathBuf) -> usize {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return 0,
    };

    let manifest: toml::Value = match toml::from_str(&content) {
        Ok(m) => m,
        Err(_) => return 0,
    };

    let mut count = 0;

    // PEP 621: [project].dependencies
    if let Some(deps) = manifest
        .get("project")
        .and_then(|p| p.get("dependencies"))
        .and_then(|d| d.as_array())
    {
        count += deps.len();
    }

    // PEP 621: [project].optional-dependencies
    if let Some(opt_deps) = manifest
        .get("project")
        .and_then(|p| p.get("optional-dependencies"))
        .and_then(|d| d.as_table())
    {
        for group_deps in opt_deps.values() {
            if let Some(arr) = group_deps.as_array() {
                count += arr.len();
            }
        }
    }

    // Poetry: [tool.poetry.dependencies] (minus python itself)
    if let Some(poetry_deps) = manifest
        .get("tool")
        .and_then(|t| t.get("poetry"))
        .and_then(|p| p.get("dependencies"))
        .and_then(|d| d.as_table())
    {
        count += poetry_deps.len();
        // Don't count "python" as a dependency
        if poetry_deps.contains_key("python") {
            count -= 1;
        }
    }

    count
}

fn get_requirements_dep_count(path: &PathBuf) -> usize {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return 0,
    };

    content
        .lines()
        .filter(|line| {
            let line = line.trim();
            !line.is_empty() && !line.starts_with('#') && !line.starts_with('-')
        })
        .count()
}

// ── Auto-detect manifest ─────────────────────────────────────────────────────

#[derive(Debug)]
enum ManifestKind {
    Cargo(PathBuf),
    PackageJson(PathBuf),
    Pyproject(PathBuf),
    RequirementsTxt(PathBuf),
    GoMod(PathBuf),
    DockerCompose(PathBuf),
    RubyGemfile(PathBuf),
    ComposerJson(PathBuf),
}

fn detect_manifest(path: Option<PathBuf>) -> Result<ManifestKind, String> {
    if let Some(p) = path {
        let name = p
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();
        if name == "cargo.toml" {
            return Ok(ManifestKind::Cargo(p));
        }
        if name == "package.json" {
            return Ok(ManifestKind::PackageJson(p));
        }
        if name == "pyproject.toml" {
            return Ok(ManifestKind::Pyproject(p));
        }
        if name == "requirements.txt" || name.ends_with(".requirements.txt") {
            return Ok(ManifestKind::RequirementsTxt(p));
        }
        if name == "go.mod" {
            return Ok(ManifestKind::GoMod(p));
        }
        if name == "docker-compose.yml" || name == "docker-compose.yaml" {
            return Ok(ManifestKind::DockerCompose(p));
        }
        if name == "gemfile" {
            return Ok(ManifestKind::RubyGemfile(p));
        }
        if name == "composer.json" {
            return Ok(ManifestKind::ComposerJson(p));
        }
        return Err(format!(
            "Unrecognised file: {}. Pass Cargo.toml, package.json, pyproject.toml, requirements.txt, go.mod, docker-compose.yml, Gemfile, or composer.json.",
            p.display()
        ));
    }

    // Auto-detect
    let cargo = PathBuf::from("Cargo.toml");
    let pkg = PathBuf::from("package.json");
    let pyproject = PathBuf::from("pyproject.toml");
    let requirements = PathBuf::from("requirements.txt");
    let go_mod = PathBuf::from("go.mod");
    let docker_compose = PathBuf::from("docker-compose.yml");
    let docker_compose_yaml = PathBuf::from("docker-compose.yaml");
    let gemfile = PathBuf::from("Gemfile");
    let composer = PathBuf::from("composer.json");

    if cargo.exists() {
        return Ok(ManifestKind::Cargo(cargo));
    }
    if pkg.exists() {
        return Ok(ManifestKind::PackageJson(pkg));
    }
    if pyproject.exists() {
        return Ok(ManifestKind::Pyproject(pyproject));
    }
    if requirements.exists() {
        return Ok(ManifestKind::RequirementsTxt(requirements));
    }
    if go_mod.exists() {
        return Ok(ManifestKind::GoMod(go_mod));
    }
    if docker_compose.exists() {
        return Ok(ManifestKind::DockerCompose(docker_compose));
    }
    if docker_compose_yaml.exists() {
        return Ok(ManifestKind::DockerCompose(docker_compose_yaml));
    }
    if gemfile.exists() {
        return Ok(ManifestKind::RubyGemfile(gemfile));
    }
    if composer.exists() {
        return Ok(ManifestKind::ComposerJson(composer));
    }

    Err("No Cargo.toml, package.json, pyproject.toml, requirements.txt, go.mod, docker-compose.yml, Gemfile, or composer.json found in the current directory.".to_string())
}

/// Load ignored packages from `.dep-age-ignore` in the same directory as the manifest.
/// One package name per line. Lines starting with `#` are comments.
fn load_ignore_file(dir: &std::path::Path) -> Vec<String> {
    let ignore_path = dir.join(".dep-age-ignore");
    if !ignore_path.exists() {
        return vec![];
    }
    std::fs::read_to_string(&ignore_path)
        .ok()
        .map(|content| {
            content
                .lines()
                .map(|l| l.trim())
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

// ── Main ─────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Load config from file if specified or auto-detected
    let config = if let Some(config_path) = &cli.config {
        match ToolConfig::from_file(config_path) {
            Ok(c) => Some(c),
            Err(e) => {
                eprintln!("{} Failed to load config: {}", "error:".red().bold(), e);
                std::process::exit(1);
            }
        }
    } else {
        ToolConfig::detect()
    };

    // Handle cache subcommands
    if let Some(Commands::Cache { action }) = cli.command {
        match action {
            CacheAction::Clear => {
                let cache = match RegistryCache::new() {
                    Ok(c) => c,
                    Err(_) => {
                        eprintln!("{} Failed to initialize cache", "error:".red().bold());
                        std::process::exit(1);
                    }
                };
                let stats_before = cache.stats().unwrap_or(dep_age::CacheStats {
                    total_entries: 0,
                    expired_entries: 0,
                    valid_entries: 0,
                    total_size_bytes: 0,
                });
                cache.clear().unwrap_or_else(|e| {
                    eprintln!("{} {}", "error:".red().bold(), e);
                    std::process::exit(1);
                });
                println!(
                    "{} Cleared {} cache entries ({} valid, {} expired)",
                    "cache:".green().bold(),
                    stats_before.total_entries,
                    stats_before.valid_entries,
                    stats_before.expired_entries,
                );
            }
            CacheAction::Stats => {
                let cache = match RegistryCache::new() {
                    Ok(c) => c,
                    Err(_) => {
                        eprintln!("{} Failed to initialize cache", "error:".red().bold());
                        std::process::exit(1);
                    }
                };
                let stats = cache.stats().unwrap_or_else(|e| {
                    eprintln!("{} {}", "error:".red().bold(), e);
                    std::process::exit(1);
                });
                println!();
                println!("  {}", "Cache Statistics".bold());
                println!("  {}", "─────────────────────────────".dimmed());
                println!(
                    "  {:<20} {}",
                    "Total entries:".dimmed(),
                    stats.total_entries
                );
                println!(
                    "  {:<20} {}",
                    "Valid entries:".dimmed(),
                    stats.valid_entries
                );
                println!(
                    "  {:<20} {}",
                    "Expired entries:".dimmed(),
                    stats.expired_entries
                );
                println!(
                    "  {:<20} {}",
                    "Total size:".dimmed(),
                    format_bytes(stats.total_size_bytes)
                );
                println!();
            }
        }
        return;
    }

    let total_deps = match detect_manifest(cli.file.clone()) {
        Ok(m) => match &m {
            ManifestKind::Cargo(p) => get_cargo_dep_count(p),
            ManifestKind::PackageJson(p) => get_package_json_dep_count(p),
            ManifestKind::Pyproject(p) => get_pyproject_dep_count(p),
            ManifestKind::RequirementsTxt(p) => get_requirements_dep_count(p),
            ManifestKind::GoMod(_) => 0,
            ManifestKind::DockerCompose(_) => 0,
            ManifestKind::RubyGemfile(_) => 0,
            ManifestKind::ComposerJson(_) => 0,
        },
        Err(_) => 0,
    };

    let progress = if !cli.json
        && !matches!(cli.format, OutputFormat::Json | OutputFormat::Csv)
        && total_deps > 0
    {
        let pb = ProgressBar::new(total_deps as u64);
        pb.set_style(
            indicatif::ProgressStyle::default_bar()
                .template("  {spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({msg})")
                .unwrap()
                .progress_chars("#-"),
        );
        pb.set_message("fetching");
        Some(pb)
    } else {
        None
    };

    // TUI mode - show results in terminal UI after collection
    #[allow(unused_variables)]
    let use_tui = false; // TUI mode disabled for now

    let on_progress = progress.as_ref().map(|pb| {
        let pb = pb.clone();
        std::sync::Arc::new(move |_| pb.inc(1)) as std::sync::Arc<dyn Fn(usize) + Send + Sync>
    });

    let manifest = match detect_manifest(cli.file.clone()) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{} {}", "error:".red().bold(), e);
            std::process::exit(1);
        }
    };

    // Load .dep-age-ignore from manifest directory + merge with CLI --ignore
    let manifest_dir = match &manifest {
        ManifestKind::Cargo(p)
        | ManifestKind::PackageJson(p)
        | ManifestKind::Pyproject(p)
        | ManifestKind::RequirementsTxt(p)
        | ManifestKind::GoMod(p)
        | ManifestKind::DockerCompose(p)
        | ManifestKind::RubyGemfile(p)
        | ManifestKind::ComposerJson(p) => p.parent().unwrap_or(std::path::Path::new(".")),
    };
    let file_ignores = load_ignore_file(manifest_dir);
    let mut all_ignores = file_ignores;
    all_ignores.extend(cli.ignore.clone());

    let opts = CheckOptions {
        include_dev: !cli.no_dev && !config.as_ref().map(|c| c.get_no_dev()).unwrap_or(false),
        concurrency: cli.concurrency,
        threshold_fresh: cli.fresh,
        threshold_aging: cli.aging,
        threshold_stale: cli.stale,
        ignore_list: all_ignores,
        crates_base_url: config
            .as_ref()
            .and_then(|c| c.registry.as_ref()?.crates_base_url.clone()),
        npm_base_url: config
            .as_ref()
            .and_then(|c| c.registry.as_ref()?.npm_base_url.clone()),
        pypi_base_url: config
            .as_ref()
            .and_then(|c| c.registry.as_ref()?.pypi_base_url.clone()),
        registry_cache: if cli.cache {
            RegistryCache::new().ok()
        } else {
            None
        },
        on_progress,
        timeout_secs: 30,
        max_retries: 3,
    };

    let is_machine = cli.json
        || matches!(
            cli.format,
            OutputFormat::Json
                | OutputFormat::Csv
                | OutputFormat::Ndjson
                | OutputFormat::Html
                | OutputFormat::StepSummary
        );

    if !is_machine {
        let (file_label, reg_label) = match &manifest {
            ManifestKind::Cargo(p) => (p.display().to_string(), "crates.io"),
            ManifestKind::PackageJson(p) => (p.display().to_string(), "npm"),
            ManifestKind::Pyproject(p) => (p.display().to_string(), "PyPI"),
            ManifestKind::RequirementsTxt(p) => (p.display().to_string(), "PyPI"),
            ManifestKind::GoMod(p) => (p.display().to_string(), "go"),
            ManifestKind::DockerCompose(p) => (p.display().to_string(), "docker"),
            ManifestKind::RubyGemfile(p) => (p.display().to_string(), "rubygems"),
            ManifestKind::ComposerJson(p) => (p.display().to_string(), "packagist"),
        };
        println!();
        println!(
            "  {} {}",
            "dep-age".bold(),
            format!("checking {} via {}", file_label, reg_label).dimmed()
        );
        println!();
        print_legend();
    }

    // Get manifest path for diff before consuming manifest
    let manifest_path_for_diff = match &manifest {
        ManifestKind::Cargo(p)
        | ManifestKind::PackageJson(p)
        | ManifestKind::Pyproject(p)
        | ManifestKind::RequirementsTxt(p)
        | ManifestKind::GoMod(p)
        | ManifestKind::DockerCompose(p)
        | ManifestKind::RubyGemfile(p)
        | ManifestKind::ComposerJson(p) => p.clone(),
    };

    let summary = match manifest {
        ManifestKind::Cargo(p) => check_cargo_toml(p, &opts).await,
        ManifestKind::PackageJson(p) => check_package_json(p, &opts).await,
        ManifestKind::Pyproject(p) => dep_age::check_pyproject_toml(p, &opts).await,
        ManifestKind::RequirementsTxt(p) => dep_age::check_requirements_txt(p, &opts).await,
        ManifestKind::GoMod(p) => dep_age::check_go_mod(p, &opts).await,
        ManifestKind::DockerCompose(p) => dep_age::check_docker_compose(p, &opts).await,
        ManifestKind::RubyGemfile(p) => dep_age::check_ruby_gemfile(p, &opts).await,
        ManifestKind::ComposerJson(p) => dep_age::check_composer_json(p, &opts).await,
    };

    let summary = match summary {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{} {}", "error:".red().bold(), e);
            std::process::exit(1);
        }
    };

    if cli.json || matches!(cli.format, OutputFormat::Json) {
        print_json(&summary);
        if let Some(pb) = &progress {
            pb.finish_and_clear();
        }
        return;
    }

    // Finish and clear progress bar
    if let Some(pb) = &progress {
        pb.finish_and_clear();
    }

    // CSV output — header + rows, no decoration
    if matches!(cli.format, OutputFormat::Csv) {
        print_csv(&summary);
        return;
    }

    // GitHub Actions annotations
    if matches!(cli.format, OutputFormat::GithubChecks) {
        print!("{}", format_github_checks(&summary));
        return;
    }

    // JUnit XML output
    if matches!(cli.format, OutputFormat::Junit) {
        match format_junit(&summary) {
            Ok(output) => println!("{}", output),
            Err(e) => {
                eprintln!("{} {}", "error:".red().bold(), e);
                std::process::exit(1);
            }
        }
        return;
    }

    // SARIF output
    if matches!(cli.format, OutputFormat::Sarif) {
        match format_sarif(&summary) {
            Ok(output) => println!("{}", output),
            Err(e) => {
                eprintln!("{} {}", "error:".red().bold(), e);
                std::process::exit(1);
            }
        }
        return;
    }

    // NDJSON output - newline-delimited JSON for streaming
    if matches!(cli.format, OutputFormat::Ndjson) {
        for r in &summary.results {
            let json = serde_json::json!({
                "name": r.name,
                "version": r.version_spec,
                "latestVersion": r.latest_version,
                "publishedAt": r.published_at.map(|d| d.to_rfc3339()),
                "daysSincePublish": r.days_since_publish,
                "status": r.status.as_str(),
                "registry": match r.registry { Registry::Crates => "crates", Registry::Npm => "npm", Registry::PyPI => "pypi", Registry::Go => "go", Registry::Docker => "docker", Registry::Ruby => "ruby", Registry::Composer => "packagist" },
            });
            println!("{}", json);
        }
        return;
    }

    // HTML output
    if matches!(cli.format, OutputFormat::Html) {
        print_html(&summary);
        return;
    }

    // GitHub Actions step summary (markdown for `::group::`)
    if matches!(cli.format, OutputFormat::StepSummary) {
        print_step_summary(&summary);
        return;
    }

    // --check mode: quiet, just exit code
    if cli.check {
        if should_fail(&summary, &cli.fail_on) {
            std::process::exit(1);
        }
        return;
    }

    // Pretty output — table + legend + summary footer

    // Header
    println!(
        "  {}",
        format!(
            "  {:<30} {:<12} {:<10} {:<10}  {}",
            "package", "version", "latest", "age", "status"
        )
        .dimmed()
    );
    println!("  {}", "─".repeat(80).dimmed());

    // Sort and filter results
    let mut results = summary.results.clone();
    results.sort_by(|a, b| {
        let key = |r: &DepResult| match cli.sort {
            SortArg::Age => (sort_key(&r.status), r.days_since_publish.unwrap_or(0)),
            SortArg::Status => (sort_key(&r.status), 0),
            SortArg::Name => (4, 0), // fallback
        };
        let ak = key(a);
        let bk = key(b);
        if cli.sort == SortArg::Name {
            a.name.cmp(&b.name)
        } else {
            ak.cmp(&bk)
        }
    });

    for r in results.iter().filter(|r| should_show(r, &cli.filter)) {
        print_row(r);
    }

    // Summary footer
    println!();
    println!("  {}", "Summary".bold());
    println!("  {}", "─────────────────────────────".dimmed());

    let print_line = |label: &str, count: usize, color: &str| {
        if count == 0 {
            return;
        }
        let label_col = match color {
            "green" => format!("{:<12}", label).green(),
            "yellow" => format!("{:<12}", label).yellow(),
            "red" => format!("{:<12}", label).red(),
            "magenta" => format!("{:<12}", label).magenta(),
            _ => format!("{:<12}", label).dimmed(),
        };
        println!(
            "  {}  {} package{}",
            label_col,
            count,
            if count != 1 { "s" } else { "" }
        );
    };

    print_line("fresh", summary.fresh, "green");
    print_line("aging", summary.aging, "yellow");
    print_line("stale", summary.stale, "red");
    print_line("ancient", summary.ancient, "magenta");
    print_line("errors", summary.errors, "dim");

    println!("  {}", "─────────────────────────────".dimmed());
    println!("  {}  {} packages checked", "total".dimmed(), summary.total);

    if let Some(oldest) = &summary.oldest {
        println!();
        println!(
            "  {}  {}  {}",
            "oldest".dimmed(),
            oldest.name.bold(),
            format_age(oldest.days_since_publish).magenta()
        );
    }

    println!();

    // Handle diff output
    if cli.diff {
        let manifest_dir = manifest_path_for_diff
            .parent()
            .unwrap_or(std::path::Path::new("."));

        // Load previous run if exists
        let previous = PreviousRun::load(manifest_dir);

        if let Some(prev) = previous {
            let diffs = compute_diff(&summary, &prev);
            println!("{}", format_diff(&diffs));
        } else {
            println!("No previous run data found. Run without --diff first to establish baseline.");
        }

        // Save current run for next comparison
        let prev_run = PreviousRun::from_summary(&summary, &manifest_dir.to_string_lossy());
        if let Err(e) = prev_run.save(manifest_dir) {
            eprintln!("Warning: Failed to save diff history: {}", e);
        }
    } else {
        // Auto-save for future diffs (only on non-diff runs)
        let manifest_dir = manifest_path_for_diff
            .parent()
            .unwrap_or(std::path::Path::new("."));
        let prev_run = PreviousRun::from_summary(&summary, &manifest_dir.to_string_lossy());
        let _ = prev_run.save(manifest_dir);
    }

    // Exit with non-zero based on --fail-on flag (useful for CI)
    if should_fail(&summary, &cli.fail_on) {
        std::process::exit(1);
    }
}
