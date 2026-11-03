//! Test-only private preparation. The child executes existing preparation;
//! its output belongs to qualified kernel custody before it can do effects.
use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, Instant};

use effigy_containers::environment::{BackendSelection, EnvRef, CLEANUP_BUDGET, RUN_BUDGET};
use effigy_containers::EffectiveContainerPolicy;
use effigy_kernel::ctx::{EffectClass, EffectContext, OpenReason, Placement};
use effigy_kernel::custody::{Custody, EnvValue, Lifetime, ProcessSpec, Terminal};
use effigy_kernel::fs::{DirFd, DirectoryIdentity};
use effigy_kernel::gate::Wait;
use effigy_kernel::id::{
    CheckoutId, ExecutionTarget, InvocationSpec, ResolvedInvocation, Route, Surface, TaskId,
};
use effigy_kernel::observe::Observed;
use effigy_kernel::run::{BeginOpts, GuardedWorkspaceCleanup, Run};
use effigy_kernel::supervise::{Executable, LifecyclePlan};
use effigy_kernel::workspace_session::WorkspaceActivationReservation;
use serde::{Deserialize, Serialize};

use crate::runner::error::RunnerError;
use crate::runner::secret_session;
use crate::runner::task_engine::{self, TaskSettlement};
use crate::runner::workspace_client::container_up::cli_preflight::{read_backend, read_checkout};

const ENTRY: &str = "__kernel-workspace-private-prepare";

/// Intent only: the child reobserves these facts and authenticates its actual
/// process receipt. This carries no lease, policy, secret or output bytes.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Intent {
    directory: DirectoryIdentity,
    checkout: CheckoutId,
    environment: String,
    backend_kind: effigy_containers::environment::BackendKind,
    engine: String,
    profile: String,
    render: Option<String>,
}

fn unavailable() -> RunnerError {
    RunnerError::task_invocation(
        "workspace private preparation unavailable; no workspace apply was attempted",
    )
}

fn unavailable_at(stage: &'static str) -> RunnerError {
    RunnerError::task_invocation(format!("workspace private preparation unavailable: stage={stage}; no workspace apply was attempted"))
}

fn recheck(root: &Path, intent: &Intent) -> Result<EffectiveContainerPolicy, RunnerError> {
    if DirFd::open(root)
        .and_then(|dir| dir.identity())
        .map_err(|_| unavailable_at("directory"))?
        != intent.directory
        || read_checkout(root)? != Some(intent.checkout)
    {
        return Err(unavailable_at("identity"));
    }
    let policy = effigy_containers::load_container_policy(root, Some(&intent.environment))
        .map_err(|_| unavailable_at("policy-read"))?;
    let backend = read_backend(&policy, Some(intent.checkout))?;
    if policy.name != intent.environment
        || backend.kind != intent.backend_kind
        || backend.engine.as_str() != intent.engine
        || backend.context != intent.profile
    {
        return Err(unavailable_at("backend-changed"));
    }
    // Shared-service preparation still has its own durable output path. Do
    // not hide that path behind the parent's null streams or skip its ensure.
    if !policy.shared_services.is_empty() {
        return Err(unavailable_at("shared-services"));
    }
    sources_ready(root)?;
    if intent.render.as_ref().is_some_and(|expected| {
        policy
            .render
            .as_ref()
            .is_none_or(|render| render.closure.digest() != *expected)
    }) {
        return Err(unavailable_at("render-changed"));
    }
    Ok(policy)
}

/// Qualification consumes an already prepared source closure. Missing/stale
/// acquisition remains an explicit source-prepare operation, never a fallback.
fn sources_ready(root: &Path) -> Result<(), RunnerError> {
    let manifest = root.join("effigy.toml");
    if effigy_manifest::inspect_bundle_source(&manifest)
        .map_err(|_| unavailable_at("source-bundle"))?
        .is_some_and(|report| report.stale || !report.local_path.is_dir())
    {
        return Err(unavailable_at("source-not-prepared"));
    }
    let loaded = effigy_manifest::load_task_manifest(&manifest)
        .map_err(|_| unavailable_at("source-manifest"))?;
    if let Some(catalog) = &loaded.catalog {
        for (name, source) in &catalog.sources {
            let report = effigy_manifest::resolve_catalog_source(&manifest, name, &source.base)
                .map_err(|_| unavailable_at("source-catalog"))?;
            if report.stale || !report.local_path.is_dir() {
                return Err(unavailable_at("source-not-prepared"));
            }
        }
    }
    if effigy_manifest::inspect_pinned_sources(&manifest)
        .map_err(|_| unavailable_at("source-pins"))?
        .iter()
        .any(|source| source.drift_message().is_some())
    {
        return Err(unavailable_at("source-not-prepared"));
    }
    Ok(())
}

pub(in crate::runner) fn prepare(
    root: &Path,
    directory: DirectoryIdentity,
    checkout: CheckoutId,
    environment: &str,
    backend: &BackendSelection,
) -> Result<EffectiveContainerPolicy, RunnerError> {
    let mut intent = Intent {
        directory,
        checkout,
        environment: environment.into(),
        backend_kind: backend.kind,
        engine: backend.engine.as_str().into(),
        profile: backend.context.clone(),
        render: None,
    };
    recheck(root, &intent)?;
    task_engine::install_client_router()?;
    let _waiting = task_engine::CustodyWait::enter();
    let context = crate::runner::environment::context(root)?;
    let cx = task_engine::effect_context(OpenReason::DevServer).child(RUN_BUDGET);
    let env = EnvRef::new(checkout, environment).map_err(|_| unavailable())?;
    let task = TaskId::new(
        checkout,
        "",
        Surface::Published,
        "workspace-private-preparation",
        None,
    )
    .map_err(|_| unavailable())?;
    let invocation = ResolvedInvocation::new(InvocationSpec {
        owner: env.resource,
        task,
        route: Route::Task,
        argv: vec![],
        cwd: String::new(),
        target: ExecutionTarget::Host,
        env: BTreeMap::new(),
        gates: vec![],
        attachments: vec![],
        class: EffectClass::Mutate,
        placement: Placement::Host,
        provenance: BTreeMap::new(),
    })
    .map_err(|_| unavailable_at("invocation"))?;
    let mut adapter = TaskSettlement::default();
    let options = BeginOpts {
        id: task_engine::fresh_run_id().map_err(|_| unavailable())?,
        wait: Wait::FailFast,
        acknowledge: vec![],
        next: crate::runner::workspace_client::serving_host::recovery_invocation()
            .map_err(|_| unavailable())?,
        custody: Some(LifecyclePlan {
            executable: Executable::current().map_err(|_| unavailable())?,
            placement: effigy_kernel::supervise::Placement::Detached,
        }),
    };
    let reservation = WorkspaceActivationReservation::reserve(
        &cx,
        context.root,
        &invocation.spec().owner,
        Wait::FailFast,
    )
    .map_err(|_| unavailable_at("reservation"))?;
    let claim = match reservation.claim() {
        Ok(claim) => claim,
        Err(_) => {
            cancel_reservation(&reservation, context.root, &cx)?;
            return Err(unavailable());
        }
    };
    let admitted = Run::begin_claimed_recoverable(
        &cx,
        context.root,
        &invocation,
        options,
        || {
            cx.operation().cleanup(CLEANUP_BUDGET).map_err(|_| {
                effigy_kernel::run::RunError::unavailable("preparation cleanup budget unavailable")
            })
        },
        &mut adapter,
        claim,
    );
    let mut run = match admitted {
        Ok(run) => run,
        Err((_, custody)) => {
            if let Some(custody) = custody {
                settle_guarded(custody, &cx, &mut adapter);
            }
            cancel_reservation(&reservation, context.root, &cx)?;
            return Err(unavailable_at("admission"));
        }
    };
    let result = (|| {
        prepare_owned(root, &intent, &cx, &mut run)?;
        let policy = recheck(root, &intent)?;
        effigy_containers::policy::render::prepare_under_qualification_owner(&mut run, &policy)
            .map_err(|_| unavailable_at("render-publication"))?;
        intent.render = Some(
            policy
                .render
                .as_ref()
                .ok_or_else(unavailable)?
                .closure
                .digest()
                .to_owned(),
        );
        prepare_owned(root, &intent, &cx, &mut run)
    })();
    settle(Box::new(run), &cx, &mut adapter);
    cancel_reservation(&reservation, context.root, &cx)?;
    result?;
    recheck(root, &intent)
}

fn prepare_owned(
    root: &Path,
    intent: &Intent,
    cx: &EffectContext,
    run: &mut Run<'_>,
) -> Result<(), RunnerError> {
    recheck(root, intent)?;
    let executable = std::env::current_exe().map_err(|_| unavailable())?;
    let mut env = secret_session::workload_ambient_environment()
        .filter(|(key, _)| key != "EFFIGY_CUSTODY_NONCE" && key != "EFFIGY_PROBE_NONCE")
        .map(|(key, value)| (key, EnvValue::Plain(value)))
        .collect::<BTreeMap<_, _>>();
    if super::secret_env::secrets_required() {
        env.insert(
            "EFFIGY_SECRETS_REQUIRED".into(),
            EnvValue::Plain("1".into()),
        );
    }
    let access = secret_session::session_passphrase().is_some()
        || secret_session::local_dev_secret_access_active();
    let grant = secret_session::take_child_grant();
    if access && grant.is_none() {
        return Err(unavailable());
    }
    if let Some(grant) = &grant {
        let (key, value) = grant.env_entry();
        env.insert(key.into(), EnvValue::Plain(value.into()));
    }
    let encoded = serde_json::to_string(intent).map_err(|_| unavailable())?;
    let intent_digest = effigy_kernel::id::Digest::of(encoded.as_bytes());
    let mut process = Custody::spawn_workspace_discard_qualification(
        run,
        ProcessSpec {
            argv: vec![executable.into_os_string(), ENTRY.into(), encoded.into()],
            cwd: root.into(),
            env,
            grace: Duration::from_secs(2),
            lifetime: Lifetime::Finite(cx.operation().remaining().ok_or_else(unavailable)?),
            terminal: Terminal::Discard,
            label: "private workspace preparation".into(),
        },
        cx,
        None,
        grant.map(|grant| grant.into_reader()),
        Some(intent_digest),
    )
    .map_err(|_| unavailable_at("qualified-spawn"))?;
    let exit = match process.wait(cx.operation()) {
        Observed::Verified(exit, _) => exit,
        _ => return Err(unavailable()),
    };
    if !matches!(
        process.wait_qualification_group_closure(&cx.operation().child(Duration::from_secs(2))),
        Observed::Verified(..) | Observed::Absent(_)
    ) {
        return Err(unavailable());
    }
    process.complete(run, exit).map_err(|_| unavailable())?;
    if exit.code != 0 {
        let stage = match exit.code {
            64 => "child-intent",
            65 => "child-state",
            66 => "child-custody",
            67 => "child-identity",
            68 => "child-preparation",
            _ => "child",
        };
        return Err(unavailable_at(stage));
    }
    Ok(())
}

fn cancel_reservation(
    reservation: &WorkspaceActivationReservation,
    root: &effigy_kernel::root::StateRoot,
    cx: &EffectContext,
) -> Result<(), RunnerError> {
    let cleanup = cx
        .operation()
        .cleanup(CLEANUP_BUDGET)
        .map_err(|_| unavailable())?;
    let effects = cleanup.operation().effects().map_err(|_| unavailable())?;
    reservation
        .cancel(&effects, root, Wait::FailFast)
        .map(|_| ())
        .map_err(|_| unavailable())
}

/// Never abandon an admitted owner on Unknown. Cleanup repeats only for this
/// same owner and its existing processes, with the normal settlement budget.
fn settle(mut run: Box<Run<'_>>, cx: &EffectContext, adapter: &mut TaskSettlement) {
    loop {
        let cleanup = cx
            .operation()
            .cleanup(CLEANUP_BUDGET)
            .expect("fixed finite cleanup budget");
        match run.settle_workspace_recoverable(&cleanup, adapter) {
            Ok(_) => return,
            Err((_, owner)) => {
                run = owner;
                std::thread::sleep(Duration::from_millis(250));
            }
        }
    }
}

fn settle_guarded(
    mut owner: Box<GuardedWorkspaceCleanup<'_>>,
    cx: &EffectContext,
    adapter: &mut TaskSettlement,
) {
    loop {
        let cleanup = cx
            .operation()
            .cleanup(CLEANUP_BUDGET)
            .expect("fixed finite cleanup budget");
        match owner.settle(&cleanup, adapter) {
            Ok(_) => return,
            Err((_, retained)) => {
                owner = retained;
                std::thread::sleep(Duration::from_millis(250));
            }
        }
    }
}

/// Fixed exit classes reveal no child diagnostics or secret bytes.
pub(in crate::runner) fn entry() -> i32 {
    let result = (|| -> Result<(), i32> {
        let encoded = std::env::args().nth(2).ok_or(64)?;
        // Refusal-only fault in the existing external-I/O fixture build. A
        // semantically identical but byte-different intent must not reuse the
        // parent's receipt. Native qualification builds omit this hook.
        #[cfg(feature = "workspace-serving-fixture")]
        let encoded = if std::env::var_os("EFFIGY_WORKSPACE_ACTIVATION_FAKE").is_some()
            && std::env::var_os("EFFIGY_TEST_PREPARATION_INTENT_FAULT").is_some()
        {
            format!("{encoded} ")
        } else {
            encoded
        };
        if encoded.len() > 16 * 1024 {
            return Err(64);
        }
        let intent: Intent = serde_json::from_str(&encoded).map_err(|_| 64)?;
        let root = std::env::current_dir().map_err(|_| 67)?;
        let observe = task_engine::reading_context(effigy_kernel::ctx::Deadline::At(
            Instant::now() + Duration::from_secs(30),
        ))
        .observe()
        .map_err(|_| 65)?;
        let location = effigy_kernel::location::CheckoutLocation {
            root: effigy_core::worktree_scope::checkout_root(&root),
        };
        let observed_state = effigy_kernel::root::StateRoot::checkout_observe(&observe, &location);
        let state = observed_state.verified().ok_or(65)?;
        let owner = EnvRef::new(intent.checkout, &intent.environment).map_err(|_| 64)?;
        effigy_kernel::custody::verify_workspace_qualification_child(
            state,
            &owner.resource,
            &observe,
            &effigy_kernel::id::Digest::of(encoded.as_bytes()),
        )
        .map_err(|_| 66)?;
        let directory = DirFd::open(&root).map_err(|_| 67)?;
        recheck(&root, &intent).map_err(|_| 67)?;
        if intent.render.is_some() {
            super::lifecycle::qualification_prepare_body(&root, &intent.environment)
                .map_err(|_| 68)?;
        } else {
            effigy_containers::load_container_policy_for_qualification_inputs(
                &root,
                Some(&intent.environment),
                &mut |_| {},
            )
            .map_err(|_| 68)?;
        }
        if directory.identity().map_err(|_| 67)? != intent.directory {
            return Err(67);
        }
        recheck(&root, &intent).map_err(|_| 67)?;
        Ok(())
    })();
    result.err().unwrap_or(0)
}
