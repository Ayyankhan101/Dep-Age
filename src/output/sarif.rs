//! SARIF (Static Analysis Results Interchange Format) output formatter
//! Used by GitHub Advanced Security

use crate::{DepAgeSummary, DepResult, Registry, Status};
use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};
use quick_xml::Writer;

pub fn format_sarif(summary: &DepAgeSummary) -> Result<String, Box<dyn std::error::Error>> {
    let mut writer = Writer::new_with_indent(Vec::new(), b' ', 2);

    writer.write_event(Event::Decl(quick_xml::events::BytesDecl::new(
        "1.0",
        Some("utf-8"),
        None,
    )))?;

    let mut sarif = BytesStart::new("sarif");
    sarif.push_attribute(("version", "2.1.0"));
    sarif.push_attribute(("xmlns", "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json"));
    writer.write_event(Event::Start(sarif))?;

    writer.write_event(Event::Start(BytesStart::new("runs")))?;

    writer.write_event(Event::Start(BytesStart::new("run")))?;

    writer.write_event(Event::Start(BytesStart::new("tool")))?;
    writer.write_event(Event::Start(BytesStart::new("driver")))?;
    writer.write_event(Event::Start(BytesStart::new("name")))?;
    writer.write_event(Event::Text(BytesText::new("dep-age")))?;
    writer.write_event(Event::End(BytesEnd::new("name")))?;
    writer.write_event(Event::Start(BytesStart::new("version")))?;
    writer.write_event(Event::Text(BytesText::new(env!("CARGO_PKG_VERSION"))))?;
    writer.write_event(Event::End(BytesEnd::new("version")))?;
    writer.write_event(Event::End(BytesEnd::new("driver")))?;
    writer.write_event(Event::End(BytesEnd::new("tool")))?;

    writer.write_event(Event::Start(BytesStart::new("results")))?;

    for result in &summary.results {
        let (level, message, rule_id) = match &result.status {
            Status::Ancient => ("error", format_ancient_message(result), "DEP001"),
            Status::Stale => ("warning", format_stale_message(result), "DEP002"),
            Status::Aging => ("note", format_aging_message(result), "DEP003"),
            Status::Fresh => continue,
            Status::Error(e) => ("error", format_error_message(result, e), "DEP000"),
        };

        let mut result_elem = BytesStart::new("result");
        result_elem.push_attribute(("ruleId", rule_id));
        result_elem.push_attribute(("level", level));
        writer.write_event(Event::Start(result_elem))?;

        writer.write_event(Event::Start(BytesStart::new("message")))?;
        writer.write_event(Event::Text(BytesText::new(&message)))?;
        writer.write_event(Event::End(BytesEnd::new("message")))?;

        writer.write_event(Event::Start(BytesStart::new("locations")))?;
        writer.write_event(Event::Start(BytesStart::new("location")))?;
        writer.write_event(Event::Start(BytesStart::new("physicalLocation")))?;

        writer.write_event(Event::Start(BytesStart::new("artifactLocation")))?;
        writer.write_event(Event::Start(BytesStart::new("uri")))?;
        writer.write_event(Event::Text(BytesText::new(&registry_url(result))))?;
        writer.write_event(Event::End(BytesEnd::new("uri")))?;
        writer.write_event(Event::End(BytesEnd::new("artifactLocation")))?;

        writer.write_event(Event::End(BytesEnd::new("physicalLocation")))?;
        writer.write_event(Event::End(BytesEnd::new("location")))?;
        writer.write_event(Event::End(BytesEnd::new("locations")))?;

        writer.write_event(Event::End(BytesEnd::new("result")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("results")))?;
    writer.write_event(Event::End(BytesEnd::new("run")))?;
    writer.write_event(Event::End(BytesEnd::new("runs")))?;
    writer.write_event(Event::End(BytesEnd::new("sarif")))?;

    let result = writer.into_inner();
    Ok(String::from_utf8(result)?)
}

fn format_ancient_message(r: &DepResult) -> String {
    let days = r.days_since_publish.unwrap_or(0);
    format!(
        "Package '{}' ({}) has not been updated in {} days ({} years) - marked as ancient",
        r.name,
        r.version_spec,
        days,
        format_years(days)
    )
}

fn format_stale_message(r: &DepResult) -> String {
    let days = r.days_since_publish.unwrap_or(0);
    format!(
        "Package '{}' ({}) has not been updated in {} days - marked as stale",
        r.name, r.version_spec, days
    )
}

fn format_aging_message(r: &DepResult) -> String {
    let days = r.days_since_publish.unwrap_or(0);
    format!(
        "Package '{}' ({}) is {} days old - marked as aging",
        r.name, r.version_spec, days
    )
}

fn format_error_message(r: &DepResult, error: &str) -> String {
    format!(
        "Failed to check '{}' ({}) from {}: {}",
        r.name,
        r.version_spec,
        match r.registry {
            Registry::Crates => "crates.io",
            Registry::Npm => "npm",
            Registry::PyPI => "PyPI",
            Registry::Go => "go",
            Registry::Docker => "docker",
        },
        error
    )
}

fn format_years(days: i64) -> String {
    let years = days as f64 / 365.0;
    format!("{:.1}", years)
}

fn registry_url(r: &DepResult) -> String {
    match r.registry {
        Registry::Crates => format!("https://crates.io/crates/{}", r.name),
        Registry::Npm => format!("https://www.npmjs.com/package/{}", r.name),
        Registry::PyPI => format!("https://pypi.org/project/{}", r.name),
        Registry::Go => format!("https://pkg.go.dev/{}", r.name),
        Registry::Docker => format!("https://hub.docker.com/r/{}", r.name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DateTime;
    use chrono::Utc;

    #[test]
    fn test_format_sarif() {
        let now: DateTime<Utc> = Utc::now();
        let summary = DepAgeSummary {
            results: vec![DepResult {
                name: "time".to_string(),
                version_spec: "0.1".to_string(),
                latest_version: "0.3".to_string(),
                published_at: Some(now),
                days_since_publish: Some(1168),
                status: Status::Ancient,
                registry: Registry::Crates,
            }],
            total: 1,
            fresh: 0,
            aging: 0,
            stale: 0,
            ancient: 1,
            errors: 0,
            oldest: None,
            checked_at: now,
        };

        let output = format_sarif(&summary).unwrap();
        assert!(output.contains("sarif"));
        assert!(output.contains("version"));
        assert!(output.contains("DEP001"));
    }
}
