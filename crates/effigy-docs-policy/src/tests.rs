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

#[test]
fn collect_link_check_files_defaults_to_full_docs_tree() {
    let fixture = DocsFixture::new("link-defaults");
    fixture.mkdir("docs/logs/2026-03");
    fixture.mkdir("docs/research");
    fixture.write("README.md", "# Root\n");
    fixture.write("docs/README.md", "# Docs\n");
    fixture.write("docs/logs/2026-03/example.md", "# Log\n");
    fixture.write("docs/research/example.md", "# Research\n");

    let files = collect_link_check_files(fixture.root(), &[]);
    let rendered = files
        .iter()
        .filter_map(|path| path.strip_prefix(fixture.root()).ok())
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .collect::<Vec<_>>();

    assert!(rendered.contains(&"README.md".to_owned()));
    assert!(rendered.contains(&"docs/README.md".to_owned()));
    assert!(rendered.contains(&"docs/logs/2026-03/example.md".to_owned()));
    assert!(rendered.contains(&"docs/research/example.md".to_owned()));
}

#[test]
fn normalize_log_index_relative_path_accepts_docs_logs_prefix() {
    let normalized =
        normalize_log_index_relative_path(Path::new("docs/logs/2026-03/02-160000-my-log.md"))
            .expect("normalize path");
    assert_eq!(normalized, "2026-03/02-160000-my-log.md");
}

#[test]
fn insert_log_index_entry_places_new_entry_before_archive_marker() {
    let index = "# Logs\n\n- [`2026-03/01-000000-old.md`](./2026-03/01-000000-old.md)\n\n## Archived Validation Logs\n- older\n";
    let updated = insert_log_index_entry(
        index,
        "- [`2026-03/02-160000-my-log.md`](./2026-03/02-160000-my-log.md)",
    );
    let marker = updated.find("## Archived Validation Logs").expect("marker");
    let entry = updated.find("2026-03/02-160000-my-log.md").expect("entry");
    assert!(entry < marker);
}

#[test]
fn collect_workflow_check_files_excludes_logs_for_default_docs_scope() {
    let fixture = DocsFixture::new("workflow-paths");
    fixture.mkdir("docs/logs/2026-03");
    fixture.mkdir("docs/guides");
    fixture.write("docs/guides/example.md", "# Guide\n");
    fixture.write("docs/logs/2026-03/example.md", "# Log\n");

    let files = collect_workflow_check_files(
        &fixture.root().join("docs"),
        &fixture.root().join("docs/logs"),
        true,
    );
    let rendered = files
        .iter()
        .filter_map(|path| path.strip_prefix(fixture.root()).ok())
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .collect::<Vec<_>>();

    assert!(rendered.contains(&"docs/guides/example.md".to_owned()));
    assert!(!rendered.contains(&"docs/logs/2026-03/example.md".to_owned()));
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
        &fixture.root().join("docs/logs"),
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
            file: "docs/vision/README.md".to_owned(),
            dir: "docs/vision".to_owned(),
            section: Some("Vision Artifacts".to_owned()),
            exclude: vec!["history/**".to_owned()],
        },
    );

    let spec =
        resolve_docs_index_spec(fixture.root(), &policy, Some("vision"), None, None).expect("spec");
    assert_eq!(spec.policy_name.as_deref(), Some("vision"));
    assert_eq!(spec.index, fixture.root().join("docs/vision/README.md"));
    assert_eq!(spec.dir, fixture.root().join("docs/vision"));
    assert_eq!(spec.section.as_deref(), Some("Vision Artifacts"));
    assert_eq!(spec.exclude, vec!["history/**"]);
}

#[test]
fn first_non_empty_section_line_skips_heading_and_blank_lines() {
    let line = first_non_empty_section_line("## Next Task\n\nShip the thing.\n").expect("line");
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
            file: "docs/vision/README.md".to_owned(),
            dir: "docs/vision".to_owned(),
            section: Some("Vision Artifacts".to_owned()),
            exclude: Vec::new(),
        },
    );
    policy.next_actions.insert(
        "vision".to_owned(),
        ManifestDocsPolicyNextActionConfig {
            index: "vision".to_owned(),
            heading: "## Next Task".to_owned(),
            allowlist_file: "docs/scripts/fixtures/verbs.txt".to_owned(),
        },
    );

    let spec =
        resolve_docs_next_action_spec(fixture.root(), &policy, Some("vision")).expect("spec");
    assert_eq!(spec.policy_name.as_deref(), Some("vision"));
    assert_eq!(spec.heading, "## Next Task");
    assert_eq!(spec.heading_without_hashes, "Next Task");
    assert_eq!(
        spec.allowlist_file,
        fixture.root().join("docs/scripts/fixtures/verbs.txt")
    );
    assert_eq!(spec.index.policy_name.as_deref(), Some("vision"));
}
