use std::path::PathBuf;

use crate::TaskInvocation;

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
