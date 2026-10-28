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

use effigy_containers::{DeclaredAlias, EffectiveContainerPolicy, HostExecutionAliases};
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
/// `None` for a checkout that keeps its declared names, for a run Effigy does
/// not place on the host, and for a checkout whose container registry Effigy
/// cannot read (which leaves host execution exactly as it was).
pub(in crate::runner) fn host_alias_resolver(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
) -> Result<Option<HostAliasResolver>, RunnerError> {
    match host_execution_scope_root(preflight, selection)? {
        Some(scope_root) => HostAliasResolver::for_checkout(&scope_root),
        None => Ok(None),
    }
}

/// The declared/effective aliases a host-bound run's environment resolves
/// without proving anything is published: what a `--plan` render shows.
pub(in crate::runner) fn host_plan_env_aliases(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
) -> Result<Vec<effigy_core::host_alias::HostAlias>, RunnerError> {
    match host_execution_scope_root(preflight, selection)? {
        Some(scope_root) => host_env_aliases(&scope_root),
        None => Ok(Vec::new()),
    }
}

/// The checkout whose environment an Effigy-owned host run delivers, or `None`
/// for a run Effigy places in a container, a checkout with no container
/// registry, and a process already inside a container handoff.
fn host_execution_scope_root(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
) -> Result<Option<PathBuf>, RunnerError> {
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
    Ok(Some(inputs.scope_root))
}

/// The declared/effective alias pairs of one scoped checkout, plus the
/// backing-service proof each resolved alias needs before a run starts.
#[derive(Debug)]
pub(in crate::runner) struct HostAliasResolver {
    repo_root: PathBuf,
    aliases: HostExecutionAliases,
    /// Effective alias domain to the environment whose stack answers it.
    environments: BTreeMap<String, String>,
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
        let Some(token) = effigy_core::worktree_scope::load(scope_root)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?
        else {
            return Ok(None);
        };
        let mut aliases: Vec<DeclaredAlias> = Vec::new();
        let mut environments = BTreeMap::new();
        let mut repo_root = None;
        for policy in scoped_environment_policies(scope_root) {
            let host_map = crate::runner::container_command::scoped_host_map(
                scope_root,
                &policy,
                Some(&token),
            )?;
            repo_root.get_or_insert_with(|| policy.repo_root.clone());
            for alias in HostExecutionAliases::for_scoped_checkout(&host_map).aliases() {
                environments
                    .entry(alias.host.effective.clone())
                    .or_insert_with(|| policy.name.clone());
                if !aliases.contains(alias) {
                    aliases.push(alias.clone());
                }
            }
        }
        if aliases.is_empty() {
            return Ok(None);
        }
        Ok(Some(Self {
            repo_root: repo_root.unwrap_or_else(|| scope_root.to_path_buf()),
            aliases: HostExecutionAliases::from_declared(aliases),
            environments,
            published: BTreeSet::new(),
        }))
    }

    /// The declared/effective pairs every host-side renderer applies.
    pub(in crate::runner) fn hosts(&self) -> Vec<effigy_core::host_alias::HostAlias> {
        self.aliases.hosts()
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

    /// Prove every alias a rendered session command names before the session
    /// starts: a managed plan bakes its environment into the command string, so
    /// the check reads the rendered text.
    pub(in crate::runner) fn require_published_in(
        &mut self,
        task: &str,
        text: &str,
    ) -> Result<(), RunnerError> {
        let used = self
            .aliases
            .aliases()
            .iter()
            .filter(|alias| text.contains(alias.host.effective.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        for alias in used {
            self.require_published(task, &alias)?;
        }
        Ok(())
    }

    /// Resolve one value, or `None` when it names no declared alias endpoint.
    /// The key only ever appears in refusal text; a value never does.
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
        let effective = alias.host.effective.as_str();
        if !self.published.insert(effective.to_owned()) {
            return Ok(());
        }
        let table_path = crate::runner::gateway_command::gateway_dir()?.join("routes.json");
        let table = EndpointStore::for_routes(&table_path)
            .snapshot()
            .and_then(|snapshot| snapshot.routes())
            .map_err(|error| {
                RunnerError::task_invocation(format!(
                    "task `{key}` uses `{}`, which this runtime-scoped checkout resolves to its \
                     own `{}` service on `{effective}`, but Effigy could not read the gateway \
                     route table at {}: {error}",
                    alias.host.declared,
                    alias.service,
                    table_path.display()
                ))
            })?;
        let Some(route) = table.lookup(effective) else {
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
        let environment = self
            .environments
            .get(&alias.host.effective)
            .map(String::as_str)
            .unwrap_or_default();
        let start = match environment.is_empty() {
            true => "start this checkout's stack first".to_owned(),
            false => format!(
                "start this checkout's stack first with `effigy container up {environment}`"
            ),
        };
        RunnerError::task_invocation(format!(
            "task `{key}` uses `{}`, which this runtime-scoped checkout resolves to its own `{}` \
             service on `{}`, but {reason}. Running it would reach another checkout's `{}`; {start} \
             in {}",
            alias.host.declared,
            alias.service,
            alias.host.effective,
            alias.service,
            self.repo_root.display()
        ))
    }
}

/// Resolve a host suite's delivered environment for one checkout.
pub(in crate::runner) fn resolve_host_env(
    root: &Path,
    env: &mut BTreeMap<String, String>,
) -> Result<(), RunnerError> {
    let Some(mut resolver) = HostAliasResolver::for_checkout(root)? else {
        return Ok(());
    };
    resolver.resolve_plain_env(env)
}

/// The declared/effective aliases one checkout's host environments resolve,
/// for renders that happen before an environment exists.
pub(in crate::runner) fn host_env_aliases(
    root: &Path,
) -> Result<Vec<effigy_core::host_alias::HostAlias>, RunnerError> {
    Ok(HostAliasResolver::for_checkout(root)?
        .map(|resolver| resolver.hosts())
        .unwrap_or_default())
}

/// Every container environment of this scoped checkout, so a checkout that
/// names no default environment still resolves its aliases instead of failing
/// an ordinary host run (effigy#431 review).
fn scoped_environment_policies(scope_root: &Path) -> Vec<EffectiveContainerPolicy> {
    match effigy_containers::load_container_policy(scope_root, None) {
        Ok(policy) => vec![policy],
        // An unreadable registry leaves host execution as it was: this is not
        // a backing-service refusal, and an ordinary host task must not fail
        // for it.
        Err(_) => effigy_containers::load_all_container_policies(scope_root).unwrap_or_default(),
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
