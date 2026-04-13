use clap::{Parser, Subcommand, ValueEnum};
use colored::*;
use dep_age::{check_cargo_toml, check_package_json, CheckOptions, DepResult, Registry, RegistryCache, Status};
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

    /// Output raw JSON
    #[arg(long)]
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

    /// Exit code 1 when packages match this status or worse
    #[arg(long, value_enum, default_value = "ancient")]
    fail_on: FailOnArg,
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

// ── Display helpers ─────────────────────────────────────────────────────────

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
    let name = format!("{:<34}", r.name);
    let ver = format!("{:<14}", r.version_spec);
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
    };

    println!(
        "  {}  {} {} {}  {}  {}",
        icon,
        name.bold(),
        ver.dimmed(),
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
        FailOnArg::Any => {
            summary.aging > 0 || summary.stale > 0 || summary.ancient > 0
        }
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
                "registry": match r.registry { Registry::Crates => "crates", Registry::Npm => "npm", Registry::PyPI => "pypi" },
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
    if let Some(deps) = manifest
        .get("dev-dependencies")
        .and_then(|v| v.as_table())
    {
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
    if let Some(deps) = pkg
        .get("devDependencies")
        .and_then(|v| v.as_object())
    {
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
        return Err(format!(
            "Unrecognised file: {}. Pass Cargo.toml, package.json, pyproject.toml, or requirements.txt.",
            p.display()
        ));
    }

    // Auto-detect
    let cargo = PathBuf::from("Cargo.toml");
    let pkg = PathBuf::from("package.json");
    let pyproject = PathBuf::from("pyproject.toml");
    let requirements = PathBuf::from("requirements.txt");

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

    Err("No Cargo.toml, package.json, pyproject.toml, or requirements.txt found in the current directory.".to_string())
}

// ── Main ─────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

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
                    total_entries: 0, expired_entries: 0, valid_entries: 0, total_size_bytes: 0,
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
                println!("  {:<20} {}", "Total entries:".dimmed(), stats.total_entries);
                println!("  {:<20} {}", "Valid entries:".dimmed(), stats.valid_entries);
                println!("  {:<20} {}", "Expired entries:".dimmed(), stats.expired_entries);
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
        },
        Err(_) => 0,
    };

    let progress = if !cli.json && total_deps > 0 {
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

    let on_progress = progress.as_ref().map(|pb| {
        let pb = pb.clone();
        std::sync::Arc::new(move |_| pb.inc(1)) as std::sync::Arc<dyn Fn(usize) + Send + Sync>
    });

    let opts = CheckOptions {
        include_dev: !cli.no_dev,
        concurrency: cli.concurrency,
        threshold_fresh: cli.fresh,
        threshold_aging: cli.aging,
        threshold_stale: cli.stale,
        registry_cache: if cli.cache {
            RegistryCache::new().ok()
        } else {
            None
        },
        on_progress,
        ..CheckOptions::default()
    };

    let manifest = match detect_manifest(cli.file) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{} {}", "error:".red().bold(), e);
            std::process::exit(1);
        }
    };

    if !cli.json {
        let (file_label, reg_label) = match &manifest {
            ManifestKind::Cargo(p) => (p.display().to_string(), "crates.io"),
            ManifestKind::PackageJson(p) => (p.display().to_string(), "npm"),
            ManifestKind::Pyproject(p) => (p.display().to_string(), "PyPI"),
            ManifestKind::RequirementsTxt(p) => (p.display().to_string(), "PyPI"),
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

    let summary = match manifest {
        ManifestKind::Cargo(p) => check_cargo_toml(p, &opts).await,
        ManifestKind::PackageJson(p) => check_package_json(p, &opts).await,
        ManifestKind::Pyproject(p) => {
            dep_age::check_pyproject_toml(p, &opts).await
        }
        ManifestKind::RequirementsTxt(p) => {
            dep_age::check_requirements_txt(p, &opts).await
        }
    };

    let summary = match summary {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{} {}", "error:".red().bold(), e);
            std::process::exit(1);
        }
    };

    if cli.json {
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

    // Header
    println!(
        "  {}",
        format!(
            "  {:<34} {:<14} {:<10}  {}",
            "package", "version", "age", "status"
        )
        .dimmed()
    );
    println!("  {}", "─".repeat(72).dimmed());

    // Sort and filter results
    let mut results = summary.results.clone();
    results.sort_by_key(|r| sort_key(&r.status));

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
    println!(
        "  {}  {} packages checked",
        "total".dimmed(),
        summary.total
    );

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

    // Exit with non-zero based on --fail-on flag (useful for CI)
    if should_fail(&summary, &cli.fail_on) {
        std::process::exit(1);
    }
}
