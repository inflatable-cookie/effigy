use std::path::PathBuf;

use super::super::super::prelude::{fs, temp_workspace, write_manifest, write_root_manifest, Path};

pub(in crate::runner::tests::builtin_command_tests::scan_tests) fn write_large_code_file(
    path: &Path,
    line_count: usize,
) {
    let body = (0..line_count)
        .map(|idx| format!("const line_{idx} = {idx};"))
        .collect::<Vec<String>>()
        .join("\n");
    fs::write(path, format!("{body}\n")).expect("write large code file");
}

pub(in crate::runner::tests::builtin_command_tests::scan_tests) fn write_large_rust_file(
    path: &Path,
    line_count: usize,
) {
    let body = (0..line_count)
        .map(|idx| format!("pub fn line_{idx}() -> usize {{ {idx} }}"))
        .collect::<Vec<String>>()
        .join("\n");
    fs::write(path, format!("{body}\n")).expect("write large rust file");
}

pub(in crate::runner::tests::builtin_command_tests::scan_tests) fn write_asset_file(
    path: &Path,
    size: usize,
) {
    fs::write(path, vec![b'a'; size]).expect("write asset file");
}

pub(in crate::runner::tests::builtin_command_tests::scan_tests) fn write_attention_file(
    path: &Path,
    lines: &[&str],
) {
    fs::write(path, format!("{}\n", lines.join("\n"))).expect("write attention file");
}

pub(in crate::runner::tests::builtin_command_tests::scan_tests) fn write_duplicate_block_file(
    path: &Path,
    block_prefix: &str,
) {
    let mut lines = vec![format!("pub fn {block_prefix}_alpha() -> usize {{")];
    lines.push("    let seed = 1;".to_owned());
    for idx in 0..18 {
        lines.push(format!("    let acc_{idx} = seed + {idx};"));
    }
    lines.push("    acc_17".to_owned());
    lines.push("}".to_owned());
    let block = format!("{}\n", lines.join("\n"));
    fs::write(path, format!("// header comment\n{block}\n")).expect("write duplicate block file");
}

pub(in crate::runner::tests::builtin_command_tests::scan_tests) fn write_comment_ratio_file(
    path: &Path,
    comment_lines: usize,
    code_lines: usize,
) {
    let mut lines = (0..comment_lines)
        .map(|idx| format!("// commentary line {idx}"))
        .collect::<Vec<String>>();
    lines.extend((0..code_lines).map(|idx| format!("const line_{idx} = {idx};")));
    fs::write(path, format!("{}\n", lines.join("\n"))).expect("write comment ratio file");
}

pub(in crate::runner::tests::builtin_command_tests::scan_tests) fn setup_scan_workspace(
    name: &str,
    manifest_text: Option<&str>,
    dirs: &[&str],
) -> PathBuf {
    let root = temp_workspace(name);
    match manifest_text {
        Some(text) => write_manifest(&root.join("effigy.toml"), text),
        None => write_root_manifest(&root, ""),
    }
    for dir in dirs {
        fs::create_dir_all(root.join(dir)).expect("mkdir scan workspace dir");
    }
    root
}

pub(in crate::runner::tests::builtin_command_tests::scan_tests) fn setup_fanout_scan_workspace(
    name: &str,
    child_catalog: &str,
    child_dir: &str,
) -> (PathBuf, PathBuf) {
    let root = temp_workspace(name);
    let child = root.join(child_catalog);
    fs::create_dir_all(child.join(child_dir)).expect("mkdir child scan dir");
    fs::write(root.join(".gitignore"), "*\n!.gitignore\n!effigy.toml\n")
        .expect("write root gitignore");
    write_manifest(&root.join("effigy.toml"), "[catalog]\nalias = \"root\"\n");
    write_manifest(
        &child.join("effigy.toml"),
        &format!("[catalog]\nalias = \"{child_catalog}\"\n"),
    );
    (root, child)
}
