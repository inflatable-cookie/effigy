//! Supervisor-local container activation. Only command intent crosses the
//! process boundary; a render seal, reservation, backend handle or deadline
//! must never be serialized and reconstructed by the front end.

use std::time::{Duration, Instant};

use effigy_containers::environment::Retention;
use effigy_kernel::run::RunError;
use serde::{Deserialize, Serialize};

use super::serving_host::ServingHostRequest;

mod cleanup_integrations;
mod cli_preflight;
mod declaration;
#[cfg(feature = "workspace-serving-fixture")]
mod fake_io;
pub(in crate::runner) mod integrations;
mod plan;
mod production;
mod recovery;
mod supervisor_preflight;
pub(crate) use cli_preflight::{prevalidate_cli, PreparedIntent};
pub(crate) use declaration::PostPrepDeclaration;

#[cfg(feature = "workspace-serving-fixture")]
pub(crate) fn fixture_diagnostic(message: &'static str) {
    if fake_io::configured() {
        fake_io::diagnostic(message);
    }
}

#[cfg(feature = "workspace-serving-fixture")]
pub(in crate::runner) fn fixture_delay_final_revalidation(until: Instant) {
    fake_io::delay_final_revalidation(until);
}

const PREPARATION_UNAVAILABLE: &str =
    "bounded workspace Phase-A preparation is unavailable; no preparation or apply was attempted";

/// Public lifecycle routing must not fall through to the ordinary reconciler
/// when the declaration requires a reserved workspace owner. Classification
/// reads declarations only; it neither materializes policy nor refreshes sources.
pub(crate) fn require_supported_activation(
    repo_root: &std::path::Path,
    name: Option<&str>,
) -> Result<(), crate::runner::error::RunnerError> {
    use crate::runner::error::RunnerError;
    let repo_root = repo_root.to_path_buf();
    let name = name.map(str::to_owned);
    let declared = crate::runner::workspace_command::provisioning::bounded_publication_read(
        "workspace-up-routing",
        Instant::now() + Duration::from_secs(30),
        move || {
            let manifest = effigy_manifest::load_task_manifest(&repo_root.join("effigy.toml"))
                .map_err(|_| {
                    RunnerError::task_invocation("workspace activation declaration unavailable")
                })?;
            let selected = name.as_deref().or_else(|| {
                manifest
                    .containers
                    .as_ref()
                    .and_then(|config| config.default.as_deref())
            });
            effigy_containers::resolve_operator_workspace_for_container(&manifest, selected)
                .map(|workspace| workspace.is_some())
                .map_err(|_| {
                    RunnerError::task_invocation(
                        "workspace activation declaration is ambiguous or unavailable",
                    )
                })
        },
    )
    .map_err(|_| RunnerError::task_invocation("workspace activation declaration unavailable"))?;
    if declared {
        return Err(RunnerError::task_invocation(PREPARATION_UNAVAILABLE));
    }
    Ok(())
}

/// Ordinary CLI preparation finishes before launch. Only this declarative
/// intent crosses the process boundary; all authority is created by the host.
pub(crate) fn launch_prepared(
    policy: &effigy_containers::EffectiveContainerPolicy,
    prepared: PreparedIntent,
) -> Result<super::serving_host::ContainerStartupReport, crate::runner::error::RunnerError> {
    use super::serving_host::{self, Activation};
    use crate::runner::error::RunnerError;
    let cx = crate::runner::task_engine::effect_context(effigy_kernel::ctx::OpenReason::DevServer);
    let state_home = crate::runner::task_engine::state_home_path()
        .ok_or_else(|| RunnerError::task_invocation("workspace serving state home unavailable"))?;
    let location = effigy_kernel::location::CheckoutLocation {
        root: effigy_core::worktree_scope::checkout_root(&policy.repo_root),
    };
    let root = effigy_kernel::root::StateRoot::checkout_effects(&cx, &state_home, &location)
        .map_err(|_| RunnerError::task_invocation("workspace serving launch root unavailable"))?;
    let budget = effigy_containers::environment::RUN_BUDGET;
    let request = ServingHostRequest {
        schema: serving_host::SCHEMA.into(),
        state_home,
        project_path: policy.repo_root.clone(),
        checkout: *prepared.checkout.token(),
        environment: policy.name.clone(),
        profile: prepared.backend.context,
        activation: Activation::ContainerUp {
            request: ContainerUpRequest {
                environment: policy.name.clone(),
                retention: Retention::Persistent,
                budget_ms: budget.as_millis() as u64,
                post_prep: Some(prepared.declaration),
            },
        },
    };
    let executable = crate::runner::task_engine::startup_executable()
        .map_err(|_| RunnerError::task_invocation("workspace serving executable unavailable"))?;
    let mut host = serving_host::start(&cx, &root, &request, &executable)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let published = host
        .await_serving(&cx, budget)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let report = published.report.ok_or_else(|| {
        RunnerError::task_invocation(
            "workspace host did not provide its committed integration report",
        )
    })?;
    host.detach(&cx)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    Ok(report)
}

/// The front end's intent. No credential or purported observation is accepted.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ContainerUpRequest {
    pub(crate) environment: String,
    pub(crate) retention: Retention,
    pub(crate) budget_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) post_prep: Option<PostPrepDeclaration>,
}

impl ContainerUpRequest {
    fn deadline(&self, host: &ServingHostRequest) -> Result<Instant, RunError> {
        if self.environment.is_empty() || self.environment != host.environment {
            return Err(RunError::refused(
                "workspace activation environment differs from its host",
            ));
        }
        if self.retention != Retention::Persistent {
            return Err(RunError::refused(
                "held workspace activation has no detached holder handoff",
            ));
        }
        if !host.project_path.is_absolute() || !host.state_home.is_absolute() {
            return Err(RunError::refused(
                "workspace activation requires absolute checkout and state paths",
            ));
        }
        if self.budget_ms == 0 {
            return Err(RunError::refused("workspace activation budget is empty"));
        }
        Instant::now()
            .checked_add(Duration::from_millis(self.budget_ms))
            .ok_or_else(|| RunError::refused("workspace activation budget overflows its deadline"))
    }
}

/// Observe identity before any preparation or mount/secret effect. The worker
/// is read-only, so timeout cannot leave an abandoned mutator behind.
fn validate_identity(host: &ServingHostRequest, until: Instant) -> Result<(), RunError> {
    use crate::runner::workspace_command::provisioning::bounded_publication_read;
    let host = host.clone();
    bounded_publication_read("workspace-startup-identity", until, move || {
        let unavailable = || {
            crate::runner::error::RunnerError::task_invocation(
                "workspace checkout/backend identity observation unavailable",
            )
        };
        let token = effigy_core::worktree_scope::checkout_token(&host.project_path)
            .map_err(|_| unavailable())?
            .ok_or_else(unavailable)?;
        let expected: String = host
            .checkout
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        if token != expected {
            return Err(crate::runner::error::RunnerError::task_invocation(
                "workspace activation checkout differs from its host",
            ));
        }
        let kind = match effigy_core::worktree_scope::scope_kind(&host.project_path)
            .map_err(|_| unavailable())?
        {
            Some(effigy_core::worktree_scope::ScopeKind::LinkedWorktree) => {
                effigy_kernel::id::CheckoutKind::LinkedWorktree
            }
            Some(effigy_core::worktree_scope::ScopeKind::EphemeralClone) => {
                effigy_kernel::id::CheckoutKind::EphemeralClone
            }
            None => effigy_kernel::id::CheckoutKind::Primary,
        };
        let environment = effigy_containers::environment::EnvRef::new(
            effigy_kernel::id::CheckoutId::new(kind, host.checkout),
            &host.environment,
        )
        .map_err(|_| unavailable())?;
        let observe = crate::runner::task_engine::observe_context(
            until.saturating_duration_since(Instant::now()),
        );
        let location = effigy_kernel::location::CheckoutLocation {
            root: effigy_core::worktree_scope::checkout_root(&host.project_path),
        };
        let root_observed = effigy_kernel::root::StateRoot::checkout_observe(&observe, &location);
        let root = root_observed.verified().ok_or_else(unavailable)?;
        let record_observed =
            effigy_containers::environment::EnvRecord::read(&observe, root, &environment.resource);
        let record = record_observed.verified().ok_or_else(unavailable)?;
        let selected = record.recorded_backend().ok_or_else(unavailable)?;
        if selected.context != host.profile {
            return Err(crate::runner::error::RunnerError::task_invocation(
                "workspace activation backend profile differs from its host",
            ));
        }
        Ok(())
    })
    .map_err(|error| RunError::unavailable(error.to_string()))
}

/// Read-only validation is completed before effects/run admission. Its one
/// deadline and directory descriptor are moved into the actual activation.
pub(crate) struct PreparedActivation {
    validation: supervisor_preflight::PrevalidatedContainer,
}

pub(crate) fn activate(
    host: &ServingHostRequest,
    request: ContainerUpRequest,
    prepared: PreparedActivation,
    session: &mut effigy_kernel::supervise::EngineSession,
    run: &mut effigy_kernel::run::Run<'_>,
) -> Result<production::ActivatedContainer, RunError> {
    production::activate_with(
        host,
        request,
        prepared.validation,
        session,
        run,
        |owner, backend, bound| {
            owner.reacquire_target(backend, |_view, until, _same_backend| {
                bound.observe_resume(until)
            })
        },
        recovery::cleanup,
    )
}

/// No live seal, policy values, owner or backend handle crosses a process.
/// Legacy intents without the required post-prep declaration still fail closed.
pub(crate) fn preflight(
    host: &ServingHostRequest,
    request: &ContainerUpRequest,
) -> Result<PreparedActivation, RunError> {
    let until = request.deadline(host)?;
    if request.post_prep.is_none() {
        validate_identity(host, until)?;
        return Err(RunError::unavailable(
            "workspace activation requires a post-prep declaration",
        ));
    }
    let validation = supervisor_preflight::validate(host, request, until)?;
    validation.recheck_directory()?;
    Ok(PreparedActivation { validation })
}

#[cfg(test)]
#[path = "container_up_tests.rs"]
mod tests;
