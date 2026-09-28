use super::{
    check_contains, check_headings, check_paths, check_workflow_paths,
    collect_included_index_markdown_links, collect_index_markdown_links, collect_link_check_files,
    collect_markdown_children, collect_workflow_check_files, extract_fenced_json_blocks,
    extract_h2_section, extract_lead_verb, first_non_empty_section_line, insert_log_index_entry,
    normalize_log_index_relative_path, path_matches_exclude, resolve_docs_index_spec,
    resolve_docs_next_action_spec, scan_markdown_links,
};
use effigy_manifest::config_sections::{
    ManifestDocsPolicyIndexConfig, ManifestDocsPolicyNextActionConfig,
};
use effigy_manifest::ManifestDocsPolicyConfig;
use std::{
    fs,
    path::{Path, PathBuf},
};

struct DocsFixture {
    root: PathBuf,
}

impl DocsFixture {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "effigy-docs-policy-{name}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("mkdir");
        Self { root }
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn mkdir(&self, relative: impl AsRef<Path>) {
        fs::create_dir_all(self.root.join(relative.as_ref())).expect("mkdir");
    }

    fn write(&self, relative: impl AsRef<Path>, contents: &str) {
        let path = self.root.join(relative.as_ref());
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("mkdir parent");
        }
        fs::write(path, contents).expect("write fixture file");
    }
}

#[test]
fn extract_h2_section_returns_requested_section_only() {
    let content = "## One\nalpha\n## Two\nbeta\n## Three\ngamma\n";
    let section = extract_h2_section(content, "Two").expect("section");
    assert_eq!(section, "## Two\nbeta");
}

#[test]
fn extract_h2_section_matches_numbered_heading_without_ordinal() {
    let content =
            "## 8) Bootstrap (`effigy.bootstrap.v1`)\nalpha\n## 19) Completion Candidates (`effigy.completion.candidates.v1`)\nbeta\n";
    let section = extract_h2_section(
        content,
        "Completion Candidates (`effigy.completion.candidates.v1`)",
    )
    .expect("section");
    assert_eq!(
        section,
        "## 19) Completion Candidates (`effigy.completion.candidates.v1`)\nbeta"
    );
}

#[test]
fn extract_fenced_json_blocks_returns_json_blocks_only() {
    let section =
        "## Two\n```json\n{\"ok\":true}\n```\n```txt\nignored\n```\n```json\n{\"ok\":false}\n```\n";
    let blocks = extract_fenced_json_blocks(section);
    assert_eq!(blocks.len(), 2);
    assert!(blocks[0].contains("{\"ok\":true}"));
    assert!(blocks[1].contains("{\"ok\":false}"));
}

#[test]
fn scan_markdown_links_ignores_fenced_code_blocks() {
    let fixture = DocsFixture::new("links");
    fixture.write(
        "README.md",
        "[ok](./existing.md)\n```md\n[skip](./missing.md)\n```\n",
    );
    fixture.write("existing.md", "exists\n");

    let failures = scan_markdown_links(&fixture.root().join("README.md")).expect("scan");
    assert!(failures.is_empty());
}

fn rendered_relative_paths(root: &Path, files: &[PathBuf]) -> Vec<String> {
    files
        .iter()
        .filter_map(|path| path.strip_prefix(root).ok())
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .collect()
}

#[test]
fn collect_link_check_files_defaults_to_full_docs_tree() {
    let fixture = DocsFixture::new("link-defaults");
    fixture.mkdir("docs/notes/2026-03");
    fixture.mkdir("docs/research");
    fixture.write("README.md", "# Root\n");
    fixture.write("docs/README.md", "# Docs\n");
    fixture.write("docs/notes/2026-03/example.md", "# Log\n");
    fixture.write("docs/research/example.md", "# Research\n");

    let files = collect_link_check_files(fixture.root(), &[]);
    let rendered = rendered_relative_paths(fixture.root(), &files);

    assert!(rendered.contains(&"README.md".to_owned()));
    assert!(rendered.contains(&"docs/README.md".to_owned()));
    assert!(rendered.contains(&"docs/notes/2026-03/example.md".to_owned()));
    assert!(rendered.contains(&"docs/research/example.md".to_owned()));
}

#[test]
fn collect_link_check_files_skips_nested_generated_build_trees() {
    let fixture = DocsFixture::new("link-generated");
    fixture.write("README.md", "# Root\n");
    fixture.write("docs/guide.md", "# Guide\n");
    fixture.write(
        "docs/packages/gpui/preview/target/debug/incremental/out.md",
        "# Cargo output\n",
    );
    fixture.write("docs/node_modules/pkg/README.md", "# Package\n");

    let files = collect_link_check_files(fixture.root(), &[]);
    let rendered = rendered_relative_paths(fixture.root(), &files);

    assert!(rendered.contains(&"docs/guide.md".to_owned()));
    assert!(!rendered.iter().any(|path| path.contains("/target/")));
    assert!(!rendered.iter().any(|path| path.contains("/node_modules/")));
}

#[test]
fn collect_link_check_files_ignores_disappearing_generated_entries() {
    let fixture = DocsFixture::new("link-vanish");
    fixture.write("README.md", "# Root\n");
    fixture.write("docs/guide.md", "# Guide\n");
    fixture.write(
        "docs/packages/gpui/preview/target/debug/incremental/out.md",
        "# Cargo output\n",
    );
    let vanish = fixture
        .root()
        .join("docs/packages/gpui/preview/target/debug/rmeta-tmp");
    fs::create_dir_all(&vanish).expect("mkdir vanishing generated dir");

    let vanish_for_thread = vanish.clone();
    let walker = std::thread::spawn({
        let root = fixture.root().to_path_buf();
        move || {
            let mut collected = Vec::new();
            for _ in 0..32 {
                collected.push(collect_link_check_files(&root, &[]));
            }
            collected
        }
    });
    for _ in 0..32 {
        let _ = fs::remove_dir_all(&vanish_for_thread);
        let _ = fs::create_dir_all(&vanish_for_thread);
    }
    let walks = walker.join().expect("walker thread");
    for files in walks {
        let rendered = rendered_relative_paths(fixture.root(), &files);
        assert!(rendered.contains(&"docs/guide.md".to_owned()));
        assert!(!rendered.iter().any(|path| path.contains("/target/")));
    }
}

#[test]
fn collect_link_check_files_keeps_explicit_file_inside_generated_tree() {
    let fixture = DocsFixture::new("link-explicit-target");
    fixture.write("docs/target/notes.md", "# Notes\n");

    let files = collect_link_check_files(fixture.root(), &[PathBuf::from("docs/target/notes.md")]);
    let rendered = rendered_relative_paths(fixture.root(), &files);
    assert_eq!(rendered, vec!["docs/target/notes.md".to_owned()]);
}

#[test]
fn collect_link_check_files_keeps_explicit_missing_owned_file() {
    let fixture = DocsFixture::new("link-missing-owned");
    let files = collect_link_check_files(fixture.root(), &[PathBuf::from("docs/gone.md")]);
    assert_eq!(files, vec![fixture.root().join("docs/gone.md")]);
}

#[test]
fn scan_markdown_links_reports_missing_owned_file() {
    let fixture = DocsFixture::new("missing-owned");
    fixture.write("docs/owned.md", "# Owned\n");
    let path = fixture.root().join("docs/owned.md");
    fs::remove_file(&path).expect("remove owned markdown");

    let failure = scan_markdown_links(&path).expect_err("missing owned file");
    assert_eq!(failure.file, path);
    assert!(!failure.reason.is_empty());
}

#[cfg(unix)]
#[test]
fn scan_markdown_links_reports_unreadable_owned_file() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = DocsFixture::new("unreadable-owned");
    fixture.write("docs/owned.md", "# Owned\n");
    let path = fixture.root().join("docs/owned.md");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).expect("chmod");
    let result = scan_markdown_links(&path);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("restore chmod");
    let failure = result.expect_err("unreadable owned file");
    assert_eq!(failure.file, path);
    assert!(!failure.reason.is_empty());
}

#[test]
fn normalize_log_index_relative_path_accepts_docs_logs_prefix() {
    let normalized =
        normalize_log_index_relative_path(Path::new("docs/notes/2026-03/02-160000-my-log.md"))
            .expect("normalize path");
    assert_eq!(normalized, "2026-03/02-160000-my-log.md");
}

#[test]
fn insert_log_index_entry_puts_new_entry_first_in_active_logs() {
    let index = "# Logs\n\n## Active logs\n\n- [`2026-03/01-000000-old.md`](./2026-03/01-000000-old.md)\n\n## Next move\n- dispatch g10.004\n";
    let entry = "- [`2026-03/02-160000-my-log.md`](./2026-03/02-160000-my-log.md)";
    let updated = insert_log_index_entry(index, entry).expect("insert");
    let active = updated.find("## Active logs").expect("active heading");
    let next = updated.find("## Next move").expect("next heading");
    let new = updated
        .find("2026-03/02-160000-my-log.md")
        .expect("new entry");
    let old = updated.find("2026-03/01-000000-old.md").expect("old entry");
    assert!(active < new, "entry lands inside Active logs");
    assert!(new < next, "entry stays before Next move");
    assert!(new < old, "newest entry stays first");
    assert_eq!(
        updated,
        "# Logs\n\n## Active logs\n\n- [`2026-03/02-160000-my-log.md`](./2026-03/02-160000-my-log.md)\n- [`2026-03/01-000000-old.md`](./2026-03/01-000000-old.md)\n\n## Next move\n- dispatch g10.004\n"
    );
}

#[test]
fn insert_log_index_entry_handles_empty_active_logs_before_next_task() {
    let index = "# Logs\n\n## Active logs\n\n## Next move\n- dispatch g10.004\n";
    let entry = "- [`2026-03/02-160000-my-log.md`](./2026-03/02-160000-my-log.md)";
    let updated = insert_log_index_entry(index, entry).expect("insert");
    assert_eq!(
        updated,
        "# Logs\n\n## Active logs\n\n- [`2026-03/02-160000-my-log.md`](./2026-03/02-160000-my-log.md)\n\n## Next move\n- dispatch g10.004\n"
    );
}

#[test]
fn insert_log_index_entry_rejects_missing_active_logs() {
    let index = "# Logs\n\n## Archived logs\n- older\n\n## Next move\n- dispatch\n";
    let entry = "- [`2026-03/02-160000-my-log.md`](./2026-03/02-160000-my-log.md)";
    assert!(insert_log_index_entry(index, entry).is_err());
}

#[test]
fn insert_log_index_entry_rejects_duplicate_active_logs() {
    let index = "# Logs\n\n## Active logs\n\n- old\n\n## Active logs\n\n- older\n";
    let entry = "- [`2026-03/02-160000-my-log.md`](./2026-03/02-160000-my-log.md)";
    assert!(insert_log_index_entry(index, entry).is_err());
}

#[test]
fn insert_log_index_entry_repeat_run_stays_idempotent() {
    let index = "# Logs\n\n## Active logs\n\n- [`2026-03/01-000000-old.md`](./2026-03/01-000000-old.md)\n\n## Next move\n- dispatch\n";
    let entry = "- [`2026-03/02-160000-my-log.md`](./2026-03/02-160000-my-log.md)";
    // Mirror the runner: skip insertion when the exact bullet already exists.
    let once = insert_log_index_entry(index, entry).expect("first insert");
    let twice = if once.lines().any(|line| line.trim() == entry) {
        once.clone()
    } else {
        insert_log_index_entry(&once, entry).expect("second insert")
    };
    assert_eq!(once, twice);
    assert_eq!(twice.matches("2026-03/02-160000-my-log.md").count(), 2);
}

#[test]
fn collect_workflow_check_files_excludes_logs_for_default_docs_scope() {
    let fixture = DocsFixture::new("workflow-paths");
    fixture.mkdir("docs/notes/2026-03");
    fixture.mkdir("docs/guides");
    fixture.write("docs/guides/example.md", "# Guide\n");
    fixture.write("docs/notes/2026-03/example.md", "# Log\n");

    let files = collect_workflow_check_files(
        &fixture.root().join("docs"),
        &fixture.root().join("docs/notes"),
        true,
    );
    let rendered = files
        .iter()
        .filter_map(|path| path.strip_prefix(fixture.root()).ok())
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .collect::<Vec<_>>();

    assert!(rendered.contains(&"docs/guides/example.md".to_owned()));
    assert!(!rendered.contains(&"docs/notes/2026-03/example.md".to_owned()));
}

#[test]
fn collect_markdown_children_respects_excludes() {
    let fixture = DocsFixture::new("index");
    fixture.mkdir("history");
    fixture.write("README.md", "# Root\n");
    fixture.write("active.md", "# Active\n");
    fixture.write("history/old.md", "# Old\n");

    let files = collect_markdown_children(fixture.root(), &[String::from("history/**")]);
    assert!(files.contains("active.md"));
    assert!(!files.contains("history/old.md"));
}

#[test]
fn collect_markdown_children_skips_generated_trees_but_scans_root_named_target() {
    let fixture = DocsFixture::new("index-target-root");
    fixture.write("target/owned.md", "# Owned\n");
    fixture.write("target/node_modules/pkg/README.md", "# Package\n");
    fixture.write("target/nested/target/debug/out.md", "# Nested cargo\n");

    let files = collect_markdown_children(&fixture.root().join("target"), &[]);
    assert!(files.contains("owned.md"));
    assert!(!files.contains("node_modules/pkg/README.md"));
    assert!(!files.iter().any(|path| path.contains("/target/")));
}

#[test]
fn collect_workflow_check_files_skips_generated_build_trees() {
    let fixture = DocsFixture::new("workflow-generated");
    fixture.write("docs/guides/example.md", "# Guide\n");
    fixture.write("docs/target/debug/out.md", "# Cargo\n");
    fixture.write("docs/node_modules/pkg/README.md", "# Package\n");

    let files = collect_workflow_check_files(
        &fixture.root().join("docs"),
        &fixture.root().join("docs/notes"),
        true,
    );
    let rendered = rendered_relative_paths(fixture.root(), &files);
    assert!(rendered.contains(&"docs/guides/example.md".to_owned()));
    assert!(!rendered.iter().any(|path| path.contains("/target/")));
    assert!(!rendered.iter().any(|path| path.contains("/node_modules/")));
}

#[test]
fn collect_index_markdown_links_can_scope_to_section() {
    let fixture = DocsFixture::new("index-section");
    fixture.write(
        "README.md",
        "# Root\n\n## Vision Artifacts\n- [One](./one.md)\n\n## Other\n- [Two](./two.md)\n",
    );

    let links =
        collect_index_markdown_links(&fixture.root().join("README.md"), Some("Vision Artifacts"))
            .expect("links");
    assert!(links.contains("one.md"));
    assert!(!links.contains("two.md"));
}

#[test]
fn collect_index_markdown_links_accepts_plain_relative_targets() {
    let fixture = DocsFixture::new("index-plain-relative");
    fixture.write(
        "README.md",
        "# Root\n\n- [One](one.md)\n- [Nested](dir/two.md)\n- `three.md` is not a link\n",
    );

    let links =
        collect_index_markdown_links(&fixture.root().join("README.md"), None).expect("links");
    assert!(links.contains("one.md"));
    assert!(links.contains("dir/two.md"));
    assert!(!links.contains("three.md"));
}

#[test]
fn collect_included_index_markdown_links_respects_excludes() {
    let fixture = DocsFixture::new("index-excludes");
    fixture.write(
        "README.md",
        "# Root\n\n- [One](one.md)\n- [Archive](archive/README.md)\n",
    );

    let links = collect_included_index_markdown_links(
        &fixture.root().join("README.md"),
        None,
        &[String::from("archive/**")],
    )
    .expect("links");
    assert!(links.contains("one.md"));
    assert!(!links.contains("archive/README.md"));
}

#[test]
fn check_headings_reports_missing_heading() {
    let fixture = DocsFixture::new("headings");
    fixture.write("README.md", "# Root\n");

    let (_, findings) = check_headings(
        fixture.root(),
        &[Path::new("README.md").to_path_buf()],
        &[String::from("## Vision Alignment")],
    )
    .expect("check");
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].heading, "## Vision Alignment");
}

#[test]
fn check_contains_and_paths_report_missing_items() {
    let fixture = DocsFixture::new("contains-paths");
    fixture.write("README.md", "# Root\n");

    let (_, contains_findings) = check_contains(
        fixture.root(),
        &[Path::new("README.md").to_path_buf()],
        &[String::from("Vision")],
    )
    .expect("contains");
    assert_eq!(contains_findings.len(), 1);

    let (_, path_findings) = check_paths(fixture.root(), &[Path::new("missing.md").to_path_buf()]);
    assert_eq!(path_findings.len(), 1);
}

#[test]
fn check_workflow_paths_reports_stale_reference() {
    let fixture = DocsFixture::new("workflow-stale");
    fixture.mkdir(".github-bak/workflows");
    fixture.mkdir("docs/guides");
    fixture.write(".github-bak/workflows/example.yml", "name: Example\n");
    fixture.write(
        "docs/guides/example.md",
        "See `.github/workflows/example.yml`.\n",
    );

    let findings = check_workflow_paths(
        fixture.root(),
        &fixture.root().join("docs"),
        &fixture.root().join("docs/notes"),
        true,
    )
    .expect("workflow check");
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].reason, "stale workflow path");
}

#[test]
fn path_matches_exclude_supports_recursive_suffix() {
    assert!(path_matches_exclude("history/one.md", "history/**"));
    assert!(!path_matches_exclude("active/one.md", "history/**"));
}

#[test]
fn resolve_docs_index_spec_loads_named_policy_index() {
    let fixture = DocsFixture::new("policy");

    let mut policy = ManifestDocsPolicyConfig::default();
    policy.indexes.insert(
        "vision".to_owned(),
        ManifestDocsPolicyIndexConfig {
            file: "docs/knowledge/README.md".to_owned(),
            dir: "docs/knowledge".to_owned(),
            section: Some("Vision Artifacts".to_owned()),
            exclude: vec!["history/**".to_owned()],
        },
    );

    let spec =
        resolve_docs_index_spec(fixture.root(), &policy, Some("vision"), None, None).expect("spec");
    assert_eq!(spec.policy_name.as_deref(), Some("vision"));
    assert_eq!(spec.index, fixture.root().join("docs/knowledge/README.md"));
    assert_eq!(spec.dir, fixture.root().join("docs/knowledge"));
    assert_eq!(spec.section.as_deref(), Some("Vision Artifacts"));
    assert_eq!(spec.exclude, vec!["history/**"]);
}

#[test]
fn first_non_empty_section_line_skips_heading_and_blank_lines() {
    let line = first_non_empty_section_line("## Next move\n\nShip the thing.\n").expect("line");
    assert_eq!(line, "Ship the thing.");
}

#[test]
fn extract_lead_verb_handles_bullets_and_numbering() {
    assert_eq!(extract_lead_verb("- Execute cleanup."), "execute");
    assert_eq!(extract_lead_verb("1. Review follow-up."), "review");
    assert_eq!(extract_lead_verb("(1) Ship parity."), "ship");
}

#[test]
fn resolve_docs_next_action_spec_loads_named_policy() {
    let fixture = DocsFixture::new("next-action");
    fixture.mkdir("docs/scripts/fixtures");

    let mut policy = ManifestDocsPolicyConfig::default();
    policy.indexes.insert(
        "vision".to_owned(),
        ManifestDocsPolicyIndexConfig {
            file: "docs/knowledge/README.md".to_owned(),
            dir: "docs/knowledge".to_owned(),
            section: Some("Vision Artifacts".to_owned()),
            exclude: Vec::new(),
        },
    );
    policy.next_actions.insert(
        "vision".to_owned(),
        ManifestDocsPolicyNextActionConfig {
            index: "vision".to_owned(),
            heading: "## Next move".to_owned(),
            allowlist_file: "docs/scripts/fixtures/verbs.txt".to_owned(),
        },
    );

    let spec =
        resolve_docs_next_action_spec(fixture.root(), &policy, Some("vision")).expect("spec");
    assert_eq!(spec.policy_name.as_deref(), Some("vision"));
    assert_eq!(spec.heading, "## Next move");
    assert_eq!(spec.heading_without_hashes, "Next move");
    assert_eq!(
        spec.allowlist_file,
        fixture.root().join("docs/scripts/fixtures/verbs.txt")
    );
    assert_eq!(spec.index.policy_name.as_deref(), Some("vision"));
}
