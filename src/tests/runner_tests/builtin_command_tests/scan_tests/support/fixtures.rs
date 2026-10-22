use crate::runner::tests::prelude::{fs, Path};

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
