use std::fs;
use std::path::Path;

#[derive(Debug, Default)]
pub(super) struct RepoSetupContext {
    pub(super) has_package_json: bool,
    pub(super) has_makefile: bool,
    pub(super) has_cargo_aliases: bool,
    pub(super) has_graph_index: bool,
    pub(super) has_secrets: bool,
    pub(super) has_bundle: bool,
    pub(super) has_containers: bool,
    pub(super) has_state: bool,
    pub(super) has_deploy: bool,
    pub(super) has_distribution: bool,
    pub(super) has_release: bool,
    pub(super) has_qa_task: bool,
    pub(super) has_validate_task: bool,
}

pub(super) fn inspect_repo_setup_context(target_root: &Path) -> RepoSetupContext {
    let manifest_snippets = load_manifest_snippets(target_root);
    RepoSetupContext {
        has_package_json: target_root.join("package.json").is_file(),
        has_makefile: target_root.join("Makefile").is_file(),
        has_cargo_aliases: cargo_alias_config_present(target_root),
        has_graph_index: target_root.join(".effigy/graph/graph.db").is_file(),
        has_secrets: manifest_declares(&manifest_snippets, "secrets"),
        has_bundle: manifest_declares(&manifest_snippets, "bundle"),
        has_containers: manifest_declares(&manifest_snippets, "containers")
            || manifest_declares(&manifest_snippets, "systems")
            || manifest_declares(&manifest_snippets, "workspace"),
        has_state: manifest_declares(&manifest_snippets, "state"),
        has_deploy: manifest_declares(&manifest_snippets, "deploy"),
        has_distribution: manifest_declares(&manifest_snippets, "distribution"),
        has_release: manifest_declares(&manifest_snippets, "release"),
        has_qa_task: manifest_snippets.contains("[tasks.qa]")
            || manifest_snippets.contains("\"qa\""),
        has_validate_task: manifest_snippets.contains("[tasks.validate]")
            || manifest_snippets.contains("\"validate\""),
    }
}

fn load_manifest_snippets(target_root: &Path) -> String {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(target_root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("toml")
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name == "effigy.toml" || name.starts_with("effigy."))
            {
                files.push(path);
            }
        }
    }
    files.sort();
    files
        .into_iter()
        .filter_map(|path| fs::read_to_string(path).ok())
        .collect::<Vec<_>>()
        .join("\n")
}

fn cargo_alias_config_present(target_root: &Path) -> bool {
    [".cargo/config.toml", ".cargo/config"]
        .into_iter()
        .map(|path| target_root.join(path))
        .find(|path| path.is_file())
        .and_then(|path| fs::read_to_string(path).ok())
        .is_some_and(|contents| contents.contains("[alias]"))
}

fn manifest_declares(manifest_text: &str, section: &str) -> bool {
    let exact = format!("[{section}]");
    let nested = format!("[{section}.");
    manifest_text.contains(&exact) || manifest_text.contains(&nested)
}
