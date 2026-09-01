use super::context::refresh_progress_notice;
use crate::contract_test_support::temp_workspace;
use effigy_codegraph::run_index;
use std::fs;

fn docs_fixture(name: &str) -> std::path::PathBuf {
    let root = temp_workspace(name);
    fs::create_dir_all(root.join("docs")).expect("mkdir docs");
    fs::write(
        root.join("docs/README.md"),
        "# Docs Home\n\nThe contracts define the working rules.\n",
    )
    .expect("write docs");
    root
}

#[test]
fn refresh_notice_claims_cold_build_only_when_index_is_missing() {
    let root = docs_fixture("docs-refresh-notice-cold");
    let notice = refresh_progress_notice(&root).expect("cold graph must announce");
    assert!(notice.contains("missing"), "cold notice: {notice}");
    assert!(notice.contains("docs context"), "cold notice: {notice}");
}

#[test]
fn refresh_notice_stays_silent_on_current_graph() {
    let root = docs_fixture("docs-refresh-notice-current");
    run_index(&root).expect("index the fixture");
    assert_eq!(refresh_progress_notice(&root), None);
}

#[test]
fn refresh_notice_claims_stale_rebuild_after_content_change() {
    let root = docs_fixture("docs-refresh-notice-stale");
    run_index(&root).expect("index the fixture");
    assert_eq!(refresh_progress_notice(&root), None, "fresh index is quiet");

    fs::write(
        root.join("docs/README.md"),
        "# Docs Home\n\nThe contracts define the working rules, updated.\n",
    )
    .expect("rewrite docs");
    let notice = refresh_progress_notice(&root).expect("stale graph must announce");
    assert!(notice.contains("stale"), "stale notice: {notice}");
    assert!(notice.contains("docs context"), "stale notice: {notice}");
}
