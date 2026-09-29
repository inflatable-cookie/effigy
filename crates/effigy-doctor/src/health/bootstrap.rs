use std::path::{Path, PathBuf};

use effigy_manifest::{ManifestJsPackageManager, TaskManifest};
use globset::GlobBuilder;

/// Why the selected catalog scope has no usable local JS install.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BootstrapReason {
    /// No committed lock exists anywhere in the scope's repository boundary.
    MissingLocalLock,
    /// A lock exists but its required install root has no local install.
    MissingLocalInstall,
    /// The nearest lock belongs to an ancestor that does not declare this
    /// scope as a workspace member.
    UnverifiedAncestorLock,
}

impl BootstrapReason {
    fn label(self) -> &'static str {
        match self {
            Self::MissingLocalLock => "missing-local-lock",
            Self::MissingLocalInstall => "missing-local-install",
            Self::UnverifiedAncestorLock => "ancestor-lock-without-workspace-membership",
        }
    }

    /// Bun needs an explicit lock seed through `--refresh-lock`; pnpm and npm
    /// installs can create their own lock. Every other reason is a plain
    /// frozen install into the named route.
    fn bootstrap_command(self, manager: &str, route: &str) -> String {
        match self {
            Self::MissingLocalLock | Self::UnverifiedAncestorLock if manager == "bun" => {
                format!("effigy bootstrap deps sync --refresh-lock {route}")
            }
            _ => format!("effigy bootstrap deps sync {route}"),
        }
    }
}

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
    pub(super) lock_root: Option<PathBuf>,
    pub(super) lock_name: Option<String>,
    pub(super) reason: BootstrapReason,
    route: String,
}

impl JsBootstrapGap {
    pub(super) fn evidence(&self) -> String {
        let lock_root = self
            .lock_root
            .as_ref()
            .map(|root| root.display().to_string())
            .unwrap_or_else(|| "<none>".to_owned());
        let lock_name = self.lock_name.as_deref().unwrap_or("<none>");
        format!(
            "observed=missing-local-install; reason={}; manager={}; scope={}; install_root={}; lock_root={lock_root}; lock={lock_name}; package_json={}; node_modules={}",
            self.reason.label(),
            self.manager,
            self.scope_root.display(),
            self.install_root.display(),
            self.package_json.display(),
            self.install_root.join("node_modules").display(),
        )
    }

    pub(super) fn remediation(&self) -> String {
        let bootstrap_command = self.reason.bootstrap_command(self.manager, &self.route);
        match self.reason {
            BootstrapReason::UnverifiedAncestorLock => {
                let lock_root = self
                    .lock_root
                    .as_ref()
                    .map(|root| root.display().to_string())
                    .unwrap_or_else(|| "<none>".to_owned());
                format!(
                    "The nearest {} lock at `{lock_root}` does not declare `{}` as a workspace member. Give this checkout its own lock and local install with `{bootstrap_command}` (or add it to the owning workspace), then rerun `effigy doctor --deep`.",
                    self.manager,
                    self.scope_root.display(),
                )
            }
            BootstrapReason::MissingLocalLock => format!(
                "No committed {} lock or local `node_modules` was found within this checkout. Create them with `{bootstrap_command}` for `{}`, then rerun `effigy doctor --deep`.",
                self.manager,
                self.route,
            ),
            BootstrapReason::MissingLocalInstall => format!(
                "JavaScript dependencies are missing for `{}`; prepare this checkout with `{bootstrap_command}` ({} install), then rerun `effigy doctor --deep`.",
                self.route, self.manager,
            ),
        }
    }
}

/// Returns the missing JS dependency bootstrap for `scope_root`, if any.
///
/// The check is driven by the selected catalog's declared `[package_manager].js`
/// and the manager's committed lock evidence, never by a universal
/// `node_modules` rule. A scope with a local `node_modules` is always
/// considered bootstrapped. Otherwise, the scope's own lock requires a
/// scope-local install; an ancestor lock only satisfies the scope when it
/// declares the scope as a manager-authoritative workspace member inside the
/// same repository boundary; and a scope with no lock at all inside its own
/// repository boundary is reported as missing its bootstrap. Both the lock
/// search and the membership check stay inside the scope's repository boundary
/// and never rise above the workspace root.
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
    if scope_root.join("node_modules").is_dir() {
        return None;
    }

    let (manager_label, lock_names) = manager_evidence(manager);
    let lock_root = locked_install_root(scope_root, workspace_root, lock_names);
    let lock_name = match lock_root {
        Some(root) => lock_names
            .iter()
            .find(|lock| root.join(*lock).is_file())
            .map(|lock| (*lock).to_owned()),
        None => None,
    };

    let (install_root, reason) = match lock_root {
        None => (scope_root.to_path_buf(), BootstrapReason::MissingLocalLock),
        Some(root) if root == scope_root => (
            scope_root.to_path_buf(),
            BootstrapReason::MissingLocalInstall,
        ),
        Some(root) if is_declared_workspace_member(root, scope_root, manager) => {
            if root.join("node_modules").is_dir() {
                return None;
            }
            (root.to_path_buf(), BootstrapReason::MissingLocalInstall)
        }
        Some(_) => (
            scope_root.to_path_buf(),
            BootstrapReason::UnverifiedAncestorLock,
        ),
    };

    Some(JsBootstrapGap {
        manager: manager_label,
        scope_root: scope_root.to_path_buf(),
        package_json,
        install_root: install_root.clone(),
        lock_root: lock_root.map(Path::to_path_buf),
        lock_name,
        reason,
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

/// Whether the ancestor declares `scope_root` as a workspace member.
///
/// The declared manager selects the authoritative membership source rather
/// than unioning them: pnpm reads only `pnpm-workspace.yaml` `packages`, while
/// Bun and npm read only `package.json` `workspaces`. Both support `!`
/// exclusions. A `*` never crosses a path separator, so a nested path under a
/// matched workspace package is not itself a member.
fn is_declared_workspace_member(
    ancestor: &Path,
    scope_root: &Path,
    manager: ManifestJsPackageManager,
) -> bool {
    let Ok(relative) = scope_root.strip_prefix(ancestor) else {
        return false;
    };
    let relative = relative.to_string_lossy().replace('\\', "/");
    if relative.is_empty() {
        return false;
    }
    let patterns = workspace_membership_patterns(ancestor, manager);
    membership_matches(&patterns, &relative)
}

fn workspace_membership_patterns(
    ancestor: &Path,
    manager: ManifestJsPackageManager,
) -> Vec<String> {
    match manager {
        // `pnpm-workspace.yaml` is the sole authority for pnpm workspaces;
        // unioning `package.json` `workspaces` can re-include an excluded
        // child and let it borrow a parent install.
        ManifestJsPackageManager::Pnpm => pnpm_workspace_patterns(ancestor),
        _ => package_json_workspace_patterns(ancestor).unwrap_or_default(),
    }
}

fn membership_matches(patterns: &[String], relative: &str) -> bool {
    let mut included = false;
    for raw in patterns {
        let raw = raw.trim();
        let (exclusion, pattern) = match raw.strip_prefix('!') {
            Some(rest) => (true, rest.trim()),
            None => (false, raw),
        };
        let pattern = pattern.trim_start_matches("./").trim_end_matches('/');
        if pattern.is_empty() {
            continue;
        }
        let Ok(glob) = GlobBuilder::new(pattern).literal_separator(true).build() else {
            continue;
        };
        if glob.compile_matcher().is_match(relative) {
            if exclusion {
                return false;
            }
            included = true;
        }
    }
    included
}

fn package_json_workspace_patterns(ancestor: &Path) -> Option<Vec<String>> {
    let raw = std::fs::read_to_string(ancestor.join("package.json")).ok()?;
    let value = serde_json::from_str::<serde_json::Value>(&raw).ok()?;
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

#[derive(Debug, Default, serde::Deserialize)]
struct PnpmWorkspaceManifest {
    #[serde(default)]
    packages: Vec<String>,
}

fn pnpm_workspace_patterns(ancestor: &Path) -> Vec<String> {
    let Ok(raw) = std::fs::read_to_string(ancestor.join("pnpm-workspace.yaml")) else {
        return Vec::new();
    };
    serde_yaml::from_str::<PnpmWorkspaceManifest>(&raw)
        .map(|manifest| manifest.packages)
        .unwrap_or_default()
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
        assert_eq!(gap.lock_name.as_deref(), Some("bun.lock"));
        assert_eq!(gap.reason, BootstrapReason::MissingLocalInstall);
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

        assert_eq!(gap.reason, BootstrapReason::UnverifiedAncestorLock);
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

        assert_eq!(gap.reason, BootstrapReason::UnverifiedAncestorLock);

        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn standalone_child_with_its_own_git_boundary_needs_a_local_bootstrap() {
        let root = temp_root("standalone");
        let workspace = root.join("workspace");
        let child = workspace.join("child");
        let manifest = manifest(&child);
        write_workspace_root(&workspace, None);
        mark_repo(&child);
        write(
            &child.join("package.json"),
            r#"{"dependencies":{"left-pad":"1.3.0"}}"#,
        );
        // The ancestor provides a lock and install, but the child is its own
        // repository with no lock and must not borrow it.
        std::fs::create_dir_all(workspace.join("node_modules")).expect("ancestor install");

        let gap = health_js_bootstrap_gap(&child, &workspace, &manifest).expect("gap");

        assert_eq!(gap.reason, BootstrapReason::MissingLocalLock);
        assert_eq!(gap.install_root, child);
        assert_eq!(gap.route, "child");
        assert!(gap.evidence().contains("reason=missing-local-lock"));
        assert!(gap
            .remediation()
            .contains("effigy bootstrap deps sync --refresh-lock child"));

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
    fn child_without_any_lock_or_install_is_a_gap() {
        let root = temp_root("no-lock");
        let workspace = root.join("workspace");
        let child = workspace.join("child");
        let manifest = manifest(&child);
        mark_repo(&workspace);
        write(
            &child.join("package.json"),
            r#"{"dependencies":{"left-pad":"1.3.0"}}"#,
        );

        let gap = health_js_bootstrap_gap(&child, &workspace, &manifest).expect("gap");

        assert_eq!(gap.reason, BootstrapReason::MissingLocalLock);
        assert_eq!(gap.install_root, child);
        assert_eq!(gap.route, "child");
        assert!(gap.lock_root.is_none());
        assert!(gap.lock_name.is_none());
        assert!(gap.evidence().contains("reason=missing-local-lock"));

        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn workspace_member_detection_matches_common_patterns() {
        let root = temp_root("membership");
        let workspace = root.join("workspace");
        let child = workspace.join("packages").join("core");
        write_workspace_root(&workspace, Some(r#"["packages/*"]"#));

        assert!(is_declared_workspace_member(
            &workspace,
            &child,
            ManifestJsPackageManager::Bun
        ));
        assert!(!is_declared_workspace_member(
            &workspace,
            &workspace,
            ManifestJsPackageManager::Bun
        ));
        // A nested path below a matched workspace package is not itself a
        // declared member: `*` never crosses a path separator.
        assert!(!is_declared_workspace_member(
            &workspace,
            &child.join("nested"),
            ManifestJsPackageManager::Bun
        ));
        assert!(!is_declared_workspace_member(
            &workspace,
            &workspace.join("tools").join("nested"),
            ManifestJsPackageManager::Bun
        ));

        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn pnpm_workspace_yaml_declares_membership() {
        let root = temp_root("pnpm-membership");
        let workspace = root.join("workspace");
        let member = workspace.join("packages").join("core");
        let outsider = workspace.join("packages").join("core").join("nested");
        std::fs::create_dir_all(&workspace).expect("mkdir workspace");
        write(
            &workspace.join("pnpm-workspace.yaml"),
            "packages:\n  - 'packages/*'\n",
        );

        assert!(is_declared_workspace_member(
            &workspace,
            &member,
            ManifestJsPackageManager::Pnpm
        ));
        assert!(!is_declared_workspace_member(
            &workspace,
            &outsider,
            ManifestJsPackageManager::Pnpm
        ));

        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn pnpm_workspace_exclusion_removes_membership() {
        let root = temp_root("pnpm-exclusion");
        let workspace = root.join("workspace");
        let member = workspace.join("packages").join("core");
        std::fs::create_dir_all(&workspace).expect("mkdir workspace");
        write(
            &workspace.join("pnpm-workspace.yaml"),
            "packages:\n  - 'packages/*'\n  - '!packages/core'\n",
        );

        assert!(!is_declared_workspace_member(
            &workspace,
            &member,
            ManifestJsPackageManager::Pnpm
        ));

        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn pnpm_workspace_yaml_exclusion_beats_package_json_workspaces() {
        let root = temp_root("pnpm-union");
        let workspace = root.join("workspace");
        let child = workspace.join("child");
        let manifest = {
            let path = child.join("effigy.toml");
            write(&path, "[package_manager]\njs = \"pnpm\"\n");
            load_task_manifest(&path).expect("manifest")
        };
        mark_repo(&workspace);
        write(&workspace.join("pnpm-lock.yaml"), "");
        write(
            &workspace.join("pnpm-workspace.yaml"),
            "packages:\n  - 'packages/*'\n",
        );
        // The compatibility `package.json` pattern lists the child, but the
        // authoritative pnpm source excludes it. Membership must not union the
        // two sources and let the excluded child borrow the parent install.
        write(
            &workspace.join("package.json"),
            r#"{"private":true,"workspaces":["child"]}"#,
        );
        write(
            &child.join("package.json"),
            r#"{"dependencies":{"left-pad":"1.3.0"}}"#,
        );
        std::fs::create_dir_all(workspace.join("node_modules")).expect("parent install");

        let gap = health_js_bootstrap_gap(&child, &workspace, &manifest).expect("gap");

        assert_eq!(gap.manager, "pnpm");
        assert_eq!(gap.reason, BootstrapReason::UnverifiedAncestorLock);

        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn pnpm_child_without_membership_is_a_gap() {
        let root = temp_root("pnpm-gap");
        let workspace = root.join("workspace");
        let child = workspace.join("child");
        let manifest = {
            let path = child.join("effigy.toml");
            write(&path, "[package_manager]\njs = \"pnpm\"\n");
            load_task_manifest(&path).expect("manifest")
        };
        mark_repo(&workspace);
        // A pnpm lock plus a pnpm-workspace.yaml that excludes the child.
        write(&workspace.join("pnpm-lock.yaml"), "");
        write(
            &workspace.join("pnpm-workspace.yaml"),
            "packages:\n  - 'packages/*'\n",
        );
        write(
            &child.join("package.json"),
            r#"{"dependencies":{"left-pad":"1.3.0"}}"#,
        );
        std::fs::create_dir_all(workspace.join("node_modules")).expect("parent install");

        let gap = health_js_bootstrap_gap(&child, &workspace, &manifest).expect("gap");

        assert_eq!(gap.manager, "pnpm");
        assert_eq!(gap.reason, BootstrapReason::UnverifiedAncestorLock);
        assert!(gap
            .remediation()
            .contains("effigy bootstrap deps sync child"));

        std::fs::remove_dir_all(root).ok();
    }
}
