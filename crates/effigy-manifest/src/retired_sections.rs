//! Sections for features v0.15 removed, ignored instead of refused.
//!
//! A v0.14.1 project must keep loading after an update (contract 052 §11,
//! `docs/knowledge/vision.md`). When a section configures a feature v0.15
//! removed and ignoring it changes nothing else, Effigy drops the section and
//! records it in the manifest's one deprecation notice
//! ([`crate::retired_config`]) instead of failing the parse (effigy#274).
//!
//! `[drafts]` is the exception and stays a manifest error: dropping it would
//! silently drop task definitions. The removed feature's own command still
//! refuses with its replacement, which these statements repeat so an operator
//! reading the notice knows what to do.

use toml::Value;

use crate::retired_config::RetiredConfigDeclaration;

/// Scanner tables whose scanners v0.15 removed with the code graph. The five
/// Q-009 cuts come first, then the graph-backed three (including boundary
/// layers). `god_files` and `duplicate_blocks` remain.
const RETIRED_SCAN_SECTIONS: &[(&str, &str)] = &[
    (
        "attention_markers",
        "`[scan.attention_markers]` is ignored; search markers with `rg -n 'TODO|FIXME' <path>` instead",
    ),
    (
        "comment_ratio",
        "`[scan.comment_ratio]` is ignored; the scanner was removed and has no replacement",
    ),
    (
        "generated_assets",
        "`[scan.generated_assets]` is ignored; keep bulky generated output out of versioned paths with `.gitignore`",
    ),
    (
        "generated_in_src",
        "`[scan.generated_in_src]` is ignored; keep generated output outside source trees",
    ),
    (
        "stale_suppressions",
        "`[scan.stale_suppressions]` is ignored; let each tool report its own unused suppressions",
    ),
    (
        "boundary_violations",
        "`[scan.boundary_violations]` is ignored; the graph-backed boundary layers are gone and have no replacement",
    ),
    (
        "dead_code",
        "`[scan.dead_code]` is ignored; the graph-backed scanner is gone and has no replacement",
    ),
    (
        "validation_gaps",
        "`[scan.validation_gaps]` is ignored; use `effigy test --plan` to choose tests for changed files",
    ),
];

/// Drop the retired sections `root` still declares and record one statement
/// per section. The value is the composed manifest, so a section coming from a
/// bundle or an include is ignored the same way.
pub(crate) fn strip(root: &mut Value) -> RetiredConfigDeclaration {
    let mut declaration = RetiredConfigDeclaration::default();
    let Some(root) = root.as_table_mut() else {
        return declaration;
    };

    if let Some(Value::Table(catalog)) = root.get_mut("catalog") {
        if catalog.remove("graph").is_some() {
            declaration.record(
                "`[catalog.graph]` is ignored; the code graph is gone and has no replacement: use `rg` for code navigation and `effigy test --plan` to choose tests",
            );
        }
    }

    match root.remove("scan") {
        Some(Value::Table(mut scan)) => {
            for (key, statement) in RETIRED_SCAN_SECTIONS {
                if scan.remove(*key).is_some() {
                    declaration.record(*statement);
                }
            }
            if !scan.is_empty() {
                root.insert("scan".to_owned(), Value::Table(scan));
            }
        }
        Some(other) => {
            // Not a table: leave the type error to the deserializer.
            root.insert("scan".to_owned(), other);
        }
        None => {}
    }

    if root.remove("demos").is_some() {
        declaration.record(format!(
            "`[demos]` is ignored; {}",
            effigy_core::retirement_text::DEMO_TO_TASK_RECIPE
        ));
    }

    if root.remove("distribution").is_some() {
        declaration.record(
            "`[distribution]` is ignored; the distribution commands were removed in v0.15: declare the release in `[release]` (identities, gates, artifacts and targets, contract 054), and keep project-specific release checks as `[tasks]` wired as gates or `verify`",
        );
    }

    if root.remove("shell").is_some() {
        declaration.record(
            "`[shell]` is ignored; managed shell tabs always run `exec ${SHELL:-/bin/zsh} -i` and there is no manifest override",
        );
    }

    declaration
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::TaskManifest;

    fn parse(text: &str) -> TaskManifest {
        toml::from_str(text).expect("a retired v0.15 section keeps loading")
    }

    fn notice(manifest: &TaskManifest) -> String {
        manifest
            .retired_config
            .notice(Path::new("effigy.toml"))
            .expect("one deprecation notice")
    }

    #[test]
    fn every_retired_scanner_table_loads_and_is_named_once() {
        for (key, expected) in [
            ("attention_markers", "rg -n 'TODO|FIXME'"),
            ("comment_ratio", "no replacement"),
            ("generated_assets", ".gitignore"),
            ("generated_in_src", "generated output outside source trees"),
            ("stale_suppressions", "unused suppressions"),
            ("boundary_violations", "boundary layers are gone"),
            ("dead_code", "graph-backed scanner is gone"),
            ("validation_gaps", "effigy test --plan"),
        ] {
            let manifest = parse(&format!(
                "[scan.{key}]\nwarn = 1\n\n[tasks.build]\nrun = \"true\"\n"
            ));
            assert!(manifest.tasks.contains_key("build"), "{key}");
            assert!(manifest.scan.is_none(), "{key}: the table is dropped");
            let notice = notice(&manifest);
            assert!(
                notice.contains(&format!("`[scan.{key}]` is ignored")),
                "{key}: {notice}"
            );
            assert!(notice.contains(expected), "{key}: {notice}");
            assert_eq!(notice.matches(" is ignored").count(), 1, "{key}: {notice}");
        }
    }

    #[test]
    fn remaining_scanner_tables_still_configure_the_scan_section() {
        let manifest = parse(
            "[scan.god_files]\nwarn = 1200\n\n[scan.duplicate_blocks]\nwarn = 20\n\n[tasks.build]\nrun = \"true\"\n",
        );
        let scan = manifest.scan.expect("scan section");
        assert_eq!(
            scan.god_files.and_then(|god_files| god_files.warn),
            Some(1200)
        );
        assert!(scan.duplicate_blocks.is_some());
        assert!(manifest.retired_config.is_empty());
    }

    #[test]
    fn a_catalog_graph_table_is_ignored_and_membership_still_loads() {
        let manifest = parse(
            "[catalog]\nalias = \"bovine\"\n\n[catalog.graph]\nsegmented = true\n\n[tasks.build]\nrun = \"true\"\n",
        );
        assert_eq!(
            manifest
                .catalog
                .as_ref()
                .and_then(|catalog| catalog.alias.clone()),
            Some("bovine".to_owned())
        );
        let notice = notice(&manifest);
        assert!(notice.contains("`[catalog.graph]` is ignored"), "{notice}");
        assert!(notice.contains("`rg` for code navigation"), "{notice}");
    }

    #[test]
    fn a_demos_table_is_ignored_with_its_conversion_recipe() {
        let manifest = parse(
            "[demos.checkout]\ntitle = \"Checkout\"\nsummary = \"Pay for a cart\"\nrun = \"bun run demo\"\n\n[tasks.build]\nrun = \"true\"\n",
        );
        assert!(manifest.tasks.contains_key("build"));
        let notice = notice(&manifest);
        assert!(notice.contains("`[demos]` is ignored"), "{notice}");
        assert!(notice.contains("`[tasks.\"demo:<name>\"]`"), "{notice}");
        assert!(notice.contains("`[demos.checkout]`"), "{notice}");
        assert!(notice.contains("`[tasks.\"demo:checkout\"]`"), "{notice}");
        assert!(notice.contains("group = \"Demos\""), "{notice}");
        assert!(
            !notice.contains("external tool"),
            "recording sessions elsewhere is not the answer: {notice}"
        );
    }

    #[test]
    fn a_distribution_table_is_ignored_with_its_replacement() {
        let manifest = parse(
            "[distribution.package]\nname = \"my-tool\"\n\n[distribution.preflight]\ndocs-task = \"qa:docs\"\n\n[tasks.build]\nrun = \"true\"\n",
        );
        assert!(manifest.tasks.contains_key("build"));
        let notice = notice(&manifest);
        assert!(notice.contains("`[distribution]` is ignored"), "{notice}");
        assert!(notice.contains("`[release]`"), "{notice}");
        assert!(notice.contains("contract 054"), "{notice}");
    }

    #[test]
    fn a_shell_table_is_ignored_with_its_replacement() {
        let manifest = parse("[shell]\nrun = \"exec zsh -i\"\n\n[tasks.build]\nrun = \"true\"\n");
        assert!(manifest.tasks.contains_key("build"));
        let notice = notice(&manifest);
        assert!(notice.contains("`[shell]` is ignored"), "{notice}");
        assert!(notice.contains("${SHELL:-/bin/zsh}"), "{notice}");
        assert!(notice.contains("no manifest override"), "{notice}");
    }

    #[test]
    fn several_retired_sections_print_one_notice_per_manifest() {
        let manifest = parse(
            "[scan.attention_markers]\nwarn = 1\n\n[scan.generated_in_src]\ninclude = [\"src/**\"]\n\n[catalog.graph]\nsegmented = true\n\n[shell]\nrun = \"exec zsh -i\"\n\n[tasks.build]\nrun = \"true\"\n",
        );
        let notice = notice(&manifest);
        assert_eq!(notice.matches("effigy.toml").count(), 1, "{notice}");
        assert_eq!(notice.matches(" is ignored").count(), 4, "{notice}");
        assert_eq!(manifest.retired_config.statements().len(), 4);
    }

    #[test]
    fn a_plain_manifest_records_nothing() {
        let manifest = parse("[tasks.build]\nrun = \"true\"\n");
        assert!(manifest.retired_config.is_empty());
        assert!(manifest
            .retired_config
            .notice(Path::new("effigy.toml"))
            .is_none());
    }
}
