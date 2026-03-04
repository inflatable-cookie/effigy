pub(super) use super::super::prelude::*;

pub(super) fn assert_task_output_equals(root: &Path, task: &str, marker: &Path, expected: &str) {
    assert_eq!(run_builtin_ok(root.to_path_buf(), task, &[]), "");
    let body = fs::read_to_string(marker).expect("read marker");
    assert_eq!(body, expected);
}

pub(super) fn expected_cargo_paths(root: &Path) -> String {
    let canonical_root = fs::canonicalize(root).expect("canonicalize root");
    format!(
        "{}/.cargo/home|{}/.cargo/target",
        canonical_root.display(),
        canonical_root.display()
    )
}
