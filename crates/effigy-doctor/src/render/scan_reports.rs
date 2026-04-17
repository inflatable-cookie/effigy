use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use super::contracts::DoctorFindingSection;
use crate::contracts::check_id;
use crate::DoctorError;
use crate::DoctorSeverity;

pub(super) fn sync_scan_detail_reports(
    resolved_root: &str,
    sections: &[DoctorFindingSection],
) -> Result<HashMap<String, String>, DoctorError> {
    let root = Path::new(resolved_root);
    let report_dir = root.join(".effigy/reports/doctor");
    let active_scan_sections = sections
        .iter()
        .filter_map(|section| {
            scan_report_file_name(&section.check_id).map(|file_name| (section, file_name))
        })
        .collect::<Vec<(&DoctorFindingSection, &'static str)>>();
    let active_file_names = active_scan_sections
        .iter()
        .map(|(_, file_name)| *file_name)
        .collect::<HashSet<&'static str>>();

    for stale_file_name in known_scan_report_file_names() {
        if active_file_names.contains(stale_file_name) {
            continue;
        }
        let stale_path = report_dir.join(stale_file_name);
        if stale_path.exists() {
            fs::remove_file(&stale_path)
                .map_err(|error| DoctorError::task_invocation_failed_write(&stale_path, error))?;
        }
    }

    if active_scan_sections.is_empty() {
        return Ok(HashMap::new());
    }

    fs::create_dir_all(&report_dir)
        .map_err(|error| DoctorError::task_invocation_failed_write(&report_dir, error))?;

    let mut paths = HashMap::<String, String>::new();
    for (section, file_name) in active_scan_sections {
        let report_path = report_dir.join(file_name);
        fs::write(&report_path, render_scan_detail_report(section))
            .map_err(|error| DoctorError::task_invocation_failed_write(&report_path, error))?;
        paths.insert(
            section.check_id.clone(),
            display_scan_report_path(root, &report_path),
        );
    }
    Ok(paths)
}

pub(super) fn scan_finding_counts(section: &DoctorFindingSection) -> (usize, usize) {
    section
        .findings
        .iter()
        .fold((0usize, 0usize), |(warning, error), item| {
            match item.severity {
                DoctorSeverity::Warning => (warning + 1, error),
                DoctorSeverity::Error => (warning, error + 1),
                DoctorSeverity::Info => (warning, error),
            }
        })
}

fn render_scan_detail_report(section: &DoctorFindingSection) -> String {
    let (warning_count, error_count) = scan_finding_counts(section);
    let mut out = String::new();
    out.push_str(&format!("# Doctor Scan Details: {}\n\n", section.check_id));
    out.push_str(&format!("- Severity: `{}`\n", section.severity.as_str()));
    out.push_str(&format!("- Findings: `{}`\n", section.findings.len()));
    out.push_str(&format!("- Warning findings: `{}`\n", warning_count));
    out.push_str(&format!("- Error findings: `{}`\n", error_count));
    out.push('\n');
    out.push_str("## Findings\n\n");
    for item in &section.findings {
        out.push_str(&format!(
            "- `{}` `{}`\n",
            item.severity.as_str(),
            item.evidence.replace('`', "'"),
        ));
    }
    out
}

fn display_scan_report_path(root: &Path, report_path: &Path) -> String {
    report_path
        .strip_prefix(root)
        .unwrap_or(report_path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn scan_report_file_name(check_id: &str) -> Option<&'static str> {
    match check_id {
        check_id::SCAN_GOD_FILES => Some("scan-god-files.md"),
        check_id::SCAN_DUPLICATE_BLOCKS => Some("scan-duplicate-blocks.md"),
        check_id::SCAN_COMMENT_RATIO => Some("scan-comment-ratio.md"),
        check_id::SCAN_GENERATED_ASSETS => Some("scan-generated-assets.md"),
        check_id::SCAN_GENERATED_IN_SRC => Some("scan-generated-in-src.md"),
        check_id::SCAN_ATTENTION_MARKERS => Some("scan-attention-markers.md"),
        check_id::SCAN_STALE_SUPPRESSIONS => Some("scan-stale-suppressions.md"),
        _ => None,
    }
}

fn known_scan_report_file_names() -> &'static [&'static str] {
    &[
        "scan-god-files.md",
        "scan-duplicate-blocks.md",
        "scan-comment-ratio.md",
        "scan-generated-assets.md",
        "scan-generated-in-src.md",
        "scan-attention-markers.md",
        "scan-stale-suppressions.md",
    ]
}
