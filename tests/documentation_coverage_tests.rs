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
fn public_help_families_and_current_contract_paths_are_documented() {
    let matrix = read("docs/guides/025-command-reference-matrix.md");
    for route in [
        "`effigy version",
        "`effigy uninstall",
        "`effigy tasks migrate",
        "`effigy tasks unlock",
        "`effigy tasks cache",
        "`effigy config completion",
        "`effigy changelog",
        "`effigy scan",
        "`effigy rhai",
    ] {
        assert!(
            matrix.contains(route),
            "command matrix misses help family `{route}`"
        );
    }

    assert!(matrix.contains("status --refresh"));
    assert!(matrix.contains("EFFIGY_GRAPH_TIMEOUT_MS"));
    assert!(matrix.contains("[docs_policy.graph]"));

    let root = read("README.md");
    assert_contains_all(
        "root agent route",
        &root,
        &[
            "Route by job",
            "`effigy graph` for code understanding",
            "`effigy tasks` for selector inventory",
            "`effigy doctor` when routing or repo health is unclear",
            "`effigy test --plan` when test execution shape matters",
        ],
    );
    assert!(!root.contains("start with `doctor`, `tasks`, and `test --plan`"));
    let graph_guide = read("docs/guides/076-code-graph-and-agent-workflows.md");
    assert!(graph_guide.contains("Plain `graph status` is report-only"));
    assert!(graph_guide.contains("`--refresh` is the explicit mutating exception"));
    assert!(root.contains(".result.catalog_tasks[].task"));
    assert!(root.contains(".result.targets[].name"));
    assert!(!root.contains(".tasks[].name"));
    assert!(!root.contains(".selected_runner"));

    for relative in [
        "skills/effigy/references/json-envelope.md",
        ".agents/skills/effigy/references/json-envelope.md",
    ] {
        let contents = read(relative);
        assert!(
            contents.contains(".result.findings[]"),
            "{relative} misses live doctor path"
        );
        assert!(
            contents.contains(".error.kind"),
            "{relative} misses live error path"
        );
        assert!(
            !contents.contains(".result.payload.checks[]"),
            "{relative} keeps stale doctor path"
        );
        assert!(
            !contents.contains("effigy --json completion candidates"),
            "{relative} keeps stale completion route"
        );
    }

    for relative in [
        "skills/effigy/references/workflow-shortcuts.md",
        ".agents/skills/effigy/references/workflow-shortcuts.md",
    ] {
        let contents = read(relative);
        assert!(contents.contains("effigy --json changelog extract"));
        assert!(!contents.contains("effigy changelog --json extract"));
    }
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
