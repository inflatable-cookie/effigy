use std::path::Path;

use super::super::super::cache::ops::{
    cache_entries, cache_entry, cache_entry_key, invalidate_all_cache_entries,
    invalidate_cache_keys,
};
use super::super::super::model::catalog::LoadedCatalog;
use super::output::{
    render_inspect_all_response, render_inspect_response, render_invalidate_response,
};
use super::request::{InspectRequest, InvalidateRequest};
use super::selection::resolve_cache_selector;
use crate::runner::error::RunnerError;

pub(super) fn run_inspect(
    target_root: &Path,
    catalogs: &[LoadedCatalog],
    invocation_cwd: &Path,
    request: InspectRequest,
) -> Result<Option<String>, RunnerError> {
    if let Some(selector_raw) = request.selector.as_deref() {
        let (manifest_path, task_name) =
            resolve_cache_selector(selector_raw, catalogs, invocation_cwd)?;
        let entry = cache_entry(target_root, &manifest_path, &task_name)?;
        return render_inspect_response(
            request.output_json,
            target_root,
            selector_raw,
            entry.as_ref(),
        );
    }

    let entries = cache_entries(target_root)?;
    render_inspect_all_response(request.output_json, target_root, &entries)
}

pub(super) fn run_invalidate(
    target_root: &Path,
    catalogs: &[LoadedCatalog],
    invocation_cwd: &Path,
    request: InvalidateRequest,
) -> Result<Option<String>, RunnerError> {
    let removed = if request.invalidate_all {
        let count = invalidate_all_cache_entries(target_root)?;
        vec![format!("<all:{count}>")]
    } else {
        let mut keys = Vec::with_capacity(request.selectors.len());
        for selector_raw in &request.selectors {
            let (manifest_path, task_name) =
                resolve_cache_selector(selector_raw, catalogs, invocation_cwd)?;
            keys.push(cache_entry_key(&manifest_path, &task_name));
        }
        invalidate_cache_keys(target_root, &keys)?
    };

    render_invalidate_response(
        request.output_json,
        target_root,
        request.invalidate_all,
        &request.selectors,
        &removed,
    )
}
