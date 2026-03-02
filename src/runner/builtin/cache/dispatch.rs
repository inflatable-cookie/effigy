use std::path::{Path, PathBuf};

use serde_json::json;

use crate::TaskInvocation;

use super::super::super::cache::{
    cache_entries, cache_entry, cache_entry_key, invalidate_all_cache_entries,
    invalidate_cache_keys,
};
use super::super::super::catalog::select_catalog_and_task;
use super::super::super::util::parse_task_selector;
use super::super::super::{LoadedCatalog, RunnerError};

pub(super) fn run_inspect(
    task: &TaskInvocation,
    target_root: &Path,
    catalogs: &[LoadedCatalog],
    invocation_cwd: &Path,
    output_json: bool,
    selectors: Vec<String>,
) -> Result<Option<String>, RunnerError> {
    if selectors.len() > 1 {
        return Err(RunnerError::TaskInvocation(format!(
            "`{}` cache inspect accepts at most one selector",
            task.name
        )));
    }

    if let Some(selector_raw) = selectors.first() {
        let (manifest_path, task_name) =
            resolve_cache_selector(selector_raw, catalogs, invocation_cwd)?;
        let entry = cache_entry(target_root, &manifest_path, &task_name)?;
        if output_json {
            let payload = json!({
                "schema": "effigy.cache.v1",
                "schema_version": 1,
                "ok": true,
                "action": "inspect",
                "root": target_root.display().to_string(),
                "selector": selector_raw,
                "entry": entry,
            });
            return super::output::encode_cache_json(payload);
        }
        return Ok(Some(super::output::render_inspect_text(
            target_root,
            selector_raw,
            entry,
        )));
    }

    let entries = cache_entries(target_root)?;
    if output_json {
        let payload = json!({
            "schema": "effigy.cache.v1",
            "schema_version": 1,
            "ok": true,
            "action": "inspect",
            "root": target_root.display().to_string(),
            "entries": entries,
        });
        return super::output::encode_cache_json(payload);
    }

    Ok(Some(super::output::render_inspect_all_text(
        target_root,
        entries,
    )))
}

pub(super) fn run_invalidate(
    target_root: &Path,
    catalogs: &[LoadedCatalog],
    invocation_cwd: &Path,
    output_json: bool,
    invalidate_all: bool,
    selectors: Vec<String>,
) -> Result<Option<String>, RunnerError> {
    if invalidate_all && !selectors.is_empty() {
        return Err(RunnerError::TaskInvocation(
            "`cache invalidate` accepts either `--all` or selectors, not both".to_owned(),
        ));
    }
    if !invalidate_all && selectors.is_empty() {
        return Err(RunnerError::TaskInvocation(
            "`cache invalidate` requires one or more selectors (or `--all`)".to_owned(),
        ));
    }

    let removed = if invalidate_all {
        let count = invalidate_all_cache_entries(target_root)?;
        vec![format!("<all:{count}>")]
    } else {
        let mut keys = Vec::with_capacity(selectors.len());
        for selector_raw in &selectors {
            let (manifest_path, task_name) =
                resolve_cache_selector(selector_raw, catalogs, invocation_cwd)?;
            keys.push(cache_entry_key(&manifest_path, &task_name));
        }
        invalidate_cache_keys(target_root, &keys)?
    };

    if output_json {
        let payload = json!({
            "schema": "effigy.cache.v1",
            "schema_version": 1,
            "ok": true,
            "action": "invalidate",
            "root": target_root.display().to_string(),
            "all": invalidate_all,
            "requested": selectors,
            "removed": removed,
        });
        return super::output::encode_cache_json(payload);
    }

    Ok(Some(super::output::render_invalidate_text(
        target_root,
        invalidate_all,
        removed,
    )))
}

fn resolve_cache_selector(
    selector_raw: &str,
    catalogs: &[LoadedCatalog],
    invocation_cwd: &Path,
) -> Result<(PathBuf, String), RunnerError> {
    let selector = parse_task_selector(selector_raw)?;
    let selection = select_catalog_and_task(&selector, catalogs, invocation_cwd)?;
    Ok((
        selection.catalog.manifest_path.clone(),
        selector.task_name.clone(),
    ))
}
