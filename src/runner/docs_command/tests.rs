use super::map_docs_policy_error;
use effigy_docs_policy::{
    collect_index_markdown_links, collect_link_check_files, collect_markdown_children,
    collect_workflow_check_files, extract_fenced_json_blocks, extract_h2_section,
    extract_lead_verb, first_non_empty_section_line, insert_log_index_entry,
    normalize_log_index_relative_path, path_matches_exclude, resolve_docs_index_spec,
    resolve_docs_next_action_spec, scan_markdown_links, DocsPolicyError,
};
use effigy_manifest::config_sections::{
    ManifestDocsPolicyIndexConfig, ManifestDocsPolicyNextActionConfig,
};
use effigy_manifest::ManifestDocsPolicyConfig;
use std::{fs, path::Path};

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
    let root = std::env::temp_dir().join(format!(
        "effigy-doc-links-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("mkdir");
    let markdown = root.join("README.md");
    fs::write(
        &markdown,
        "[ok](./existing.md)\n```md\n[skip](./missing.md)\n```\n",
    )
    .expect("write markdown");
    fs::write(root.join("existing.md"), "exists\n").expect("write existing");

    let failures = scan_markdown_links(&markdown).expect("scan");
    assert!(failures.is_empty());
}

#[test]
fn collect_link_check_files_defaults_to_full_docs_tree() {
    let root = std::env::temp_dir().join(format!(
        "effigy-doc-link-defaults-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    fs::create_dir_all(root.join("docs/logs/2026-03")).expect("mkdir logs");
    fs::create_dir_all(root.join("docs/research")).expect("mkdir research");
    fs::write(root.join("README.md"), "# Root\n").expect("write root");
    fs::write(root.join("docs/README.md"), "# Docs\n").expect("write docs readme");
    fs::write(root.join("docs/logs/2026-03/example.md"), "# Log\n").expect("write log");
    fs::write(root.join("docs/research/example.md"), "# Research\n").expect("write research");

    let files = collect_link_check_files(&root, &[]);
    let rendered = files
        .iter()
        .filter_map(|path| path.strip_prefix(&root).ok())
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
    let root = std::env::temp_dir().join(format!(
        "effigy-doc-workflow-paths-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    fs::create_dir_all(root.join("docs/logs/2026-03")).expect("mkdir logs");
    fs::create_dir_all(root.join("docs/guides")).expect("mkdir guides");
    fs::write(root.join("docs/guides/example.md"), "# Guide\n").expect("write guide");
    fs::write(root.join("docs/logs/2026-03/example.md"), "# Log\n").expect("write log");

    let files = collect_workflow_check_files(&root.join("docs"), &root.join("docs/logs"), true);
    let rendered = files
        .iter()
        .filter_map(|path| path.strip_prefix(&root).ok())
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .collect::<Vec<_>>();

    assert!(rendered.contains(&"docs/guides/example.md".to_owned()));
    assert!(!rendered.contains(&"docs/logs/2026-03/example.md".to_owned()));
}

#[test]
fn collect_markdown_children_respects_excludes() {
    let root = std::env::temp_dir().join(format!(
        "effigy-doc-index-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    fs::create_dir_all(root.join("history")).expect("mkdir history");
    fs::write(root.join("README.md"), "# Root\n").expect("write readme");
    fs::write(root.join("active.md"), "# Active\n").expect("write active");
    fs::write(root.join("history/old.md"), "# Old\n").expect("write old");

    let files = collect_markdown_children(&root, &[String::from("history/**")]);
    assert!(files.contains("active.md"));
    assert!(!files.contains("history/old.md"));
}

#[test]
fn collect_index_markdown_links_can_scope_to_section() {
    let root = std::env::temp_dir().join(format!(
        "effigy-doc-index-section-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("mkdir");
    let index = root.join("README.md");
    fs::write(
        &index,
        "# Root\n\n## Vision Artifacts\n- [One](./one.md)\n\n## Other\n- [Two](./two.md)\n",
    )
    .expect("write index");

    let links = collect_index_markdown_links(&index, Some("Vision Artifacts")).expect("links");
    assert!(links.contains("one.md"));
    assert!(!links.contains("two.md"));
}

#[test]
fn path_matches_exclude_supports_recursive_suffix() {
    assert!(path_matches_exclude("history/one.md", "history/**"));
    assert!(!path_matches_exclude("active/one.md", "history/**"));
}

#[test]
fn resolve_docs_index_spec_loads_named_policy_index() {
    let root = std::env::temp_dir().join(format!(
        "effigy-doc-policy-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("mkdir");
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

    let spec = resolve_docs_index_spec(&root, &policy, Some("vision"), None, None).expect("spec");
    assert_eq!(spec.policy_name.as_deref(), Some("vision"));
    assert_eq!(spec.index, root.join("docs/vision/README.md"));
    assert_eq!(spec.dir, root.join("docs/vision"));
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
    let root = std::env::temp_dir().join(format!(
        "effigy-doc-next-action-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    fs::create_dir_all(root.join("docs/scripts/fixtures")).expect("mkdir");
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

    let spec = resolve_docs_next_action_spec(&root, &policy, Some("vision")).expect("spec");
    assert_eq!(spec.policy_name.as_deref(), Some("vision"));
    assert_eq!(spec.heading, "## Next Task");
    assert_eq!(spec.heading_without_hashes, "Next Task");
    assert_eq!(
        spec.allowlist_file,
        root.join("docs/scripts/fixtures/verbs.txt")
    );
    assert_eq!(spec.index.policy_name.as_deref(), Some("vision"));
}

#[test]
fn map_docs_policy_error_preserves_user_facing_message_shape() {
    let err = map_docs_policy_error(DocsPolicyError::Message("bad docs policy".to_owned()));
    assert_eq!(err.to_string(), "bad docs policy");
}
