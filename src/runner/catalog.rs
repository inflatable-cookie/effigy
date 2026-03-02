use std::collections::{HashMap, HashSet};
use std::fs::{self, FileType};
use std::path::{Component, Path, PathBuf};

use super::{
    CatalogSelectionMode, LoadedCatalog, RunnerError, TaskManifest, TaskSelection, TaskSelector,
    TASK_MANIFEST_FILE,
};

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

pub(super) fn select_catalog_and_task<'a>(
    selector: &TaskSelector,
    catalogs: &'a [LoadedCatalog],
    cwd: &Path,
) -> Result<TaskSelection<'a>, RunnerError> {
    if let Some(prefix) = &selector.prefix {
        let Some(catalog) = resolve_catalog_by_prefix(prefix, catalogs, cwd) else {
            return Err(RunnerError::TaskCatalogPrefixNotFound {
                prefix: prefix.clone(),
                available: sorted_catalog_aliases(catalogs),
            });
        };
        return build_task_selection(
            selector,
            catalog,
            CatalogSelectionMode::ExplicitPrefix,
            vec![selection_evidence_for_prefix(prefix, catalog)],
        );
    }

    let matches = catalogs_matching_task(catalogs, &selector.task_name);

    if matches.is_empty() {
        return Err(RunnerError::TaskNotFoundAny {
            name: selector.task_name.clone(),
            catalogs: catalogs.iter().map(format_catalog).collect(),
        });
    }

    let (selected, mode, evidence) = select_unprefixed_catalog(cwd, &matches, &selector.task_name)?;
    build_task_selection(selector, selected, mode, vec![evidence])
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

fn selection_evidence_for_prefix(prefix: &str, catalog: &LoadedCatalog) -> String {
    if catalog.alias == prefix {
        format!("selected catalog via explicit prefix `{prefix}`")
    } else {
        format!(
            "selected catalog via relative prefix `{prefix}` -> `{}`",
            catalog.alias
        )
    }
}

fn selection_evidence_for_cwd(catalog: &LoadedCatalog, cwd: &Path) -> String {
    format!(
        "selected nearest in-scope catalog `{}` for cwd {}",
        catalog.alias,
        cwd.display()
    )
}

fn selection_evidence_for_shallowest(catalog: &LoadedCatalog) -> String {
    format!(
        "selected shallowest catalog `{}` by depth {} from workspace root",
        catalog.alias, catalog.depth
    )
}

fn select_unprefixed_catalog<'a>(
    cwd: &Path,
    matches: &[&'a LoadedCatalog],
    task_name: &str,
) -> Result<(&'a LoadedCatalog, CatalogSelectionMode, String), RunnerError> {
    if let Some(selected) = select_in_scope_catalog(cwd, matches, task_name)? {
        return Ok((
            selected,
            CatalogSelectionMode::CwdNearest,
            selection_evidence_for_cwd(selected, cwd),
        ));
    }
    let selected = select_shallowest_catalog(matches, task_name)?;
    Ok((
        selected,
        CatalogSelectionMode::RootShallowest,
        selection_evidence_for_shallowest(selected),
    ))
}

fn sorted_catalog_aliases(catalogs: &[LoadedCatalog]) -> Vec<String> {
    let mut available = catalogs
        .iter()
        .map(|catalog| catalog.alias.clone())
        .collect::<Vec<String>>();
    available.sort();
    available
}

fn catalogs_matching_task<'a>(
    catalogs: &'a [LoadedCatalog],
    task_name: &str,
) -> Vec<&'a LoadedCatalog> {
    catalogs
        .iter()
        .filter(|catalog| catalog.manifest.tasks.contains_key(task_name))
        .collect()
}

fn select_in_scope_catalog<'a>(
    cwd: &Path,
    catalogs: &[&'a LoadedCatalog],
    task_name: &str,
) -> Result<Option<&'a LoadedCatalog>, RunnerError> {
    let in_scope = catalogs
        .iter()
        .copied()
        .filter(|catalog| cwd.starts_with(&catalog.catalog_root))
        .collect::<Vec<&LoadedCatalog>>();
    if in_scope.is_empty() {
        return Ok(None);
    }
    let max_depth = in_scope
        .iter()
        .map(|catalog| catalog.depth)
        .max()
        .unwrap_or_default();
    select_unique_catalog_by_depth(&in_scope, max_depth, task_name).map(Some)
}

fn select_shallowest_catalog<'a>(
    catalogs: &[&'a LoadedCatalog],
    task_name: &str,
) -> Result<&'a LoadedCatalog, RunnerError> {
    let min_depth = catalogs
        .iter()
        .map(|catalog| catalog.depth)
        .min()
        .unwrap_or_default();
    select_unique_catalog_by_depth(catalogs, min_depth, task_name)
}

fn select_unique_catalog_by_depth<'a>(
    catalogs: &[&'a LoadedCatalog],
    depth: usize,
    task_name: &str,
) -> Result<&'a LoadedCatalog, RunnerError> {
    let matches = catalogs
        .iter()
        .copied()
        .filter(|catalog| catalog.depth == depth)
        .collect::<Vec<&LoadedCatalog>>();
    match matches.as_slice() {
        [] => Err(RunnerError::TaskNotFoundAny {
            name: task_name.to_owned(),
            catalogs: catalogs.iter().copied().map(format_catalog).collect(),
        }),
        [catalog] => Ok(*catalog),
        _ => Err(RunnerError::TaskAmbiguous {
            name: task_name.to_owned(),
            candidates: matches.into_iter().map(format_catalog).collect(),
        }),
    }
}

pub(super) fn format_catalog(catalog: &LoadedCatalog) -> String {
    format!("{} ({})", catalog.alias, catalog.manifest_path.display())
}

fn catalog_depth(workspace_root: &Path, catalog_root: &Path) -> usize {
    catalog_root
        .strip_prefix(workspace_root)
        .map(|rel| rel.components().count())
        .unwrap_or(usize::MAX)
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

fn resolve_catalog_by_relative_prefix<'a>(
    prefix: &str,
    catalogs: &'a [LoadedCatalog],
    cwd: &Path,
) -> Option<&'a LoadedCatalog> {
    if !is_relative_path_prefix(prefix) {
        return None;
    }

    let resolved = normalize_path(if Path::new(prefix).is_absolute() {
        PathBuf::from(prefix)
    } else {
        cwd.join(prefix)
    });

    catalogs
        .iter()
        .find(|catalog| normalize_path(catalog.catalog_root.clone()) == resolved)
}

pub(super) fn resolve_catalog_by_prefix<'a>(
    prefix: &str,
    catalogs: &'a [LoadedCatalog],
    cwd: &Path,
) -> Option<&'a LoadedCatalog> {
    catalogs
        .iter()
        .find(|catalog| catalog.alias == prefix)
        .or_else(|| resolve_catalog_by_relative_prefix(prefix, catalogs, cwd))
}

fn is_relative_path_prefix(prefix: &str) -> bool {
    prefix.starts_with('.')
        || prefix.starts_with('/')
        || prefix.contains('/')
        || prefix.contains('\\')
}

fn normalize_path(path: PathBuf) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn task_catalog_read_dir_error(path: &Path, error: std::io::Error) -> RunnerError {
    RunnerError::TaskCatalogReadDir {
        path: path.to_path_buf(),
        error,
    }
}

fn build_task_selection<'a>(
    selector: &TaskSelector,
    catalog: &'a LoadedCatalog,
    mode: CatalogSelectionMode,
    evidence: Vec<String>,
) -> Result<TaskSelection<'a>, RunnerError> {
    let Some(task) = catalog.manifest.tasks.get(&selector.task_name) else {
        return Err(RunnerError::TaskNotFound {
            name: selector.task_name.clone(),
            path: catalog.manifest_path.clone(),
        });
    };
    Ok(TaskSelection {
        catalog,
        task,
        mode,
        evidence,
    })
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
