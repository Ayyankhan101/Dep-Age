//! Output formatters for various formats
//!
//! This module provides formatters for different output formats including:
//! - GitHub Actions annotations
//! - JUnit XML
//! - SARIF (Static Analysis Results Interchange Format)

pub mod github_checks;
pub mod junit;
pub mod sarif;

pub use github_checks::format_github_checks;
pub use junit::format_junit;
pub use sarif::format_sarif;
