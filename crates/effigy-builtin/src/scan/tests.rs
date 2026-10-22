use std::path::PathBuf;

use effigy_cli::TaskInvocation;

use super::request::parse_scan_request;
use effigy_scan::ScanRenderFormat;

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
fn parse_scan_request_refuses_retired_scanners_with_their_replacement() {
    let task = TaskInvocation {
        name: "scan".to_owned(),
        args: Vec::new(),
    };
    for (name, expected) in [
        ("attention-markers", "rg -n 'TODO|FIXME'"),
        ("comment-ratio", "[scan.comment_ratio]"),
        ("generated-assets", "[scan.generated_assets]"),
        ("generated-in-src", "[scan.generated_in_src]"),
        ("stale-suppressions", "[scan.stale_suppressions]"),
        ("boundary-violations", "no replacement"),
        ("dead-code", "[scan.dead_code]"),
        ("validation-gaps", "effigy test --plan"),
    ] {
        let error = parse_scan_request(&task, &[name.to_owned()])
            .expect_err("retired scan subcommand should fail");
        let message = error.to_string();
        assert!(message.contains("removed in v0.15"), "{message}");
        assert!(message.contains(expected), "{message}");
    }
}

#[test]
fn parse_scan_request_refuses_retired_scanner_flags() {
    let task = TaskInvocation {
        name: "scan".to_owned(),
        args: Vec::new(),
    };
    for (flag, value, expected) in [
        ("--min-code-lines", Some("20"), "scan comment-ratio"),
        ("--source-root", Some("src/**"), "scan generated-in-src"),
        ("--warning-marker", Some("TODO"), "rg -n '<pattern>'"),
        ("--high-marker", Some("FIXME"), "rg -n '<pattern>'"),
        ("--critical-marker", Some("BUG"), "rg -n '<pattern>'"),
    ] {
        let mut args = vec!["god-files".to_owned(), flag.to_owned()];
        if let Some(value) = value {
            args.push(value.to_owned());
        }
        let error = parse_scan_request(&task, &args).expect_err("retired flag should fail");
        let message = error.to_string();
        assert!(message.contains("removed in v0.15"), "{message}");
        assert!(message.contains(expected), "{message}");
    }
}

#[test]
fn parse_scan_request_refuses_the_retired_graph_context_flag() {
    let task = TaskInvocation {
        name: "scan".to_owned(),
        args: Vec::new(),
    };
    let error = parse_scan_request(
        &task,
        &["god-files".to_owned(), "--graph-context".to_owned()],
    )
    .expect_err("retired flag should fail");
    assert!(error.to_string().contains("--graph-context"), "{error}");
}
