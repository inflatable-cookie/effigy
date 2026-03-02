use std::path::Path;

use serde_json::json;

use crate::TaskInvocation;

use super::super::cache::{
    cache_entries, cache_entry, cache_entry_key, invalidate_all_cache_entries,
    invalidate_cache_keys,
};
use super::super::catalog::select_catalog_and_task;
use super::super::util::parse_task_selector;
use super::super::{LoadedCatalog, RunnerError, TaskRuntimeArgs};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CacheCommand {
    Inspect,
    Invalidate,
}

struct CacheArgs {
    command: CacheCommand,
    output_json: bool,
    invalidate_all: bool,
    selectors: Vec<String>,
}

pub(super) fn run_builtin_cache(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    target_root: &Path,
    catalogs: &[LoadedCatalog],
    invocation_cwd: &Path,
) -> Result<Option<String>, RunnerError> {
    if runtime_args.verbose_root {
        return Err(RunnerError::TaskInvocation(
            "`--verbose-root` is not supported for built-in `cache`".to_owned(),
        ));
    }

    if runtime_args
        .passthrough
        .iter()
        .any(|arg| arg == "--help" || arg == "-h")
    {
        return Ok(Some(render_cache_help()));
    }

    let parsed = parse_cache_args(task, &runtime_args.passthrough)?;

    match parsed.command {
        CacheCommand::Inspect => run_inspect(
            task,
            target_root,
            catalogs,
            invocation_cwd,
            parsed.output_json,
            parsed.selectors,
        ),
        CacheCommand::Invalidate => run_invalidate(
            target_root,
            catalogs,
            invocation_cwd,
            parsed.output_json,
            parsed.invalidate_all,
            parsed.selectors,
        ),
    }
}

fn parse_cache_args(task: &TaskInvocation, args: &[String]) -> Result<CacheArgs, RunnerError> {
    let mut iter = args.iter();
    let Some(command_raw) = iter.next() else {
        return Err(RunnerError::TaskInvocation(
            "`cache` requires a subcommand: `inspect` or `invalidate`".to_owned(),
        ));
    };
    let command = match command_raw.as_str() {
        "inspect" => CacheCommand::Inspect,
        "invalidate" => CacheCommand::Invalidate,
        other => {
            return Err(RunnerError::TaskInvocation(format!(
                "unknown cache subcommand `{other}` (expected `inspect` or `invalidate`)"
            )));
        }
    };

    let mut output_json = false;
    let mut invalidate_all = false;
    let mut selectors = Vec::<String>::new();
    for arg in iter {
        match arg.as_str() {
            "--json" => output_json = true,
            "--all" => invalidate_all = true,
            value => selectors.push(value.to_owned()),
        }
    }
    let _ = task;
    Ok(CacheArgs {
        command,
        output_json,
        invalidate_all,
        selectors,
    })
}

fn run_inspect(
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
            return encode_cache_json(payload);
        }
        return Ok(Some(render_inspect_text(target_root, selector_raw, entry)));
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
        return encode_cache_json(payload);
    }

    Ok(Some(render_inspect_all_text(target_root, entries)))
}

fn run_invalidate(
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
        return encode_cache_json(payload);
    }

    Ok(Some(render_invalidate_text(
        target_root,
        invalidate_all,
        removed,
    )))
}

fn resolve_cache_selector(
    selector_raw: &str,
    catalogs: &[LoadedCatalog],
    invocation_cwd: &Path,
) -> Result<(std::path::PathBuf, String), RunnerError> {
    let selector = parse_task_selector(selector_raw)?;
    let selection = select_catalog_and_task(&selector, catalogs, invocation_cwd)?;
    Ok((
        selection.catalog.manifest_path.clone(),
        selector.task_name.clone(),
    ))
}

fn render_cache_help() -> String {
    [
        "cache Help",
        "",
        "Usage",
        "effigy cache inspect [<selector>] [--json]",
        "effigy cache invalidate [<selector>...] [--all] [--json]",
        "",
        "Notes",
        "- phase-1 cache is explicit opt-in via `[tasks.<name>.cache]`",
        "- cache hit requires matching fingerprint and declared outputs to exist",
        "",
        "Examples",
        "- effigy cache inspect",
        "- effigy cache inspect build",
        "- effigy cache invalidate build",
        "- effigy cache invalidate --all",
        "- effigy cache inspect --json",
    ]
    .join("\n")
}

fn encode_cache_json(payload: serde_json::Value) -> Result<Option<String>, RunnerError> {
    serde_json::to_string_pretty(&payload)
        .map(Some)
        .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")))
}

fn render_inspect_text(
    target_root: &Path,
    selector_raw: &str,
    entry: Option<super::super::cache::TaskCacheEntry>,
) -> String {
    let mut lines = vec![format!("cache root: {}", target_root.display())];
    lines.push(format!("selector: {selector_raw}"));
    match entry {
        Some(entry) => {
            lines.push("status: present".to_owned());
            lines.push(format!("fingerprint: {}", entry.fingerprint));
            lines.push(format!(
                "updated_at_epoch_ms: {}",
                entry.updated_at_epoch_ms
            ));
            lines.push(format!("command: {}", entry.command));
            lines.push(format!("outputs_exist: {}", entry.outputs_exist));
        }
        None => lines.push("status: missing".to_owned()),
    }
    lines.join("\n")
}

fn render_inspect_all_text(
    target_root: &Path,
    entries: Vec<super::super::cache::TaskCacheEntry>,
) -> String {
    let mut lines = vec![format!("cache root: {}", target_root.display())];
    lines.push(format!("entries: {}", entries.len()));
    for entry in entries {
        lines.push(format!(
            "- {} [{}] fingerprint={} outputs_exist={}",
            entry.task_name, entry.manifest_path, entry.fingerprint, entry.outputs_exist
        ));
    }
    lines.join("\n")
}

fn render_invalidate_text(
    target_root: &Path,
    invalidate_all: bool,
    removed: Vec<String>,
) -> String {
    let mut lines = vec![format!("cache root: {}", target_root.display())];
    if invalidate_all {
        lines.push("mode: all".to_owned());
    } else {
        lines.push("mode: selectors".to_owned());
    }
    lines.push(format!("removed: {}", removed.len()));
    for key in removed {
        lines.push(format!("- {key}"));
    }
    lines.join("\n")
}
