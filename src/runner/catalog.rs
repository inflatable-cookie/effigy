use std::collections::HashMap;
use std::fs;
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
        let manifest_src =
            fs::read_to_string(&manifest_path).map_err(|error| RunnerError::TaskManifestRead {
                path: manifest_path.clone(),
                error,
            })?;
        let manifest: TaskManifest =
            toml::from_str(&manifest_src).map_err(|error| RunnerError::TaskManifestParse {
                path: manifest_path.clone(),
                error,
            })?;

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

fn discover_manifest_paths(workspace_root: &Path) -> Result<Vec<PathBuf>, RunnerError> {
    let mut pending: Vec<PathBuf> = vec![workspace_root.to_path_buf()];
    let mut manifests_by_catalog: HashMap<PathBuf, PathBuf> = HashMap::new();

    while let Some(dir) = pending.pop() {
        let entries = fs::read_dir(&dir).map_err(|error| RunnerError::TaskCatalogReadDir {
            path: dir.clone(),
            error,
        })?;

        for entry in entries {
            let entry = entry.map_err(|error| RunnerError::TaskCatalogReadDir {
                path: dir.clone(),
                error,
            })?;

            let path = entry.path();
            let file_type = entry
                .file_type()
                .map_err(|error| RunnerError::TaskCatalogReadDir {
                    path: path.clone(),
                    error,
                })?;

            if file_type.is_dir() {
                if should_skip_dir(&path) {
                    continue;
                }
                pending.push(path);
                continue;
            }

            if file_type.is_file()
                && path.file_name().and_then(|n| n.to_str()) == Some(TASK_MANIFEST_FILE)
            {
                let catalog_root = path.parent().map(Path::to_path_buf).unwrap_or_default();
                manifests_by_catalog.insert(catalog_root, path);
                continue;
            }
        }
    }

    let mut manifests: Vec<PathBuf> = manifests_by_catalog.into_values().collect();
    manifests.sort();
    Ok(manifests)
}

fn should_skip_dir(path: &Path) -> bool {
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
        let mut available = catalogs
            .iter()
            .map(|c| c.alias.clone())
            .collect::<Vec<String>>();
        available.sort();

        let selected_catalog = resolve_catalog_by_prefix(prefix, catalogs, cwd);

        let Some(catalog) = selected_catalog else {
            return Err(RunnerError::TaskCatalogPrefixNotFound {
                prefix: prefix.clone(),
                available,
            });
        };

        let Some(task) = catalog.manifest.tasks.get(&selector.task_name) else {
            return Err(RunnerError::TaskNotFound {
                name: selector.task_name.clone(),
                path: catalog.manifest_path.clone(),
            });
        };
        return Ok(TaskSelection {
            catalog,
            task,
            mode: CatalogSelectionMode::ExplicitPrefix,
            evidence: vec![if catalog.alias == *prefix {
                format!("selected catalog via explicit prefix `{prefix}`")
            } else {
                format!(
                    "selected catalog via relative prefix `{prefix}` -> `{}`",
                    catalog.alias
                )
            }],
        });
    }

    let matches = catalogs
        .iter()
        .filter(|c| c.manifest.tasks.contains_key(&selector.task_name))
        .collect::<Vec<&LoadedCatalog>>();

    if matches.is_empty() {
        return Err(RunnerError::TaskNotFoundAny {
            name: selector.task_name.clone(),
            catalogs: catalogs.iter().map(format_catalog).collect(),
        });
    }

    let in_scope = matches
        .iter()
        .copied()
        .filter(|c| cwd.starts_with(&c.catalog_root))
        .collect::<Vec<&LoadedCatalog>>();

    if !in_scope.is_empty() {
        let max_depth = in_scope.iter().map(|c| c.depth).max().unwrap_or_default();
        let deepest = in_scope
            .into_iter()
            .filter(|c| c.depth == max_depth)
            .collect::<Vec<&LoadedCatalog>>();
        if deepest.len() > 1 {
            return Err(RunnerError::TaskAmbiguous {
                name: selector.task_name.clone(),
                candidates: deepest.into_iter().map(format_catalog).collect(),
            });
        }
        let selected = deepest[0];
        let evidence = vec![format!(
            "selected nearest in-scope catalog `{}` for cwd {}",
            selected.alias,
            cwd.display()
        )];
        let task = selected
            .manifest
            .tasks
            .get(&selector.task_name)
            .expect("task existence already validated");
        return Ok(TaskSelection {
            catalog: selected,
            task,
            mode: CatalogSelectionMode::CwdNearest,
            evidence,
        });
    }

    let min_depth = matches.iter().map(|c| c.depth).min().unwrap_or_default();
    let shallowest = matches
        .into_iter()
        .filter(|c| c.depth == min_depth)
        .collect::<Vec<&LoadedCatalog>>();
    if shallowest.len() > 1 {
        return Err(RunnerError::TaskAmbiguous {
            name: selector.task_name.clone(),
            candidates: shallowest.into_iter().map(format_catalog).collect(),
        });
    }
    let selected = shallowest[0];
    let evidence = vec![format!(
        "selected shallowest catalog `{}` by depth {} from workspace root",
        selected.alias, selected.depth
    )];
    let task = selected
        .manifest
        .tasks
        .get(&selector.task_name)
        .expect("task existence already validated");
    Ok(TaskSelection {
        catalog: selected,
        task,
        mode: CatalogSelectionMode::RootShallowest,
        evidence,
    })
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

fn default_alias(catalog_root: &Path, workspace_root: &Path) -> String {
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
