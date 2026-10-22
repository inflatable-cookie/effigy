use crate::runner::json_contract_tests::prelude::{execution::*, harness::*, json::*, runtime::*};

fn write_large_code_file(path: &std::path::Path, line_count: usize) {
    let body = (0..line_count)
        .map(|idx| format!("const line_{idx} = {idx};"))
        .collect::<Vec<String>>()
        .join("\n");
    fs::write(path, format!("{body}\n")).expect("write code file");
}

fn write_duplicate_block_file(path: &std::path::Path, block_prefix: &str) {
    let mut lines = vec![format!("pub fn {block_prefix}_alpha() -> usize {{")];
    lines.push("    let seed = 1;".to_owned());
    for idx in 0..18 {
        lines.push(format!("    let acc_{idx} = seed + {idx};"));
    }
    lines.push("    acc_17".to_owned());
    lines.push("}".to_owned());
    let body = lines.join("\n");
    fs::write(path, format!("{body}\n")).expect("write duplicate block file");
}

fn assert_base_scan_payload(parsed: &serde_json::Value, schema: &str, scan: &str, heading: &str) {
    assert_schema_v1(parsed, schema);
    assert_eq!(parsed["scan"], scan);
    assert_eq!(parsed["format"], "text");
    assert_eq!(parsed["finding_count"], 1);
    assert_eq!(parsed["fail_on_findings"], false);
    assert_eq!(parsed["respect_gitignore"], true);
    assert!(parsed["findings"].is_array());
    assert!(parsed["text"]
        .as_str()
        .is_some_and(|text| text.contains(heading)));
}

fn run_non_zero_scan_json(root: PathBuf, args: &[&str]) -> serde_json::Value {
    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "scan".to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        },
        root,
    )
    .expect_err("expected non-zero scan result");

    let rendered = match err {
        RunnerError::BuiltinScanNonZero { rendered, .. } => rendered,
        other => panic!("unexpected error: {other}"),
    };
    parse_json(&rendered)
}

fn assert_non_zero_scan_payload(parsed: &serde_json::Value, schema: &str) {
    assert_schema_v1(parsed, schema);
    assert_eq!(parsed["finding_count"], 1);
    assert_eq!(parsed["fail_on_findings"], true);
}

mod duplicate_blocks;
mod god_files;
