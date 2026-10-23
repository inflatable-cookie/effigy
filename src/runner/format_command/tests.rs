use std::fs;
use std::path::{Path, PathBuf};

use super::{format_files, resolve_rust_files};

/// Create a throwaway directory unique to this test run.
fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "effigy-fmt-{name}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn write(dir: &Path, relative: &str, contents: &str) -> PathBuf {
    let path = dir.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("parent dir");
    }
    fs::write(&path, contents).expect("write fixture");
    path
}

/// Formatting one file must leave every other file alone, including a child
/// module that rustfmt would otherwise follow.
#[test]
fn fmt_rust_formats_only_the_named_files() {
    let root = temp_dir("named-files");
    let named = write(
        &root,
        "src/named.rs",
        "pub fn named( )->u32{1}\nmod child;\n",
    );
    let child = write(&root, "src/child.rs", "pub fn child( )->u32{2}\n");
    let unrelated = write(&root, "crates/other/src/lib.rs", "pub fn other( )->u32{3}\n");

    let report = format_files(&root, std::slice::from_ref(&named), false).expect("format");
    assert!(report.contains("src/named.rs"), "{report}");

    assert_eq!(
        fs::read_to_string(&named).expect("named"),
        "pub fn named() -> u32 {\n    1\n}\nmod child;\n"
    );
    assert_eq!(
        fs::read_to_string(&child).expect("child"),
        "pub fn child( )->u32{2}\n",
        "a child module must not be followed"
    );
    assert_eq!(
        fs::read_to_string(&unrelated).expect("unrelated"),
        "pub fn other( )->u32{3}\n",
        "an unrelated crate must stay untouched"
    );

    fs::remove_dir_all(&root).ok();
}

/// The file's own `rustfmt.toml` wins over the repository root's, because
/// rustfmt runs from the file's directory.
#[test]
fn fmt_rust_uses_the_nearest_rustfmt_config() {
    let root = temp_dir("nearest-config");
    write(&root, "rustfmt.toml", "max_width = 100\n");
    write(&root, "consumer/rustfmt.toml", "max_width = 40\n");
    let wide = write(&root, "wide.rs", "pub fn wide() -> u32 { 1 }\n");
    let narrow = write(
        &root,
        "consumer/narrow.rs",
        "pub fn narrow() -> u32 { \"aaaaaaaaaa\".len() as u32 + \"bbbbbbbbbb\".len() as u32 }\n",
    );

    format_files(&root, &[wide.clone(), narrow.clone()], false).expect("format");

    let narrow_source = fs::read_to_string(&narrow).expect("narrow");
    let widest_line = narrow_source
        .lines()
        .map(str::chars)
        .map(Iterator::count)
        .max()
        .expect("lines");
    assert!(
        widest_line <= 40,
        "the consumer's own rustfmt.toml (max_width 40) must apply: {narrow_source}"
    );
    assert_eq!(
        fs::read_to_string(&wide).expect("wide"),
        "pub fn wide() -> u32 {\n    1\n}\n"
    );

    fs::remove_dir_all(&root).ok();
}

#[test]
fn fmt_rust_check_reports_without_writing() {
    let root = temp_dir("check");
    let unformatted = write(&root, "lib.rs", "pub fn a( )->u32{1}\n");

    let error = format_files(&root, std::slice::from_ref(&unformatted), true)
        .expect_err("an unformatted file must fail the check");
    assert!(
        error.to_string().contains("needs formatting: lib.rs"),
        "{error}"
    );
    assert_eq!(
        fs::read_to_string(&unformatted).expect("lib"),
        "pub fn a( )->u32{1}\n",
        "check mode writes nothing"
    );

    format_files(&root, &[unformatted], false).expect("format");
    let report = format_files(&root, &[root.join("lib.rs")], true).expect("clean check");
    assert!(report.contains("Rust formatting clean"), "{report}");

    fs::remove_dir_all(&root).ok();
}

#[test]
fn fmt_rust_refuses_files_outside_the_repository() {
    let root = temp_dir("outside-root");
    let outside = temp_dir("outside-file");
    let linked = write(&outside, "linked.rs", "pub fn linked( )->u32{1}\n");

    let error = format_files(&root, std::slice::from_ref(&linked), false)
        .expect_err("a path outside the repository must be refused");
    assert!(error.to_string().contains("stays inside"), "{error}");
    assert_eq!(
        fs::read_to_string(&linked).expect("linked"),
        "pub fn linked( )->u32{1}\n"
    );

    fs::remove_dir_all(&root).ok();
    fs::remove_dir_all(&outside).ok();
}

#[test]
fn fmt_rust_refuses_non_rust_and_missing_files() {
    let root = temp_dir("refusals");
    let markdown = write(&root, "notes.md", "# Notes\n");

    let error = format_files(&root, std::slice::from_ref(&markdown), false)
        .expect_err("a non-Rust file must be refused");
    assert!(error.to_string().contains("only formats Rust"), "{error}");

    let error = format_files(&root, &[root.join("missing.rs")], false)
        .expect_err("a missing file must be refused");
    assert!(error.to_string().contains("cannot read"), "{error}");

    let canonical_root = fs::canonicalize(&root).expect("canonical root");
    let error = resolve_rust_files(&canonical_root, &[]).expect_err("no files");
    assert!(error.to_string().contains("at least one file"), "{error}");

    fs::remove_dir_all(&root).ok();
}
