use super::{
    containers_lines, docs_policy_graph_lines, package_manager_lines, secrets_lines,
    tasks_canonical_lines, test_section_lines, ConfigDocProfile,
};

#[test]
fn containers_reference_documents_optional_dns_base_domain() {
    let reference = containers_lines(ConfigDocProfile::Reference);

    assert!(reference.contains(&"# Optional TCP alias and container-host cookie apex."));
    assert!(reference.contains(&"# Defaults to the first DNS route's domain."));
    assert!(reference.contains(&"# base_domain = \"project.test\""));
}

#[test]
fn docs_policy_graph_profile_lines_are_repository_neutral() {
    for profile in [ConfigDocProfile::Reference, ConfigDocProfile::Schema] {
        let lines = docs_policy_graph_lines(profile);
        let rendered = lines.join("\n");

        assert!(lines.contains(&"[docs_policy.graph]"));
        assert!(lines.contains(&"roots = [\"README.md\", \"docs\"]"));
        assert!(lines.contains(&"[docs_policy.graph.fields.state]"));
        assert!(lines.contains(&"[docs_policy.graph.kinds.reference]"));
        assert!(lines.contains(&"[docs_policy.graph.relations.related]"));
        assert!(lines.contains(&"default-currentness = \"historical\""));
        assert!(!lines.contains(&"default_currentness = \"historical\""));
        for northstar_token in [
            "strict-ready",
            "archived-spec",
            "docs/knowledge/contracts",
            "docs/knowledge/contracts/archive",
            "Next move",
        ] {
            assert!(
                !rendered.contains(northstar_token),
                "generic graph profile contains Northstar token `{northstar_token}`"
            );
        }
    }
}

#[test]
fn package_manager_profile_lines_contract_is_stable() {
    let reference = package_manager_lines(ConfigDocProfile::Reference);
    let schema = package_manager_lines(ConfigDocProfile::Schema);

    assert_eq!(reference[0], "[package_manager]");
    assert_eq!(
        reference[1],
        "# Preferred JS/TS package manager for built-in test runners."
    );
    assert_eq!(reference[2], "js = \"bun\"  # applies to JS/TS tooling");
    assert_eq!(schema[2], "js = \"bun\"");
}

#[test]
fn config_output_loads_through_the_manifest_loader() {
    // `effigy config` must emit a manifest the loader accepts. The retired
    // `[tasks.build.cache]` table used to be documented here while the runtime
    // refused it, so this loads the emitted file the way a user's does.
    let fixture = tempfile::tempdir().expect("private config fixture");
    for profile in [ConfigDocProfile::Reference, ConfigDocProfile::Schema] {
        let source = tasks_canonical_lines(profile).join("\n");
        assert!(!source.contains("[tasks.build.cache]"), "{source}");
        let path = fixture.path().join("effigy.toml");
        std::fs::write(&path, &source).expect("write config output");
        effigy_manifest::load_task_manifest(&path)
            .unwrap_or_else(|error| panic!("emitted config failed to load: {error}\n{source}"));
    }
}

#[test]
fn task_docs_surface_uses_system_workspace_contract() {
    let reference = tasks_canonical_lines(ConfigDocProfile::Reference);

    assert!(reference.contains(&"workspace = \"app\""));
    assert!(reference.contains(&"[systems.dev.workspaces.app]"));
    assert!(reference.contains(&"container = \"web\""));
}

#[test]
fn default_test_sections_reference_and_schema_contract_is_stable() {
    let reference = test_section_lines(true, ConfigDocProfile::Reference, None);
    let schema = test_section_lines(true, ConfigDocProfile::Schema, None);

    assert!(reference.contains(&"[test]"));
    assert!(reference.contains(&"cargo_env_match = \"prefix-aware\""));
    assert!(reference.contains(&"exclude_catalogs = [\"legacy\"]"));
    assert!(reference.contains(&"[test.suites.managed]"));
    assert!(reference.contains(&"teardown_policy = \"always\""));
    assert!(reference.contains(&"default = false"));
    assert!(reference.contains(&"[test.runners.vitest]"));
    assert!(schema.contains(&"[test]"));
    assert!(schema.contains(&"cargo_env_match = \"prefix-aware\""));
    assert!(schema.contains(&"[test.suites.managed]"));
    assert!(schema.contains(&"env = \"managed-test\""));
    assert!(!schema.contains(&"[test.runners.vitest]"));
}

#[test]
fn runner_filtered_test_section_contract_is_stable() {
    let filtered = test_section_lines(false, ConfigDocProfile::Schema, Some("cargo-nextest"));

    assert!(!filtered.contains(&"[test]"));
    assert!(filtered.contains(&"[test.runners]"));
    assert!(filtered.contains(&"\"cargo-nextest\" = \"cargo nextest run\""));
    assert!(!filtered.contains(&"vitest = \"bun x vitest run\""));
    assert!(!filtered.contains(&"\"cargo-test\" = \"cargo test\""));
}

#[test]
fn secrets_profiles_emit_supported_vault_config() {
    for profile in [ConfigDocProfile::Reference, ConfigDocProfile::Schema] {
        let source = secrets_lines(profile).join("\n");
        let manifest: effigy_manifest::TaskManifest =
            toml::from_str(&source).expect("generated secrets example must load");
        let vault = manifest
            .secrets
            .expect("secrets section")
            .vault
            .expect("vault config");
        assert_eq!(vault.path.as_deref(), Some(".effigy/secrets/local.vault"));
    }
}
