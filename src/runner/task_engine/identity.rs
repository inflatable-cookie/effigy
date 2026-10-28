//! One resolved task identity (K1). Lock, status, logs and the recovery
//! command all derive from the resolver's selection, never from the selector
//! as typed: `build`, `api/build` and `./apps/api/build` are one task, and
//! `a:b` and `a-b` stay two.

use std::path::{Path, PathBuf};

use effigy_core::task_selection::TaskSurface;
use effigy_kernel::id::{CheckoutId, Digest, ResourceId, Scope, Surface, TaskId};
use effigy_manifest::{LoadedCatalog, TaskSelection};

use crate::runner::error::RunnerError;

/// The one task that opts into an argument-scoped lock (Q-015, contract 017
/// §Exclusion): two filters may run together, identical normalized
/// invocations exclude one another.
pub(in crate::runner) const ARGUMENT_SCOPED_TASK: &str = "test:focused";
/// How long an identical normalized invocation waits for its live peer when
/// `--lock-wait-ms` and `EFFIGY_LOCK_WAIT_MS` name no wait at all.
pub(in crate::runner) const ARGUMENT_SCOPED_LOCK_WAIT_MS: u64 = 120_000;

/// The coordinates of one declared task under one Effigy root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct TaskTarget {
    pub(in crate::runner) root: PathBuf,
    /// Root-relative catalog path; "" for the root catalog.
    pub(in crate::runner) catalog: String,
    pub(in crate::runner) task: String,
    pub(in crate::runner) surface: TaskSurface,
    /// A managed profile, recorded on the run; exclusion stays per task.
    pub(in crate::runner) profile: Option<String>,
    /// `lock = "<name>"`: a checkout-wide exclusion shared across tasks.
    pub(in crate::runner) shared_lock: Option<String>,
    /// The normalized arguments that key an argument-scoped exclusion
    /// (Q-015); `None` keeps the declared task's own gate.
    pub(in crate::runner) argument_scope: Option<String>,
    /// A runnable spelling: bare for the root catalog, `alias/task` below it.
    pub(in crate::runner) label: String,
}

impl TaskTarget {
    pub(in crate::runner) fn for_selection(
        root: &Path,
        task_name: &str,
        selection: &TaskSelection<'_>,
    ) -> Self {
        let mut target = Self::for_catalog(root, task_name, selection.catalog);
        target.surface = selection.surface;
        target.shared_lock = selection
            .task
            .lock
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned);
        target
    }

    pub(in crate::runner) fn for_catalog(
        root: &Path,
        task_name: &str,
        catalog: &LoadedCatalog,
    ) -> Self {
        let label = if catalog.depth == 0 || catalog.alias.is_empty() {
            task_name.to_owned()
        } else {
            format!("{}/{task_name}", catalog.alias)
        };
        Self {
            root: root.to_path_buf(),
            catalog: catalog_path(root, &catalog.catalog_root),
            task: task_name.to_owned(),
            surface: TaskSurface::Published,
            profile: None,
            shared_lock: None,
            argument_scope: None,
            label,
        }
    }

    pub(in crate::runner) fn with_profile(mut self, profile: &str) -> Self {
        self.profile = Some(profile.to_owned());
        self
    }

    /// Key an argument-scoped task's exclusion by the arguments its command
    /// receives (Q-015). Effigy's own flags are already split off, so the
    /// wait option never changes the key; each argument is quoted
    /// canonically, so shell quoting does not either. Order and repetition
    /// are kept, so two different filters stay two keys. No arguments means
    /// no filter, which keeps the declared task's own gate.
    pub(in crate::runner) fn with_arguments(mut self, args: &[String]) -> Self {
        if self.task == ARGUMENT_SCOPED_TASK {
            let normalized = crate::runner::util::render_passthrough_args(args);
            self.argument_scope = (!normalized.is_empty()).then_some(normalized);
        }
        self
    }

    pub(in crate::runner) fn argument_scope(&self) -> Option<&str> {
        self.argument_scope.as_deref()
    }

    /// Whether this declared task keys its exclusion by its arguments
    /// (Q-015), whatever key a particular invocation resolves.
    pub(in crate::runner) fn is_argument_scoped(&self) -> bool {
        self.task == ARGUMENT_SCOPED_TASK
    }

    /// How long this target's runs wait for a live peer by default.
    pub(in crate::runner) fn default_lock_wait_ms(&self) -> u64 {
        match self.argument_scope {
            Some(_) => ARGUMENT_SCOPED_LOCK_WAIT_MS,
            None => 0,
        }
    }

    /// The identity recorded on the run, including its managed profile.
    pub(in crate::runner) fn task_id(&self, checkout: CheckoutId) -> Result<TaskId, RunnerError> {
        self.id(checkout, self.profile.as_deref())
    }

    /// The exclusion owner: one gate per task, whatever the profile, or one
    /// gate per normalized argument set for an argument-scoped task.
    pub(in crate::runner) fn owner(&self, checkout: CheckoutId) -> Result<ResourceId, RunnerError> {
        let Some(scope) = self.argument_scope() else {
            return Ok(self.id(checkout, None)?.resource());
        };
        // The kernel has one task key shape, so the scope joins the task
        // coordinate the gate is keyed by. The run's recorded identity, label
        // and status coordinates stay the declared task (K1).
        TaskId::new(
            checkout,
            &self.catalog,
            kernel_surface(self.surface),
            &argument_scoped_gate_task(&self.task, scope),
            None,
        )
        .map(|task| task.resource())
        .map_err(identity_error)
    }

    /// The shared lock's own resource, owned by a nested run.
    pub(in crate::runner) fn shared_lock_owner(
        &self,
        checkout: CheckoutId,
    ) -> Result<Option<ResourceId>, RunnerError> {
        self.shared_lock
            .as_deref()
            .map(|name| {
                ResourceId::store(Scope::Checkout(checkout), &shared_lock_store(name))
                    .map_err(identity_error)
            })
            .transpose()
    }

    fn id(&self, checkout: CheckoutId, profile: Option<&str>) -> Result<TaskId, RunnerError> {
        TaskId::new(
            checkout,
            &self.catalog,
            kernel_surface(self.surface),
            &self.task,
            profile,
        )
        .map_err(identity_error)
    }
}

fn kernel_surface(surface: TaskSurface) -> Surface {
    match surface {
        TaskSurface::Published => Surface::Published,
        TaskSurface::Draft => Surface::Draft,
    }
}

/// The task coordinate an argument-scoped gate is keyed by: the declared
/// task plus a digest of its normalized arguments, so the gate name stays
/// fixed-length whatever the filter contains and one task's filters never
/// collide with another's.
fn argument_scoped_gate_task(task: &str, scope: &str) -> String {
    let digest = Digest::of(format!("{task}\0{scope}").as_bytes());
    let hex: String = digest.0[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("{task}#{hex}")
}

/// Store name for a task-declared shared lock.
pub(in crate::runner) fn shared_lock_store(name: &str) -> String {
    format!("task-lock:{name}")
}

fn catalog_path(root: &Path, catalog_root: &Path) -> String {
    let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let catalog_root =
        std::fs::canonicalize(catalog_root).unwrap_or_else(|_| catalog_root.to_path_buf());
    super::kernel::relative(&root, &catalog_root).unwrap_or_else(|| {
        // A catalog outside its root (an external task source) keeps a
        // stable, distinct identity derived from its absolute path.
        let digest = effigy_kernel::id::Digest::of(catalog_root.as_os_str().as_encoded_bytes());
        let hex: String = digest.0[..8]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        format!("@external-{hex}")
    })
}

fn identity_error(error: effigy_kernel::id::IdentityError) -> RunnerError {
    RunnerError::task_invocation(format!("task identity is invalid: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use effigy_kernel::id::CheckoutKind;

    fn catalog(root: &Path, relative: &str, alias: &str, depth: usize) -> LoadedCatalog {
        LoadedCatalog::for_test(
            alias,
            root.join(relative),
            toml::from_str("").expect("empty manifest"),
            depth,
        )
    }

    #[test]
    fn spellings_of_one_task_share_identity_and_distinct_names_never_collide() {
        let root = tempfile::tempdir().expect("root");
        std::fs::create_dir_all(root.path().join("apps/api")).expect("catalog");
        let checkout = CheckoutId::new(CheckoutKind::Primary, [7; 16]);
        let api = catalog(root.path(), "apps/api", "api", 1);
        let via_alias = TaskTarget::for_catalog(root.path(), "build", &api);
        let via_dot_path = TaskTarget::for_catalog(
            &root.path().join("."),
            "build",
            &catalog(root.path(), "./apps/api/", "api", 1),
        );
        assert_eq!(
            via_alias.owner(checkout).unwrap(),
            via_dot_path.owner(checkout).unwrap()
        );
        assert_eq!(via_alias.label, "api/build");

        let root_catalog = catalog(root.path(), "", "", 0);
        let colon = TaskTarget::for_catalog(root.path(), "a:b", &root_catalog);
        let dash = TaskTarget::for_catalog(root.path(), "a-b", &root_catalog);
        assert_ne!(
            colon.owner(checkout).unwrap().name(),
            dash.owner(checkout).unwrap().name()
        );
        assert_eq!(colon.catalog, "");
    }

    #[test]
    fn profiles_share_the_task_owner_but_not_the_recorded_identity() {
        let root = tempfile::tempdir().expect("root");
        let checkout = CheckoutId::new(CheckoutKind::Primary, [9; 16]);
        let base = TaskTarget::for_catalog(root.path(), "dev", &catalog(root.path(), "", "", 0));
        let profiled = base.clone().with_profile("api");
        assert_eq!(
            base.owner(checkout).unwrap(),
            profiled.owner(checkout).unwrap()
        );
        assert_ne!(
            base.task_id(checkout).unwrap(),
            profiled.task_id(checkout).unwrap()
        );
    }

    fn focused(root: &Path, args: &[&str]) -> TaskTarget {
        let args = args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
        TaskTarget::for_catalog(root, ARGUMENT_SCOPED_TASK, &catalog(root, "", "", 0))
            .with_arguments(&args)
    }

    /// Q-015: `test:focused` keys its exclusion by the arguments its command
    /// receives, so different filters overlap and identical ones serialize.
    /// Every other task keeps the declared task's gate.
    #[test]
    fn only_the_focused_task_keys_its_gate_by_its_arguments() {
        let root = tempfile::tempdir().expect("root");
        let checkout = CheckoutId::new(CheckoutKind::Primary, [3; 16]);
        let api = focused(root.path(), &["-p", "api"]);
        let api_again = focused(root.path(), &["-p", "api"]);
        let web = focused(root.path(), &["-p", "web"]);
        let reordered = focused(root.path(), &["api", "-p"]);
        let bare = focused(root.path(), &[]);
        let plain =
            TaskTarget::for_catalog(root.path(), "validate", &catalog(root.path(), "", "", 0))
                .with_arguments(&["-p".to_owned(), "api".to_owned()]);

        assert_eq!(
            api.owner(checkout).unwrap(),
            api_again.owner(checkout).unwrap()
        );
        assert_ne!(api.owner(checkout).unwrap(), web.owner(checkout).unwrap());
        assert_ne!(
            api.owner(checkout).unwrap(),
            reordered.owner(checkout).unwrap(),
            "argument order is meaning: it never collapses into one key"
        );
        assert_eq!(
            bare.owner(checkout).unwrap(),
            TaskTarget::for_catalog(
                root.path(),
                ARGUMENT_SCOPED_TASK,
                &catalog(root.path(), "", "", 0)
            )
            .owner(checkout)
            .unwrap(),
            "no arguments is no filter: the declared task's own gate"
        );
        assert_eq!(
            plain.owner(checkout).unwrap(),
            TaskTarget::for_catalog(root.path(), "validate", &catalog(root.path(), "", "", 0))
                .owner(checkout)
                .unwrap(),
            "another task's arguments never key its gate"
        );
        // The recorded identity and its label stay the declared task (K1).
        assert_eq!(
            api.task_id(checkout).unwrap(),
            bare.task_id(checkout).unwrap()
        );
        assert_eq!(api.label, ARGUMENT_SCOPED_TASK);
        assert!(api.is_argument_scoped());
        assert!(!plain.is_argument_scoped());
    }

    /// The opt-in also names the default wait: about two minutes for an
    /// identical peer, nothing for every other selector (Q-015).
    #[test]
    fn the_focused_task_takes_the_argument_scope_wait_and_others_fail_fast() {
        let root = tempfile::tempdir().expect("root");
        assert_eq!(ARGUMENT_SCOPED_LOCK_WAIT_MS, 120_000);
        assert_eq!(
            focused(root.path(), &["-p", "api"]).default_lock_wait_ms(),
            ARGUMENT_SCOPED_LOCK_WAIT_MS
        );
        assert_eq!(focused(root.path(), &[]).default_lock_wait_ms(), 0);
        assert_eq!(
            TaskTarget::for_catalog(root.path(), "validate", &catalog(root.path(), "", "", 0))
                .with_arguments(&["-p".to_owned(), "api".to_owned()])
                .default_lock_wait_ms(),
            0
        );
    }
}
