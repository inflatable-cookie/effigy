use super::{package_manager_lines, tasks_canonical_lines, test_section_lines, ConfigDocProfile};

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
fn tasks_cache_comment_profile_contract_is_stable() {
    let reference = tasks_canonical_lines(ConfigDocProfile::Reference);
    let schema = tasks_canonical_lines(ConfigDocProfile::Schema);

    assert!(
        reference.contains(&"# Phase-1 task cache contract: explicit opt-in declarations only.")
    );
    assert!(
        schema.contains(&"# Phase-1 cache contract: explicit opt-in only, no implicit discovery.")
    );
    assert!(reference.contains(&"[tasks.build.cache]"));
    assert!(schema.contains(&"[tasks.build.cache]"));
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
