use super::prelude::{
    assert_doctor_non_zero_contains, assert_file_text_contains_all, assert_output_contains_all,
    assert_output_excludes_all, fs, run_doctor_err_from_cwd, run_doctor_task, temp_workspace,
    write_manifest,
};

fn write_duplicate_block_file(path: &std::path::Path, block_prefix: &str, body_lines: usize) {
    let mut lines = vec![format!("pub fn {block_prefix}_alpha() -> usize {{")];
    lines.push("    let seed = 1;".to_owned());
    for idx in 0..body_lines {
        lines.push(format!("    let acc_{idx} = seed + {idx};"));
    }
    lines.push(format!("    acc_{}", body_lines.saturating_sub(1)));
    lines.push("}".to_owned());
    fs::write(path, format!("{}\n", lines.join("\n"))).expect("write duplicate block file");
}

fn write_comment_ratio_file(path: &std::path::Path, comment_lines: usize, code_lines: usize) {
    let mut lines = (0..comment_lines)
        .map(|idx| format!("// commentary line {idx}"))
        .collect::<Vec<String>>();
    lines.extend((0..code_lines).map(|idx| format!("const line_{idx} = {idx};")));
    fs::write(path, format!("{}\n", lines.join("\n"))).expect("write comment ratio file");
}

mod core;
mod duplicate_and_comment;
mod markers;
mod size_and_generated;
