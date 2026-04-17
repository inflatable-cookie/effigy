use std::path::PathBuf;

use effigy_cli::TaskInvocation;

use super::super::super::scan::model::ScanRenderFormat;
use super::request::parse_scan_request;

#[test]
fn parse_scan_request_requires_subcommand() {
    let task = TaskInvocation {
        name: "scan".to_owned(),
        args: Vec::new(),
    };
    let err = parse_scan_request(&task, &[]).expect_err("missing subcommand should fail");
    assert!(err.to_string().contains("scan requires a subcommand"));
}

#[test]
fn parse_scan_request_accepts_god_files_thresholds_and_output_flags() {
    let task = TaskInvocation {
        name: "scan".to_owned(),
        args: Vec::new(),
    };
    let parsed = parse_scan_request(
        &task,
        &[
            "god-files".to_owned(),
            "--threshold".to_owned(),
            "300".to_owned(),
            "--markdown".to_owned(),
            "--show-warnings".to_owned(),
            "--out".to_owned(),
            "reports/god-files.md".to_owned(),
        ],
    )
    .expect("scan request should parse");
    assert_eq!(parsed.warn, Some(300));
    assert_eq!(parsed.format, Some(ScanRenderFormat::Markdown));
    assert!(parsed.show_warnings);
    assert_eq!(
        parsed.out.expect("output path"),
        PathBuf::from("reports/god-files.md")
    );
}

#[test]
fn parse_scan_request_accepts_comment_ratio_thresholds() {
    let task = TaskInvocation {
        name: "scan".to_owned(),
        args: Vec::new(),
    };
    let parsed = parse_scan_request(
        &task,
        &[
            "comment-ratio".to_owned(),
            "--threshold".to_owned(),
            "1.5".to_owned(),
            "--high".to_owned(),
            "2.5".to_owned(),
            "--critical".to_owned(),
            "3.5".to_owned(),
            "--min-code-lines".to_owned(),
            "30".to_owned(),
        ],
    )
    .expect("scan request should parse");
    assert_eq!(parsed.ratio_warn, Some(1.5));
    assert_eq!(parsed.ratio_high, Some(2.5));
    assert_eq!(parsed.ratio_critical, Some(3.5));
    assert_eq!(parsed.min_code_lines, Some(30));
}

#[test]
fn parse_scan_request_accepts_generated_in_src_source_roots() {
    let task = TaskInvocation {
        name: "scan".to_owned(),
        args: Vec::new(),
    };
    let parsed = parse_scan_request(
        &task,
        &[
            "generated-in-src".to_owned(),
            "--threshold".to_owned(),
            "10".to_owned(),
            "--source-root".to_owned(),
            "src/**".to_owned(),
            "--source-root".to_owned(),
            "packages/*/src/**".to_owned(),
        ],
    )
    .expect("scan request should parse");
    assert_eq!(parsed.warn, Some(10));
    assert_eq!(
        parsed.source_roots,
        vec!["src/**".to_owned(), "packages/*/src/**".to_owned()]
    );
}

#[test]
fn parse_scan_request_accepts_stale_suppression_marker_overrides() {
    let task = TaskInvocation {
        name: "scan".to_owned(),
        args: Vec::new(),
    };
    let parsed = parse_scan_request(
        &task,
        &[
            "stale-suppressions".to_owned(),
            "--warning-marker".to_owned(),
            "@ts-ignore".to_owned(),
            "--high-marker".to_owned(),
            "#[allow(".to_owned(),
            "--critical-marker".to_owned(),
            "nolint".to_owned(),
        ],
    )
    .expect("scan request should parse");
    assert_eq!(parsed.warning_markers, vec!["@ts-ignore".to_owned()]);
    assert_eq!(parsed.high_markers, vec!["#[allow(".to_owned()]);
    assert_eq!(parsed.critical_markers, vec!["nolint".to_owned()]);
}
