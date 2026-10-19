use std::fs;
use std::path::{Path, PathBuf};

use crate::fs_probe::PathPresenceCache;
use crate::repo_markers::{task_manifest_path, ROOT_MARKERS, TASK_MANIFEST_FILE};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolutionMode {
    Explicit,
    AutoNearest,
    AutoPromoted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTarget {
    pub resolved_root: PathBuf,
    pub resolution_mode: ResolutionMode,
    pub evidence: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug)]
pub enum ResolveError {
    Cwd(std::io::Error),
    InvalidExplicitRoot { path: PathBuf },
    NoCandidateRoot { cwd: PathBuf },
    RetiredManifestRoot { path: PathBuf },
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolveError::Cwd(err) => write!(f, "failed to resolve current directory: {err}"),
            ResolveError::InvalidExplicitRoot { path } => {
                write!(
                    f,
                    "explicit --repo path is not a directory: {}",
                    path.display()
                )
            }
            ResolveError::RetiredManifestRoot { path } => write!(f, "`[manifest].root` in {} is retired; remove it and select the intended root explicitly with --repo <PATH>", path.display()),
            ResolveError::NoCandidateRoot { cwd } => write!(
                f,
                "could not resolve a project root from cwd {} (use --repo <path>)",
                cwd.display()
            ),
        }
    }
}

impl std::error::Error for ResolveError {}

pub fn resolve_target_root(
    cwd: PathBuf,
    repo_override: Option<PathBuf>,
) -> Result<ResolvedTarget, ResolveError> {
    if let Some(explicit) = repo_override {
        let canonical = canonicalize_best_effort(explicit);
        if !canonical.is_dir() {
            return Err(ResolveError::InvalidExplicitRoot { path: canonical });
        }
        return Ok(ResolvedTarget {
            resolved_root: canonical,
            resolution_mode: ResolutionMode::Explicit,
            evidence: vec!["resolved via explicit --repo override".to_owned()],
            warnings: Vec::new(),
        });
    }

    let mut probe = PathPresenceCache::new();
    let nearest =
        find_nearest_candidate(&cwd, &mut probe).ok_or(ResolveError::NoCandidateRoot { cwd })?;

    if has_retired_manifest_root(&nearest, &mut probe) {
        return Err(ResolveError::RetiredManifestRoot {
            path: task_manifest_path(&nearest),
        });
    }

    if let Some(declaring) = declaring_root(&nearest, &mut probe) {
        return Ok(declaring);
    }

    if let Some(promoted) = maybe_promote_to_parent_workspace(&nearest, &mut probe) {
        return Ok(promoted);
    }

    Ok(ResolvedTarget {
        resolved_root: nearest.clone(),
        resolution_mode: ResolutionMode::AutoNearest,
        evidence: vec![format!(
            "selected nearest root candidate {}",
            nearest.display()
        )],
        warnings: Vec::new(),
    })
}

fn find_nearest_candidate(cwd: &Path, probe: &mut PathPresenceCache) -> Option<PathBuf> {
    let mut current = Some(canonicalize_best_effort(cwd.to_path_buf()));
    while let Some(path) = current {
        if is_candidate_root(&path, probe) {
            return Some(path);
        }
        current = path.parent().map(Path::to_path_buf);
    }
    None
}

fn is_candidate_root(path: &Path, probe: &mut PathPresenceCache) -> bool {
    ROOT_MARKERS
        .iter()
        .any(|marker| probe.child_exists(path, marker))
}

fn maybe_promote_to_parent_workspace(
    child: &Path,
    probe: &mut PathPresenceCache,
) -> Option<ResolvedTarget> {
    let parent = child.parent()?;
    if !parent.is_dir() {
        return None;
    }

    let child_name = child.file_name()?.to_string_lossy().to_string();

    let mut evidence: Vec<String> = Vec::new();
    let mut should_promote = false;

    if probe.child_exists(parent, TASK_MANIFEST_FILE)
        && child_manifest_declares_catalog(child, probe)
    {
        should_promote = true;
        evidence.push("parent effigy.toml anchors child workspace".to_owned());
    }

    let parent_package = parent.join("package.json");
    if probe.exists(&parent_package) {
        let content = read_to_string(&parent_package);
        if content.contains("\"workspaces\"")
            && (content.contains(&child_name) || content.contains('*'))
        {
            should_promote = true;
            evidence.push("parent package.json workspace includes child".to_owned());
        }
    }

    let parent_cargo = parent.join("Cargo.toml");
    if probe.exists(&parent_cargo) {
        let content = read_to_string(&parent_cargo);
        if content.contains("[workspace]")
            && content.contains("members")
            && (content.contains(&child_name) || content.contains('*'))
        {
            should_promote = true;
            evidence.push("parent Cargo.toml workspace includes child".to_owned());
        }
    }

    if !should_promote {
        return None;
    }

    let child_has_own_git = probe.child_exists(child, ".git");
    if child_has_own_git {
        return Some(ResolvedTarget {
            resolved_root: child.to_path_buf(),
            resolution_mode: ResolutionMode::AutoNearest,
            evidence: vec![format!(
                "child repo {} has standalone .git; kept nearest root",
                child.display()
            )],
            warnings: vec![
                "workspace promotion skipped due to standalone child repository".to_owned(),
            ],
        });
    }

    Some(ResolvedTarget {
        resolved_root: parent.to_path_buf(),
        resolution_mode: ResolutionMode::AutoPromoted,
        evidence,
        warnings: Vec::new(),
    })
}

/// The topmost ancestor root whose catalog membership names `catalog`
/// (contract 037): a member catalog run from inside itself, however deep,
/// resolves to the root that declares it. The walk stops at a Git top level
/// or a `[manifest].root = true` boundary, and never starts from a catalog
/// that is itself a root boundary or its own repository.
fn declaring_root(catalog: &Path, probe: &mut PathPresenceCache) -> Option<ResolvedTarget> {
    if !probe.child_exists(catalog, TASK_MANIFEST_FILE) || probe.child_exists(catalog, ".git") {
        return None;
    }
    let target = canonicalize_best_effort(catalog.to_path_buf());
    let mut declaring = None;
    let mut current = catalog.parent();
    while let Some(dir) = current {
        if probe.child_exists(dir, TASK_MANIFEST_FILE) {
            if let Some(handle) = member_handle_for(dir, &target, probe) {
                declaring = Some((dir.to_path_buf(), handle));
            }
        }
        if probe.child_exists(dir, ".git") {
            break;
        }
        current = dir.parent();
    }
    let (root, handle) = declaring?;
    Some(ResolvedTarget {
        evidence: vec![format!(
            "catalog {} is member `{handle}` of {}; resolved to its declaring root",
            catalog.display(),
            task_manifest_path(&root).display()
        )],
        resolved_root: root,
        resolution_mode: ResolutionMode::AutoPromoted,
        warnings: Vec::new(),
    })
}

/// The `[catalog.members]` handle under `root` that names `target`, read from
/// the root manifest, its committed includes and its local overlay.
fn member_handle_for(root: &Path, target: &Path, probe: &mut PathPresenceCache) -> Option<String> {
    let manifest = manifest_value(root, probe)?;
    let mut sources = vec![manifest.clone()];
    let includes = manifest
        .get("manifest")
        .and_then(|section| section.get("include"))
        .and_then(toml::Value::as_array)
        .cloned()
        .unwrap_or_default();
    for include in includes.iter().filter_map(toml::Value::as_str) {
        if let Ok(value) = toml::from_str::<toml::Value>(&read_to_string(&root.join(include))) {
            sources.push(value);
        }
    }
    if let Ok(value) =
        toml::from_str::<toml::Value>(&read_to_string(&root.join("effigy.local.toml")))
    {
        sources.push(value);
    }
    sources.iter().find_map(|source| {
        source
            .get("catalog")
            .and_then(|catalog| catalog.get("members"))
            .and_then(toml::Value::as_table)?
            .iter()
            .find(|(_, path)| {
                path.as_str()
                    .is_some_and(|path| canonicalize_best_effort(root.join(path)) == target)
            })
            .map(|(handle, _)| handle.clone())
    })
}

fn read_to_string(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

fn has_retired_manifest_root(path: &Path, probe: &mut PathPresenceCache) -> bool {
    let Some(value) = manifest_value(path, probe) else {
        return false;
    };
    let Some(table) = value.as_table() else {
        return false;
    };
    let Some(manifest) = table.get("manifest").and_then(toml::Value::as_table) else {
        return false;
    };
    manifest.contains_key("root")
}

fn child_manifest_declares_catalog(path: &Path, probe: &mut PathPresenceCache) -> bool {
    let Some(value) = manifest_value(path, probe) else {
        return false;
    };
    value
        .as_table()
        .is_some_and(|table| table.get("catalog").is_some())
}

fn manifest_value(path: &Path, probe: &mut PathPresenceCache) -> Option<toml::Value> {
    let manifest_path = task_manifest_path(path);
    if !probe.exists(&manifest_path) {
        return None;
    }

    let raw = read_to_string(&manifest_path);
    let Ok(value) = toml::from_str::<toml::Value>(&raw) else {
        return None;
    };

    Some(value)
}

fn canonicalize_best_effort(path: PathBuf) -> PathBuf {
    fs::canonicalize(&path).unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::{has_retired_manifest_root, resolve_target_root, ResolutionMode};
    use crate::fs_probe::PathPresenceCache;
    use std::fs;
    use tempfile::TempDir;

    fn workspace_with_child_manifest(root_marker: &str) -> (TempDir, std::path::PathBuf) {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"child\"]\n",
        )
        .expect("write workspace cargo");
        let child = root.join("child");
        fs::create_dir_all(&child).expect("mkdir child");
        fs::write(child.join("effigy.toml"), root_marker).expect("write child manifest");
        (temp, child)
    }

    #[test]
    fn deep_member_catalog_resolves_to_its_declaring_root() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        fs::create_dir(root.join(".git")).expect("git toplevel");
        fs::write(
            root.join("effigy.toml"),
            "[catalog.members]\nfarmyard = \"apps/farmyard\"\n",
        )
        .expect("root manifest");
        let catalog = root.join("apps/farmyard");
        fs::create_dir_all(catalog.join("src")).expect("catalog dirs");
        fs::write(
            catalog.join("effigy.toml"),
            "[catalog]\nalias = \"farmyard\"\n",
        )
        .expect("catalog manifest");

        let resolved = resolve_target_root(catalog.join("src"), None).expect("resolve");
        assert_eq!(
            fs::canonicalize(&resolved.resolved_root).expect("canonical"),
            fs::canonicalize(root).expect("canonical root")
        );
        assert_eq!(resolved.resolution_mode, ResolutionMode::AutoPromoted);
        assert!(resolved.evidence[0].contains("member `farmyard`"));
    }

    #[test]
    fn undeclared_deep_catalog_and_root_boundaries_keep_the_nearest_root() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        fs::write(root.join("effigy.toml"), "[tasks.root]\nrun = \"true\"\n").expect("root");
        let undeclared = root.join("apps/loose");
        fs::create_dir_all(&undeclared).expect("dir");
        fs::write(
            undeclared.join("effigy.toml"),
            "[catalog]\nalias = \"loose\"\n",
        )
        .expect("m");
        let resolved = resolve_target_root(undeclared.clone(), None).expect("resolve");
        assert_eq!(
            fs::canonicalize(&resolved.resolved_root).expect("canonical"),
            fs::canonicalize(&undeclared).expect("canonical undeclared")
        );

        // A nested repository is its own root even when an outer manifest
        // names it as a member.
        let nested = root.join("apps/own");
        fs::create_dir_all(nested.join(".git")).expect("nested repo");
        fs::write(nested.join("effigy.toml"), "[catalog]\nalias = \"own\"\n").expect("m");
        fs::write(
            root.join("effigy.toml"),
            "[catalog.members]\nown = \"apps/own\"\n",
        )
        .expect("root");
        let resolved = resolve_target_root(nested.clone(), None).expect("resolve");
        assert_eq!(
            fs::canonicalize(&resolved.resolved_root).expect("canonical"),
            fs::canonicalize(&nested).expect("canonical nested")
        );
    }

    #[test]
    fn retirement_root_marker_refuses_before_parent_promotion() {
        for value in ["true", "false", "1"] {
            let (_temp, child) =
                workspace_with_child_manifest(&format!("[manifest]\nroot = {value}\n"));
            assert!(has_retired_manifest_root(
                &child,
                &mut PathPresenceCache::new()
            ));
            let error = resolve_target_root(child, None).unwrap_err().to_string();
            assert!(
                error.contains("retired") && error.contains("--repo"),
                "{error}"
            );
        }
    }

    #[test]
    fn nested_effigy_manifest_without_root_marker_still_promotes_to_workspace() {
        let (temp, child) = workspace_with_child_manifest("[catalog]\nalias = \"child\"\n");

        let resolved = resolve_target_root(child, None).expect("resolve");

        assert_eq!(
            fs::canonicalize(&resolved.resolved_root).expect("canonical resolved"),
            fs::canonicalize(temp.path()).expect("canonical temp")
        );
        assert_eq!(resolved.resolution_mode, ResolutionMode::AutoPromoted);
    }

    #[test]
    fn nested_effigy_manifest_promotes_to_parent_effigy_root_without_other_workspace_markers() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        fs::write(
            root.join("effigy.toml"),
            "[tasks.root]\nrun = \"printf root\"\n",
        )
        .expect("write root manifest");
        let child = root.join("child");
        fs::create_dir_all(&child).expect("mkdir child");
        fs::write(child.join("effigy.toml"), "[catalog]\nalias = \"child\"\n")
            .expect("write child manifest");

        let resolved = resolve_target_root(child, None).expect("resolve");

        assert_eq!(
            fs::canonicalize(&resolved.resolved_root).expect("canonical resolved"),
            fs::canonicalize(root).expect("canonical root")
        );
        assert_eq!(resolved.resolution_mode, ResolutionMode::AutoPromoted);
        assert!(resolved
            .evidence
            .iter()
            .any(|item| item.contains("parent effigy.toml anchors child workspace")));
    }

    #[test]
    fn nested_standalone_effigy_manifest_does_not_promote_to_parent_effigy_root() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        fs::write(
            root.join("effigy.toml"),
            "[tasks.root]\nrun = \"printf root\"\n",
        )
        .expect("write root manifest");
        let child = root.join("child");
        fs::create_dir_all(&child).expect("mkdir child");
        fs::write(
            child.join("effigy.toml"),
            "[tasks.qa]\nrun = \"printf child\"\n",
        )
        .expect("write child manifest");

        let resolved = resolve_target_root(child.clone(), None).expect("resolve");

        assert_eq!(
            fs::canonicalize(&resolved.resolved_root).expect("canonical resolved"),
            fs::canonicalize(&child).expect("canonical child")
        );
        assert_eq!(resolved.resolution_mode, ResolutionMode::AutoNearest);
    }
}
