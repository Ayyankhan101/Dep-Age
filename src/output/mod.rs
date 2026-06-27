//! Output formatters for various formats
//!
//! This module provides formatters for different output formats including:
//! - GitHub Actions annotations
//! - JUnit XML
//! - SARIF (Static Analysis Results Interchange Format)

use crate::{DepResult, Registry};

pub mod github_checks;
pub mod junit;
pub mod sarif;

pub use github_checks::format_github_checks;
pub use junit::format_junit;
pub use sarif::format_sarif;

/// Format years from days for display.
pub(crate) fn format_years(days: i64) -> String {
    let years = days as f64 / 365.0;
    format!("{:.1}", years)
}

/// Get the registry URL for a dependency.
pub(crate) fn registry_url(r: &DepResult) -> String {
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
