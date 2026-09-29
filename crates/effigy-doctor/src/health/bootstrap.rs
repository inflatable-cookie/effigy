use std::path::{Path, PathBuf};

use effigy_manifest::{ManifestJsPackageManager, TaskManifest};

/// A JS dependency bootstrap gap for the selected catalog scope.
///
/// The selected catalog declares a JS package manager and a `package.json`
/// with runtime dependencies, but the owning locked install root has no local
/// `node_modules`. Running the `health` task from that scope lets Node/Bun
/// resolve packages from an ancestor checkout and report a misleading task
/// failure instead of the missing bootstrap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct JsBootstrapGap {
    pub(super) manager: &'static str,
    pub(super) scope_root: PathBuf,
    pub(super) package_json: PathBuf,
    pub(super) install_root: PathBuf,
    pub(super) lock_name: String,
    route: String,
}

impl JsBootstrapGap {
    pub(super) fn evidence(&self) -> String {
        format!(
            "observed=missing-local-install; manager={}; scope={}; install_root={}; lock={}; package_json={}; node_modules={}",
            self.manager,
            self.scope_root.display(),
            self.install_root.display(),
            self.lock_name,
            self.package_json.display(),
            self.install_root.join("node_modules").display(),
        )
    }

    pub(super) fn remediation(&self) -> String {
        format!(
            "JavaScript dependencies are missing for `{}`; prepare this checkout with `effigy bootstrap deps sync {}` ({} install), then rerun `effigy doctor --deep`.",
            self.route, self.route, self.manager,
        )
    }
}

/// Returns the missing JS dependency bootstrap for `scope_root`, if any.
///
/// The check is driven by the selected catalog's declared `[package_manager].js`
/// and the manager's committed lock evidence, never by a universal
/// `node_modules` rule. It resolves the owning locked install root from the
/// scope upward, bounded by the scope's repository boundary and the workspace
/// root, so a parent installation only satisfies the requirement when the
/// scope intentionally shares that ancestor's locked workspace. A scope with
/// its own lock always requires its own local install.
pub(super) fn health_js_bootstrap_gap(
    scope_root: &Path,
    workspace_root: &Path,
    manifest: &TaskManifest,
) -> Option<JsBootstrapGap> {
    let manager = manifest
        .package_manager
        .as_ref()
        .and_then(|config| config.js)?;
    if manager == ManifestJsPackageManager::Direct {
        return None;
    }

    let package_json = scope_root.join("package.json");
    if !package_json.is_file() || !declares_js_dependencies(&package_json) {
        return None;
    }

    let (manager_label, lock_names) = manager_evidence(manager);
    let install_root = locked_install_root(scope_root, workspace_root, lock_names)?;
    let lock_name = lock_names
        .iter()
        .find(|lock| install_root.join(lock).is_file())?
        .to_string();
    if install_root.join("node_modules").is_dir() {
        return None;
    }

    Some(JsBootstrapGap {
        manager: manager_label,
        scope_root: scope_root.to_path_buf(),
        package_json,
        install_root: install_root.to_path_buf(),
        lock_name,
        route: bootstrap_route(install_root, workspace_root),
    })
}

fn manager_evidence(manager: ManifestJsPackageManager) -> (&'static str, &'static [&'static str]) {
    match manager {
        ManifestJsPackageManager::Bun => ("bun", &["bun.lock", "bun.lockb"]),
        ManifestJsPackageManager::Pnpm => ("pnpm", &["pnpm-lock.yaml"]),
        ManifestJsPackageManager::Npm => ("npm", &["package-lock.json", "npm-shrinkwrap.json"]),
        ManifestJsPackageManager::Direct => ("direct", &[]),
    }
}

/// Nearest ancestor of `scope_root` that carries the manager's committed lock.
///
/// The walk stays inside the scope's repository boundary (the nearest
/// ancestor `.git`) and never rises above the workspace root, so a lock
/// outside either boundary cannot satisfy the scope's requirement.
fn locked_install_root<'a>(
    scope_root: &'a Path,
    workspace_root: &Path,
    lock_names: &[&str],
) -> Option<&'a Path> {
    let repo_root = scope_root
        .ancestors()
        .find(|ancestor| ancestor.join(".git").exists())
        .unwrap_or(scope_root);
    for ancestor in scope_root.ancestors() {
        if lock_names.iter().any(|lock| ancestor.join(lock).is_file()) {
            return Some(ancestor);
        }
        if ancestor == repo_root || ancestor == workspace_root {
            return None;
        }
    }
    None
}

fn bootstrap_route(install_root: &Path, workspace_root: &Path) -> String {
    match install_root.strip_prefix(workspace_root) {
        Ok(relative) if relative.as_os_str().is_empty() => ".".to_owned(),
        Ok(relative) => relative.display().to_string(),
        Err(_) => install_root.display().to_string(),
    }
}

fn declares_js_dependencies(package_json: &Path) -> bool {
    let Ok(raw) = std::fs::read_to_string(package_json) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return false;
    };
    [
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
    ]
    .iter()
    .any(|key| {
        value
            .get(key)
            .and_then(|deps| deps.as_object())
            .is_some_and(|deps| !deps.is_empty())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use effigy_manifest::load_task_manifest;

    fn write(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("mkdir");
        }
        std::fs::write(path, contents).expect("write");
    }

    fn manifest(dir: &Path) -> TaskManifest {
        let path = dir.join("effigy.toml");
        write(&path, "[package_manager]\njs = \"bun\"\n");
        load_task_manifest(&path).expect("manifest")
    }

    fn temp_root(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "effigy-doctor-health-bootstrap-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("mkdir root");
        root
    }

    fn mark_repo(dir: &Path) {
        std::fs::create_dir_all(dir.join(".git")).expect("git boundary");
    }

    #[test]
    fn child_with_own_lock_and_no_local_install_is_a_gap() {
        let root = temp_root("gap");
        let workspace = root.join("workspace");
        let child = workspace.join("child");
        let manifest = manifest(&child);
        mark_repo(&workspace);
        write(
            &child.join("package.json"),
            r#"{"dependencies":{"left-pad":"1.3.0"}}"#,
        );
        write(&child.join("bun.lock"), "");
        // The parent workspace provides the package.
        std::fs::create_dir_all(workspace.join("node_modules")).expect("parent install");

        let gap = health_js_bootstrap_gap(&child, &workspace, &manifest).expect("gap");

        assert_eq!(gap.manager, "bun");
        assert_eq!(gap.route, "child");
        assert_eq!(gap.lock_name, "bun.lock");
        assert!(gap
            .remediation()
            .contains("effigy bootstrap deps sync child"));
        assert!(gap.evidence().contains("observed=missing-local-install"));

        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn child_with_local_install_is_not_a_gap() {
        let root = temp_root("installed");
        let workspace = root.join("workspace");
        let child = workspace.join("child");
        let manifest = manifest(&child);
        mark_repo(&workspace);
        write(
            &child.join("package.json"),
            r#"{"dependencies":{"left-pad":"1.3.0"}}"#,
        );
        write(&child.join("bun.lock"), "");
        std::fs::create_dir_all(child.join("node_modules")).expect("child install");

        assert!(health_js_bootstrap_gap(&child, &workspace, &manifest).is_none());

        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn parent_workspace_install_satisfies_a_child_without_its_own_lock() {
        let root = temp_root("shared");
        let workspace = root.join("workspace");
        let child = workspace.join("child");
        let manifest = manifest(&child);
        mark_repo(&workspace);
        write(
            &child.join("package.json"),
            r#"{"dependencies":{"left-pad":"1.3.0"}}"#,
        );
        // Only the workspace root locks and installs; the child intentionally
        // shares it.
        write(&workspace.join("bun.lock"), "");
        std::fs::create_dir_all(workspace.join("node_modules")).expect("shared install");

        assert!(health_js_bootstrap_gap(&child, &workspace, &manifest).is_none());

        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn dependency_free_or_non_js_child_is_not_a_gap() {
        let root = temp_root("free");
        let workspace = root.join("workspace");
        let child = workspace.join("child");
        let manifest = manifest(&child);
        mark_repo(&workspace);
        write(&child.join("package.json"), r#"{"dependencies":{}}"#);
        write(&child.join("bun.lock"), "");

        assert!(health_js_bootstrap_gap(&child, &workspace, &manifest).is_none());

        let non_js = workspace.join("non-js");
        write(&non_js.join("effigy.toml"), "[tasks]\nhealth = \"true\"\n");
        let non_js_manifest = load_task_manifest(&non_js.join("effigy.toml")).expect("manifest");
        write(
            &non_js.join("package.json"),
            r#"{"dependencies":{"left-pad":"1.3.0"}}"#,
        );
        write(&non_js.join("bun.lock"), "");
        assert!(health_js_bootstrap_gap(&non_js, &workspace, &non_js_manifest).is_none());

        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn child_without_lock_evidence_is_not_a_gap() {
        let root = temp_root("no-lock");
        let workspace = root.join("workspace");
        let child = workspace.join("child");
        let manifest = manifest(&child);
        mark_repo(&workspace);
        write(
            &child.join("package.json"),
            r#"{"dependencies":{"left-pad":"1.3.0"}}"#,
        );

        assert!(health_js_bootstrap_gap(&child, &workspace, &manifest).is_none());

        std::fs::remove_dir_all(root).ok();
    }
}
