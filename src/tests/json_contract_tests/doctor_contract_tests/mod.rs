use super::prelude::{execution::*, harness::*, json::*, runtime::*};

fn write_duplicate_block_file(path: &std::path::Path, block_prefix: &str, body_lines: usize) {
    let mut lines = vec![format!("pub fn {block_prefix}_alpha() -> usize {{")];
    lines.push("    let seed = 1;".to_owned());
    for idx in 0..body_lines {
        lines.push(format!("    let acc_{idx} = seed + {idx};"));
    }
    lines.push(format!("    acc_{}", body_lines.saturating_sub(1)));
    lines.push("}".to_owned());
    fs::write(path, format!("{}\n", lines.join("\n"))).expect("write duplicate block file");
}

fn write_comment_ratio_file(path: &std::path::Path, comment_lines: usize, code_lines: usize) {
    let mut lines = (0..comment_lines)
        .map(|idx| format!("// commentary line {idx}"))
        .collect::<Vec<String>>();
    lines.extend((0..code_lines).map(|idx| format!("const line_{idx} = {idx};")));
    fs::write(path, format!("{}\n", lines.join("\n"))).expect("write comment ratio file");
}

fn run_doctor_rendered(root: PathBuf, output_json: bool) -> String {
    match run_doctor(DoctorArgs {
        repo_override: Some(root),
        output_json,
        fix: false,
        verbose: false,
        explain: None,
    }) {
        Ok(rendered) => rendered,
        Err(RunnerError::DoctorNonZero { rendered, .. }) => rendered,
        Err(other) => panic!("unexpected doctor error: {other}"),
    }
}

fn run_doctor_json(root: PathBuf) -> serde_json::Value {
    let out = run_doctor(DoctorArgs {
        repo_override: Some(root),
        output_json: true,
        fix: false,
        verbose: false,
        explain: None,
    })
    .expect("run doctor json");
    parse_json(&out)
}

fn find_section<'a>(parsed: &'a serde_json::Value, check_id: &str) -> &'a serde_json::Value {
    parsed["sections"]
        .as_array()
        .expect("sections array")
        .iter()
        .find(|section| section["check_id"] == check_id)
        .unwrap_or_else(|| panic!("{check_id} section"))
}

fn flattened_scan_findings<'a>(
    parsed: &'a serde_json::Value,
    check_id: &str,
) -> Vec<&'a serde_json::Value> {
    parsed["findings"]
        .as_array()
        .expect("findings array")
        .iter()
        .filter(|finding| finding["check_id"] == check_id)
        .collect::<Vec<&serde_json::Value>>()
}

fn assert_scan_omitted(parsed: &serde_json::Value, check_id: &str) {
    assert!(parsed["sections"]
        .as_array()
        .expect("sections array")
        .iter()
        .all(|section| section["check_id"] != check_id));
    assert!(parsed["findings"]
        .as_array()
        .expect("findings array")
        .iter()
        .all(|finding| finding["check_id"] != check_id));
}

fn assert_scan_findings(
    parsed: &serde_json::Value,
    check_id: &str,
    section_severity: &str,
    expected_findings: &[(&str, &str)],
) {
    let scan_section = find_section(parsed, check_id);
    let section_findings = scan_section["findings"].as_array().expect("section findings");
    assert_eq!(scan_section["severity"], section_severity);
    assert_eq!(section_findings.len(), expected_findings.len());
    for (severity, evidence_snippet) in expected_findings {
        assert!(section_findings.iter().any(|finding| {
            finding["severity"] == *severity
                && finding["evidence"]
                    .as_str()
                    .is_some_and(|evidence| evidence.contains(evidence_snippet))
        }));
    }

    let flattened = flattened_scan_findings(parsed, check_id);
    assert_eq!(flattened.len(), expected_findings.len());
}

fn parse_explain_prefix_rows(rendered: &str) -> Vec<(String, String)> {
    let (prefix_block, _) = rendered
        .split_once("\ncandidate-catalogs:\n")
        .expect("expected candidate-catalogs section");
    prefix_block
        .lines()
        .skip(2)
        .filter_map(|line| {
            let (key, value) = line.split_once(": ")?;
            Some((key.to_owned(), value.to_owned()))
        })
        .collect::<Vec<(String, String)>>()
}

fn explain_row_value<'a>(rows: &'a [(String, String)], key: &str) -> &'a str {
    rows.iter()
        .find(|(candidate, _)| candidate == key)
        .map(|(_, value)| value.as_str())
        .unwrap_or_else(|| panic!("missing explain row `{key}`"))
}

mod core;
mod duplicate_and_comment;
mod explain;
mod markers;
mod size_and_generated;
