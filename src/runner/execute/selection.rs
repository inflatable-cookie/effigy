#[path = "selection/fallback.rs"]
mod fallback;
#[path = "selection/result.rs"]
mod result;

use effigy_cli::TaskInvocation;
use effigy_execution::{
    ExecutionSelectionCatalogSummary, ExecutionSelectionInput, ExecutionSelectionPlan,
};

use super::planning::ExecutionPreflight;
use crate::runner::error::RunnerError;

pub(in crate::runner) use result::SelectionResolution;

/// Resolves `task` through the one resolver and carries out a non-task
/// route (built-in, exec alias, deferral) at once.
pub(in crate::runner) fn resolve_task_selection<'a>(
    task: &TaskInvocation,
    preflight: &'a ExecutionPreflight,
) -> Result<SelectionResolution<'a>, RunnerError> {
    let route = super::route::resolve_route(task, preflight)?;
    if let Some(expected) = crate::runner::managed_run::workspace_resolution()? {
        validate_workspace_resolution(task, preflight, &route, &expected)?;
    }
    fallback::run_fallback_route(task, preflight, route)
}

/// Compare the dispatcher-owned snapshot before a fallback route can execute
/// or an engine can admit the reserved run.
fn validate_workspace_resolution(
    task: &TaskInvocation,
    preflight: &ExecutionPreflight,
    route: &super::route::Route<'_>,
    expected: &crate::runner::managed_run::WorkspaceResolution,
) -> Result<(), RunnerError> {
    use effigy_kernel::ctx::{EffectClass, Placement};
    use effigy_kernel::id::{ExecutionTarget, InvocationSpec, ResolvedInvocation, Route};
    use std::collections::BTreeMap;
    let refuse = || {
        RunnerError::task_invocation("reserved workspace task resolution changed or is unavailable")
    };
    let original = ResolvedInvocation::decode(&expected.invocation).map_err(|_| refuse())?;
    let super::route::Route::Task { selection, .. } = route else {
        return Err(refuse());
    };
    if preflight.plan || preflight.help || preflight.runtime_args_raw.repo_override.is_some() {
        return Err(refuse());
    }
    let checkout = expected.checkout.canonicalize().map_err(|_| refuse())?;
    let actual_root = preflight
        .resolved
        .resolved_root
        .canonicalize()
        .map_err(|_| refuse())?;
    let opened = effigy_kernel::fs::DirFd::open(&expected.checkout).map_err(|_| refuse())?;
    if checkout != actual_root || opened.identity().map_err(|_| refuse())? != expected.identity {
        return Err(refuse());
    }
    let catalog = selection
        .catalog
        .catalog_root
        .canonicalize()
        .map_err(|_| refuse())?;
    if !catalog.starts_with(&checkout) {
        return Err(refuse());
    }
    let target = crate::runner::task_engine::TaskTarget::for_selection(
        &preflight.resolved.resolved_root,
        &preflight.selector.task_name,
        selection,
    );
    if target.shared_lock.is_some() {
        return Err(refuse());
    }
    let checkout_id = crate::runner::task_engine::checkout::read(&target.root)
        .map_err(|_| refuse())?
        .ok_or_else(refuse)?;
    let provenance = BTreeMap::from([(
        "workspace:catalog-snapshot".to_owned(),
        crate::runner::workspace_client::host_dispatch::catalog_snapshot(&preflight.catalogs)
            .map_err(|_| refuse())?,
    )]);
    let actual = ResolvedInvocation::new(InvocationSpec {
        task: target.task_id(checkout_id).map_err(|_| refuse())?,
        route: Route::Task,
        argv: std::iter::once(target.label.clone())
            .chain(task.args.iter().cloned())
            .map(std::ffi::OsString::from)
            .collect(),
        cwd: target.catalog.clone(),
        target: ExecutionTarget::Host,
        env: BTreeMap::new(),
        owner: target.owner(checkout_id).map_err(|_| refuse())?,
        gates: Vec::new(),
        attachments: Vec::new(),
        class: EffectClass::Mutate,
        placement: Placement::Host,
        provenance,
    })
    .map_err(|_| refuse())?;
    if actual.digest() != original.digest() {
        return Err(refuse());
    }
    // The existing reserved engine supports managed runs only; standard tasks
    // and managed control verbs must not escape into an unreserved run.
    if selection.task.mode.as_deref() != Some("tui") {
        return Err(RunnerError::task_invocation(
            "reserved workspace standard-task admission is unavailable",
        ));
    }
    let managed = effigy_managed::parse_managed_invocation(&preflight.runtime_args_exec)?;
    if !matches!(
        managed.action,
        effigy_managed::ManagedInvocation::Run { detach: false, .. }
    ) {
        return Err(RunnerError::task_invocation(
            "reserved workspace managed control or detach is unavailable",
        ));
    }
    Ok(())
}

pub(super) fn build_execution_selection_plan(
    preflight: &ExecutionPreflight,
    selection: &effigy_manifest::TaskSelection<'_>,
) -> ExecutionSelectionPlan {
    ExecutionSelectionPlan::new(
        ExecutionSelectionInput::from_discovery(&preflight.discovery_plan),
        ExecutionSelectionCatalogSummary {
            alias: selection.catalog.alias.clone(),
            catalog_root: selection.catalog.catalog_root.clone(),
            manifest_path: selection.catalog.manifest_path.clone(),
            depth: selection.catalog.depth,
        },
        selection.mode,
        selection.evidence.clone(),
        preflight.selector.task_name.clone(),
    )
}

#[cfg(test)]
mod workspace_resolution_tests {
    use super::*;
    use effigy_kernel::workspace_channel::{
        InvocationBody, OutputMode, Resolver, Selector, Target,
    };

    #[test]
    fn reserved_workspace_rechecks_composed_task_before_fallback() {
        let fixture = tempfile::tempdir().unwrap();
        let checkout = fixture.path().join("checkout");
        std::fs::create_dir(&checkout).unwrap();
        let manifest = checkout.join("effigy.toml");
        let source = |command: &str| {
            format!("[tasks.build]\nmode = 'tui'\nconcurrent = [{{ name = 'one', run = '{command}' }}]\n")
        };
        std::fs::write(&manifest, source("echo original")).unwrap();
        let cx = crate::runner::task_engine::effect_context(effigy_kernel::ctx::OpenReason::Shell);
        let root = effigy_kernel::root::StateRoot::effects(&cx, &checkout).unwrap();
        let identity = root.owning_directory().identity().unwrap();
        let mut dispatcher = crate::runner::workspace_client::host_dispatch::HostDispatch::new(
            checkout.clone(),
            root,
        );
        let request = InvocationBody {
            selector: Selector::Task("build".into()),
            cwd: ".".into(),
            target: Target::Host,
            args: vec![],
            output: OutputMode::Text,
        };
        let resolved = dispatcher.resolve(&cx, &request).unwrap();
        let expected = crate::runner::managed_run::WorkspaceResolution {
            invocation: serde_json::to_vec(&resolved).unwrap(),
            checkout: checkout.clone(),
            identity,
        };
        let task = TaskInvocation {
            name: "build".into(),
            args: vec![],
        };
        let first = super::super::api::build_execution_preflight(&task, checkout.clone()).unwrap();
        let route = super::super::route::resolve_route(&task, &first).unwrap();
        validate_workspace_resolution(&task, &first, &route, &expected)
            .expect("unchanged managed task remains admissible");
        assert!(
            validate_workspace_resolution(
                &task,
                &first,
                &super::super::route::Route::Builtin,
                &expected
            )
            .is_err(),
            "fallback must refuse before execution"
        );
        std::fs::write(&manifest, source("echo replacement")).unwrap();
        let second = super::super::api::build_execution_preflight(&task, checkout.clone()).unwrap();
        let route = super::super::route::resolve_route(&task, &second).unwrap();
        let error = validate_workspace_resolution(&task, &second, &route, &expected).unwrap_err();
        assert!(error.to_string().contains("resolution changed"));

        let mut control = request.clone();
        control.args = vec!["stop".into()];
        let control_resolution = dispatcher.resolve(&cx, &control).unwrap();
        let control_expected = crate::runner::managed_run::WorkspaceResolution {
            invocation: serde_json::to_vec(&control_resolution).unwrap(),
            checkout: checkout.clone(),
            identity,
        };
        let control_task = TaskInvocation {
            name: "build".into(),
            args: control.args,
        };
        let control_preflight =
            super::super::api::build_execution_preflight(&control_task, checkout.clone()).unwrap();
        let route = super::super::route::resolve_route(&control_task, &control_preflight).unwrap();
        assert!(validate_workspace_resolution(
            &control_task,
            &control_preflight,
            &route,
            &control_expected
        )
        .unwrap_err()
        .to_string()
        .contains("managed control or detach"));

        std::fs::write(&manifest, "[tasks]\nbuild = 'echo standard'\n").unwrap();
        let standard_resolution = dispatcher.resolve(&cx, &request).unwrap();
        let standard_expected = crate::runner::managed_run::WorkspaceResolution {
            invocation: serde_json::to_vec(&standard_resolution).unwrap(),
            checkout: checkout.clone(),
            identity,
        };
        let standard_preflight =
            super::super::api::build_execution_preflight(&task, checkout).unwrap();
        let route = super::super::route::resolve_route(&task, &standard_preflight).unwrap();
        assert!(validate_workspace_resolution(
            &task,
            &standard_preflight,
            &route,
            &standard_expected
        )
        .unwrap_err()
        .to_string()
        .contains("standard-task admission is unavailable"));
    }
}
