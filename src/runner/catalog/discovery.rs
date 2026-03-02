use std::collections::{HashMap, HashSet};
use std::fs::{self, FileType};
use std::path::{Path, PathBuf};

use super::super::{LoadedCatalog, RunnerError, TaskManifest, TASK_MANIFEST_FILE};

pub(super) fn discover_catalogs(workspace_root: &Path) -> Result<Vec<LoadedCatalog>, RunnerError> {
    let manifest_paths = discover_manifest_paths(workspace_root)?;
    if manifest_paths.is_empty() {
        return Err(RunnerError::TaskCatalogsMissing {
            root: workspace_root.to_path_buf(),
        });
    }

    let mut catalogs: Vec<LoadedCatalog> = Vec::new();
    let mut alias_map: HashMap<String, PathBuf> = HashMap::new();

    for manifest_path in manifest_paths {
        let manifest = load_catalog_manifest(&manifest_path)?;

        let catalog_root = manifest_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| workspace_root.to_path_buf());
        let alias = manifest
            .catalog
            .as_ref()
            .and_then(|meta| meta.alias.clone())
            .unwrap_or_else(|| default_alias(&catalog_root, workspace_root));

        if let Some(first_path) = alias_map.insert(alias.clone(), manifest_path.clone()) {
            return Err(RunnerError::TaskCatalogAliasConflict {
                alias,
                first_path,
                second_path: manifest_path,
            });
        }

        catalogs.push(LoadedCatalog {
            alias,
            depth: catalog_depth(workspace_root, &catalog_root),
            catalog_root,
            manifest_path,
            defer_run: manifest.defer.as_ref().map(|defer| defer.run.clone()),
            manifest,
        });
    }

    Ok(catalogs)
}

pub(super) fn discover_catalogs_allow_missing(
    workspace_root: &Path,
) -> Result<Vec<LoadedCatalog>, RunnerError> {
    match discover_catalogs(workspace_root) {
        Ok(catalogs) => Ok(catalogs),
        Err(RunnerError::TaskCatalogsMissing { .. }) => Ok(Vec::new()),
        Err(error) => Err(error),
    }
}

pub(super) fn discover_manifest_paths(workspace_root: &Path) -> Result<Vec<PathBuf>, RunnerError> {
    let mut pending: Vec<PathBuf> = vec![workspace_root.to_path_buf()];
    let mut visited_dirs: HashSet<PathBuf> = HashSet::new();
    let mut manifests_by_catalog: HashMap<PathBuf, PathBuf> = HashMap::new();

    while let Some(dir) = pending.pop() {
        let canonical_dir = fs::canonicalize(&dir).unwrap_or_else(|_| dir.clone());
        if !visited_dirs.insert(canonical_dir) {
            continue;
        }
        let entries =
            fs::read_dir(&dir).map_err(|error| task_catalog_read_dir_error(&dir, error))?;

        for entry in entries {
            let entry = entry.map_err(|error| task_catalog_read_dir_error(&dir, error))?;

            let path = entry.path();
            let file_type = entry
                .file_type()
                .map_err(|error| task_catalog_read_dir_error(&path, error))?;

            if file_type_matches(&file_type, &path, EntryKind::Directory) {
                if should_skip_dir(&path) {
                    continue;
                }
                pending.push(path);
                continue;
            }

            if is_task_manifest_file(&file_type, &path) {
                let catalog_root = path.parent().map(Path::to_path_buf).unwrap_or_default();
                manifests_by_catalog.insert(catalog_root, path);
            }
        }
    }

    let mut manifests: Vec<PathBuf> = manifests_by_catalog.into_values().collect();
    manifests.sort();
    Ok(manifests)
}

pub(super) fn should_skip_dir(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|n| n.to_str()),
        Some(".git" | "node_modules" | "target" | ".next")
    )
}

pub(super) fn default_alias(catalog_root: &Path, workspace_root: &Path) -> String {
    if catalog_root == workspace_root {
        return "root".to_owned();
    }

    catalog_root
        .file_name()
        .and_then(|n| n.to_str())
        .map(|v| v.to_owned())
        .unwrap_or_else(|| "catalog".to_owned())
}

fn load_catalog_manifest(manifest_path: &Path) -> Result<TaskManifest, RunnerError> {
    let manifest_src =
        fs::read_to_string(manifest_path).map_err(|error| RunnerError::TaskManifestRead {
            path: manifest_path.to_path_buf(),
            error,
        })?;
    toml::from_str(&manifest_src).map_err(|error| RunnerError::TaskManifestParse {
        path: manifest_path.to_path_buf(),
        error,
    })
}

fn catalog_depth(workspace_root: &Path, catalog_root: &Path) -> usize {
    catalog_root
        .strip_prefix(workspace_root)
        .map(|rel| rel.components().count())
        .unwrap_or(usize::MAX)
}

fn task_catalog_read_dir_error(path: &Path, error: std::io::Error) -> RunnerError {
    RunnerError::TaskCatalogReadDir {
        path: path.to_path_buf(),
        error,
    }
}

#[derive(Clone, Copy)]
enum EntryKind {
    Directory,
    File,
}

fn file_type_matches(file_type: &FileType, path: &Path, want: EntryKind) -> bool {
    if matches!(want, EntryKind::Directory) && file_type.is_dir() {
        return true;
    }
    if matches!(want, EntryKind::File) && file_type.is_file() {
        return true;
    }
    if !file_type.is_symlink() {
        return false;
    }
    fs::metadata(path)
        .map(|meta| match want {
            EntryKind::Directory => meta.is_dir(),
            EntryKind::File => meta.is_file(),
        })
        .unwrap_or(false)
}

fn is_task_manifest_file(file_type: &FileType, path: &Path) -> bool {
    file_type_matches(file_type, path, EntryKind::File)
        && path.file_name().and_then(|n| n.to_str()) == Some(TASK_MANIFEST_FILE)
}
