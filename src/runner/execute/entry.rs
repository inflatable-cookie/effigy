use std::cell::Cell;
use std::collections::BTreeMap;

use effigy_cli::TaskInvocation;
use effigy_execution::{ExecutionDispatchPlan, ExecutionPreflightInput, TaskExecutionRequest};
use effigy_manifest::{ManifestTaskAdmission, TaskSelection};

use super::pipeline::run_execution_pipeline;
use super::planning::build_execution_preflight_from_input;
use crate::runner::error::RunnerError;
use crate::runner::host_scheduler::{self, Route, SubmitContext};

thread_local! {
    // None means a top-level invocation. Some(false) is still a completed
    // admission decision: children of a direct run must stay direct.
    static ADMISSION: Cell<Option<bool>> = const { Cell::new(None) };
}

struct AdmissionScope(Option<bool>);

impl AdmissionScope {
    fn enter(host_admitted: bool) -> Self {
        Self(ADMISSION.replace(Some(host_admitted)))
    }
}

impl Drop for AdmissionScope {
    fn drop(&mut self) {
        ADMISSION.set(self.0);
    }
}

fn run_manifest_task_with_preflight_input(
    task: &TaskInvocation,
    mut input: ExecutionPreflightInput,
) -> Result<String, RunnerError> {
    let (task, request_id) = task_without_request_identity(task)?;
    if request_id.is_some() {
        input.args = task.args.clone();
    }
    let preflight = build_execution_preflight_from_input(input)?;
    let _local_dev_secrets = request_id.is_none().then(|| {
        crate::runner::secret_session::activate_local_dev_secret_access(
            !preflight.plan && preflight.selector.task_name == "dev",
        )
    });
    if preflight.plan {
        if request_id.is_some() {
            return Err(host_scheduler::refuse(
                2,
                "--host-run-request-id applies only to an invocation that can submit a heavy selector",
            ));
        }
        return run_execution_pipeline(&task, preflight);
    }
    let (selection, selection_plan) = match super::selection::resolve_task_selection(
        &task, &preflight,
    )? {
        super::selection::SelectionResolution::Selected { selection, plan } => (selection, plan),
        super::selection::SelectionResolution::Output(output) => {
            if request_id.is_some() {
                return Err(host_scheduler::refuse(
                    2,
                    "--host-run-request-id cannot be used with a selector that does not submit a run",
                ));
            }
            return Ok(output);
        }
    };
    run_selected_task_with_scheduler(
        &preflight,
        &selection,
        request_id.as_deref(),
        |host_admitted| {
            let _request_local_dev_secrets = request_id.as_ref().map(|_| {
                crate::runner::secret_session::activate_local_dev_secret_access(
                    preflight.selector.task_name == "dev",
                )
            });
            if let Some(output) =
                super::pipeline::managed::run_managed_task(&preflight, &selection, &selection_plan)?
            {
                return Ok(output);
            }
            super::pipeline::standard::run_standard_task(
                &preflight,
                &selection,
                &selection_plan,
                host_admitted,
            )
        },
    )
}

fn run_manifest_task_with_preflight_input_and_env(
    task: &TaskInvocation,
    mut input: ExecutionPreflightInput,
    env_overrides: &BTreeMap<String, String>,
) -> Result<String, RunnerError> {
    let (task, request_id) = task_without_request_identity(task)?;
    if request_id.is_some() {
        input.args = task.args.clone();
    }
    let preflight = build_execution_preflight_from_input(input)?;
    let _local_dev_secrets = request_id.is_none().then(|| {
        crate::runner::secret_session::activate_local_dev_secret_access(
            !preflight.plan && preflight.selector.task_name == "dev",
        )
    });
    if preflight.plan {
        if request_id.is_some() {
            return Err(host_scheduler::refuse(
                2,
                "--host-run-request-id applies only to an invocation that can submit a heavy selector",
            ));
        }
        return run_execution_pipeline(&task, preflight);
    }
    let (selection, selection_plan) = match super::selection::resolve_task_selection(
        &task, &preflight,
    )? {
        super::selection::SelectionResolution::Selected { selection, plan } => (selection, plan),
        super::selection::SelectionResolution::Output(output) => {
            if request_id.is_some() {
                return Err(host_scheduler::refuse(
                    2,
                    "--host-run-request-id cannot be used with a selector that does not submit a run",
                ));
            }
            return Ok(output);
        }
    };

    let mut overridden_task = selection.task.clone();
    for (key, value) in env_overrides {
        overridden_task.env.insert(key.clone(), value.clone());
    }
    let overridden_selection = effigy_manifest::TaskSelection {
        catalog: selection.catalog,
        task: &overridden_task,
        mode: selection.mode,
        evidence: selection.evidence,
        surface: selection.surface,
    };

    run_selected_task_with_scheduler(
        &preflight,
        &overridden_selection,
        request_id.as_deref(),
        |host_admitted| {
            let _request_local_dev_secrets = request_id.as_ref().map(|_| {
                crate::runner::secret_session::activate_local_dev_secret_access(
                    preflight.selector.task_name == "dev",
                )
            });
            if let Some(output) = super::pipeline::managed::run_managed_task(
                &preflight,
                &overridden_selection,
                &selection_plan,
            )? {
                return Ok(output);
            }
            super::pipeline::standard::run_standard_task(
                &preflight,
                &overridden_selection,
                &selection_plan,
                host_admitted,
            )
        },
    )
}

fn run_selected_task_with_scheduler(
    preflight: &super::planning::ExecutionPreflight,
    selection: &TaskSelection<'_>,
    client_request_id: Option<&str>,
    execute: impl FnOnce(bool) -> Result<String, RunnerError>,
) -> Result<String, RunnerError> {
    // Canonical resolution and task gates still apply to children. Host-run
    // admission belongs to the outer invocation only (Q-006): even a nested
    // `qa` or explicitly heavy step inherits a direct parent's decision.
    if let Some(host_admitted) = ADMISSION.get() {
        if client_request_id.is_some() {
            return Err(host_scheduler::refuse(
                2,
                "--host-run-request-id requires a top-level heavy selector submission",
            ));
        }
        return execute(host_admitted);
    }
    let execute = |host_admitted| {
        let _admission = AdmissionScope::enter(host_admitted);
        execute(host_admitted)
    };
    let in_owned_child_scope = crate::runner::owned_children::signal_scope_active();
    if in_owned_child_scope && client_request_id.is_none() {
        return execute(host_scheduler::active_nested().is_some());
    }
    let task_name = preflight.selector.task_name.as_str();
    let managed_control = is_managed_control_invocation(
        selection.task.mode.as_deref(),
        &preflight.runtime_args_exec,
    )?;
    if managed_control && client_request_id.is_none() {
        let parent = host_scheduler::validate_parent_if_present(&preflight.invocation_cwd)?;
        return execute(parent.is_some());
    }
    let selected_heavy = matches!(selection.task.admission, Some(ManifestTaskAdmission::Heavy));
    let heavy = selected_heavy || matches!(task_name, "qa" | "ci" | "ci:fresh");
    let selector = preflight.selector.prefix.as_ref().map_or_else(
        || task_name.to_owned(),
        |prefix| format!("{prefix}/{task_name}"),
    );
    if client_request_id.is_some() && !heavy {
        let _ =
            host_scheduler::route_heavy(&selector, &preflight.invocation_cwd, client_request_id)?;
        return Err(host_scheduler::refuse(
            2,
            "--host-run-request-id requires a top-level heavy selector submission",
        ));
    }
    if client_request_id.is_some() && (managed_control || in_owned_child_scope) {
        let _ =
            host_scheduler::route_heavy(&selector, &preflight.invocation_cwd, client_request_id)?;
        return Err(host_scheduler::refuse(
            2,
            "--host-run-request-id cannot be used by a managed control or owned child invocation",
        ));
    }
    if !heavy {
        let parent = host_scheduler::validate_parent_if_present(&preflight.invocation_cwd)?;
        return execute(parent.is_some());
    }

    match host_scheduler::route_heavy(&selector, &preflight.invocation_cwd, client_request_id)? {
        Route::Nested(_) => {
            if client_request_id.is_some() {
                return Err(host_scheduler::refuse(
                    2,
                    "--host-run-request-id cannot be honored by nested or override execution",
                ));
            }
            let scope =
                crate::runner::owned_children::OwnedChildrenScope::enter().map_err(|error| {
                    RunnerError::task_invocation(format!(
                        "cannot install heavy-run signal forwarding: {error}"
                    ))
                })?;
            let result = execute(true);
            drop(scope);
            result
        }
        Route::Override => {
            if client_request_id.is_some() {
                return Err(host_scheduler::refuse(
                    2,
                    "--host-run-request-id cannot be honored by nested or override execution",
                ));
            }
            let scope =
                crate::runner::owned_children::OwnedChildrenScope::enter().map_err(|error| {
                    RunnerError::task_invocation(format!(
                        "cannot install heavy-run signal forwarding: {error}"
                    ))
                })?;
            let result = execute(false);
            drop(scope);
            result
        }
        Route::Submit => {
            let selector_env_names = host_scheduler::selector_env_names_for_tasks(
                [(
                    selection.catalog.alias.as_str(),
                    preflight.selector.task_name.as_str(),
                    selection.surface,
                )],
                &preflight.catalogs,
                &preflight.invocation_cwd,
            )
            .map_err(RunnerError::task_invocation)?;
            let settled = host_scheduler::submit_and_settle(SubmitContext {
                selector: &selector,
                client_request_id,
                class_source: host_scheduler::class_source(selected_heavy),
                repository: &preflight.invocation_cwd,
                cwd: &preflight.invocation_cwd,
                selector_env_names,
            })?;
            Err(settled.into_error())
        }
    }
}

fn task_without_request_identity(
    task: &TaskInvocation,
) -> Result<(TaskInvocation, Option<String>), RunnerError> {
    let marker = effigy_cli::INTERNAL_HOST_RUN_REQUEST_ID_ARG;
    let marker_index = task.args.iter().position(|arg| arg == marker);
    let Some(index) = marker_index else {
        return Ok((task.clone(), None));
    };
    if index + 2 != task.args.len() {
        return Err(host_scheduler::refuse(
            2,
            "internal host-run request identity is malformed",
        ));
    }
    let request_id = task.args.get(index + 1).cloned().ok_or_else(|| {
        host_scheduler::refuse(2, "internal host-run request identity is missing")
    })?;
    let mut args = task.args.clone();
    args.truncate(index);
    Ok((
        TaskInvocation {
            name: task.name.clone(),
            args,
        },
        Some(request_id),
    ))
}

fn is_managed_control_invocation(
    mode: Option<&str>,
    runtime_args: &effigy_tasks::TaskRuntimeArgs,
) -> Result<bool, RunnerError> {
    if mode != Some("tui") {
        return Ok(false);
    }
    let invocation = effigy_managed::parse_managed_invocation(runtime_args)?;
    Ok(!matches!(
        invocation.action,
        effigy_managed::ManagedInvocation::Run { .. }
    ))
}

pub(in crate::runner) fn run_manifest_task_request(
    request: TaskExecutionRequest,
) -> Result<String, RunnerError> {
    let runtime_context = request.runtime_context.clone();
    if runtime_context.task_source().is_some() {
        crate::runner::command_context::with_runtime_context(&runtime_context, || {
            run_manifest_task_request_inner(request)
        })
    } else {
        run_manifest_task_request_inner(request)
    }
}

fn run_manifest_task_request_inner(request: TaskExecutionRequest) -> Result<String, RunnerError> {
    let plan = ExecutionDispatchPlan::from_request(request)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let invocation = TaskInvocation {
        name: plan.selector.clone(),
        args: plan.args.clone(),
    };
    let preflight_input = plan.preflight_input();

    if plan.request.environment.env.is_empty() {
        return run_manifest_task_with_preflight_input(&invocation, preflight_input);
    }

    let mut env_overrides = BTreeMap::new();
    for (key, value) in plan.request.environment.env {
        let value = value.into_string().map_err(|_| {
            RunnerError::task_invocation(format!(
                "execution request env override `{key}` is not valid UTF-8"
            ))
        })?;
        env_overrides.insert(key, value);
    }
    run_manifest_task_with_preflight_input_and_env(&invocation, preflight_input, &env_overrides)
}

#[cfg(test)]
mod tests {
    use super::{is_managed_control_invocation, AdmissionScope, ADMISSION};
    use effigy_tasks::TaskRuntimeArgs;

    fn args(passthrough: &[&str]) -> TaskRuntimeArgs {
        TaskRuntimeArgs {
            passthrough: passthrough
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            ..TaskRuntimeArgs::default()
        }
    }

    #[test]
    fn managed_tui_controls_do_not_request_heavy_admission() {
        for action in ["status", "logs", "stop"] {
            assert!(
                is_managed_control_invocation(Some("tui"), &args(&[action])).expect("parse"),
                "{action} should bypass validation admission"
            );
        }
    }

    #[test]
    fn managed_tui_run_and_non_tui_tasks_are_not_controls() {
        assert!(!is_managed_control_invocation(Some("tui"), &args(&[])).expect("parse"));
        assert!(!is_managed_control_invocation(None, &args(&["status"]))
            .expect("non-managed task args are ignored"));
    }

    #[test]
    fn admission_scope_restores_the_outer_decision_after_unwind() {
        assert_eq!(ADMISSION.get(), None);
        {
            let _outer = AdmissionScope::enter(false);
            let unwound = std::panic::catch_unwind(|| {
                let _inner = AdmissionScope::enter(true);
                assert_eq!(ADMISSION.get(), Some(true));
                panic!("fixture unwind");
            });
            assert!(unwound.is_err());
            assert_eq!(ADMISSION.get(), Some(false));
        }
        assert_eq!(ADMISSION.get(), None);
    }
}
