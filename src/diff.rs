//! Diff/Trend tracking for dependency age changes
//!
//! Stores results from previous runs and provides diff output

use crate::{DepAgeSummary, Status};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Previous run data stored in cache
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviousRun {
    pub manifest_path: String,
    pub checked_at: chrono::DateTime<chrono::Utc>,
    pub results: Vec<PreviousResult>,
}

/// Simplified result for storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviousResult {
    pub name: String,
    pub version_spec: String,
    pub status: String,
    pub days_since_publish: Option<i64>,
}

impl PreviousRun {
    pub fn from_summary(summary: &DepAgeSummary, manifest_path: &str) -> Self {
        let results = summary
            .results
            .iter()
            .map(|r| PreviousResult {
                name: r.name.clone(),
                version_spec: r.version_spec.clone(),
                status: r.status.as_str().to_string(),
                days_since_publish: r.days_since_publish,
            })
            .collect();

        Self {
            manifest_path: manifest_path.to_string(),
            checked_at: summary.checked_at,
            results,
        }
    }

    pub fn load(manifest_dir: &std::path::Path) -> Option<Self> {
        let cache_file = manifest_dir.join(".dep-age-history.json");
        if !cache_file.exists() {
            return None;
        }

        let content = std::fs::read_to_string(&cache_file).ok()?;
        serde_json::from_str(&content).ok()
    }

    pub fn save(&self, manifest_dir: &std::path::Path) -> Result<(), std::io::Error> {
        let cache_file = manifest_dir.join(".dep-age-history.json");
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(cache_file, content)
    }
}

/// Diff result between current and previous run
#[derive(Debug, Clone)]
pub struct DiffResult {
    pub package: String,
    pub change: DiffChange,
    pub previous_status: Option<String>,
    pub current_status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffChange {
    NewlyStale,
    MoreStale,
    Improved,
    New,
    Unchanged,
}

/// Compare current summary with previous run
pub fn compute_diff(current: &DepAgeSummary, previous: &PreviousRun) -> Vec<DiffResult> {
    let mut diffs = Vec::new();
    let prev_map: HashMap<String, &PreviousResult> = previous
        .results
        .iter()
        .map(|r| (r.name.clone(), r))
        .collect();

    for result in &current.results {
        let prev = prev_map.get(&result.name);

        let change = match prev {
            None => DiffChange::New,
            Some(p) => {
                let prev_stale = p.status == "stale" || p.status == "ancient";
                let curr_stale = result.status == Status::Stale || result.status == Status::Ancient;

                if curr_stale && !prev_stale {
                    DiffChange::NewlyStale
                } else if curr_stale && prev_stale {
                    if let (Some(curr_days), Some(prev_days)) =
                        (result.days_since_publish, p.days_since_publish)
                    {
                        if curr_days > prev_days + 30 {
                            DiffChange::MoreStale
                        } else {
                            DiffChange::Unchanged
                        }
                    } else {
                        DiffChange::Unchanged
                    }
                } else if !curr_stale && prev_stale {
                    DiffChange::Improved
                } else {
                    DiffChange::Unchanged
                }
            }
        };

        if change != DiffChange::Unchanged {
            diffs.push(DiffResult {
                package: result.name.clone(),
                change,
                previous_status: prev.map(|p| p.status.clone()),
                current_status: result.status.as_str().to_string(),
            });
        }
    }

    diffs
}

/// Print diff results in human-readable format
pub fn format_diff(diffs: &[DiffResult]) -> String {
    let mut output = String::new();

    if diffs.is_empty() {
        return "No changes since last run.".to_string();
    }

    let newly_stale: Vec<_> = diffs
        .iter()
        .filter(|d| d.change == DiffChange::NewlyStale)
        .collect();
    let improved: Vec<_> = diffs
        .iter()
        .filter(|d| d.change == DiffChange::Improved)
        .collect();
    let new: Vec<_> = diffs
        .iter()
        .filter(|d| d.change == DiffChange::New)
        .collect();
    let more_stale: Vec<_> = diffs
        .iter()
        .filter(|d| d.change == DiffChange::MoreStale)
        .collect();

    if !newly_stale.is_empty() {
        output.push_str("! Newly stale packages:\n");
        for d in &newly_stale {
            output.push_str(&format!(
                "  - {} (was {})\n",
                d.package,
                d.previous_status.as_ref().unwrap_or(&"unknown".to_string())
            ));
        }
    }

    if !more_stale.is_empty() {
        output.push_str("! Packages getting older:\n");
        for d in &more_stale {
            output.push_str(&format!(
                "  - {} (was {})\n",
                d.package,
                d.previous_status.as_ref().unwrap_or(&"unknown".to_string())
            ));
        }
    }

    if !improved.is_empty() {
        output.push_str("✓ Improved packages:\n");
        for d in &improved {
            output.push_str(&format!(
                "  - {} (was {})\n",
                d.package,
                d.previous_status.as_ref().unwrap_or(&"unknown".to_string())
            ));
        }
    }

    if !new.is_empty() {
        output.push_str("+ New packages:\n");
        for d in &new {
            output.push_str(&format!("  - {}\n", d.package));
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DepResult;
    use chrono::Utc;

    #[test]
    fn test_diff_newly_stale() {
        let prev = PreviousRun {
            manifest_path: "Cargo.toml".to_string(),
            checked_at: Utc::now(),
            results: vec![PreviousResult {
                name: "old-crate".to_string(),
                version_spec: "1.0".to_string(),
                status: "fresh".to_string(),
                days_since_publish: Some(30),
            }],
        };

        let curr_result = DepResult {
            name: "old-crate".to_string(),
            version_spec: "1.0".to_string(),
            latest_version: "1.0".to_string(),
            published_at: Some(Utc::now()),
            days_since_publish: Some(400),
            status: Status::Stale,
            registry: crate::Registry::Crates,
        };

        let curr = DepAgeSummary {
            results: vec![curr_result],
            total: 1,
            fresh: 0,
            aging: 0,
            stale: 1,
            ancient: 0,
            errors: 0,
            oldest: None,
            checked_at: Utc::now(),
        };

        let diffs = compute_diff(&curr, &prev);
        assert!(!diffs.is_empty());
        assert_eq!(diffs[0].change, DiffChange::NewlyStale);
    }
}
