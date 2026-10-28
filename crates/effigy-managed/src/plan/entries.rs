use std::collections::HashSet;
use std::path::{Path, PathBuf};

use effigy_manifest::{
    LoadedCatalog, ManifestManagedConcurrentEntry, ManifestTask, ManifestTaskRunIn, TaskResolverFn,
};
use effigy_tasks::TaskSelector;

use super::{invalid_managed_process_definition, select_run_or_task};
use crate::references;
use crate::run_spec::{render_run_step_sequence, wrap_reference_command_in_cwd, HostAlias};
use crate::{
    resolve_catalog_env_schema_with_ancestors, ManagedProcessRole, ManagedProcessSpec,
    DEFAULT_MANAGED_SHELL_RUN,
};

#[derive(Clone, Copy)]
struct EntryResolutionContext<'a> {
    selector: &'a TaskSelector,
    task: &'a ManifestTask,
    catalog: &'a LoadedCatalog,
    catalogs: &'a [LoadedCatalog],
    task_scope_cwd: &'a Path,
    host_env_aliases: &'a [HostAlias],
    resolver: TaskResolverFn<'a>,
}

#[allow(clippy::too_many_arguments)]
pub fn resolve_concurrent_process_entries<'a>(
    selector: &TaskSelector,
    task: &ManifestTask,
    entries: &[ManifestManagedConcurrentEntry],
    catalog: &LoadedCatalog,
    catalogs: &'a [LoadedCatalog],
    task_scope_cwd: &Path,
    host_env_aliases: &'a [HostAlias],
    resolver: TaskResolverFn<'a>,
) -> Result<Vec<super::ConcurrentResolvedProcess>, crate::ManagedError> {
    let context = EntryResolutionContext {
        selector,
        task,
        catalog,
        catalogs,
        task_scope_cwd,
        host_env_aliases,
        resolver,
    };
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

        let (run, cwd) = resolve_process_run_and_cwd(context, &process_name, entry)?;
        let setup = resolve_process_setup(context, &process_name, entry, &cwd)?;
        resolved.push(super::ConcurrentResolvedProcess {
            spec: ManagedProcessSpec {
                name: process_name,
                role: normalized.role,
                run,
                secret_env: Default::default(),
                setup,
                setup_steps: entry.setup.clone(),
                cwd,
                service: normalized.service,
                start_after_ms: normalized.start_after_ms,
                after: normalized.after,
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

/// The task's own `run`, as the managed session's one workload, for a task
/// bound to a container environment with no `concurrent` entries. It renders
/// exactly as the task's standard execution would, and keeps the passthrough
/// arguments the operator typed after the profile.
#[allow(clippy::too_many_arguments)]
pub fn resolve_own_task_run_process(
    selector: &TaskSelector,
    task: &ManifestTask,
    catalog: &LoadedCatalog,
    catalogs: &[LoadedCatalog],
    task_scope_cwd: &Path,
    passthrough: &[String],
    host_env_aliases: &[HostAlias],
    resolver: TaskResolverFn<'_>,
) -> Result<Option<super::ConcurrentResolvedProcess>, crate::ManagedError> {
    let Some(run_spec) = task.run.as_ref() else {
        return Ok(None);
    };
    let args = passthrough.get(1..).unwrap_or_default();
    let env_schema_resolved =
        resolve_catalog_env_schema_with_ancestors(catalogs, task_scope_cwd, None)?;
    let mut task_env = env_schema_resolved
        .as_ref()
        .map(|resolved| resolved.plain_env())
        .unwrap_or_default();
    for (key, value) in &task.env {
        task_env.insert(key.clone(), value.clone());
    }
    let command = crate::run_spec::render_task_run_spec(
        run_spec,
        crate::run_spec::RunSpecContext {
            task_name: &selector.task_name,
            task_env: &task_env,
            task_env_file: task.env_file.as_ref(),
            env_profiles: &catalog.manifest.env,
            args_rendered: &effigy_tasks::render_passthrough_args(args),
            args_raw: args,
            repo_root: task_scope_cwd,
            bundle_root: catalog.bundle_root.as_deref(),
            catalogs,
            task_scope_cwd,
            invocation_cwd: task_scope_cwd,
            runtime_env_schema_override: None,
            host_env_aliases,
            depth: 0,
            resolver,
        },
    )?;
    Ok(Some(super::ConcurrentResolvedProcess {
        spec: ManagedProcessSpec {
            name: selector.task_name.clone(),
            role: ManagedProcessRole::Standard,
            run: command,
            secret_env: Default::default(),
            setup: None,
            setup_steps: Vec::new(),
            cwd: task_scope_cwd.to_path_buf(),
            service: None,
            start_after_ms: 0,
            shutdown_on_exit: false,
            run_on_host: matches!(task.run_in, Some(ManifestTaskRunIn::Host)),
            after: Vec::new(),
        },
        start_rank: 1,
        tab_rank: 1,
        index: 0,
    }))
}

fn resolve_process_setup<'a>(
    context: EntryResolutionContext<'a>,
    process_name: &str,
    entry: &ManifestManagedConcurrentEntry,
    process_cwd: &Path,
) -> Result<Option<String>, crate::ManagedError> {
    let EntryResolutionContext {
        selector,
        task,
        catalog,
        catalogs,
        resolver,
        ..
    } = context;
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
        &entry.setup,
        crate::run_spec::RunStepSequenceContext {
            owner_label: process_name,
            task_env: &task.env,
            task_env_file: task.env_file.as_ref(),
            env_profiles: &catalog.manifest.env,
            repo_root: &catalog.catalog_root,
            bundle_root: catalog.bundle_root.as_deref(),
            catalogs,
            task_scope_cwd: process_cwd,
            runtime_env_schema_override: None,
            host_env_aliases: context.host_env_aliases,
            resolver,
        },
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
    after: Vec<String>,
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
    let after = normalize_after(selector, &process_name, entry)?;
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
        after,
        shutdown_on_exit: lifecycle_shutdown_on_exit(selector, entry, role)?,
        run_on_host,
    })
}

/// `after` names a dependency the session must observe before this entry
/// starts. Today only the environment phase is available: it always precedes
/// every workload, so declaring it states the relationship rather than
/// changing the order. Ordering between environment host processes is their
/// own `depends_on`.
fn normalize_after(
    selector: &TaskSelector,
    process_name: &str,
    entry: &ManifestManagedConcurrentEntry,
) -> Result<Vec<String>, crate::ManagedError> {
    let mut after = Vec::new();
    for value in &entry.after {
        let dependency = value.trim();
        if dependency.is_empty() {
            return Err(invalid_managed_process_definition(
                selector,
                process_name,
                "`after` entries must name a dependency",
            ));
        }
        if dependency != crate::MANAGED_ENTRY_AFTER_ENVIRONMENT {
            return Err(invalid_managed_process_definition(
                selector,
                process_name,
                &format!(
                    "unsupported `after = \"{dependency}\"`; the one supported dependency is \
                     `\"environment\"` (host processes order by `depends_on`)"
                ),
            ));
        }
        if !after.iter().any(|existing| existing == dependency) {
            after.push(dependency.to_owned());
        }
    }
    Ok(after)
}

fn resolve_process_run_and_cwd<'a>(
    context: EntryResolutionContext<'a>,
    process_name: &str,
    entry: &ManifestManagedConcurrentEntry,
) -> Result<(String, PathBuf), crate::ManagedError> {
    let EntryResolutionContext {
        selector,
        task_scope_cwd,
        ..
    } = context;
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
        super::RunOrTaskRef::Task(task_ref) => {
            resolve_task_process_run_and_cwd(context, process_name, task_ref)
        }
        super::RunOrTaskRef::Run(run) => Ok((run.to_owned(), task_scope_cwd.to_path_buf())),
    }
}

fn resolve_task_process_run_and_cwd<'a>(
    context: EntryResolutionContext<'a>,
    process_name: &str,
    task_ref: &str,
) -> Result<(String, PathBuf), crate::ManagedError> {
    let EntryResolutionContext {
        selector,
        catalogs,
        task_scope_cwd,
        host_env_aliases,
        resolver,
        ..
    } = context;
    if task_ref.trim() == "shell" {
        return resolve_shell_process_run(selector, process_name, task_scope_cwd);
    }
    references::resolve_task_reference_run(
        &selector.task_name,
        process_name,
        task_ref,
        catalogs,
        task_scope_cwd,
        host_env_aliases,
        resolver,
    )
}

fn resolve_shell_process_run(
    selector: &TaskSelector,
    process_name: &str,
    task_scope_cwd: &Path,
) -> Result<(String, PathBuf), crate::ManagedError> {
    if process_name != "shell" {
        return Err(invalid_managed_process_definition(
            selector,
            process_name,
            "task `shell` must use process name `shell` (omit `name` or set `name = \"shell\"`)",
        ));
    }
    Ok((
        DEFAULT_MANAGED_SHELL_RUN.to_owned(),
        task_scope_cwd.to_path_buf(),
    ))
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
    match entry
        .role
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
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
            if !task.container_lifecycle.unwrap_or(false) {
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
                    "`role = \"lifecycle\"` owns container startup/shutdown; omit `run` and `task`",
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
                    "`role = \"shell\"` owns the container shell; omit `run` and `task`",
                ));
            }
            Ok(ManagedProcessRole::Shell)
        }
        Some(role) => Err(invalid_managed_process_definition(
            selector,
            process,
            &format!("unsupported `role = \"{role}\"`; supported roles: `lifecycle`, `shell`"),
        )),
    }
}

fn task_has_container_backed_execution_binding(
    catalog: &LoadedCatalog,
    selector: &TaskSelector,
    task: &ManifestTask,
) -> Result<bool, crate::ManagedError> {
    super::task_has_container_backed_execution_binding(catalog, selector, task)
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
