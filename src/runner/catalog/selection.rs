use std::path::{Component, Path, PathBuf};

use super::super::{CatalogSelectionMode, LoadedCatalog, RunnerError, TaskSelection, TaskSelector};

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

pub(super) fn format_catalog(catalog: &LoadedCatalog) -> String {
    format!("{} ({})", catalog.alias, catalog.manifest_path.display())
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
