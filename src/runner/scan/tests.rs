use super::model::{GodFileScanOptions, GodFileSeverity, GodFileThresholds};
use super::support::{count_code_lines, is_generated_artifact, normalize_rel_path};
use std::path::{Path, PathBuf};

#[test]
fn god_file_thresholds_validate_ordering() {
    let mut options = GodFileScanOptions::default();
    options.thresholds = GodFileThresholds {
        warn: 300,
        high: 250,
        critical: 700,
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
fn generated_artifact_detection_uses_markers_and_minified_names() {
    assert!(is_generated_artifact(
        Path::new("src/generated.ts"),
        "/* @generated */\nexport const ok = true;\n"
    ));
    assert!(is_generated_artifact(Path::new("app.min.js"), "const a=1;"));
    assert!(!is_generated_artifact(
        Path::new("src/app.rs"),
        "fn main() {\n    println!(\"ok\");\n}\n"
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
