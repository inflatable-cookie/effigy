use std::path::Path;

use super::output::{
    render_inspect_all_response, render_inspect_response, render_invalidate_response,
};
use super::request::{InspectRequest, InvalidateRequest};
use super::selection::resolve_cache_selector;
use crate::BuiltinError;
use crate::BuiltinRuntimePorts;
use effigy_manifest::LoadedCatalog;

pub(super) fn run_inspect(
    ports: &dyn BuiltinRuntimePorts,
    target_root: &Path,
    catalogs: &[LoadedCatalog],
    invocation_cwd: &Path,
    request: InspectRequest,
) -> Result<Option<String>, BuiltinError> {
    if let Some(selector_raw) = request.selector.as_deref() {
        let (manifest_path, task_name) =
            resolve_cache_selector(selector_raw, catalogs, invocation_cwd)?;
        let entry = ports.cache_entry(target_root, &manifest_path, &task_name)?;
        return render_inspect_response(
            request.output_json,
            target_root,
            selector_raw,
            entry.as_ref(),
        );
    }

    let entries = ports.cache_entries(target_root)?;
    render_inspect_all_response(request.output_json, target_root, &entries)
}

pub(super) fn run_invalidate(
    ports: &dyn BuiltinRuntimePorts,
    target_root: &Path,
    catalogs: &[LoadedCatalog],
    invocation_cwd: &Path,
    request: InvalidateRequest,
) -> Result<Option<String>, BuiltinError> {
    let removed = if request.invalidate_all {
        let count = ports.invalidate_all_cache_entries(target_root)?;
        vec![format!("<all:{count}>")]
    } else {
        let mut keys = Vec::with_capacity(request.selectors.len());
        for selector_raw in &request.selectors {
            let (manifest_path, task_name) =
                resolve_cache_selector(selector_raw, catalogs, invocation_cwd)?;
            keys.push(ports.cache_entry_key(&manifest_path, &task_name));
        }
        ports.invalidate_cache_keys(target_root, &keys)?
    };

    render_invalidate_response(
        request.output_json,
        target_root,
        request.invalidate_all,
        &request.selectors,
        &removed,
    )
}
