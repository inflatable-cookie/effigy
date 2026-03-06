use super::harness_assertions::assert_contains_all;
use super::runtime::{fs, Path};

pub(in crate::runner::tests) fn assert_output_contains_all(output: &str, expected: &[&str]) {
    assert_contains_all(output, expected);
}

pub(in crate::runner::tests) fn assert_output_contains_any(output: &str, expected_any: &[&str]) {
    assert!(
        expected_any.iter().any(|needle| output.contains(needle)),
        "expected output to contain one of {expected_any:?}, got:\n{output}"
    );
}

pub(in crate::runner::tests) fn assert_output_excludes_all(output: &str, excluded: &[&str]) {
    for needle in excluded {
        assert!(
            !output.contains(needle),
            "expected output to not contain `{needle}`, got:\n{output}"
        );
    }
}

pub(in crate::runner::tests) fn assert_output_equals(output: &str, expected: &str) {
    assert_eq!(output, expected);
}

pub(in crate::runner::tests) fn assert_output_contains_derived(
    output: &str,
    expected: impl IntoIterator<Item = String>,
) {
    for needle in expected {
        assert!(
            output.contains(&needle),
            "expected output to contain `{needle}`, got:\n{output}"
        );
    }
}

pub(in crate::runner::tests) fn assert_path_exists(path: &Path, label: &str) {
    assert!(
        path.exists(),
        "expected {label} to exist at {}",
        path.display()
    );
}

pub(in crate::runner::tests) fn assert_path_missing(path: &Path, label: &str) {
    assert!(
        !path.exists(),
        "expected {label} to be missing at {}",
        path.display()
    );
}

pub(in crate::runner::tests) fn read_file_text(path: &Path) -> String {
    fs::read_to_string(path).expect("read file")
}

pub(in crate::runner::tests) fn assert_file_text_equals(path: &Path, expected: &str) {
    let body = read_file_text(path);
    assert_eq!(body, expected);
}

pub(in crate::runner::tests) fn assert_file_text_contains_all(path: &Path, expected: &[&str]) {
    let body = read_file_text(path);
    assert_output_contains_all(&body, expected);
}

pub(in crate::runner::tests) fn assert_file_text_excludes_all(path: &Path, excluded: &[&str]) {
    let body = read_file_text(path);
    assert_output_excludes_all(&body, excluded);
}

pub(in crate::runner::tests) fn assert_string_items_contains_all(
    items: &[String],
    expected: &[String],
) {
    for needle in expected {
        assert!(
            items.iter().any(|item| item == needle),
            "expected items to contain `{needle}`, got {items:?}"
        );
    }
}

pub(in crate::runner::tests) fn assert_string_items_excludes_all(
    items: &[String],
    excluded: &[String],
) {
    for needle in excluded {
        assert!(
            !items.iter().any(|item| item == needle),
            "expected items to exclude `{needle}`, got {items:?}"
        );
    }
}
