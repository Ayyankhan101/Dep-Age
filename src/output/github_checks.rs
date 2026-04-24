//! GitHub Actions workflow job annotations output formatter

use crate::{DepAgeSummary, DepResult, Registry};

const ANNOTATION_LEVEL: &str = "warning";

pub fn format_github_checks(summary: &DepAgeSummary) -> String {
    let mut output = String::new();

    for result in &summary.results {
        let message = match &result.status {
            crate::Status::Ancient => format_ancient_message(result),
            crate::Status::Stale => format_stale_message(result),
            crate::Status::Aging => format_aging_message(result),
            crate::Status::Fresh => continue,
            crate::Status::Error(e) => format_error_message(result, e),
        };

        output.push_str(&format!(
            "::{} file={},line=1,col=1::{}\n",
            ANNOTATION_LEVEL, result.name, message
        ));
    }

    output
}

fn format_ancient_message(r: &DepResult) -> String {
    let days = r.days_since_publish.unwrap_or(0);
    let years = days as f64 / 365.0;
    format!(
        "{} ({}) is {} years old (ancient) - {}",
        r.name,
        r.version_spec,
        format_years(years),
        registry_url(r)
    )
}

fn format_stale_message(r: &DepResult) -> String {
    let days = r.days_since_publish.unwrap_or(0);
    format!(
        "{} ({}) has not been updated in {} days (stale) - {}",
        r.name,
        r.version_spec,
        days,
        registry_url(r)
    )
}

fn format_aging_message(r: &DepResult) -> String {
    let days = r.days_since_publish.unwrap_or(0);
    format!(
        "{} ({}) is {} days old (aging) - {}",
        r.name,
        r.version_spec,
        days,
        registry_url(r)
    )
}

fn format_error_message(r: &DepResult, error: &str) -> String {
    format!(
        "Failed to check {} ({}) from {}: {}",
        r.name,
        r.version_spec,
        match r.registry {
            Registry::Crates => "crates.io",
            Registry::Npm => "npm",
            Registry::PyPI => "PyPI",
            Registry::Go => "go",
            Registry::Docker => "docker",
            Registry::Ruby => "rubygems",
            Registry::Composer => "packagist",
        },
        error
    )
}

fn format_years(years: f64) -> String {
    format!("{:.1}", years)
}

fn registry_url(r: &DepResult) -> String {
    match r.registry {
        Registry::Crates => format!("https://crates.io/crates/{}", r.name),
        Registry::Npm => format!("https://www.npmjs.com/package/{}", r.name),
        Registry::PyPI => format!("https://pypi.org/project/{}", r.name),
        Registry::Go => format!("https://pkg.go.dev/{}", r.name),
        Registry::Docker => format!("https://hub.docker.com/r/{}", r.name),
        Registry::Ruby => format!("https://rubygems.org/gems/{}", r.name),
        Registry::Composer => format!("https://packagist.org/packages/{}", r.name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Status;

    #[test]
    fn test_format_ancient_message() {
        let result = DepResult {
            name: "time".to_string(),
            version_spec: "0.1".to_string(),
            latest_version: "0.3".to_string(),
            published_at: None,
            days_since_publish: Some(1168),
            status: Status::Ancient,
            registry: Registry::Crates,
        };

        let msg = format_ancient_message(&result);
        assert!(msg.contains("time"));
        assert!(msg.contains("ancient"));
    }

    #[test]
    fn test_format_stale_message() {
        let result = DepResult {
            name: "toml".to_string(),
            version_spec: "0.5".to_string(),
            latest_version: "0.8".to_string(),
            published_at: None,
            days_since_publish: Some(500),
            status: Status::Stale,
            registry: Registry::Crates,
        };

        let msg = format_stale_message(&result);
        assert!(msg.contains("toml"));
        assert!(msg.contains("stale"));
    }
}
