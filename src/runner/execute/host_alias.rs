//! Scoped resolution of the canonical declared backing-service alias in
//! Effigy-owned host execution.
//!
//! A linked worktree or marked ephemeral clone answers its own backing service
//! on the scope's effective alias (`postgres.<apex>-w<key>`), while the
//! consumer's configuration keeps the canonical declared name
//! (`postgres.<apex>`). A host process resolving the canonical name reaches
//! the gateway route that published it — normally the primary checkout's — so
//! a host run in a worktree would read another scope's database (effigy#264,
//! contract 005). Effigy therefore resolves the declared alias in the
//! environment it delivers to a host run, before the child starts, and refuses
//! when this checkout has no published backing service to resolve it to.
//!
//! The alias source is the same declared/effective host map `container hosts`
//! reports, the gateway registers and the Compose network aliases carry. Value
//! source precedence is untouched: a task's own `env` still beats an
//! env-schema value, which still beats a `.env` value or a schema default; the
//! winning value is resolved after that precedence is decided. Container-bound
//! runs are not touched: the declared alias already names their own backing
//! service on the scope's own Compose network.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use effigy_containers::{DeclaredAlias, HostExecutionAliases};
use effigy_env::secret::SecretString;
use effigy_gateway::endpoint_store::EndpointStore;
use effigy_gateway::routes::Route;
use effigy_manifest::TaskSelection;

use crate::runner::error::RunnerError;
use crate::runner::execute::api::{
    effective_runtime_inputs, resolve_execution_binding_resolution, ContainerExecutionBinding,
};
use crate::runner::execute::planning::ExecutionPreflight;

/// The alias resolution an Effigy-owned host run owes its environment, or
/// `None` for a checkout that keeps its declared names and for every run
/// Effigy does not place on the host.
pub(in crate::runner) fn host_alias_resolver(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
) -> Result<Option<HostAliasResolver>, RunnerError> {
    if super::workspace_seeded::inside_container_handoff() {
        return Ok(None);
    }
    let inputs =
        effective_runtime_inputs(&preflight.invocation_cwd, &preflight.catalogs, selection);
    if inputs.containers.is_none() {
        return Ok(None);
    }
    let binding = resolve_execution_binding_resolution(
        inputs.default_run_in,
        inputs.systems.as_ref(),
        inputs.containers.as_ref(),
        &preflight.selector.task_name,
        selection.task,
        "host alias resolution",
    )?;
    if !matches!(
        binding.binding(),
        ContainerExecutionBinding::None | ContainerExecutionBinding::Host
    ) {
        return Ok(None);
    }
    HostAliasResolver::for_checkout(&inputs.scope_root)
}

/// The declared/effective alias pairs of one scoped checkout, plus the
/// backing-service proof each resolved alias needs before a run starts.
#[derive(Debug)]
pub(in crate::runner) struct HostAliasResolver {
    repo_root: PathBuf,
    environment: String,
    aliases: HostExecutionAliases,
    /// Effective domains already proven published in this run.
    published: BTreeSet<String>,
}

impl HostAliasResolver {
    /// Resolve this checkout's scoped aliases, or `None` when it keeps its
    /// declared names, declares no container environment, or is already
    /// executing inside a container handoff (where the guest's own network
    /// carries both names).
    pub(in crate::runner) fn for_checkout(scope_root: &Path) -> Result<Option<Self>, RunnerError> {
        if super::workspace_seeded::inside_container_handoff() {
            return Ok(None);
        }
        let token = effigy_core::worktree_scope::load(scope_root)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        let Some(token) = token else {
            return Ok(None);
        };
        let policy =
            effigy_containers::load_container_policy(scope_root, None).map_err(|error| {
                RunnerError::task_invocation(format!(
                    "this checkout is a runtime scope, so a declared backing-service alias has to \
                 resolve inside it, but its container policy did not load: {error}"
                ))
            })?;
        let host_map =
            crate::runner::container_command::scoped_host_map(scope_root, &policy, Some(&token))?;
        let aliases = HostExecutionAliases::for_scoped_checkout(&host_map);
        if aliases.is_empty() {
            return Ok(None);
        }
        Ok(Some(Self {
            repo_root: policy.repo_root.clone(),
            environment: policy.name.clone(),
            aliases,
            published: BTreeSet::new(),
        }))
    }

    /// Resolve every plain value Effigy is about to deliver.
    pub(in crate::runner) fn resolve_plain_env(
        &mut self,
        env: &mut BTreeMap<String, String>,
    ) -> Result<(), RunnerError> {
        for (key, value) in env.iter_mut() {
            if let Some(resolved) = self.resolve_value(key, value)? {
                *value = resolved;
            }
        }
        Ok(())
    }

    /// Resolve every secret value Effigy is about to deliver. A secret keeps
    /// its secret handling: only the host part changes, and no value reaches a
    /// report.
    fn resolve_secret_env(
        &mut self,
        env: &mut [(String, SecretString)],
    ) -> Result<(), RunnerError> {
        for (key, value) in env.iter_mut() {
            let Some(resolved) = self.resolve_value(key, value.expose())? else {
                continue;
            };
            *value = SecretString::new(resolved);
        }
        Ok(())
    }

    /// An owned, resolved copy of the secret values a host run delivers. The
    /// caller keeps the originals: only this copy reaches the child.
    pub(in crate::runner) fn resolved_secret_env(
        &mut self,
        pairs: &[(&str, &SecretString)],
    ) -> Result<Vec<(String, SecretString)>, RunnerError> {
        let mut owned = pairs
            .iter()
            .map(|(key, value)| {
                (
                    (*key).to_owned(),
                    SecretString::new(value.expose().to_owned()),
                )
            })
            .collect::<Vec<_>>();
        self.resolve_secret_env(&mut owned)?;
        Ok(owned)
    }

    /// Resolve one value, or `None` when it names no declared alias. The key
    /// only ever appears in refusal text; a value never does.
    fn resolve_value(&mut self, key: &str, value: &str) -> Result<Option<String>, RunnerError> {
        let Some((resolved, used)) = self.aliases.resolve(value) else {
            return Ok(None);
        };
        for alias in used {
            self.require_published(key, &alias)?;
        }
        Ok(Some(resolved))
    }

    /// Refuse before the run starts when this checkout has no published
    /// backing service for the alias: the value would otherwise reach another
    /// checkout's database.
    fn require_published(&mut self, key: &str, alias: &DeclaredAlias) -> Result<(), RunnerError> {
        if !self.published.insert(alias.effective.clone()) {
            return Ok(());
        }
        let table_path = crate::runner::gateway_command::gateway_dir()?.join("routes.json");
        let table = EndpointStore::for_routes(&table_path)
            .snapshot()
            .and_then(|snapshot| snapshot.routes())
            .map_err(|error| {
                RunnerError::task_invocation(format!(
                    "task `{key}` uses `{}`, which this runtime-scoped checkout resolves to its \
                     own `{}` service on `{}`, but Effigy could not read the gateway route table \
                     at {}: {error}",
                    alias.declared,
                    alias.service,
                    alias.effective,
                    table_path.display()
                ))
            })?;
        let Some(route) = table.lookup(&alias.effective) else {
            return Err(self.unpublished(key, alias, "no route for it is recorded"));
        };
        if !self.owns(route) {
            return Err(self.unpublished(
                key,
                alias,
                &format!("the route belongs to `{}`", route.project),
            ));
        }
        if route.tcp_target.is_none() {
            return Err(self.unpublished(key, alias, "its route names no published upstream port"));
        }
        Ok(())
    }

    /// A route is this checkout's when it records this checkout's path, the
    /// same identity `container down` withdraws by.
    fn owns(&self, route: &Route) -> bool {
        let recorded = Path::new(route.project.as_str());
        if recorded == self.repo_root.as_path() {
            return true;
        }
        recorded
            .canonicalize()
            .is_ok_and(|resolved| resolved == self.repo_root)
    }

    fn unpublished(&self, key: &str, alias: &DeclaredAlias, reason: &str) -> RunnerError {
        RunnerError::task_invocation(format!(
            "task `{key}` uses `{}`, which this runtime-scoped checkout resolves to its own `{}` \
             service on `{}`, but {reason}. Running it would reach another checkout's `{}`; start \
             this checkout's stack first with `effigy container up {}` in {}",
            alias.declared,
            alias.service,
            alias.effective,
            alias.service,
            self.environment,
            self.repo_root.display()
        ))
    }
}

/// Borrow resolved secret pairs for a spawn.
pub(in crate::runner) fn secret_env_slice(
    pairs: &[(String, SecretString)],
) -> Vec<(&str, &SecretString)> {
    pairs
        .iter()
        .map(|(key, value)| (key.as_str(), value))
        .collect()
}
