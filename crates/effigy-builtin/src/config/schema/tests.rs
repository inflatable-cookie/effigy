use super::{
    render_builtin_config_schema, render_builtin_config_schema_minimal,
    render_builtin_config_schema_target, CANONICAL_SECTIONS,
};
use crate::config::request::ConfigSchemaTarget;

/// The whole point of the "strict-valid" header is that the assembled text is a
/// manifest the loader accepts. The per-section tests assert fragments; this
/// one loads the assembled template through the same loader a repository uses.
#[test]
fn the_canonical_template_and_minimal_starter_are_strict_valid_manifests() {
    for (label, template) in [
        ("canonical", render_builtin_config_schema()),
        ("minimal", render_builtin_config_schema_minimal()),
    ] {
        let dir = tempfile::tempdir().expect("template fixture");
        let path = dir.path().join(effigy_manifest::TASK_MANIFEST_FILE);
        std::fs::write(&path, &template).expect("write template");
        // The template's own `include` graph names these fragments; the
        // loader resolving them is part of what "strict-valid" claims. The
        // docs fragment carries the index the template's override names.
        std::fs::write(dir.path().join("effigy.tasks.toml"), "")
            .expect("write included task fragment");
        std::fs::write(
            dir.path().join("effigy.docs.toml"),
            "[docs_policy.indexes.topic]\nfile = \"docs/README.md\"\ndir = \"docs\"\n",
        )
        .expect("write included docs fragment");

        let loaded =
            effigy_manifest::load_task_manifest_with_inspection(&path).unwrap_or_else(|error| {
                panic!("the {label} template must load as a manifest: {error}\n{template}")
            });
        assert_eq!(loaded.manifest_path, path);
        // The sections the template claims are the sections the loader
        // decoded, not merely lines that look like them.
        assert!(
            loaded.manifest.package_manager.is_some()
                && loaded.manifest.test.is_some()
                && loaded.manifest.task_defaults.is_some(),
            "the loaded {label} template keeps its declared sections"
        );
    }
}

/// Every target view is a view of the canonical template, not a second list of
/// section text: each of its lines appears in the canonical template, in the
/// order it renders.
#[test]
fn target_views_are_slices_of_the_canonical_template() {
    let canonical = render_builtin_config_schema();
    let canonical_lines = canonical.lines().collect::<Vec<_>>();
    for target in [
        ConfigSchemaTarget::Manifest,
        ConfigSchemaTarget::PackageManager,
        ConfigSchemaTarget::Test,
        ConfigSchemaTarget::Tasks,
        ConfigSchemaTarget::Defer,
        ConfigSchemaTarget::Scan,
    ] {
        let view = render_builtin_config_schema_target(target, false);
        let mut cursor = 0usize;
        for line in view.lines().skip(2) {
            let found = canonical_lines[cursor..]
                .iter()
                .position(|candidate| *candidate == line)
                .unwrap_or_else(|| {
                    panic!(
                        "`{}` target line {line:?} is not in the canonical template after line {cursor}",
                        target.as_str()
                    )
                });
            cursor += found + 1;
        }
    }
}

#[test]
fn the_canonical_template_documents_every_section_once() {
    let canonical = render_builtin_config_schema();
    for (heading, count) in [
        ("[manifest]", 1usize),
        ("[package_manager]", 1),
        ("[containers]", 1),
        ("[secrets]", 1),
        ("[deploy.uat]", 1),
    ] {
        assert_eq!(
            canonical.matches(heading).count(),
            count,
            "`{heading}` must appear {count} time(s) in the canonical template"
        );
    }
    assert!(CANONICAL_SECTIONS.len() > 5);
}
