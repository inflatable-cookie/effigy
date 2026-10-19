//! One resolved task identity (K1). Lock, status, logs and the recovery
//! command all derive from the resolver's selection, never from the selector
//! as typed: `build`, `api/build` and `./apps/api/build` are one task, and
//! `a:b` and `a-b` stay two.

use std::path::{Path, PathBuf};

use effigy_core::task_selection::TaskSurface;
use effigy_kernel::id::{CheckoutId, ResourceId, Scope, Surface, TaskId};
use effigy_manifest::{LoadedCatalog, TaskSelection};

use crate::runner::error::RunnerError;

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
            label,
        }
    }

    pub(in crate::runner) fn with_profile(mut self, profile: &str) -> Self {
        self.profile = Some(profile.to_owned());
        self
    }

    /// The identity recorded on the run, including its managed profile.
    pub(in crate::runner) fn task_id(&self, checkout: CheckoutId) -> Result<TaskId, RunnerError> {
        self.id(checkout, self.profile.as_deref())
    }

    /// The exclusion owner: one gate per task, whatever the profile.
    pub(in crate::runner) fn owner(&self, checkout: CheckoutId) -> Result<ResourceId, RunnerError> {
        Ok(self.id(checkout, None)?.resource())
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
            match self.surface {
                TaskSurface::Published => Surface::Published,
                TaskSurface::Draft => Surface::Draft,
            },
            &self.task,
            profile,
        )
        .map_err(identity_error)
    }
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
        let hex: String = digest.0[..8].iter().map(|byte| format!("{byte:02x}")).collect();
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
        LoadedCatalog {
            alias: alias.to_owned(),
            catalog_root: root.join(relative),
            manifest_path: root.join(relative).join("effigy.toml"),
            bundle_root: None,
            manifest: toml::from_str("").expect("empty manifest"),
            defer_run: None,
            deferred_builtins: Default::default(),
            depth,
            draft_sources: Default::default(),
        }
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
        assert_eq!(base.owner(checkout).unwrap(), profiled.owner(checkout).unwrap());
        assert_ne!(
            base.task_id(checkout).unwrap(),
            profiled.task_id(checkout).unwrap()
        );
    }
}
