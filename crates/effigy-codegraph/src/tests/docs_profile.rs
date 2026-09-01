use super::*;

use crate::docs_profile::{glob_matches, load_docs_profile_state, DOCS_PROFILE_FINGERPRINT_KEY};

fn write_handbook_fixture(root: &Path, extra_markdown: &str) {
    fs::create_dir_all(root.join("handbook/playbooks")).expect("mkdir playbooks");
    fs::create_dir_all(root.join("handbook/bulletins")).expect("mkdir bulletins");
    fs::write(
        root.join("handbook/playbooks/setup.md"),
        format!(
            r#"# Setup playbook

State: live
Steward: ada

See also: [ops](ops.md)

## Steps

Do the work.

## See also

- [ops](ops.md)
{extra_markdown}
"#
        ),
    )
    .expect("write playbook");
    fs::write(
        root.join("handbook/playbooks/ops.md"),
        "# Ops\n\nState: live\n",
    )
    .expect("write ops");
    fs::write(
        root.join("handbook/bulletins/old.md"),
        "# Old bulletin\n\nState: retired\n",
    )
    .expect("write bulletin");
}

fn write_graph_manifest(root: &Path, body: &str) {
    fs::write(root.join("effigy.toml"), body).expect("write manifest");
}

fn generic_profile(extra_fields: &str) -> String {
    format!(
        r#"
[docs_policy.graph]
roots = ["handbook"]

[docs_policy.graph.fields.state]
labels = ["State"]
cardinality = "one"
{extra_fields}
[docs_policy.graph.currentness]
field = "state"
current = ["live"]
historical = ["retired"]

[docs_policy.graph.kinds.playbook]
include = ["handbook/playbooks/*.md"]
authority = 80

[docs_policy.graph.kinds.bulletin]
include = ["handbook/bulletins/*.md"]
authority = 20
default_currentness = "historical"

[docs_policy.graph.relations.see-also]
labels = ["See also"]
headings = ["See also"]
"#
    )
}

#[test]
fn missing_profile_indexes_exact_sections_as_document() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("docs/contracts")).expect("mkdir");
    fs::write(
        temp.path().join("docs/contracts/example.md"),
        "# Title\n\nIntro.\n\n## Alpha\n\nAlpha body.\n\n### Nested\n\nNested body.\n\n## Beta\n\nBeta body.\n",
    )
    .expect("write markdown");

    run_index(temp.path()).expect("index");
    let store = GraphStore::open(temp.path()).expect("store");
    let symbols = store.list_symbols().expect("symbols");
    let document = symbols
        .iter()
        .find(|symbol| symbol.canonical_name == "docs/contracts/example.md")
        .expect("document");
    assert_eq!(document.kind, "document");
    assert_eq!(document.span.start.line, 1);
    assert_eq!(document.span.start.byte, 0);

    let title = symbols
        .iter()
        .find(|symbol| symbol.canonical_name == "docs/contracts/example.md#title")
        .expect("title");
    let alpha = symbols
        .iter()
        .find(|symbol| symbol.canonical_name == "docs/contracts/example.md#alpha")
        .expect("alpha");
    let nested = symbols
        .iter()
        .find(|symbol| symbol.canonical_name == "docs/contracts/example.md#nested")
        .expect("nested");
    let beta = symbols
        .iter()
        .find(|symbol| symbol.canonical_name == "docs/contracts/example.md#beta")
        .expect("beta");

    assert_eq!(title.kind, "heading-h1");
    assert_eq!(alpha.kind, "heading-h2");
    assert_eq!(nested.kind, "heading-h3");
    assert_eq!(beta.kind, "heading-h2");
    assert_eq!(title.span.start.line, 1);
    assert!(
        title.span.end.byte > beta.span.start.byte,
        "h1 section continues through later headings"
    );
    assert!(
        alpha.span.end.byte == beta.span.start.byte,
        "alpha ends at the peer heading"
    );
    assert!(
        nested.span.start.byte >= alpha.span.start.byte
            && nested.span.end.byte <= alpha.span.end.byte,
        "nested heading stays inside alpha"
    );
    assert_eq!(beta.span.end.byte, document.span.end.byte);
    assert!(
        !symbols.iter().any(|symbol| symbol.kind == "contract"),
        "baseline extraction must not invent a contract kind"
    );
}

#[test]
fn setext_heading_gets_an_exact_section_span() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("notes")).expect("mkdir");
    fs::write(
        temp.path().join("notes/intro.md"),
        "Title\n=====\n\nBody line.\n",
    )
    .expect("write setext");

    run_index(temp.path()).expect("index");
    let store = GraphStore::open(temp.path()).expect("store");
    let symbols = store.list_symbols().expect("symbols");
    let heading = symbols
        .iter()
        .find(|symbol| symbol.kind == "heading-h1")
        .expect("setext heading");
    assert_eq!(heading.span.start.line, 1);
    assert_eq!(heading.span.start.byte, 0);
    assert_eq!(
        heading.span.end.byte,
        fs::read_to_string(temp.path().join("notes/intro.md"))
            .expect("read")
            .len() as u32
    );
}

#[test]
fn profile_extracts_fields_and_typed_relations_outside_fences() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_handbook_fixture(
        temp.path(),
        "\n```\nState: ignored\nSee also: [ignored](ops.md)\n```\n",
    );
    write_graph_manifest(temp.path(), &generic_profile(""));

    run_index(temp.path()).expect("index");
    let store = GraphStore::open(temp.path()).expect("store");
    let symbols = store.list_symbols().expect("symbols");
    let playbook = symbols
        .iter()
        .find(|symbol| symbol.canonical_name == "handbook/playbooks/setup.md")
        .expect("playbook document");
    assert_eq!(playbook.kind, "playbook");

    let state_facts: Vec<_> = symbols
        .iter()
        .filter(|symbol| {
            symbol.kind == "doc-field"
                && symbol.canonical_name == "handbook/playbooks/setup.md#state"
        })
        .collect();
    assert_eq!(state_facts.len(), 1);
    assert_eq!(state_facts[0].display_name, "live");
    assert_eq!(state_facts[0].span.start.line, 3);

    let edges = store.list_edges().expect("edges");
    let typed: Vec<_> = edges
        .iter()
        .filter(|edge| {
            edge.kind == "doc-rel" && edge.provenance.detail.as_deref() == Some("see-also")
        })
        .collect();
    assert_eq!(typed.len(), 1);
    assert_eq!(
        typed[0].to_id.as_ref().map(GraphId::as_str),
        Some("file:handbook/playbooks/ops.md")
    );

    let references = store.list_references().expect("references");
    assert!(references.iter().any(|reference| {
        reference.kind == "doc-rel"
            && reference.provenance.detail.as_deref() == Some("see-also")
            && reference.span.start.byte < reference.span.end.byte
    }));
    assert!(store
        .list_diagnostics()
        .expect("diagnostics")
        .iter()
        .all(
            |diagnostic| diagnostic.severity != DiagnosticSeverity::Error
                || !diagnostic.message.contains("duplicate")
        ));
}

#[test]
fn duplicate_single_valued_field_is_a_diagnostic() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_handbook_fixture(temp.path(), "State: retired\n");
    write_graph_manifest(temp.path(), &generic_profile(""));

    run_index(temp.path()).expect("index");
    let store = GraphStore::open(temp.path()).expect("store");
    let diagnostics = store.list_diagnostics().expect("diagnostics");
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("duplicate single-valued field `state`")
            && diagnostic.span.is_some()
    }));
    let facts = store
        .list_symbols()
        .expect("symbols")
        .into_iter()
        .filter(|symbol| {
            symbol.kind == "doc-field"
                && symbol.canonical_name == "handbook/playbooks/setup.md#state"
        })
        .count();
    assert_eq!(facts, 2);
}

#[test]
fn profile_only_edit_forces_semantic_reindex() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_handbook_fixture(temp.path(), "");
    write_graph_manifest(temp.path(), &generic_profile(""));
    run_index(temp.path()).expect("first index");

    let store = GraphStore::open(temp.path()).expect("store");
    let first_fingerprint = store
        .metadata_value(DOCS_PROFILE_FINGERPRINT_KEY)
        .expect("fingerprint")
        .expect("stored");
    assert!(store
        .list_symbols()
        .expect("symbols")
        .iter()
        .all(|symbol| symbol.canonical_name != "handbook/playbooks/setup.md#steward"));

    write_graph_manifest(
        temp.path(),
        &generic_profile(
            r#"
[docs_policy.graph.fields.steward]
labels = ["Steward"]
cardinality = "one"
"#,
        ),
    );

    let status_payload = status(temp.path()).expect("status");
    assert!(
        status_payload
            .stale_paths
            .iter()
            .any(|path| path == "handbook/playbooks/setup.md"),
        "profile-only edit should stale markdown: {:?}",
        status_payload.stale_paths
    );

    run_index(temp.path()).expect("second index");
    let store = GraphStore::open(temp.path()).expect("store");
    let second_fingerprint = store
        .metadata_value(DOCS_PROFILE_FINGERPRINT_KEY)
        .expect("fingerprint")
        .expect("stored");
    assert_ne!(first_fingerprint, second_fingerprint);
    assert!(store.list_symbols().expect("symbols").iter().any(|symbol| {
        symbol.kind == "doc-field"
            && symbol.canonical_name == "handbook/playbooks/setup.md#steward"
            && symbol.display_name == "ada"
    }));
}

#[test]
fn overlapping_kinds_fail_before_indexing() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_handbook_fixture(temp.path(), "");
    write_graph_manifest(
        temp.path(),
        r#"
[docs_policy.graph]
roots = ["handbook"]

[docs_policy.graph.kinds.playbook]
include = ["handbook/playbooks/*.md"]

[docs_policy.graph.kinds.duplicate]
include = ["handbook/**/*.md"]
"#,
    );

    let error = run_index(temp.path()).expect_err("overlap");
    let message = error.to_string();
    assert!(
        message.contains("kinds overlap on `handbook/playbooks/"),
        "{message}"
    );
    assert!(message.contains("duplicate"), "{message}");
    assert!(message.contains("playbook"), "{message}");
}

#[test]
fn symlink_root_escape_is_rejected() {
    let temp = tempfile::tempdir().expect("tempdir");
    let outside = tempfile::tempdir().expect("outside");
    fs::write(outside.path().join("secret.md"), "# Secret\n").expect("write outside");
    std::os::unix::fs::symlink(outside.path(), temp.path().join("handbook")).expect("symlink");
    write_graph_manifest(
        temp.path(),
        r#"
[docs_policy.graph]
roots = ["handbook"]
"#,
    );

    let error = load_docs_profile_state(temp.path()).expect_err("escape");
    assert!(
        error
            .to_string()
            .contains("escapes the selected repository"),
        "{error}"
    );
}

#[test]
fn current_dir_roots_and_globs_match_scanned_paths() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_handbook_fixture(temp.path(), "");
    write_graph_manifest(
        temp.path(),
        r#"
[docs_policy.graph]
roots = [".", "./handbook"]

[docs_policy.graph.kinds.playbook]
include = ["./handbook/playbooks/*.md"]
"#,
    );

    run_index(temp.path()).expect("index");
    let store = GraphStore::open(temp.path()).expect("store");
    let playbook = store
        .list_symbols()
        .expect("symbols")
        .into_iter()
        .find(|symbol| symbol.canonical_name == "handbook/playbooks/setup.md")
        .expect("playbook");
    assert_eq!(playbook.kind, "playbook");
}

#[test]
fn typed_relations_preserve_fragments_and_namespace_tokens() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("handbook")).expect("mkdir");
    fs::create_dir_all(temp.path().join("src")).expect("mkdir src");
    fs::create_dir_all(temp.path().join(".effigy")).expect("mkdir hidden");
    fs::write(
        temp.path().join("handbook/source.md"),
        r#"# Source

See also: [ops a](target.md#section-a) [ops b](target.md#section-b) [self](#source) [missing](missing.md#gone) [missing anchor](target.md#gone) [code](../src/lib.rs#helper) [hidden](../.effigy/hidden.md#secret) [escaped](escape.md#secret) [ignored](ignored.md#secret) [alias](alias.md#section-a) [site](https://example.test/doc#frag)

## See also

- [ops a](target.md#section-a)
"#,
    )
    .expect("write source");
    fs::write(
        temp.path().join("handbook/target.md"),
        "# Target\n\n## Section A\n\nA.\n\n## Section B\n\nB.\n",
    )
    .expect("write target");
    fs::write(temp.path().join("src/lib.rs"), "pub fn helper() {}\n").expect("write rust");
    fs::write(temp.path().join(".effigy/hidden.md"), "# Secret\n").expect("write hidden");
    fs::write(temp.path().join(".ignore"), "handbook/ignored.md\n").expect("write ignore");
    fs::write(temp.path().join("handbook/ignored.md"), "# Secret\n").expect("write ignored");
    let outside = tempfile::tempdir().expect("outside");
    fs::write(outside.path().join("secret.md"), "# Secret\n").expect("write outside");
    std::os::unix::fs::symlink(
        outside.path().join("secret.md"),
        temp.path().join("handbook/escape.md"),
    )
    .expect("symlink");
    std::os::unix::fs::symlink("target.md", temp.path().join("handbook/alias.md"))
        .expect("internal symlink");
    write_graph_manifest(
        temp.path(),
        r#"
[docs_policy.graph]
roots = ["handbook"]

[docs_policy.graph.relations.contains]
labels = ["See also"]
headings = ["See also"]
"#,
    );

    run_index(temp.path()).expect("index");
    let store = GraphStore::open(temp.path()).expect("store");
    let typed: Vec<_> = store
        .list_edges()
        .expect("edges")
        .into_iter()
        .filter(|edge| {
            edge.kind == "doc-rel" && edge.provenance.detail.as_deref() == Some("contains")
        })
        .collect();
    assert!(
        typed.iter().any(|edge| {
            edge.to_id.as_ref().map(GraphId::as_str)
                == Some("symbol:doc:handbook/target.md:#section-a")
        }),
        "resolved fragment a: {typed:?}"
    );
    assert!(
        typed.iter().any(|edge| {
            edge.to_id.as_ref().map(GraphId::as_str)
                == Some("symbol:doc:handbook/target.md:#section-b")
        }),
        "resolved fragment b: {typed:?}"
    );
    assert!(
        typed.iter().any(|edge| {
            edge.to_id.as_ref().map(GraphId::as_str)
                == Some("symbol:doc:handbook/source.md:#source")
        }),
        "same-document fragment: {typed:?}"
    );
    assert!(
        typed.iter().any(|edge| {
            edge.to_id.is_none() && edge.unresolved_target.as_deref() == Some("missing.md#gone")
        }),
        "unresolved missing file fragment: {typed:?}"
    );
    assert!(
        typed.iter().any(|edge| {
            edge.to_id.is_none() && edge.unresolved_target.as_deref() == Some("target.md#gone")
        }),
        "existing file with missing heading stays unresolved: {typed:?}"
    );
    assert!(
        typed.iter().any(|edge| {
            edge.to_id.is_none()
                && edge.unresolved_target.as_deref() == Some("../src/lib.rs#helper")
        }),
        "non-markdown local fragment stays unresolved: {typed:?}"
    );
    assert!(
        typed.iter().any(|edge| {
            edge.to_id.is_none()
                && edge.unresolved_target.as_deref() == Some("../.effigy/hidden.md#secret")
        }),
        "ignored target stays unresolved: {typed:?}"
    );
    assert!(
        typed.iter().any(|edge| {
            edge.to_id.is_none() && edge.unresolved_target.as_deref() == Some("escape.md#secret")
        }),
        "symlink escape stays unresolved: {typed:?}"
    );
    assert!(
        typed.iter().any(|edge| {
            edge.to_id.is_none() && edge.unresolved_target.as_deref() == Some("ignored.md#secret")
        }),
        ".ignore target stays unresolved: {typed:?}"
    );
    assert!(
        typed.iter().any(|edge| {
            edge.to_id.is_none() && edge.unresolved_target.as_deref() == Some("alias.md#section-a")
        }),
        "internal symlink stays unresolved: {typed:?}"
    );
    assert!(
        typed.iter().any(|edge| {
            edge.to_id.is_none()
                && edge.unresolved_target.as_deref() == Some("https://example.test/doc#frag")
        }),
        "external fragment: {typed:?}"
    );
    let markdown_contains_collisions = store
        .list_edges()
        .expect("edges")
        .into_iter()
        .filter(|edge| {
            edge.provenance.source_path == "handbook/source.md"
                && edge.kind == "contains"
                && edge.provenance.detail.as_deref() == Some("contains")
        })
        .count();
    assert_eq!(
        markdown_contains_collisions, 0,
        "repository relation token must stay namespaced as doc-rel"
    );
}

#[test]
fn typed_relations_stay_visible_when_an_unchanged_source_loses_its_target() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("handbook")).expect("mkdir");
    let source = temp.path().join("handbook/source.md");
    fs::write(
        &source,
        "# Source\n\nSee also: [target](target.md#section)\n",
    )
    .expect("write source");
    fs::write(
        temp.path().join("handbook/target.md"),
        "# Target\n\n## Section\n\nBody.\n",
    )
    .expect("write target");
    write_graph_manifest(
        temp.path(),
        r#"
[docs_policy.graph]
roots = ["handbook"]

[docs_policy.graph.relations.contains]
labels = ["See also"]
"#,
    );

    run_index(temp.path()).expect("index");
    let typed = source_contains_edges(&GraphStore::open(temp.path()).expect("store"));
    assert!(
        typed.iter().any(|edge| {
            edge.to_id.as_ref().map(GraphId::as_str)
                == Some("symbol:doc:handbook/target.md:#section")
        }),
        "resolved before ignore: {typed:?}"
    );
    let source_bytes = fs::read(&source).expect("read source");

    fs::write(temp.path().join(".ignore"), "handbook/target.md\n").expect("write ignore");
    run_index(temp.path()).expect("reindex after ignore");
    assert_eq!(
        fs::read(&source).expect("reread source"),
        source_bytes,
        "source must stay byte-for-byte unchanged"
    );
    let typed = source_contains_edges(&GraphStore::open(temp.path()).expect("store"));
    assert!(
        typed.iter().any(|edge| {
            edge.to_id.is_none() && edge.unresolved_target.as_deref() == Some("target.md#section")
        }),
        "ignored target should remain as an unresolved relation: {typed:?}"
    );
}

#[test]
fn typed_relations_stay_visible_when_a_target_heading_is_removed() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("handbook")).expect("mkdir");
    let source = temp.path().join("handbook/source.md");
    fs::write(
        &source,
        "# Source\n\nSee also: [target](target.md#section)\n",
    )
    .expect("write source");
    fs::write(
        temp.path().join("handbook/target.md"),
        "# Target\n\n## Section\n\nBody.\n",
    )
    .expect("write target");
    write_graph_manifest(
        temp.path(),
        r#"
[docs_policy.graph]
roots = ["handbook"]

[docs_policy.graph.relations.contains]
labels = ["See also"]
"#,
    );

    run_index(temp.path()).expect("index");
    let source_bytes = fs::read(&source).expect("read source");
    fs::write(
        temp.path().join("handbook/target.md"),
        "# Target\n\nNo section.\n",
    )
    .expect("remove heading");
    run_index(temp.path()).expect("reindex after heading removal");
    assert_eq!(
        fs::read(&source).expect("reread source"),
        source_bytes,
        "source must stay byte-for-byte unchanged"
    );
    let store = GraphStore::open(temp.path()).expect("store");
    let typed = source_contains_edges(&store);
    assert!(
        typed.iter().any(|edge| {
            edge.to_id.is_none() && edge.unresolved_target.as_deref() == Some("target.md#section")
        }),
        "removed heading should remain as an unresolved relation: {typed:?}"
    );
}

#[test]
fn typed_relations_revalidate_escaped_destinations_for_edges_and_references() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("handbook")).expect("mkdir");
    let source = temp.path().join("handbook/source.md");
    fs::write(
        &source,
        "# Source\n\nSee also: [target](target\\(one\\).md#section)\n",
    )
    .expect("write source");
    fs::write(
        temp.path().join("handbook/target(one).md"),
        "# Target\n\n## Section\n\nBody.\n",
    )
    .expect("write target");
    write_graph_manifest(
        temp.path(),
        r#"
[docs_policy.graph]
roots = ["handbook"]

[docs_policy.graph.relations.contains]
labels = ["See also"]
"#,
    );

    let resolved = Some("symbol:doc:handbook/target(one).md:#section");
    run_index(temp.path()).expect("index");
    let store = GraphStore::open(temp.path()).expect("store");
    assert_contains_relation(&store, resolved, None, "first index");
    let source_bytes = fs::read(&source).expect("read source");

    fs::write(temp.path().join(".ignore"), "handbook/target(one).md\n").expect("write ignore");
    run_index(temp.path()).expect("reindex after ignore");
    assert_eq!(fs::read(&source).expect("reread source"), source_bytes);
    let store = GraphStore::open(temp.path()).expect("store");
    assert_contains_relation(
        &store,
        None,
        Some("target(one).md#section"),
        "ignored escaped target",
    );

    fs::remove_file(temp.path().join(".ignore")).expect("clear ignore");
    run_index(temp.path()).expect("reindex after restore");
    assert_eq!(fs::read(&source).expect("reread source"), source_bytes);
    let store = GraphStore::open(temp.path()).expect("store");
    assert_contains_relation(&store, resolved, None, "restored escaped target");
}

#[test]
fn leading_yaml_frontmatter_is_metadata_not_a_heading() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("notes")).expect("mkdir");
    let markdown =
        "---\ntitle: Example\nState: live\n---\n# Real\n\nSetext Title\n------------\n\nBody.\n";
    let path = temp.path().join("notes/intro.md");
    fs::write(&path, markdown).expect("write markdown");

    run_index(temp.path()).expect("index");
    let store = GraphStore::open(temp.path()).expect("store");
    let symbols = store.list_symbols().expect("symbols");

    let document = symbols
        .iter()
        .find(|symbol| symbol.canonical_name == "notes/intro.md")
        .expect("document");
    assert_eq!(document.kind, "document");
    assert_eq!(document.span.start.line, 1);
    assert_eq!(document.span.start.byte, 0);
    assert_eq!(document.span.end.byte, markdown.len() as u32);

    assert!(
        !symbols.iter().any(|symbol| {
            symbol.kind.starts_with("heading-h")
                && (symbol.display_name.contains("title: Example")
                    || symbol.display_name.contains("State: live")
                    || symbol.display_name.contains("---"))
        }),
        "frontmatter must not become a heading: {symbols:?}"
    );

    let real = symbols
        .iter()
        .find(|symbol| symbol.canonical_name == "notes/intro.md#real")
        .expect("real ATX heading");
    assert_eq!(real.kind, "heading-h1");
    assert_eq!(real.display_name, "Real");
    assert_eq!(real.span.start.line, 5);
    assert_eq!(
        real.span.start.byte,
        markdown.find("# Real").expect("real offset") as u32
    );

    let setext = symbols
        .iter()
        .find(|symbol| symbol.canonical_name == "notes/intro.md#setext-title")
        .expect("setext heading");
    assert_eq!(setext.kind, "heading-h2");
    assert_eq!(setext.display_name, "Setext Title");
    assert_eq!(setext.span.start.line, 7);
    assert_eq!(
        setext.span.start.byte,
        markdown.find("Setext Title").expect("setext offset") as u32
    );
}

#[test]
fn leading_yaml_frontmatter_keeps_profile_fields_and_relations() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("handbook/playbooks")).expect("mkdir");
    fs::write(
        temp.path().join("handbook/playbooks/setup.md"),
        "---\nState: live\nSee also: [ops](ops.md)\n---\n# Setup playbook\n\nBody.\n",
    )
    .expect("write playbook");
    fs::write(
        temp.path().join("handbook/playbooks/ops.md"),
        "# Ops\n\nState: live\n",
    )
    .expect("write ops");
    write_graph_manifest(temp.path(), &generic_profile(""));

    run_index(temp.path()).expect("index");
    let store = GraphStore::open(temp.path()).expect("store");
    let symbols = store.list_symbols().expect("symbols");

    assert!(
        !symbols.iter().any(|symbol| {
            symbol.kind.starts_with("heading-h")
                && (symbol.display_name.contains("State: live")
                    || symbol.display_name.contains("See also"))
        }),
        "frontmatter must stay out of heading inventory: {symbols:?}"
    );

    let state = symbols
        .iter()
        .find(|symbol| {
            symbol.kind == "doc-field"
                && symbol.canonical_name == "handbook/playbooks/setup.md#state"
        })
        .expect("state field");
    assert_eq!(state.display_name, "live");
    assert_eq!(state.span.start.line, 2);
    assert_eq!(state.span.start.byte, "---\n".len() as u32);

    let edges = store.list_edges().expect("edges");
    assert!(
        edges.iter().any(|edge| {
            edge.kind == "doc-rel"
                && edge.provenance.detail.as_deref() == Some("see-also")
                && edge.provenance.source_path == "handbook/playbooks/setup.md"
                && (edge.unresolved_target.as_deref() == Some("ops.md")
                    || edge.to_id.as_ref().map(GraphId::as_str)
                        == Some("file:handbook/playbooks/ops.md"))
        }),
        "labelled frontmatter relation must remain: {edges:?}"
    );
}

#[test]
fn incomplete_and_nonleading_yaml_delimiters_keep_document_content() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("notes")).expect("mkdir");
    fs::write(
        temp.path().join("notes/incomplete.md"),
        "---\ntitle: Example\n# Still Visible\n\nKept body.\n",
    )
    .expect("write incomplete");
    fs::write(
        temp.path().join("notes/later.md"),
        "# Lead\n\nProse before a fence.\n\n---\ntitle: not frontmatter\n---\n\n# After\n",
    )
    .expect("write later");

    run_index(temp.path()).expect("index");
    let store = GraphStore::open(temp.path()).expect("store");
    let symbols = store.list_symbols().expect("symbols");

    let incomplete = symbols
        .iter()
        .find(|symbol| symbol.canonical_name == "notes/incomplete.md#still-visible")
        .expect("incomplete file keeps ATX heading");
    assert_eq!(incomplete.display_name, "Still Visible");

    let later_setext = symbols
        .iter()
        .find(|symbol| {
            symbol.provenance.source_path == "notes/later.md"
                && symbol.kind.starts_with("heading-h")
                && symbol.display_name.contains("title: not frontmatter")
        })
        .expect("non-leading fence keeps ordinary setext behavior");
    assert!(later_setext.span.start.byte > 0);

    let after = symbols
        .iter()
        .find(|symbol| symbol.canonical_name == "notes/later.md#after")
        .expect("content after non-leading fence remains");
    assert_eq!(after.display_name, "After");
}

fn source_contains_edges(store: &GraphStore) -> Vec<crate::model::EdgeRecord> {
    store
        .list_edges()
        .expect("edges")
        .into_iter()
        .filter(|edge| {
            edge.kind == "doc-rel"
                && edge.provenance.detail.as_deref() == Some("contains")
                && edge.provenance.source_path == "handbook/source.md"
        })
        .collect()
}

fn source_contains_references(store: &GraphStore) -> Vec<crate::model::ReferenceRecord> {
    store
        .list_references()
        .expect("references")
        .into_iter()
        .filter(|reference| {
            reference.kind == "doc-rel"
                && reference.provenance.detail.as_deref() == Some("contains")
                && reference.provenance.source_path == "handbook/source.md"
        })
        .collect()
}

fn assert_contains_relation(
    store: &GraphStore,
    resolved: Option<&str>,
    unresolved: Option<&str>,
    phase: &str,
) {
    let edges = source_contains_edges(store);
    let references = source_contains_references(store);
    assert!(
        edges.iter().any(|edge| {
            edge.to_id.as_ref().map(GraphId::as_str) == resolved
                && edge.unresolved_target.as_deref() == unresolved
        }),
        "{phase} edge: {edges:?}"
    );
    assert!(
        references.iter().any(|reference| {
            reference.target_id.as_ref().map(GraphId::as_str) == resolved
                && reference.unresolved_target.as_deref() == unresolved
        }),
        "{phase} reference: {references:?}"
    );
}

#[test]
fn glob_helper_stays_generic() {
    assert!(glob_matches(
        "handbook/playbooks/*.md",
        "handbook/playbooks/setup.md"
    ));
    assert!(!include_str!("../language/markdown/extract.rs").contains("docs/contracts"));
    assert!(!include_str!("../language/markdown/extract.rs").contains("ready-card"));
}
