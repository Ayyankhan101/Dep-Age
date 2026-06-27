//! Tests for output formatters (github_checks, junit, sarif)

use chrono::Utc;
use dep_age::{DepAgeSummary, DepResult, Registry, Status};

fn make_summary(results: Vec<DepResult>) -> DepAgeSummary {
    let fresh = results.iter().filter(|r| r.status == Status::Fresh).count();
    let aging = results.iter().filter(|r| r.status == Status::Aging).count();
    let stale = results.iter().filter(|r| r.status == Status::Stale).count();
    let ancient = results
        .iter()
        .filter(|r| r.status == Status::Ancient)
        .count();
    let errors = results
        .iter()
        .filter(|r| matches!(r.status, Status::Error(_)))
        .count();
    DepAgeSummary {
        total: results.len(),
        fresh,
        aging,
        stale,
        ancient,
        errors,
        oldest: None,
        checked_at: Utc::now(),
        results,
    }
}

fn make_dep(name: &str, status: Status) -> DepResult {
    DepResult {
        name: name.to_string(),
        version_spec: "1.0".to_string(),
        latest_version: "2.0".to_string(),
        published_at: Some(Utc::now()),
        days_since_publish: Some(match status {
            Status::Fresh => 30,
            Status::Aging => 200,
            Status::Stale => 500,
            Status::Ancient => 1000,
            Status::Error(_) => 0,
        }),
        status,
        registry: Registry::Crates,
    }
}

// ── GitHub Checks ────────────────────────────────────────────────────────────

#[test]
fn test_github_checks_skips_fresh() {
    let summary = make_summary(vec![
        make_dep("fresh-pkg", Status::Fresh),
        make_dep("stale-pkg", Status::Stale),
    ]);
    let output = dep_age::output::format_github_checks(&summary);
    assert!(!output.contains("fresh-pkg"));
    assert!(output.contains("stale-pkg"));
}

#[test]
fn test_github_checks_ancient_message() {
    let summary = make_summary(vec![make_dep("old-pkg", Status::Ancient)]);
    let output = dep_age::output::format_github_checks(&summary);
    assert!(output.contains("old-pkg"));
    assert!(output.contains("ancient"));
}

#[test]
fn test_github_checks_error_message() {
    let summary = make_summary(vec![DepResult {
        name: "err-pkg".to_string(),
        version_spec: "1.0".to_string(),
        latest_version: "unknown".to_string(),
        published_at: None,
        days_since_publish: None,
        status: Status::Error("timeout".to_string()),
        registry: Registry::Npm,
    }]);
    let output = dep_age::output::format_github_checks(&summary);
    assert!(output.contains("err-pkg"));
    assert!(output.contains("timeout"));
}

// ── JUnit ────────────────────────────────────────────────────────────────────

#[test]
fn test_junit_output_valid_xml() {
    let summary = make_summary(vec![
        make_dep("fresh-pkg", Status::Fresh),
        make_dep("stale-pkg", Status::Stale),
    ]);
    let output = dep_age::output::format_junit(&summary).unwrap();
    assert!(output.contains("<testsuite"));
    assert!(output.contains("failures=\"1\""));
    assert!(output.contains("stale-pkg"));
}

#[test]
fn test_junit_empty_summary() {
    let summary = make_summary(vec![]);
    let output = dep_age::output::format_junit(&summary).unwrap();
    assert!(output.contains("tests=\"0\""));
    assert!(output.contains("failures=\"0\""));
}

// ── SARIF ────────────────────────────────────────────────────────────────────

#[test]
fn test_sarif_output_valid_xml() {
    let summary = make_summary(vec![make_dep("old-pkg", Status::Ancient)]);
    let output = dep_age::output::format_sarif(&summary).unwrap();
    assert!(output.contains("sarif"));
    assert!(output.contains("DEP001"));
    assert!(output.contains("old-pkg"));
}

#[test]
fn test_sarif_skips_fresh() {
    let summary = make_summary(vec![
        make_dep("fresh-pkg", Status::Fresh),
        make_dep("aging-pkg", Status::Aging),
    ]);
    let output = dep_age::output::format_sarif(&summary).unwrap();
    assert!(!output.contains("fresh-pkg"));
    assert!(output.contains("aging-pkg"));
    assert!(output.contains("DEP003"));
}
