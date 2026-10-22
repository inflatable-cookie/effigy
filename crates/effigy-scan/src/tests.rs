use std::path::{Path, PathBuf};

use super::model::{
    DuplicateBlockFinding, GodFileScanOptions, GodFileSeverity, GodFileThresholds,
    GodFileThresholdsSource, TextRenderOptions,
};
use super::render::render_god_file_text;
use super::support::{
    count_code_lines, is_generated_artifact, is_literal_only_block, normalize_rel_path,
    normalized_code_lines, NormalizedCodeLine,
};
use super::{run_duplicate_block_scan_workspace, run_god_file_scan_workspace};

fn explicit_god_file_options(warn: usize, high: usize, critical: usize) -> GodFileScanOptions {
    GodFileScanOptions {
        thresholds: GodFileThresholds {
            warn,
            high,
            critical,
        },
        explicit_thresholds: true,
        ..GodFileScanOptions::default()
    }
}

fn write_code_file(path: &Path, lines: usize, body: impl Fn(usize) -> String) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("fixture dir");
    }
    let body = (0..lines).map(body).collect::<Vec<String>>().join("\n");
    std::fs::write(path, format!("{body}\n")).expect("fixture file");
}

#[test]
fn god_file_thresholds_validate_ordering() {
    let options = explicit_god_file_options(300, 250, 700);
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
fn normalized_lines_keep_literal_contents_so_data_tables_stay_distinct() {
    let path = Path::new("src/routes.php");
    let first = normalized_code_lines(path, "'home' => '/home',\n'about' => '/about',\n");
    let second = normalized_code_lines(path, "'home' => '/start',\n'about' => '/about-us',\n");

    assert_ne!(
        first.first().map(|line| line.text.as_str()),
        second.first().map(|line| line.text.as_str())
    );
    assert!(first[0].text.contains("/home"));
}

#[test]
fn normalized_lines_still_drop_comments_and_blank_lines() {
    let path = Path::new("src/app.rs");
    let lines = normalized_code_lines(path, "// header\n\nfn run() {} // trailing\n");
    assert_eq!(
        lines,
        vec![NormalizedCodeLine {
            line_number: 3,
            text: "fn run() {}".to_owned(),
        }]
    );
}

#[test]
fn literal_only_blocks_are_not_actionable_duplicates() {
    let table = normalized_code_lines(
        Path::new("src/icons.php"),
        "'home' => '/home',\n'about' => '/about',\n",
    );
    let code = normalized_code_lines(
        Path::new("src/widgets.php"),
        "'home' => widget('home'),\n'about' => widget('about'),\n",
    );

    assert!(is_literal_only_block(&table));
    assert!(!is_literal_only_block(&code));
}

#[test]
fn language_defaults_apply_only_without_an_explicit_threshold() {
    let defaults = GodFileScanOptions::default();
    let rust = defaults.thresholds_for(Path::new("src/lib.rs"));
    let typescript = defaults.thresholds_for(Path::new("src/app.ts"));
    let script = defaults.thresholds_for(Path::new("scripts/release.sh"));

    assert_eq!(
        defaults.thresholds_source(),
        GodFileThresholdsSource::LanguageDefaults
    );
    assert!(rust.warn > typescript.warn);
    assert!(typescript.warn > script.warn);

    let configured = explicit_god_file_options(120, 240, 480);
    assert_eq!(
        configured.thresholds_for(Path::new("src/app.ts")),
        configured.thresholds
    );
    assert_eq!(
        configured.thresholds_source(),
        GodFileThresholdsSource::Configured
    );
}

#[test]
fn generated_artifacts_are_detected_from_headers_paths_and_names() {
    assert!(is_generated_artifact(
        Path::new("src/generated.ts"),
        "/* @generated */\nexport const ok = true;\n"
    ));
    assert!(is_generated_artifact(
        Path::new("packages/bindings/src/lib.rs"),
        "// DERIVED BINDING. Not editable authority.\n// Do not hand-edit.\nfn x() {}\n"
    ));
    assert!(is_generated_artifact(
        Path::new("platform/contracts/domains/widgets/generated/rust/widgets.rs"),
        "// g01.051 widgets proof output. GENERATED FILE - edit the source\n"
    ));
    assert!(is_generated_artifact(Path::new("app.min.js"), "const a=1;"));
    assert!(!is_generated_artifact(
        Path::new("src/markers.rs"),
        "const marker = \"@generated\";\n"
    ));
    assert!(!is_generated_artifact(
        Path::new("src/app.rs"),
        "// The generator writes this module once; see docs.\nfn main() {}\n"
    ));
}

#[test]
fn god_file_scan_skips_generated_vendored_and_ignored_files_then_reports_the_genuine_god_file() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    std::fs::write(root.join(".gitignore"), "ignored/\n").expect("gitignore");

    write_code_file(&root.join("src/huge.rs"), 420, |index| {
        format!("fn generated_{index}() -> usize {{ {index} }}")
    });
    write_code_file(&root.join("src/small.rs"), 12, |index| {
        format!("fn small_{index}() {{}}")
    });
    std::fs::write(
        root.join("src/derived.rs"),
        format!(
            "// DERIVED BINDING. Not editable authority.\n{}\n",
            (0..420)
                .map(|index| format!("fn derived_{index}() -> usize {{ {index} }}"))
                .collect::<Vec<_>>()
                .join("\n")
        ),
    )
    .expect("derived file");
    write_code_file(
        &root.join("platform/contracts/generated/widgets.rs"),
        420,
        |_| "fn generated() {}".to_owned(),
    );
    write_code_file(&root.join("vendor/bundle.js"), 420, |index| {
        format!("function bundled{index}() {{}}")
    });
    write_code_file(&root.join("ignored/huge.rs"), 420, |index| {
        format!("fn ignored_{index}() {{}}")
    });
    write_code_file(&root.join("src/output.generated.ts"), 420, |index| {
        format!("export function generated{index}() {{}}")
    });

    let options = explicit_god_file_options(100, 200, 300);
    let result =
        run_god_file_scan_workspace(root, &[root.to_path_buf()], &options).expect("god file scan");

    assert_eq!(
        result
            .findings
            .iter()
            .map(|finding| finding.path.as_str())
            .collect::<Vec<_>>(),
        vec!["src/huge.rs"]
    );
    assert_eq!(
        result.scanned_files, 2,
        "small.rs and huge.rs stay eligible"
    );
    assert_eq!(
        result.skipped_generated, 2,
        "the derived header and the generated filename are skipped"
    );
    assert_eq!(result.findings[0].severity, GodFileSeverity::Critical);
    assert_eq!(result.findings[0].language, "rs");
}

#[test]
fn duplicate_block_scan_filters_literal_tables_and_generated_files() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    std::fs::create_dir_all(root.join("src/generated")).expect("fixture dirs");

    let table = (0..26)
        .map(|index| format!("\"key_{index}\" => \"value_{index}\","))
        .collect::<Vec<String>>();
    let table_body = format!("[\n{}\n]\n", table.join("\n"));
    std::fs::write(root.join("src/icons.php"), &table_body).expect("fixture");
    std::fs::write(root.join("src/redirects.php"), &table_body).expect("fixture");

    let function = |prefix: &str| {
        let mut lines = vec![format!("pub fn {prefix}_alpha() -> usize {{")];
        lines.push("    let seed = 1;".to_owned());
        for index in 0..18 {
            lines.push(format!("    let acc_{index} = seed + {index};"));
        }
        lines.push("    acc_17".to_owned());
        lines.push("}".to_owned());
        lines.join("\n")
    };
    std::fs::write(
        root.join("src/apply.rs"),
        format!("{}\n", function("shared")),
    )
    .expect("fixture");
    std::fs::write(
        root.join("src/unlink.rs"),
        format!("{}\n", function("shared")),
    )
    .expect("fixture");
    std::fs::write(
        root.join("src/generated/mirror.rs"),
        format!("{}\n", function("shared")),
    )
    .expect("fixture");

    let options = super::model::DuplicateBlockScanOptions::default();
    let result = run_duplicate_block_scan_workspace(root, &[root.to_path_buf()], &options)
        .expect("duplicate block scan");

    let findings: &Vec<DuplicateBlockFinding> = &result.findings;
    assert_eq!(findings.len(), 1, "{findings:#?}");
    assert_eq!(findings[0].occurrences, 2);
    let paths = findings[0]
        .locations
        .iter()
        .map(|location| location.path.as_str())
        .collect::<Vec<_>>();
    assert_eq!(paths, vec!["src/apply.rs", "src/unlink.rs"]);
}

#[test]
fn god_file_text_output_reports_the_language_and_default_thresholds() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    write_code_file(&root.join("src/huge.rs"), 3200, |index| {
        format!("fn line_{index}() -> usize {{ {index} }}")
    });

    let result =
        run_god_file_scan_workspace(root, &[root.to_path_buf()], &GodFileScanOptions::default())
            .expect("god file scan");
    assert_eq!(
        result.thresholds_source,
        GodFileThresholdsSource::LanguageDefaults
    );
    assert_eq!(result.findings[0].severity, GodFileSeverity::Critical);

    let rendered = render_god_file_text(
        &result,
        TextRenderOptions {
            show_warnings: true,
            color_enabled: false,
        },
    );
    assert!(
        rendered.contains("thresholds: language-defaults"),
        "{rendered}"
    );
    assert!(rendered.contains("[rs]  src/huge.rs"), "{rendered}");
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
