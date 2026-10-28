use std::path::Path;

use effigy_manifest::{
    resolve_task_execution_binding, LoadedCatalog, ManifestTask, ResolvedTaskExecutionBinding,
    TaskResolverFn,
};
use effigy_tasks::TaskSelector;

use crate::profiles::ResolvedConcurrentProfile;
use crate::{
    ManagedError, ManagedProcessRole, ManagedProcessSpec, ManagedTaskPlan, ManagedTaskReadiness,
    DEFAULT_MANAGED_HEALTH_WAIT_TIMEOUT_SECS,
};

#[path = "plan/entries.rs"]
mod entries;
#[path = "plan/ordering.rs"]
mod ordering;

pub struct ManagedConcurrentPlanInput<'a> {
    pub selector: &'a TaskSelector,
    pub catalog: &'a LoadedCatalog,
    pub task: &'a ManifestTask,
    pub profile: ResolvedConcurrentProfile<'a>,
    pub passthrough: &'a [String],
    pub catalogs: &'a [LoadedCatalog],
    pub task_scope_cwd: &'a Path,
    /// Declared backing-service aliases a host execution environment resolves
    /// to this checkout's effective aliases (effigy#431, contract 005).
    pub host_env_aliases: &'a [crate::run_spec::HostAlias],
    pub resolver: TaskResolverFn<'a>,
}

/// What a profile declared: its own `concurrent` entries, or nothing at all.
/// A plain session over a bound environment owns that environment; a profile
/// with entries of its own says so with `container_lifecycle` or a lifecycle
/// entry (effigy#322).
#[derive(Debug, Clone, Copy)]
enum SessionShape {
    Declared,
    EnvironmentOnly { environment_bound: bool },
}

#[derive(Debug)]
struct ConcurrentResolvedProcess {
    spec: ManagedProcessSpec,
    start_rank: usize,
    tab_rank: usize,
    index: usize,
}

pub fn resolve_managed_concurrent_task_plan(
    input: ManagedConcurrentPlanInput<'_>,
) -> Result<ManagedTaskPlan, ManagedError> {
    let ManagedConcurrentPlanInput {
        selector,
        catalog,
        task,
        profile,
        passthrough,
        catalogs,
        task_scope_cwd,
        host_env_aliases,
        resolver,
    } = input;
    let environment_bound = task_is_environment_bound(catalog, selector, task)?;
    if profile.entries.is_empty() {
        // A managed task bound to a container environment is a session over
        // that environment: its host processes and services are the tab set,
        // and the task's own `run` is the one workload when it declares one.
        // Nothing needs a `concurrent` entry per process any more.
        let mut resolved = Vec::new();
        if let Some(own) = entries::resolve_own_task_run_process(
            selector,
            task,
            catalog,
            catalogs,
            task_scope_cwd,
            passthrough,
            host_env_aliases,
            resolver,
        )? {
            resolved.push(own);
        } else if !environment_bound {
            return Err(ManagedError::TaskManagedProfileEmpty {
                task: selector.task_name.clone(),
                profile: profile.profile_name.clone(),
            });
        }
        return build_managed_task_plan(
            selector,
            catalog,
            task,
            &profile.profile_name,
            passthrough,
            &mut resolved,
            SessionShape::EnvironmentOnly { environment_bound },
        );
    }

    let mut resolved = entries::resolve_concurrent_process_entries(
        selector,
        task,
        profile.entries,
        catalog,
        catalogs,
        task_scope_cwd,
        host_env_aliases,
        resolver,
    )?;
    build_managed_task_plan(
        selector,
        catalog,
        task,
        &profile.profile_name,
        passthrough,
        &mut resolved,
        SessionShape::Declared,
    )
}

fn build_managed_task_plan(
    selector: &TaskSelector,
    catalog: &LoadedCatalog,
    task: &ManifestTask,
    profile_name: &str,
    passthrough: &[String],
    resolved: &mut [ConcurrentResolvedProcess],
    shape: SessionShape,
) -> Result<ManagedTaskPlan, ManagedError> {
    let lifecycle_entry = resolved
        .iter()
        .any(|entry| entry.spec.role == ManagedProcessRole::Lifecycle);
    // A plain session over a bound environment (no `concurrent` entries of its
    // own) owns that environment; so does an explicit lifecycle entry and
    // `container_lifecycle = true`. `container_lifecycle = false` opts out.
    let owns_environment = task.container_lifecycle != Some(false)
        && (lifecycle_entry
            || task.container_lifecycle == Some(true)
            || matches!(
                shape,
                SessionShape::EnvironmentOnly {
                    environment_bound: true
                }
            ));
    validate_lifecycle_role_usage(selector, catalog, task, resolved, owns_environment)?;
    ordering::sort_resolved_processes(resolved);
    let tab_order = ordering::build_tab_order(resolved);
    let processes = resolved.iter().map(|entry| entry.spec.clone()).collect();

    Ok(ManagedTaskPlan {
        mode: "tui".to_owned(),
        profile: profile_name.to_owned(),
        processes,
        owns_environment,
        tab_order,
        fail_on_non_zero: task.fail_on_non_zero.unwrap_or(true),
        passthrough: passthrough.iter().skip(1).cloned().collect(),
        gateway_auto_start: task.gateway.unwrap_or(false),
        readiness: ManagedTaskReadiness {
            health_wait: task.health_wait.unwrap_or(false),
            timeout_secs: task
                .health_wait_timeout_secs
                .unwrap_or(DEFAULT_MANAGED_HEALTH_WAIT_TIMEOUT_SECS),
            ready_message: task.ready_message.clone(),
        },
        // The runner fills this in once it resolves the container
        // policy for lifecycle-backed plans.
    })
}

fn validate_lifecycle_role_usage(
    selector: &TaskSelector,
    catalog: &LoadedCatalog,
    task: &ManifestTask,
    resolved: &[ConcurrentResolvedProcess],
    owns_environment: bool,
) -> Result<(), ManagedError> {
    let has_container_backed_binding =
        task_has_container_backed_execution_binding(catalog, selector, task)?;
    let lifecycle_count = resolved
        .iter()
        .filter(|entry| entry.spec.role == ManagedProcessRole::Lifecycle)
        .count();
    if lifecycle_count > 1 {
        return Err(invalid_managed_process_definition(
            selector,
            "lifecycle",
            "only one `role = \"lifecycle\"` entry is supported",
        ));
    }
    // A managed task bound to a container environment owns that environment:
    // its host processes and its workloads are its tabs, with or without a
    // `role = "lifecycle"` entry (effigy#322). `container_lifecycle = false`
    // is the explicit opt-out, and `gateway`/`health_wait` need only the
    // binding.
    if task.container_lifecycle == Some(false) && lifecycle_count > 0 {
        return Err(invalid_managed_process_definition(
            selector,
            "lifecycle",
            "`role = \"lifecycle\"` requires `container_lifecycle = true` on `[tasks.<name>]`",
        ));
    }
    for (flag, enabled) in [
        ("gateway", task.gateway.unwrap_or(false)),
        ("health_wait", task.health_wait.unwrap_or(false)),
    ] {
        if !enabled {
            continue;
        }
        if !has_container_backed_binding {
            return Err(invalid_managed_process_definition(
                selector,
                "managed",
                &format!(
                    "`{flag} = true` requires a container-backed execution binding on the task (`workspace`)"
                ),
            ));
        }
        if !owns_environment {
            return Err(invalid_managed_process_definition(
                selector,
                "managed",
                &format!(
                    "`{flag} = true` requires a session that owns its environment: bind the task with no `concurrent` entries of its own, or set `container_lifecycle = true`"
                ),
            ));
        }
    }
    if let Some(ready_message) = task.ready_message.as_deref() {
        if ready_message.trim().is_empty() {
            return Err(invalid_managed_process_definition(
                selector,
                "managed",
                "`ready_message` must be a non-empty string",
            ));
        }
        if !task.health_wait.unwrap_or(false) {
            return Err(invalid_managed_process_definition(
                selector,
                "managed",
                "`ready_message` requires `health_wait = true`",
            ));
        }
    }
    if task.health_wait_timeout_secs == Some(0) {
        return Err(invalid_managed_process_definition(
            selector,
            "managed",
            "`health_wait_timeout_secs` must be greater than zero",
        ));
    }
    if task.health_wait_timeout_secs.is_some() && !task.health_wait.unwrap_or(false) {
        return Err(invalid_managed_process_definition(
            selector,
            "managed",
            "`health_wait_timeout_secs` requires `health_wait = true`",
        ));
    }
    Ok(())
}

fn task_has_container_backed_execution_binding(
    catalog: &LoadedCatalog,
    selector: &TaskSelector,
    task: &ManifestTask,
) -> Result<bool, ManagedError> {
    let binding = resolve_task_execution_binding(&catalog.manifest, &selector.task_name, task)
        .map_err(|error| ManagedError::task_invocation(error.to_string()))?;
    Ok(matches!(
        binding,
        Some(ResolvedTaskExecutionBinding::Workspace(_))
    ))
}

/// Whether this task is bound to a container environment, and so may be a
/// managed session over that environment alone.
pub(crate) fn task_is_environment_bound(
    catalog: &LoadedCatalog,
    selector: &TaskSelector,
    task: &ManifestTask,
) -> Result<bool, ManagedError> {
    task_has_container_backed_execution_binding(catalog, selector, task)
}

/// The same answer for a named task, for a caller that has no selector.
pub fn task_has_environment_binding(
    catalog: &LoadedCatalog,
    task_name: &str,
    task: &ManifestTask,
) -> Result<bool, ManagedError> {
    task_has_container_backed_execution_binding(
        catalog,
        &TaskSelector {
            prefix: None,
            task_name: task_name.to_owned(),
        },
        task,
    )
}

enum RunOrTaskRef<'a> {
    Run(&'a str),
    Task(&'a str),
}

fn select_run_or_task<'a, FBoth, FNone>(
    run: Option<&'a str>,
    task: Option<&'a str>,
    both_error: FBoth,
    none_error: FNone,
) -> Result<RunOrTaskRef<'a>, ManagedError>
where
    FBoth: FnOnce() -> ManagedError,
    FNone: FnOnce() -> ManagedError,
{
    match (run, task) {
        (Some(run), None) => Ok(RunOrTaskRef::Run(run)),
        (None, Some(task)) => Ok(RunOrTaskRef::Task(task)),
        (Some(_), Some(_)) => Err(both_error()),
        (None, None) => Err(none_error()),
    }
}

pub fn invalid_managed_process_definition(
    selector: &TaskSelector,
    process: &str,
    detail: &str,
) -> ManagedError {
    ManagedError::TaskManagedProcessInvalidDefinition {
        task: selector.task_name.clone(),
        process: process.to_owned(),
        detail: detail.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::resolve_managed_task_plan;
    use effigy_manifest::TaskManifest;
    use effigy_tasks::{TaskRuntimeArgs, TaskSelector};

    /// No test here resolves a task reference; a reference would be a bug in
    /// the fixture.
    fn no_references<'c>(
        selector: &TaskSelector,
        _catalogs: &'c [LoadedCatalog],
        _cwd: &Path,
    ) -> Result<effigy_manifest::TaskSelection<'c>, String> {
        Err(format!(
            "test fixture declares no reference to `{}`",
            selector.task_name
        ))
    }

    fn plan_for(
        manifest: &str,
        task: &str,
        passthrough: &[&str],
    ) -> (
        tempfile::TempDir,
        Result<Option<ManagedTaskPlan>, ManagedError>,
    ) {
        let root = tempfile::tempdir().expect("fixture dir");
        let manifest: TaskManifest = toml::from_str(manifest).expect("fixture manifest parses");
        let catalog = LoadedCatalog::for_test("repo", root.path(), manifest, 0);
        let selected = catalog
            .manifest
            .tasks
            .get(task)
            .unwrap_or_else(|| panic!("fixture declares task `{task}`"));
        let selector = TaskSelector {
            prefix: None,
            task_name: task.to_owned(),
        };
        let runtime_args = TaskRuntimeArgs {
            passthrough: passthrough.iter().map(|arg| (*arg).to_owned()).collect(),
            ..Default::default()
        };
        let resolver: effigy_manifest::TaskResolverFn<'_> = &no_references;
        let result = resolve_managed_task_plan(
            &selector,
            &catalog,
            selected,
            &runtime_args,
            std::slice::from_ref(&catalog),
            &catalog.catalog_root,
            &[],
            resolver,
        );
        (root, result)
    }

    const BOUND_DEV: &str = r#"
[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"
working_dir = "."

[containers]
default = "web"

[containers.web]
driver = "colima"
compose_file = "compose.yml"

[tasks.dev]
mode = "tui"
workspace = "app"
run = "bun run dev"
# BOUND_DEV_TASK_FIELDS
"#;

    /// effigy#322: a plain `[tasks.dev]` bound to an environment is a managed
    /// session. Its own `run` is the one workload; no `concurrent` entry and
    /// no `role = "lifecycle"` entry is required.
    #[test]
    fn a_bound_task_needs_no_concurrent_entries() {
        let (_root, plan) = plan_for(BOUND_DEV, "dev", &[]);
        let plan = plan
            .expect("a bound task is a managed session")
            .expect("plan");
        assert_eq!(plan.profile, crate::DEFAULT_MANAGED_PROFILE);
        assert_eq!(
            plan.processes.len(),
            1,
            "the task's own run is the workload"
        );
        assert_eq!(plan.processes[0].name, "dev");
        assert_eq!(plan.processes[0].role, ManagedProcessRole::Standard);
        assert_eq!(plan.processes[0].run, "bun run dev");
        assert_eq!(plan.tab_order, vec!["dev".to_owned()]);
    }

    /// The profile's passthrough arguments reach the task's own run, minus the
    /// profile slot the managed invocation already consumed.
    #[test]
    fn a_bound_task_passes_profile_arguments_to_its_own_run() {
        let manifest = BOUND_DEV.replace("bun run dev", "bun run dev {args}");
        let (_root, plan) = plan_for(&manifest, "dev", &["default", "--", "--port", "3000"]);
        let plan = plan.expect("managed session").expect("plan");
        assert!(
            plan.processes[0].run.contains("'--port' '3000'"),
            "the arguments after the profile reach the run: {}",
            plan.processes[0].run
        );
    }

    /// A bound environment with nothing to run is still a session: the
    /// environment's host processes and services are its tabs.
    #[test]
    fn a_bound_environment_without_a_run_is_an_environment_only_session() {
        let manifest = BOUND_DEV.replace("run = \"bun run dev\"\n", "");
        let (_root, plan) = plan_for(&manifest, "dev", &[]);
        let plan = plan.expect("managed session").expect("plan");
        assert!(
            plan.processes.is_empty(),
            "an environment-only session runs no workload of its own"
        );
    }

    /// The shape still needs one: an unbound task with no `concurrent` entries
    /// has nothing a session could show.
    #[test]
    fn an_unbound_task_without_concurrent_entries_is_refused() {
        let manifest = r#"
[tasks.dev]
mode = "tui"
run = "bun run dev"
"#;
        let (_root, plan) = plan_for(manifest, "dev", &[]);
        let error = plan
            .expect_err("an unbound task needs concurrent entries")
            .to_string();
        assert!(error.contains("requires `concurrent"), "got: {error}");
    }

    /// `container_lifecycle = true` no longer demands a lifecycle entry: the
    /// environment is the binding's.
    #[test]
    fn container_lifecycle_needs_no_lifecycle_entry() {
        let manifest = BOUND_DEV.replace("# BOUND_DEV_TASK_FIELDS", "container_lifecycle = true");
        let (_root, plan) = plan_for(&manifest, "dev", &[]);
        assert!(
            plan.expect("managed session")
                .expect("plan")
                .processes
                .len()
                == 1,
            "the environment is the binding's, not a role's"
        );
    }

    /// `gateway` and `health_wait` need a session that owns its environment,
    /// which a plain bound task is.
    #[test]
    fn gateway_and_health_wait_need_only_an_environment_session() {
        let manifest = BOUND_DEV.replace(
            "# BOUND_DEV_TASK_FIELDS",
            "gateway = true\nhealth_wait = true",
        );
        let (_root, plan) = plan_for(&manifest, "dev", &[]);
        let plan = plan.expect("managed session").expect("plan");
        assert!(plan.owns_environment);
    }

    /// A session with its own entries owns nothing unless it says so, so a
    /// readiness gate would have no environment to observe.
    #[test]
    fn health_wait_needs_a_session_that_owns_its_environment() {
        let manifest = BOUND_DEV
            .replace("# BOUND_DEV_TASK_FIELDS", "health_wait = true")
            .replace(
                "run = \"bun run dev\"",
                "concurrent = [{ name = \"api\", run = \"bun run api\" }]",
            );
        let (_root, plan) = plan_for(&manifest, "dev", &[]);
        let error = plan.expect_err("no environment to wait for").to_string();
        assert!(error.contains("owns its environment"), "got: {error}");
        assert!(error.contains("container_lifecycle = true"), "got: {error}");
    }

    #[test]
    fn after_names_the_environment_dependency() {
        let manifest = r#"
[tasks.dev]
mode = "tui"
concurrent = [
  { name = "api", run = "bun run api", after = ["environment"] },
  { name = "jobs", run = "bun run jobs", start_after_ms = 1500 }
]
"#;
        let (_root, plan) = plan_for(manifest, "dev", &[]);
        let plan = plan.expect("managed session").expect("plan");
        let api = plan
            .processes
            .iter()
            .find(|process| process.name == "api")
            .expect("api");
        assert_eq!(api.after, vec!["environment".to_owned()]);
        let jobs = plan
            .processes
            .iter()
            .find(|process| process.name == "jobs")
            .expect("jobs");
        assert_eq!(
            jobs.start_after_ms, 1500,
            "the retired timer still delays its entry for the v0.15 line"
        );
    }

    #[test]
    fn after_refuses_an_unknown_dependency() {
        let manifest = r#"
[tasks.dev]
mode = "tui"
concurrent = [{ name = "api", run = "bun run api", after = ["database"] }]
"#;
        let (_root, plan) = plan_for(manifest, "dev", &[]);
        let error = plan.expect_err("unknown dependency").to_string();
        assert!(error.contains("database"), "got: {error}");
        assert!(error.contains("environment"), "got: {error}");
    }

    #[test]
    fn a_terminal_program_with_its_environment_is_a_session() {
        let manifest = r#"
[tasks.dev]
mode = "tui"
concurrent = [{ name = "api", run = "printf api", start = 1, tab = 1 }]
"#;
        let (_root, plan) = plan_for(manifest, "dev", &[]);
        assert!(plan.expect("managed session").is_some());
    }
}
