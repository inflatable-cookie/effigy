//! Read-only Phase 0 and comparison of ordinary Phase-1 preparation output.
use std::path::Path;
use std::time::{Duration, Instant};

use effigy_containers::environment::{BackendSelection, EnvRecord, EnvRef};
use effigy_containers::EffectiveContainerPolicy;
use effigy_kernel::fs::{DirFd, DirectoryIdentity};
use effigy_kernel::id::{CheckoutId, CheckoutKind};
use effigy_kernel::observe::Observed;

use super::declaration::{BackendPin, PostPrepDeclaration};
use crate::runner::error::RunnerError;
use crate::runner::workspace_command::provisioning::{
    bounded_publication_read, workspace_path_map, workspace_source_closure,
};

fn unavailable() -> RunnerError {
    RunnerError::task_invocation("workspace checkout/backend prevalidation unavailable")
}

/// The descriptor stays open across ordinary prep. This is a read handle,
/// never a reserved run, seal or transferable activation owner.
pub(crate) struct CliPrevalidation {
    directory: DirFd,
    identity: DirectoryIdentity,
    checkout: Option<CheckoutId>,
    backend: BackendSelection,
    environment: String,
}

impl std::fmt::Debug for CliPrevalidation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CliPrevalidation")
            .field("directory", &self.identity)
            .field("environment", &self.environment)
            .finish_non_exhaustive()
    }
}

pub(crate) struct PreparedIntent {
    pub(crate) declaration: PostPrepDeclaration,
    pub(crate) checkout: CheckoutId,
    pub(crate) backend: BackendSelection,
}

impl std::fmt::Debug for PreparedIntent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedIntent")
            .field("checkout", &self.checkout)
            .finish_non_exhaustive()
    }
}

pub(crate) fn prevalidate_cli(
    repo_root: &Path,
    name: Option<&str>,
) -> Result<Option<CliPrevalidation>, RunnerError> {
    let repo_root = repo_root.to_owned();
    let name = name.map(str::to_owned);
    bounded_publication_read(
        "workspace-cli-prevalidation",
        Instant::now() + Duration::from_secs(30),
        move || {
            // Read and validate an existing identity before loading any policy.
            let checkout = read_checkout(&repo_root)?;
            let manifest = effigy_manifest::load_task_manifest(&repo_root.join("effigy.toml"))
                .map_err(|_| unavailable())?;
            let selected = name.as_deref().or_else(|| {
                manifest
                    .containers
                    .as_ref()
                    .and_then(|config| config.default.as_deref())
            });
            if effigy_containers::resolve_operator_workspace_for_container(&manifest, selected)
                .map_err(|_| unavailable())?
                .is_none()
            {
                return Ok(None);
            }
            let directory = DirFd::open(&repo_root).map_err(|_| unavailable())?;
            let identity = directory.identity().map_err(|_| unavailable())?;
            // The ordinary loader builds an in-memory render; only the explicit
            // creation/prepare APIs publish files or prepare mount sources.
            let policy = effigy_containers::load_container_policy(&repo_root, selected)
                .map_err(|_| unavailable())?;
            let backend = read_backend(&policy, checkout)?;
            BackendPin::observed(&backend).map_err(|_| unavailable())?;
            Ok(Some(CliPrevalidation {
                directory,
                identity,
                checkout,
                backend,
                environment: policy.name,
            }))
        },
    )
    .map_err(|_| unavailable())
}

pub(super) fn read_checkout(repo_root: &Path) -> Result<Option<CheckoutId>, RunnerError> {
    let Some(token) =
        effigy_core::worktree_scope::checkout_token(repo_root).map_err(|_| unavailable())?
    else {
        return Ok(None);
    };
    let mut bytes = [0u8; 16];
    let pairs = token.as_bytes().chunks_exact(2);
    if pairs.len() != bytes.len() {
        return Err(unavailable());
    }
    for (byte, pair) in bytes.iter_mut().zip(pairs) {
        let value = std::str::from_utf8(pair).map_err(|_| unavailable())?;
        *byte = u8::from_str_radix(value, 16).map_err(|_| unavailable())?;
    }
    let kind =
        match effigy_core::worktree_scope::scope_kind(repo_root).map_err(|_| unavailable())? {
            Some(effigy_core::worktree_scope::ScopeKind::LinkedWorktree) => {
                CheckoutKind::LinkedWorktree
            }
            Some(effigy_core::worktree_scope::ScopeKind::EphemeralClone) => {
                CheckoutKind::EphemeralClone
            }
            None => CheckoutKind::Primary,
        };
    Ok(Some(CheckoutId::new(kind, bytes)))
}

pub(super) fn read_backend(
    policy: &EffectiveContainerPolicy,
    checkout: Option<CheckoutId>,
) -> Result<BackendSelection, RunnerError> {
    let exact =
        effigy_containers::backend::exact_backend(&policy.repo_root, &policy.name, policy.driver);
    exact.require(&policy.name).map_err(|_| unavailable())?;
    let observe = crate::runner::task_engine::observe_context(Duration::from_secs(10));
    let location = effigy_kernel::location::CheckoutLocation {
        root: effigy_core::worktree_scope::checkout_root(&policy.repo_root),
    };
    let recorded = if let Some(checkout) = checkout {
        let env = EnvRef::new(checkout, &policy.name).map_err(|_| unavailable())?;
        match effigy_kernel::root::StateRoot::checkout_observe(&observe, &location) {
            Observed::Verified(root, _) => match EnvRecord::read(&observe, &root, &env.resource) {
                Observed::Verified(record, _) => match record.backend {
                    Some(Observed::Verified(backend, _)) => Some(backend),
                    Some(_) => return Err(unavailable()),
                    None => None,
                },
                Observed::Absent(_) => None,
                Observed::Unknown(_) => return Err(unavailable()),
            },
            Observed::Absent(_) => None,
            Observed::Unknown(_) => return Err(unavailable()),
        }
    } else {
        None
    };
    Ok(effigy_containers::environment::effective_backend(
        &policy.repo_root,
        policy,
        recorded.as_ref(),
    ))
}

impl CliPrevalidation {
    pub(crate) fn verify_prepared(
        &self,
        policy: &EffectiveContainerPolicy,
    ) -> Result<PreparedIntent, RunnerError> {
        let current = DirFd::open(&policy.repo_root).map_err(|_| unavailable())?;
        if self.identity != self.directory.identity().map_err(|_| unavailable())?
            || self.identity != current.identity().map_err(|_| unavailable())?
            || self.environment != policy.name
        {
            return Err(RunnerError::task_invocation(
                "workspace checkout changed during preparation",
            ));
        }
        let checkout = read_checkout(&policy.repo_root)?.ok_or_else(unavailable)?;
        if self.checkout.is_some_and(|original| original != checkout) {
            return Err(RunnerError::task_invocation(
                "workspace checkout token changed during preparation",
            ));
        }
        let backend = read_backend(policy, Some(checkout))?;
        if self.backend.kind != backend.kind
            || self.backend.engine != backend.engine
            || self.backend.context != backend.context
        {
            return Err(RunnerError::task_invocation(
                "workspace backend/profile changed during preparation",
            ));
        }
        let workspace = crate::runner::workspace_command::operator_workspace_for_policy(
            &policy.repo_root,
            policy,
        )?
        .ok_or_else(|| {
            RunnerError::task_invocation("workspace declaration changed during preparation")
        })?;
        let paths = workspace_path_map(&policy.repo_root, &workspace)?;
        let sources = workspace_source_closure(&policy.repo_root, &paths)?;
        let declaration =
            PostPrepDeclaration::capture(policy, &backend, self.identity, &sources.pins).map_err(
                |_| RunnerError::task_invocation("workspace post-prep declaration unavailable"),
            )?;
        Ok(PreparedIntent {
            declaration,
            checkout,
            backend,
        })
    }
}

#[cfg(test)]
#[path = "cli_preflight_tests.rs"]
mod tests;
