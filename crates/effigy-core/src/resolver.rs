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
    if has_effigy_manifest_root_marker(child, probe) {
        return Some(ResolvedTarget {
            resolved_root: child.to_path_buf(),
            resolution_mode: ResolutionMode::AutoNearest,
            evidence: vec![format!(
                "child manifest {} declares `[manifest].root = true`; kept nearest root",
                task_manifest_path(child).display()
            )],
            warnings: Vec::new(),
        });
    }

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

fn read_to_string(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

fn has_effigy_manifest_root_marker(path: &Path, probe: &mut PathPresenceCache) -> bool {
    let Some(value) = manifest_value(path, probe) else {
        return false;
    };
    let Some(table) = value.as_table() else {
        return false;
    };
    let Some(manifest) = table.get("manifest").and_then(toml::Value::as_table) else {
        return false;
    };
    manifest
        .get("root")
        .and_then(toml::Value::as_bool)
        .unwrap_or(false)
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
    use super::{has_effigy_manifest_root_marker, resolve_target_root, ResolutionMode};
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
    fn manifest_root_marker_prevents_parent_workspace_promotion() {
        let (_temp, child) = workspace_with_child_manifest("[manifest]\nroot = true\n");
        assert!(has_effigy_manifest_root_marker(
            &child,
            &mut PathPresenceCache::new()
        ));

        let resolved = resolve_target_root(child.clone(), None).expect("resolve");

        assert_eq!(
            fs::canonicalize(&resolved.resolved_root).expect("canonical resolved"),
            fs::canonicalize(&child).expect("canonical child")
        );
        assert_eq!(resolved.resolution_mode, ResolutionMode::AutoNearest);
        assert!(resolved
            .evidence
            .iter()
            .any(|item| item.contains("root = true")));
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
