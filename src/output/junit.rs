//! JUnit XML output formatter

use crate::{DepAgeSummary, Status};
use quick_xml::events::{BytesEnd, BytesStart, Event};
use quick_xml::Writer;

pub fn format_junit(summary: &DepAgeSummary) -> Result<String, Box<dyn std::error::Error>> {
    let mut writer = Writer::new_with_indent(Vec::new(), b' ', 4);

    let mut testsuite = BytesStart::new("testsuite");
    testsuite.push_attribute(("name", "dep-age"));
    testsuite.push_attribute(("tests", format!("{}", summary.total).as_str()));
    testsuite.push_attribute((
        "failures",
        format!("{}", summary.stale + summary.ancient).as_str(),
    ));
    testsuite.push_attribute(("errors", format!("{}", summary.errors).as_str()));
    testsuite.push_attribute(("time", "0.0"));
    writer.write_event(Event::Start(testsuite))?;

    for result in &summary.results {
        let (failure_message, failure_type) = match &result.status {
            Status::Ancient => (
                Some(format!(
                    "Package {} is {} days old (ancient)",
                    result.name,
                    result.days_since_publish.unwrap_or(0)
                )),
                "AncientDependency",
            ),
            Status::Stale => (
                Some(format!(
                    "Package {} is {} days old (stale)",
                    result.name,
                    result.days_since_publish.unwrap_or(0)
                )),
                "StaleDependency",
            ),
            Status::Aging => (None, "AgingDependency"),
            Status::Fresh => continue,
            Status::Error(e) => (Some(e.clone()), "Error"),
        };

        let mut testcase = BytesStart::new("testcase");
        testcase.push_attribute(("classname", "dep-age"));
        testcase.push_attribute((
            "name",
            format!("{}-{}", result.name, result.version_spec).as_str(),
        ));

        if let Some(msg) = failure_message {
            let mut failure = BytesStart::new("failure");
            failure.push_attribute(("message", msg.as_str()));
            failure.push_attribute(("type", failure_type));
            writer.write_event(Event::Start(testcase))?;
            writer.write_event(Event::Start(failure))?;
            writer.write_event(Event::End(BytesEnd::new("failure")))?;
            writer.write_event(Event::End(BytesEnd::new("testcase")))?;
        } else {
            writer.write_event(Event::Start(testcase))?;
            writer.write_event(Event::End(BytesEnd::new("testcase")))?;
        }
    }

    writer.write_event(Event::End(BytesEnd::new("testsuite")))?;

    let result = writer.into_inner();
    Ok(String::from_utf8(result)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DateTime, DepResult, Registry};
    use chrono::Utc;

    #[test]
    fn test_format_junit() {
        let now: DateTime<Utc> = Utc::now();
        let summary = DepAgeSummary {
            results: vec![
                DepResult {
                    name: "time".to_string(),
                    version_spec: "0.1".to_string(),
                    latest_version: "0.3".to_string(),
                    published_at: Some(now),
                    days_since_publish: Some(1168),
                    status: Status::Ancient,
                    registry: Registry::Crates,
                },
                DepResult {
                    name: "serde".to_string(),
                    version_spec: "1.0".to_string(),
                    latest_version: "1.0".to_string(),
                    published_at: Some(now),
                    days_since_publish: Some(30),
                    status: Status::Fresh,
                    registry: Registry::Crates,
                },
            ],
            total: 2,
            fresh: 1,
            aging: 0,
            stale: 0,
            ancient: 1,
            errors: 0,
            oldest: None,
            checked_at: now,
        };

        let output = format_junit(&summary).unwrap();
        assert!(output.contains("<testsuite"));
        assert!(output.contains("failures=\"1\""));
    }
}
