use effigy_core::builtin_tasks::BUILTIN_TASKS;
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(relative: &str) -> String {
    fs::read_to_string(repo_root().join(relative))
        .unwrap_or_else(|error| panic!("read {relative}: {error}"))
}

fn collect_relative_files(root: &Path, current: &Path, files: &mut BTreeSet<PathBuf>) {
    for entry in fs::read_dir(current)
        .unwrap_or_else(|error| panic!("read directory {}: {error}", current.display()))
    {
        let path = entry.expect("read directory entry").path();
        if path.is_dir() {
            collect_relative_files(root, &path, files);
        } else {
            files.insert(
                path.strip_prefix(root)
                    .expect("relative skill path")
                    .to_owned(),
            );
        }
    }
}

fn skill_files(root: &Path) -> BTreeSet<PathBuf> {
    let mut files = BTreeSet::new();
    collect_relative_files(root, root, &mut files);
    files
}

fn without_internal_metadata(contents: &str) -> String {
    contents.replacen("metadata:\n  internal: true\n", "", 1)
}

fn assert_contains_all(surface: &str, contents: &str, required: &[&str]) {
    for token in required {
        assert!(
            contents.contains(token),
            "{surface} is missing required documentation token `{token}`"
        );
    }
}

#[test]
fn project_local_and_distributed_effigy_skills_have_semantic_parity() {
    let local_root = repo_root().join(".agents/skills/effigy");
    let distributed_root = repo_root().join("skills/effigy");
    let local_files = skill_files(&local_root);
    let distributed_files = skill_files(&distributed_root);

    assert_eq!(
        local_files, distributed_files,
        "skill file inventories drifted"
    );

    for relative in local_files {
        let local = fs::read_to_string(local_root.join(&relative)).expect("read local skill file");
        let distributed = fs::read_to_string(distributed_root.join(&relative))
            .expect("read distributed skill file");
        if relative == Path::new("SKILL.md") {
            assert_eq!(
                without_internal_metadata(&local),
                distributed,
                "skill instructions drifted outside the local internal metadata block"
            );
        } else {
            assert_eq!(local, distributed, "skill reference drifted: {relative:?}");
        }
    }
}

#[test]
fn public_builtin_registry_routes_through_the_command_reference() {
    let matrix = read("docs/guides/025-command-reference-matrix.md");
    for (name, _) in BUILTIN_TASKS {
        let route = format!("`effigy {name}");
        assert!(
            matrix.contains(&route),
            "command reference does not route built-in family `{name}`"
        );
    }

    assert!(matrix.contains("no top-level `effigy distribution` command"));
}

#[test]
fn managed_runtime_seed_behavior_has_active_discovery_routes() {
    let seed_tokens = [
        "--headless",
        "EFFIGY_MANAGED_HEADLESS=1",
        "status",
        "logs",
        "stop",
        "health_wait_timeout_secs",
        "container.workspace-ownership",
        "workspace_user",
        "non-console",
    ];

    assert_contains_all(
        "command reference",
        &read("docs/guides/025-command-reference-matrix.md"),
        &seed_tokens,
    );
    assert_contains_all(
        "project-local skill",
        &read(".agents/skills/effigy/SKILL.md"),
        &seed_tokens,
    );
    assert_contains_all(
        "troubleshooting guide",
        &read("docs/guides/023-troubleshooting-and-failure-recipes.md"),
        &[
            "--headless",
            "EFFIGY_MANAGED_HEADLESS=1",
            "health_wait_timeout_secs",
            "container.workspace-ownership",
            "non-console",
        ],
    );
    assert_contains_all(
        "root front door",
        &read("README.md"),
        &[
            "--headless",
            "EFFIGY_MANAGED_HEADLESS=1",
            "012-dev-process-manager-tui.md",
        ],
    );
    assert_contains_all(
        "docs front door",
        &read("docs/README.md"),
        &["012-dev-process-manager-tui.md"],
    );
}
