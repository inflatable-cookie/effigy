use std::collections::HashSet;
use std::path::{Path, PathBuf};

use effigy_manifest::{
    resolve_task_execution_binding, LoadedCatalog, ManifestManagedConcurrentEntry, ManifestTask,
    ManifestTaskRunIn, ResolvedTaskExecutionBinding, TaskResolverFn,
};
use effigy_tasks::TaskSelector;

use super::{invalid_managed_process_definition, select_run_or_task};
use crate::references;
use crate::run_spec::{render_run_step_sequence, wrap_reference_command_in_cwd};
use crate::{ManagedProcessRole, ManagedProcessSpec, DEFAULT_MANAGED_SHELL_RUN};

pub fn resolve_concurrent_process_entries<'a>(
    selector: &TaskSelector,
    task: &ManifestTask,
    entries: &[ManifestManagedConcurrentEntry],
    catalog: &LoadedCatalog,
    catalogs: &'a [LoadedCatalog],
    task_scope_cwd: &Path,
    resolver: TaskResolverFn<'a>,
) -> Result<Vec<super::ConcurrentResolvedProcess>, crate::ManagedError> {
    let mut used_names = HashSet::<String>::new();
    let mut resolved = Vec::<super::ConcurrentResolvedProcess>::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        let normalized = normalize_entry(selector, task, entry, index, catalog)?;
        let process_name = normalized.process_name;
        if !used_names.insert(process_name.clone()) {
            return Err(invalid_managed_process_definition(
                selector,
                &process_name,
                "duplicate process name; set unique `name` values in `concurrent` entries",
            ));
        }

        let (run, cwd) = resolve_process_run_and_cwd(
            selector,
            task,
            &process_name,
            entry,
            catalog,
            catalogs,
            task_scope_cwd,
            resolver,
        )?;
        let setup = resolve_process_setup(
            selector,
            task,
            &process_name,
            entry,
            catalog,
            catalogs,
            &cwd,
            resolver,
        )?;
        resolved.push(super::ConcurrentResolvedProcess {
            spec: ManagedProcessSpec {
                name: process_name,
                role: normalized.role,
                run,
                setup,
                cwd,
                service: normalized.service,
                start_after_ms: normalized.start_after_ms,
                shutdown_on_exit: normalized.shutdown_on_exit,
                run_on_host: normalized.run_on_host,
            },
            start_rank: normalized.start_rank,
            tab_rank: normalized.tab_rank,
            index,
        });
    }
    Ok(resolved)
}

#[allow(clippy::too_many_arguments)]
fn resolve_process_setup<'a>(
    selector: &TaskSelector,
    task: &ManifestTask,
    process_name: &str,
    entry: &ManifestManagedConcurrentEntry,
    catalog: &LoadedCatalog,
    catalogs: &'a [LoadedCatalog],
    process_cwd: &Path,
    resolver: TaskResolverFn<'a>,
) -> Result<Option<String>, crate::ManagedError> {
    if entry.setup.is_empty() {
        return Ok(None);
    }
    if matches!(entry.role.as_deref(), Some("lifecycle") | Some("shell")) {
        return Err(invalid_managed_process_definition(
            selector,
            process_name,
            "`setup` is only supported on standard concurrent entries",
        ));
    }
    let rendered = render_run_step_sequence(
        process_name,
        &entry.setup,
        &task.env,
        task.env_file.as_ref(),
        &catalog.manifest.env,
        &catalog.catalog_root,
        catalog.bundle_root.as_deref(),
        catalogs,
        process_cwd,
        None,
        resolver,
    )?;
    Ok(Some(wrap_reference_command_in_cwd(process_cwd, &rendered)))
}

struct NormalizedConcurrentEntry {
    process_name: String,
    role: ManagedProcessRole,
    service: Option<String>,
    start_rank: usize,
    tab_rank: usize,
    start_after_ms: u64,
    shutdown_on_exit: bool,
    run_on_host: bool,
}

fn normalize_entry(
    selector: &TaskSelector,
    task: &ManifestTask,
    entry: &ManifestManagedConcurrentEntry,
    index: usize,
    catalog: &LoadedCatalog,
) -> Result<NormalizedConcurrentEntry, crate::ManagedError> {
    let ordinal = index + 1;
    let role = normalize_role(selector, task, entry, catalog)?;
    let process_name = entry
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| match role {
            ManagedProcessRole::Lifecycle => Some("lifecycle".to_owned()),
            ManagedProcessRole::Shell => Some("shell".to_owned()),
            ManagedProcessRole::Standard => None,
        })
        .or_else(|| entry.task.clone())
        .unwrap_or_else(|| format!("process-{ordinal}"));
    let start_rank = entry.start.unwrap_or(ordinal);
    let tab_rank = entry.tab.unwrap_or(start_rank);
    let run_on_host = match entry.run_in {
        Some(ManifestTaskRunIn::Host) => {
            if matches!(
                role,
                ManagedProcessRole::Lifecycle | ManagedProcessRole::Shell
            ) {
                return Err(invalid_managed_process_definition(
                    selector,
                    &process_name,
                    "`run_in = \"host\"` is only supported on standard concurrent entries (not `lifecycle` or `shell`)",
                ));
            }
            true
        }
        _ => false,
    };
    Ok(NormalizedConcurrentEntry {
        process_name,
        role,
        service: entry
            .service
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
        start_rank,
        tab_rank,
        start_after_ms: entry.start_after_ms.unwrap_or(0),
        shutdown_on_exit: lifecycle_shutdown_on_exit(selector, entry, role)?,
        run_on_host,
    })
}

fn resolve_process_run_and_cwd<'a>(
    selector: &TaskSelector,
    task: &ManifestTask,
    process_name: &str,
    entry: &ManifestManagedConcurrentEntry,
    catalog: &LoadedCatalog,
    catalogs: &'a [LoadedCatalog],
    task_scope_cwd: &Path,
    resolver: TaskResolverFn<'a>,
) -> Result<(String, PathBuf), crate::ManagedError> {
    if matches!(entry.role.as_deref(), Some("lifecycle") | Some("shell")) {
        return Ok((String::new(), task_scope_cwd.to_path_buf()));
    }
    match select_run_or_task(
        entry.run.as_deref(),
        entry.task.as_deref(),
        || {
            invalid_managed_process_definition(
                selector,
                process_name,
                "define either `task` or `run`, not both",
            )
        },
        || {
            invalid_managed_process_definition(
                selector,
                process_name,
                "missing both `task` and `run`",
            )
        },
    )? {
        super::RunOrTaskRef::Task(task_ref) => resolve_task_process_run_and_cwd(
            selector,
            task,
            process_name,
            task_ref,
            catalog,
            catalogs,
            task_scope_cwd,
            resolver,
        ),
        super::RunOrTaskRef::Run(run) => Ok((run.to_owned(), task_scope_cwd.to_path_buf())),
    }
}

fn resolve_task_process_run_and_cwd<'a>(
    selector: &TaskSelector,
    _task: &ManifestTask,
    process_name: &str,
    task_ref: &str,
    catalog: &LoadedCatalog,
    catalogs: &'a [LoadedCatalog],
    task_scope_cwd: &Path,
    resolver: TaskResolverFn<'a>,
) -> Result<(String, PathBuf), crate::ManagedError> {
    if task_ref.trim() == "shell" {
        return resolve_shell_process_run(selector, process_name, catalog, task_scope_cwd);
    }
    references::resolve_task_reference_run(
        &selector.task_name,
        process_name,
        task_ref,
        catalogs,
        task_scope_cwd,
        resolver,
    )
}

fn resolve_shell_process_run(
    selector: &TaskSelector,
    process_name: &str,
    catalog: &LoadedCatalog,
    task_scope_cwd: &Path,
) -> Result<(String, PathBuf), crate::ManagedError> {
    if process_name != "shell" {
        return Err(invalid_managed_process_definition(
            selector,
            process_name,
            "task `shell` must use process name `shell` (omit `name` or set `name = \"shell\"`)",
        ));
    }
    let shell_run = catalog
        .manifest
        .shell
        .as_ref()
        .and_then(|shell| shell.run.clone())
        .unwrap_or_else(|| DEFAULT_MANAGED_SHELL_RUN.to_owned());
    Ok((shell_run, task_scope_cwd.to_path_buf()))
}

fn normalize_role(
    selector: &TaskSelector,
    task: &ManifestTask,
    entry: &ManifestManagedConcurrentEntry,
    catalog: &LoadedCatalog,
) -> Result<ManagedProcessRole, crate::ManagedError> {
    let has_container_backed_binding =
        task_has_container_backed_execution_binding(catalog, selector, task)?;
    let process = entry
        .name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("process");
    match entry.role.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
        None => {
            if entry.service.is_some() {
                return Err(invalid_managed_process_definition(
                    selector,
                    process,
                    "`service` is only supported on `role = \"shell\"` entries",
                ));
            }
            Ok(ManagedProcessRole::Standard)
        }
        Some("lifecycle") => {
            if entry.service.is_some() {
                return Err(invalid_managed_process_definition(
                    selector,
                    process,
                    "`role = \"lifecycle\"` does not accept `service`",
                ));
            }
            if !task
                .container_lifecycle
                .unwrap_or(false)
            {
                return Err(invalid_managed_process_definition(
                    selector,
                    process,
                    "`role = \"lifecycle\"` requires `container_lifecycle = true` on `[tasks.<name>]`",
                ));
            }
            if !has_container_backed_binding {
                return Err(invalid_managed_process_definition(
                    selector,
                    process,
                    "`role = \"lifecycle\"` requires a container-backed execution binding on the task (`workspace`)",
                ));
            }
            if entry.run.is_some() || entry.task.is_some() {
                return Err(invalid_managed_process_definition(
                    selector,
                    process,
                    "`role = \"lifecycle\"` owns container startup/shutdown in this batch; omit `run` and `task`",
                ));
            }
            Ok(ManagedProcessRole::Lifecycle)
        }
        Some("shell") => {
            if !has_container_backed_binding {
                return Err(invalid_managed_process_definition(
                    selector,
                    process,
                    "`role = \"shell\"` requires a container-backed execution binding on the task (`workspace`)",
                ));
            }
            if entry.run.is_some() || entry.task.is_some() {
                return Err(invalid_managed_process_definition(
                    selector,
                    process,
                    "`role = \"shell\"` owns the container shell in this batch; omit `run` and `task`",
                ));
            }
            Ok(ManagedProcessRole::Shell)
        }
        Some(role) => Err(invalid_managed_process_definition(
            selector,
            process,
            &format!(
                "unsupported `role = \"{role}\"` in this batch; supported roles: `lifecycle`, `shell`"
            ),
        )),
    }
}

fn task_has_container_backed_execution_binding(
    catalog: &LoadedCatalog,
    selector: &TaskSelector,
    task: &ManifestTask,
) -> Result<bool, crate::ManagedError> {
    let binding = resolve_task_execution_binding(&catalog.manifest, &selector.task_name, task)
        .map_err(|error| crate::ManagedError::task_invocation(error.to_string()))?;
    Ok(matches!(
        binding,
        Some(ResolvedTaskExecutionBinding::Workspace(_))
    ))
}

fn lifecycle_shutdown_on_exit(
    selector: &TaskSelector,
    entry: &ManifestManagedConcurrentEntry,
    role: ManagedProcessRole,
) -> Result<bool, crate::ManagedError> {
    if role != ManagedProcessRole::Lifecycle {
        return Ok(entry.shutdown_on_exit.unwrap_or(false));
    }
    if entry.shutdown_on_exit == Some(false) {
        return Err(invalid_managed_process_definition(
            selector,
            entry.name.as_deref().unwrap_or("lifecycle"),
            "`role = \"lifecycle\"` must keep `shutdown_on_exit = true`",
        ));
    }
    Ok(true)
}
