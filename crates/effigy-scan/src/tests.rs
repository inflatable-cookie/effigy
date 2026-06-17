use super::model::{
    GodFileScanOptions, GodFileSeverity, GodFileThresholds, StaleSuppressionCategory,
    StaleSuppressionFinding, StaleSuppressionPatterns, StaleSuppressionScanResult,
    StaleSuppressionSeverity, TextRenderOptions,
};
use super::render::render_stale_suppression_text;
use super::support::{
    attention_marker_matches_line, comment_ratio_counts, count_code_lines, is_generated_artifact,
    normalize_rel_path, should_skip_generated_asset_path, stale_suppression_matches_line,
};
use std::path::{Path, PathBuf};

#[test]
fn god_file_thresholds_validate_ordering() {
    let options = GodFileScanOptions {
        thresholds: GodFileThresholds {
            warn: 300,
            high: 250,
            critical: 700,
        },
        ..GodFileScanOptions::default()
    };
    let err = options
        .validate()
        .expect_err("unordered thresholds should fail");
    assert!(err.to_string().contains("warn <= high <= critical"));
}

#[test]
fn count_code_lines_skips_comment_only_lines_for_slash_style_languages() {
    let path = Path::new("src/app.ts");
    let content = "// comment\nconst ok = true;\n/* block */\nfunction run() {\n  return ok;\n}\n";
    assert_eq!(count_code_lines(path, content), 4);
}

#[test]
fn count_code_lines_skips_comment_only_lines_for_hash_style_languages() {
    let path = Path::new("script.py");
    let content = "# comment\n\nvalue = 1\n# more\nprint(value)\n";
    assert_eq!(count_code_lines(path, content), 2);
}

#[test]
fn count_code_lines_ignores_comment_tokens_inside_string_literals() {
    let path = Path::new("src/help.rs");
    let content = concat!(
        "let source_root = \"packages/*/src/**\";\n",
        "let url = \"https://example.test/docs\";\n",
        "/* real comment */\n",
        "let marker = \"// still code\";\n",
    );

    assert_eq!(count_code_lines(path, content), 3);
}

#[test]
fn comment_ratio_counts_ignore_comment_tokens_inside_string_literals() {
    let path = Path::new("src/constants.rs");
    let content = concat!(
        "let source_root = \"packages/*/src/**\";\n",
        "let url = \"https://example.test/docs\";\n",
        "let marker = \"// still code\";\n",
        "// real comment\n",
        "/* another real comment */\n",
    );

    let counts = comment_ratio_counts(path, content);
    assert_eq!(counts.code_lines, 3);
    assert_eq!(counts.comment_lines, 2);
}

#[test]
fn generated_artifact_detection_uses_markers_and_minified_names() {
    assert!(is_generated_artifact(
        Path::new("src/generated.ts"),
        "/* @generated */\nexport const ok = true;\n"
    ));
    assert!(is_generated_artifact(Path::new("app.min.js"), "const a=1;"));
    assert!(!is_generated_artifact(
        Path::new("src/markers.rs"),
        "const marker = \"@generated\";\n"
    ));
    assert!(!is_generated_artifact(
        Path::new("src/app.rs"),
        "fn main() {\n    println!(\"ok\");\n}\n"
    ));
}

#[test]
fn generated_asset_scan_skips_effigy_internal_state() {
    assert!(should_skip_generated_asset_path(
        Path::new(".effigy/cache/catalog-discovery-v1.json"),
        ".effigy/cache/catalog-discovery-v1.json",
        None,
        None,
    ));
}

#[test]
fn normalize_rel_path_uses_forward_slashes() {
    let path = PathBuf::from("src/demo.rs");
    assert_eq!(normalize_rel_path(&path), "src/demo.rs");
}

#[test]
fn severity_serialization_contract_is_stable() {
    let rendered = serde_json::to_string(&GodFileSeverity::Critical).expect("serialize");
    assert_eq!(rendered, "\"critical\"");
}

#[test]
fn stale_suppression_text_render_moves_snippet_to_indented_next_line() {
    let result = StaleSuppressionScanResult {
        root: ".".to_owned(),
        scanned_files: 1,
        matched_lines: 1,
        patterns: StaleSuppressionPatterns {
            warning: vec!["eslint-disable-next-line".to_owned()],
            high: vec!["#[allow(".to_owned()],
            critical: vec!["eslint-disable".to_owned()],
        },
        findings: vec![StaleSuppressionFinding {
            path: "src/app.ts".to_owned(),
            line: 1,
            category: StaleSuppressionCategory::LintDisable,
            severity: StaleSuppressionSeverity::Warning,
            marker: "eslint-disable-next-line".to_owned(),
            snippet: "// eslint-disable-next-line no-console".to_owned(),
        }],
    };

    let rendered = render_stale_suppression_text(
        &result,
        TextRenderOptions {
            show_warnings: true,
            color_enabled: true,
        },
    );

    assert!(rendered.contains("[eslint-disable-next-line]\n"));
    assert!(rendered.contains("    // eslint-disable-next-line no-console"));
    assert!(rendered.contains("\u{1b}["));
}

#[test]
fn stale_suppression_matching_ignores_markers_inside_strings() {
    assert!(!stale_suppression_matches_line(
        r#"let message = "eslint-disable";"#,
        "eslint-disable"
    ));
    assert!(!stale_suppression_matches_line(
        r##"let message = r#"eslint-disable"#;"##,
        "eslint-disable"
    ));
    assert!(!stale_suppression_matches_line(
        r#"const marker = "type: ignore[assignment]";"#,
        "type: ignore"
    ));
    assert!(!stale_suppression_matches_line(
        r##"let marker = br#"shellcheck disable=SC2086"#;"##,
        "shellcheck disable="
    ));
    assert!(!stale_suppression_matches_line(
        r#"const marker = `shellcheck disable=SC2086`;"#,
        "shellcheck disable="
    ));
    assert!(stale_suppression_matches_line(
        r#"const marker = "eslint-disable"; // eslint-disable-next-line no-console"#,
        "eslint-disable-next-line"
    ));
}

#[test]
fn attention_marker_matching_ignores_markers_inside_strings() {
    assert!(!attention_marker_matches_line(
        r#"let value = "@deprecated";"#,
        "@deprecated"
    ));
    assert!(!attention_marker_matches_line(
        r#"let value = "TODO";"#,
        "todo"
    ));
    assert!(attention_marker_matches_line(
        r#"let value = "@deprecated"; // TODO: revisit"#,
        "todo"
    ));
    assert!(attention_marker_matches_line(
        r#"#[deprecated(note = "use new_api")]"#,
        "deprecated"
    ));
}

#[test]
fn attention_marker_matching_ignores_plain_prose_category_words() {
    assert!(!attention_marker_matches_line(
        r#"//! Categories are ordered: Breaking first, Security last."#,
        "security"
    ));
    assert!(!attention_marker_matches_line(
        r#"/// - Prior bug fix"#,
        "bug"
    ));
    assert!(!attention_marker_matches_line(
        r#"enum CategoryKind { Deprecated, Removed, Fixed }"#,
        "deprecated"
    ));
}
