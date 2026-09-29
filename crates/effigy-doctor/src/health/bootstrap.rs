use std::path::{Path, PathBuf};

use effigy_manifest::{ManifestJsPackageManager, TaskManifest};
use globset::Glob;

/// A JS dependency bootstrap gap for the selected catalog scope.
///
/// The selected catalog declares a JS package manager and a `package.json`
/// with runtime dependencies, but the scope has no local `node_modules` and no
/// verified shared workspace whose install root would supply one. Running the
/// `health` task from that scope lets Node/Bun resolve packages from an
/// ancestor checkout and report a misleading task failure instead of the
/// missing bootstrap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct JsBootstrapGap {
    pub(super) manager: &'static str,
    pub(super) scope_root: PathBuf,
    pub(super) package_json: PathBuf,
    /// Directory that must carry the local `node_modules` install.
    pub(super) install_root: PathBuf,
    /// Nearest ancestor (or the scope itself) that carries the manager's lock.
    pub(super) lock_root: PathBuf,
    pub(super) lock_name: String,
    /// `true` when the nearest lock belongs to an ancestor that does not
    /// declare this scope as a workspace member.
    pub(super) unverified_ancestor: bool,
    route: String,
}

impl JsBootstrapGap {
    pub(super) fn evidence(&self) -> String {
        let mut evidence = format!(
            "observed=missing-local-install; manager={}; scope={}; install_root={}; lock_root={}; lock={}; package_json={}; node_modules={}",
            self.manager,
            self.scope_root.display(),
            self.install_root.display(),
            self.lock_root.display(),
            self.lock_name,
            self.package_json.display(),
            self.install_root.join("node_modules").display(),
        );
        if self.unverified_ancestor {
            evidence.push_str("; reason=ancestor-lock-without-workspace-membership");
        }
        evidence
    }

    pub(super) fn remediation(&self) -> String {
        if self.unverified_ancestor {
            let bootstrap_command = if self.manager == "bun" {
                format!("effigy bootstrap deps sync --refresh-lock {}", self.route)
            } else {
                format!("effigy bootstrap deps sync {}", self.route)
            };
            return format!(
                "The nearest {} lock at `{}` does not declare `{}` as a workspace member. Give this checkout its own lock and local install with `{bootstrap_command}` (or add it to the owning workspace), then rerun `effigy doctor --deep`.",
                self.manager,
                self.lock_root.display(),
                self.scope_root.display(),
            );
        }
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
/// `node_modules` rule. A scope's own lock always requires a scope-local
/// install. An ancestor lock only satisfies the scope when the ancestor's
/// `package.json` declares the scope as a JS workspace member; otherwise the
/// ancestor is a foreign install that Node/Bun must not be allowed to leak in.
/// Both the lock search and the membership check stay inside the scope's
/// repository boundary and never rise above the workspace root.
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
    let lock_root = locked_install_root(scope_root, workspace_root, lock_names)?;
    let lock_name = lock_names
        .iter()
        .find(|lock| lock_root.join(lock).is_file())?
        .to_string();

    let shared_member =
        lock_root != scope_root && is_declared_workspace_member(lock_root, scope_root);
    let unverified_ancestor = lock_root != scope_root && !shared_member;
    let install_root = if shared_member {
        lock_root.to_path_buf()
    } else {
        scope_root.to_path_buf()
    };
    if install_root.join("node_modules").is_dir() {
        return None;
    }

    Some(JsBootstrapGap {
        manager: manager_label,
        scope_root: scope_root.to_path_buf(),
        package_json,
        install_root: install_root.clone(),
        lock_root: lock_root.to_path_buf(),
        lock_name,
        unverified_ancestor,
        route: bootstrap_route(&install_root, workspace_root),
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
/// The walk stops at the scope's repository boundary (the nearest ancestor
/// `.git`, when present) and at the workspace root, so a lock outside either
/// boundary cannot satisfy the scope's requirement.
fn locked_install_root<'a>(
    scope_root: &'a Path,
    workspace_root: &Path,
    lock_names: &[&str],
) -> Option<&'a Path> {
    let repo_root = scope_root
        .ancestors()
        .find(|ancestor| ancestor.join(".git").exists());
    for ancestor in scope_root.ancestors() {
        if lock_names.iter().any(|lock| ancestor.join(lock).is_file()) {
            return Some(ancestor);
        }
        if Some(ancestor) == repo_root || ancestor == workspace_root {
            return None;
        }
    }
    None
}

/// Whether `ancestor`'s `package.json` declares `scope_root` as a workspace
/// member through a matching `workspaces` pattern.
fn is_declared_workspace_member(ancestor: &Path, scope_root: &Path) -> bool {
    let Ok(raw) = std::fs::read_to_string(ancestor.join("package.json")) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return false;
    };
    let Some(patterns) = workspace_patterns(&value) else {
        return false;
    };
    let Ok(relative) = scope_root.strip_prefix(ancestor) else {
        return false;
    };
    let relative = relative.to_string_lossy().replace('\\', "/");
    if relative.is_empty() {
        return false;
    }
    patterns.iter().any(|pattern| {
        let pattern = pattern
            .trim()
            .trim_start_matches("./")
            .trim_end_matches('/');
        if pattern.is_empty() {
            return false;
        }
        Glob::new(pattern)
            .map(|glob| glob.compile_matcher().is_match(&relative))
            .unwrap_or(false)
    })
}

fn workspace_patterns(value: &serde_json::Value) -> Option<Vec<String>> {
    let entries = match value.get("workspaces")? {
        serde_json::Value::Array(entries) => entries,
        serde_json::Value::Object(map) => map.get("packages")?.as_array()?,
        _ => return None,
    };
    let patterns = entries
        .iter()
        .filter_map(|value| value.as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    if patterns.is_empty() {
        None
    } else {
        Some(patterns)
    }
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

    fn write_workspace_root(dir: &Path, workspaces: Option<&str>) {
        mark_repo(dir);
        let body = match workspaces {
            Some(patterns) => format!(r#"{{"private":true,"workspaces":{patterns}}}"#),
            None => r#"{"private":true}"#.to_owned(),
        };
        write(&dir.join("package.json"), &body);
        write(&dir.join("bun.lock"), "");
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
        assert_eq!(gap.install_root, child);
        assert_eq!(gap.lock_name, "bun.lock");
        assert!(!gap.unverified_ancestor);
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
    fn declared_workspace_member_shares_the_ancestor_install() {
        let root = temp_root("shared");
        let workspace = root.join("workspace");
        let child = workspace.join("child");
        let manifest = manifest(&child);
        write_workspace_root(&workspace, Some(r#"["child"]"#));
        write(
            &child.join("package.json"),
            r#"{"dependencies":{"left-pad":"1.3.0"}}"#,
        );
        // Only the workspace root locks and installs; the child is a declared
        // member and intentionally shares it.
        std::fs::create_dir_all(workspace.join("node_modules")).expect("shared install");

        assert!(health_js_bootstrap_gap(&child, &workspace, &manifest).is_none());

        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn ancestor_lock_without_workspace_membership_is_a_gap() {
        let root = temp_root("foreign");
        let workspace = root.join("workspace");
        let child = workspace.join("child");
        let manifest = manifest(&child);
        // Parent locks and installs but never declares the child a member.
        write_workspace_root(&workspace, None);
        write(
            &child.join("package.json"),
            r#"{"dependencies":{"left-pad":"1.3.0"}}"#,
        );
        std::fs::create_dir_all(workspace.join("node_modules")).expect("parent install");

        let gap = health_js_bootstrap_gap(&child, &workspace, &manifest).expect("gap");

        assert!(gap.unverified_ancestor);
        assert_eq!(gap.install_root, child);
        assert_eq!(gap.route, "child");
        assert!(gap
            .evidence()
            .contains("reason=ancestor-lock-without-workspace-membership"));
        assert!(gap
            .remediation()
            .contains("effigy bootstrap deps sync --refresh-lock child"));

        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn ancestor_workspace_pattern_that_excludes_the_child_is_a_gap() {
        let root = temp_root("excluded");
        let workspace = root.join("workspace");
        let child = workspace.join("child");
        let manifest = manifest(&child);
        write_workspace_root(&workspace, Some(r#"["packages/*"]"#));
        write(
            &child.join("package.json"),
            r#"{"dependencies":{"left-pad":"1.3.0"}}"#,
        );
        std::fs::create_dir_all(workspace.join("node_modules")).expect("parent install");

        let gap = health_js_bootstrap_gap(&child, &workspace, &manifest).expect("gap");

        assert!(gap.unverified_ancestor);

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

    #[test]
    fn workspace_member_detection_matches_common_patterns() {
        let root = temp_root("membership");
        let workspace = root.join("workspace");
        let child = workspace.join("packages").join("core");
        write_workspace_root(&workspace, Some(r#"["packages/*"]"#));

        assert!(is_declared_workspace_member(&workspace, &child));
        assert!(!is_declared_workspace_member(&workspace, &workspace));

        std::fs::remove_dir_all(root).ok();
    }
}
