use std::ffi::OsString;
use std::path::Path;
use std::time::Instant;

use effigy_containers::{
    load_workspace_ownership_plan, EffectiveContainerPolicy, WorkspaceMountKind,
    WorkspaceOwnershipPlan, WorkspaceOwnershipTarget, WorkspaceRepairAuthority,
    WorkspaceRustCacheKind,
};

use super::workspace_provisioning::{plan_workspace_permission_prep, WorkspacePermissionMode};
use super::RunnerError;
use crate::runner::exec_command::run_compose_exec_with_deadline;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct ResolvedWorkspaceIdentity {
    pub user: String,
    pub uid: u32,
    pub gid: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::runner) enum PathPresence {
    Missing,
    File,
    Directory,
    Symlink,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct PathInspection {
    pub presence: PathPresence,
    pub uid: Option<u32>,
    pub gid: Option<u32>,
    pub mode: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) enum AccessProbe {
    Ready,
    Unwritable,
    LockFailed,
    Missing,
    Symlink,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PermissionPrepContext {
    pub profile: String,
    pub project_name: String,
    pub container_name: String,
    pub service: String,
}

#[derive(Debug)]
pub(in crate::runner) struct PermissionPrepError {
    pub message: String,
}

impl PermissionPrepError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl From<PermissionPrepError> for RunnerError {
    fn from(error: PermissionPrepError) -> Self {
        RunnerError::task_invocation(error.message)
    }
}

pub(in crate::runner) trait WorkspaceAccessBackend {
    fn resolve_identity(
        &self,
        user: &str,
    ) -> Result<ResolvedWorkspaceIdentity, PermissionPrepError>;
    fn resolve_doctor_identity(
        &self,
        user: &str,
    ) -> Result<ResolvedWorkspaceIdentity, PermissionPrepError> {
        self.resolve_identity(user)
    }
    fn inspect(&self, path: &str) -> Result<PathInspection, PermissionPrepError>;
    fn mkdir_p(&mut self, path: &str) -> Result<(), PermissionPrepError>;
    fn chown(&mut self, path: &str, uid: u32, gid: u32) -> Result<(), PermissionPrepError>;
    /// Repairs ownership of every non-symlink entry under `path` inside one
    /// bounded backend operation. Never follows symlinks or leaves the
    /// filesystem of `path`. Runtime round trips must not scale with entries.
    fn chown_tree_unowned(
        &mut self,
        path: &str,
        uid: u32,
        gid: u32,
    ) -> Result<(), PermissionPrepError>;
    fn chmod_owner_write(&mut self, path: &str, directory: bool)
        -> Result<(), PermissionPrepError>;
    fn list_unowned(
        &self,
        path: &str,
        uid: u32,
        gid: u32,
    ) -> Result<Vec<String>, PermissionPrepError>;
    fn user_can_read_write(
        &self,
        identity: &ResolvedWorkspaceIdentity,
        path: &str,
    ) -> Result<bool, PermissionPrepError>;
    fn user_create_lock(
        &mut self,
        identity: &ResolvedWorkspaceIdentity,
        directory: &str,
    ) -> Result<(), PermissionPrepError>;

    fn inspect_many(
        &mut self,
        paths: &[String],
    ) -> Result<Vec<PathInspection>, PermissionPrepError> {
        paths.iter().map(|path| self.inspect(path)).collect()
    }

    fn find_unowned_many(
        &mut self,
        paths: &[String],
        uid: u32,
        gid: u32,
    ) -> Result<Vec<Option<String>>, PermissionPrepError> {
        paths
            .iter()
            .map(|path| {
                self.list_unowned(path, uid, gid)
                    .map(|unowned| unowned.into_iter().next())
            })
            .collect()
    }

    fn mkdir_many(&mut self, paths: &[String]) -> Result<(), PermissionPrepError> {
        for path in paths {
            self.mkdir_p(path)?;
        }
        Ok(())
    }

    fn chown_shallow_many(
        &mut self,
        paths: &[String],
        uid: u32,
        gid: u32,
    ) -> Result<(), PermissionPrepError> {
        for path in paths {
            self.chown(path, uid, gid)?;
        }
        Ok(())
    }

    fn chmod_owner_write_many(
        &mut self,
        paths: &[(String, bool)],
    ) -> Result<(), PermissionPrepError> {
        for (path, directory) in paths {
            self.chmod_owner_write(path, *directory)?;
        }
        Ok(())
    }

    fn probe_access_many(
        &mut self,
        identity: &ResolvedWorkspaceIdentity,
        paths: &[(String, bool)],
    ) -> Result<Vec<AccessProbe>, PermissionPrepError> {
        paths
            .iter()
            .map(|(path, directory)| {
                if !self.user_can_read_write(identity, path)? {
                    return Ok(AccessProbe::Unwritable);
                }
                if *directory {
                    return Ok(if self.user_create_lock(identity, path).is_ok() {
                        AccessProbe::Ready
                    } else {
                        AccessProbe::LockFailed
                    });
                }
                Ok(AccessProbe::Ready)
            })
            .collect()
    }

    fn inspect_doctor_paths(
        &mut self,
        identity: &ResolvedWorkspaceIdentity,
        paths: &[String],
    ) -> Result<Vec<Vec<String>>, PermissionPrepError> {
        paths
            .iter()
            .map(|path| {
                let inspection = self.inspect(path)?;
                let can_read_write = match inspection.presence {
                    PathPresence::Missing | PathPresence::Symlink => None,
                    _ => Some(self.user_can_read_write(identity, path)?),
                };
                Ok(doctor_path_samples(
                    identity,
                    path,
                    &inspection,
                    can_read_write,
                ))
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PreparedTarget {
    path: String,
    authority: WorkspaceRepairAuthority,
    mode: WorkspacePermissionMode,
    rust_cache: Option<WorkspaceRustCacheKind>,
    mount_kind: WorkspaceMountKind,
    source: Option<String>,
}

pub(super) fn ensure_workspace_permissions_ready_with(
    policy: &EffectiveContainerPolicy,
    container_name: Option<&str>,
    backend: &mut impl WorkspaceAccessBackend,
) -> Result<(), RunnerError> {
    let Some(user) = policy.workspace_user.as_deref() else {
        return Ok(());
    };
    let plan = load_workspace_ownership_plan(policy)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let context = PermissionPrepContext {
        profile: policy.profile.clone(),
        project_name: policy.project_name.clone(),
        container_name: container_name.unwrap_or(policy.name.as_str()).to_owned(),
        service: policy.primary_service.clone(),
    };
    prepare_workspace_permissions(
        user,
        &plan,
        policy.workspace_home.as_deref(),
        &context,
        backend,
    )
    .map_err(Into::into)
}

pub(super) fn prepare_workspace_permissions(
    user: &str,
    plan: &WorkspaceOwnershipPlan,
    workspace_home: Option<&str>,
    context: &PermissionPrepContext,
    backend: &mut impl WorkspaceAccessBackend,
) -> Result<(), PermissionPrepError> {
    let targets = prepared_targets(plan, workspace_home);
    if targets.is_empty() {
        return Ok(());
    }
    let identity = backend.resolve_doctor_identity(user)?;
    if identity.uid == 0 {
        return Err(PermissionPrepError::new(format!(
            "workspace user `{}` resolved to uid 0 in container `{}` service `{}`; refusing to treat root as the declared workspace identity",
            identity.user, context.container_name, context.service
        )));
    }

    let probe_paths = prepared_probe_paths(&targets);
    let mut inspections = backend.inspect_many(&probe_paths)?;
    require_batch_len("metadata", probe_paths.len(), inspections.len())?;
    for (path, inspection) in probe_paths.iter().zip(&inspections) {
        if inspection.presence == PathPresence::Symlink {
            return Err(fail_with_identity(
                &identity,
                target_for_probe_path(&targets, path),
                context,
                format!("refusing to follow symlink `{path}`"),
            ));
        }
    }
    for target in &targets {
        if target.authority == WorkspaceRepairAuthority::OwnedDisposable
            && inspection_for(&probe_paths, &inspections, &target.path)
                .is_some_and(|inspection| inspection.presence == PathPresence::File)
        {
            return Err(fail_with_identity(
                &identity,
                target,
                context,
                format!("disposable path `{}` is not a directory", target.path),
            ));
        }
    }
    for target in &targets {
        if target.authority == WorkspaceRepairAuthority::Forbidden && target.rust_cache.is_some() {
            return Err(fail_with_identity(
                &identity,
                target,
                context,
                format!(
                    "declared rust build/cache path `{}` is a foreign, shared, or read-only mount",
                    target.path
                ),
            ));
        }
    }

    let missing_owned_paths = targets
        .iter()
        .filter(|target| target.authority == WorkspaceRepairAuthority::OwnedDisposable)
        .filter(|target| {
            inspection_for(&probe_paths, &inspections, &target.path)
                .is_some_and(|inspection| inspection.presence == PathPresence::Missing)
        })
        .map(|target| target.path.clone())
        .collect::<Vec<_>>();
    backend.mkdir_many(&missing_owned_paths).map_err(|error| {
        PermissionPrepError::new(format!(
            "failed to create owned workspace path: {}",
            error.message
        ))
    })?;
    if !missing_owned_paths.is_empty() {
        inspections = backend.inspect_many(&probe_paths)?;
        require_batch_len("metadata", probe_paths.len(), inspections.len())?;
        for (path, inspection) in probe_paths.iter().zip(&inspections) {
            if inspection.presence == PathPresence::Symlink {
                return Err(fail_with_identity(
                    &identity,
                    target_for_probe_path(&targets, path),
                    context,
                    format!("refusing to follow symlink `{path}`"),
                ));
            }
        }
    }

    let recursive_paths = targets
        .iter()
        .filter(|target| {
            target.authority == WorkspaceRepairAuthority::OwnedDisposable
                && target.mode == WorkspacePermissionMode::Recursive
        })
        .map(|target| target.path.clone())
        .collect::<Vec<_>>();
    let unowned = backend.find_unowned_many(&recursive_paths, identity.uid, identity.gid)?;
    require_batch_len("ownership", recursive_paths.len(), unowned.len())?;
    let dirty_paths = recursive_paths
        .iter()
        .zip(unowned)
        .filter_map(|(path, first_unowned)| first_unowned.map(|_| path.clone()))
        .collect::<Vec<_>>();

    let shallow_repairs = targets
        .iter()
        .filter(|target| {
            target.authority == WorkspaceRepairAuthority::OwnedDisposable
                && target.mode == WorkspacePermissionMode::Shallow
        })
        .filter(|target| {
            inspection_for(&probe_paths, &inspections, &target.path).is_some_and(|inspection| {
                inspection.presence != PathPresence::Missing
                    && (inspection.uid != Some(identity.uid)
                        || inspection.gid != Some(identity.gid))
            })
        })
        .map(|target| target.path.clone())
        .collect::<Vec<_>>();

    for path in &dirty_paths {
        let target = target_for_probe_path(&targets, path);
        backend
            .chown_tree_unowned(path, identity.uid, identity.gid)
            .map_err(|error| {
                fail_with_identity(
                    &identity,
                    target,
                    context,
                    format!(
                        "failed to repair ownership under `{path}`: {}",
                        error.message
                    ),
                )
            })?;
    }
    backend
        .chown_shallow_many(&shallow_repairs, identity.uid, identity.gid)
        .map_err(|error| {
            PermissionPrepError::new(format!(
                "failed to repair shallow workspace ownership: {}",
                error.message
            ))
        })?;

    if !dirty_paths.is_empty() || !shallow_repairs.is_empty() {
        inspections = backend.inspect_many(&probe_paths)?;
        require_batch_len("post-repair metadata", probe_paths.len(), inspections.len())?;
        for path in &dirty_paths {
            let inspection = inspection_for(&probe_paths, &inspections, path);
            if inspection.is_none_or(|value| {
                matches!(
                    value.presence,
                    PathPresence::Missing | PathPresence::Symlink
                )
            }) {
                return Err(fail_with_identity(
                    &identity,
                    target_for_probe_path(&targets, path),
                    context,
                    format!(
                        "`{path}` changed identity during ownership repair; refusing to continue"
                    ),
                ));
            }
        }
        for (path, inspection) in probe_paths.iter().zip(&inspections) {
            if inspection.presence == PathPresence::Symlink {
                return Err(fail_with_identity(
                    &identity,
                    target_for_probe_path(&targets, path),
                    context,
                    format!("refusing to follow symlink `{path}` after repair"),
                ));
            }
        }
        if !dirty_paths.is_empty() {
            let remaining = backend.find_unowned_many(&dirty_paths, identity.uid, identity.gid)?;
            require_batch_len("post-repair ownership", dirty_paths.len(), remaining.len())?;
            if let Some((path, Some(first))) = dirty_paths
                .iter()
                .zip(remaining)
                .find(|(_, first)| first.is_some())
            {
                return Err(fail_with_identity(
                    &identity,
                    target_for_probe_path(&targets, path),
                    context,
                    format!(
                        "ownership entries remain under `{path}` after repair (first: `{first}`)"
                    ),
                ));
            }
        }
    }

    let access_paths = probe_paths
        .iter()
        .filter_map(|path| {
            let target = target_for_probe_path(&targets, path);
            let inspection = inspection_for(&probe_paths, &inspections, path)?;
            if inspection.presence == PathPresence::Missing
                || (target.authority == WorkspaceRepairAuthority::Forbidden
                    && target.rust_cache.is_none())
            {
                return None;
            }
            Some((path.clone(), inspection.presence == PathPresence::Directory))
        })
        .collect::<Vec<_>>();
    let access = backend.probe_access_many(&identity, &access_paths)?;
    require_batch_len("numeric-user access", access_paths.len(), access.len())?;
    let mut chmod_paths = Vec::new();
    for ((path, directory), result) in access_paths.iter().zip(access) {
        let target = target_for_probe_path(&targets, path);
        match result {
            AccessProbe::Ready => {}
            AccessProbe::Unwritable
                if target.authority == WorkspaceRepairAuthority::OwnedDisposable =>
            {
                chmod_paths.push((path.clone(), *directory));
            }
            AccessProbe::Unwritable => {
                return Err(fail_with_identity(
                    &identity,
                    target,
                    context,
                    format!(
                        "resolved user `{}` (uid={} gid={}) cannot read/write `{path}`",
                        identity.user, identity.uid, identity.gid
                    ),
                ));
            }
            AccessProbe::LockFailed => {
                return Err(fail_with_identity(
                    &identity,
                    target,
                    context,
                    format!(
                        "resolved user `{}` (uid={} gid={}) cannot create a lock under `{path}`",
                        identity.user, identity.uid, identity.gid
                    ),
                ));
            }
            AccessProbe::Missing
                if path == &target.path
                    && target.authority == WorkspaceRepairAuthority::OwnedDisposable =>
            {
                return Err(fail_with_identity(
                    &identity,
                    target,
                    context,
                    format!("disposable path `{path}` is missing after preparation"),
                ));
            }
            AccessProbe::Missing => {}
            AccessProbe::Symlink => {
                return Err(fail_with_identity(
                    &identity,
                    target,
                    context,
                    format!("refusing to follow symlink `{path}` during access verification"),
                ));
            }
        }
    }
    if !chmod_paths.is_empty() {
        backend
            .chmod_owner_write_many(&chmod_paths)
            .map_err(|error| {
                PermissionPrepError::new(format!(
                    "failed to restore owner write during workspace preparation: {}",
                    error.message
                ))
            })?;
        let repaired_access = backend.probe_access_many(&identity, &chmod_paths)?;
        require_batch_len(
            "post-chmod numeric-user access",
            chmod_paths.len(),
            repaired_access.len(),
        )?;
        for ((path, _), result) in chmod_paths.iter().zip(repaired_access) {
            if result != AccessProbe::Ready {
                let target = target_for_probe_path(&targets, path);
                return Err(fail_with_identity(
                    &identity,
                    target,
                    context,
                    format!(
                        "resolved user `{}` (uid={} gid={}) still cannot use `{path}` after owner-write repair",
                        identity.user, identity.uid, identity.gid
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn prepared_targets(
    plan: &WorkspaceOwnershipPlan,
    workspace_home: Option<&str>,
) -> Vec<PreparedTarget> {
    let mut prepared = Vec::new();
    let mut owned_paths = Vec::new();
    for target in &plan.targets {
        if let Some(home) = workspace_home {
            if target.path == home && authority_allows_home_expansion(target) {
                prepared.push(PreparedTarget {
                    path: home.to_owned(),
                    authority: target.repair_authority,
                    mode: WorkspacePermissionMode::Shallow,
                    rust_cache: target.rust_cache,
                    mount_kind: target.mount_kind,
                    source: target.source.clone(),
                });
                if target.repair_authority == WorkspaceRepairAuthority::OwnedDisposable {
                    for suffix in [".cache", ".config", ".local"] {
                        prepared.push(PreparedTarget {
                            path: format!("{home}/{suffix}"),
                            authority: WorkspaceRepairAuthority::OwnedDisposable,
                            mode: WorkspacePermissionMode::Recursive,
                            rust_cache: None,
                            mount_kind: target.mount_kind,
                            source: target.source.clone(),
                        });
                    }
                }
                continue;
            }
        }
        match target.repair_authority {
            WorkspaceRepairAuthority::OwnedDisposable => {
                owned_paths.push(target.path.clone());
            }
            WorkspaceRepairAuthority::VerifyOnly | WorkspaceRepairAuthority::Forbidden => {
                prepared.push(PreparedTarget::from_ownership(
                    target,
                    WorkspacePermissionMode::Recursive,
                ));
            }
        }
    }
    let owned_plan = plan_workspace_permission_prep(&owned_paths);
    for target in owned_plan.targets {
        let source = plan
            .targets
            .iter()
            .find(|candidate| {
                candidate.path == target.path || path_is_under(&target.path, &candidate.path)
            })
            .cloned();
        prepared.push(PreparedTarget {
            path: target.path,
            authority: WorkspaceRepairAuthority::OwnedDisposable,
            mode: target.mode,
            rust_cache: source.as_ref().and_then(|value| value.rust_cache),
            mount_kind: source
                .as_ref()
                .map(|value| value.mount_kind)
                .unwrap_or(WorkspaceMountKind::NamedVolume),
            source: source.and_then(|value| value.source),
        });
    }
    prepared
}

fn authority_allows_home_expansion(target: &WorkspaceOwnershipTarget) -> bool {
    target.repair_authority != WorkspaceRepairAuthority::Forbidden
}

impl PreparedTarget {
    fn from_ownership(target: &WorkspaceOwnershipTarget, mode: WorkspacePermissionMode) -> Self {
        Self {
            path: target.path.clone(),
            authority: target.repair_authority,
            mode,
            rust_cache: target.rust_cache,
            mount_kind: target.mount_kind,
            source: target.source.clone(),
        }
    }
}

fn path_is_under(path: &str, parent: &str) -> bool {
    let path = path.trim_end_matches('/');
    let parent = parent.trim_end_matches('/');
    if parent.is_empty() || path == parent {
        return false;
    }
    if parent == "/" {
        return path.starts_with('/');
    }
    path.starts_with(parent) && path.as_bytes().get(parent.len()) == Some(&b'/')
}

fn prepared_probe_paths(targets: &[PreparedTarget]) -> Vec<String> {
    let mut paths = Vec::new();
    for target in targets {
        for path in std::iter::once(target.path.clone()).chain(nested_verify_paths(target)) {
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
    }
    paths
}

fn target_for_probe_path<'a>(targets: &'a [PreparedTarget], path: &str) -> &'a PreparedTarget {
    targets
        .iter()
        .find(|target| target.path == path)
        .or_else(|| {
            targets
                .iter()
                .filter(|target| path_is_under(path, &target.path))
                .max_by_key(|target| target.path.len())
        })
        .expect("probe paths are derived from prepared targets")
}

fn inspection_for<'a>(
    paths: &[String],
    inspections: &'a [PathInspection],
    path: &str,
) -> Option<&'a PathInspection> {
    paths
        .iter()
        .position(|candidate| candidate == path)
        .and_then(|index| inspections.get(index))
}

fn require_batch_len(
    label: &str,
    expected: usize,
    actual: usize,
) -> Result<(), PermissionPrepError> {
    if actual == expected {
        return Ok(());
    }
    Err(PermissionPrepError::new(format!(
        "workspace ownership {label} batch returned {actual} records for {expected} paths"
    )))
}

fn nested_verify_paths(target: &PreparedTarget) -> Vec<String> {
    rust_nested_probe_paths(&target.path, target.rust_cache)
}

pub(in crate::runner) fn rust_nested_probe_paths(
    path: &str,
    rust_cache: Option<WorkspaceRustCacheKind>,
) -> Vec<String> {
    match rust_cache {
        Some(WorkspaceRustCacheKind::RustTarget) => {
            vec![
                format!("{path}/debug"),
                format!("{path}/debug/.cargo-build-lock"),
            ]
        }
        Some(WorkspaceRustCacheKind::CargoGit) => {
            vec![format!("{path}/checkouts")]
        }
        Some(WorkspaceRustCacheKind::CargoRegistry) => {
            vec![format!("{path}/src")]
        }
        Some(WorkspaceRustCacheKind::CargoHome) => vec![
            format!("{path}/registry/src"),
            format!("{path}/git/checkouts"),
        ],
        None => Vec::new(),
    }
}

fn fail_with_identity(
    identity: &ResolvedWorkspaceIdentity,
    target: &PreparedTarget,
    context: &PermissionPrepContext,
    reason: String,
) -> PermissionPrepError {
    let mut message = format!(
        "{reason}\nruntime profile=`{}` project=`{}` container=`{}` service=`{}` path=`{}` user=`{}` uid={} gid={} mount={} source=`{}`",
        context.profile,
        context.project_name,
        context.container_name,
        context.service,
        target.path,
        identity.user,
        identity.uid,
        identity.gid,
        mount_kind_label(target.mount_kind),
        target.source.as_deref().unwrap_or("-"),
    );
    message.push('\n');
    message.push_str(&safe_repair_guidance(identity, target, context));
    PermissionPrepError::new(message)
}

fn mount_kind_label(kind: WorkspaceMountKind) -> &'static str {
    match kind {
        WorkspaceMountKind::NamedVolume => "named-volume",
        WorkspaceMountKind::Bind => "bind",
        WorkspaceMountKind::Image => "image",
    }
}

fn safe_repair_guidance(
    identity: &ResolvedWorkspaceIdentity,
    target: &PreparedTarget,
    context: &PermissionPrepContext,
) -> String {
    match target.authority {
        WorkspaceRepairAuthority::OwnedDisposable => format!(
            "safe repair (owned disposable {}): compose exec -u 0 {} chown -R {}:{} {}",
            mount_kind_label(target.mount_kind),
            context.service,
            identity.uid,
            identity.gid,
            target.path
        ),
        WorkspaceRepairAuthority::VerifyOnly | WorkspaceRepairAuthority::Forbidden => format!(
            "Effigy will not chown this {} path. Isolate disposable rust caches with catalog `isolated_dirs`, or repair the host mount for uid {} without a recursive host-source chown.",
            mount_kind_label(target.mount_kind),
            identity.uid
        ),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::runner) enum WorkspaceOwnershipProbeStatus {
    NotProbedNoUser,
    NotProbedStopped,
    Unavailable,
    Clean,
    Finding,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct WorkspaceOwnershipDiagnosis {
    pub status: WorkspaceOwnershipProbeStatus,
    pub evidence: Option<String>,
    pub warning: Option<String>,
    pub samples: Vec<String>,
    pub identity: Option<ResolvedWorkspaceIdentity>,
}

pub(in crate::runner) fn diagnose_workspace_ownership(
    policy: &EffectiveContainerPolicy,
    running: Result<bool, String>,
    extra_env_targets: &[String],
    backend: &mut impl WorkspaceAccessBackend,
) -> WorkspaceOwnershipDiagnosis {
    let Some(user) = policy.workspace_user.as_deref() else {
        return WorkspaceOwnershipDiagnosis {
            status: WorkspaceOwnershipProbeStatus::NotProbedNoUser,
            evidence: Some(format!(
                "container `{}` workspace ownership: not probed (no workspace user)",
                policy.name
            )),
            warning: None,
            samples: Vec::new(),
            identity: None,
        };
    };
    match running {
        Ok(false) => {
            return WorkspaceOwnershipDiagnosis {
                status: WorkspaceOwnershipProbeStatus::NotProbedStopped,
                evidence: Some(format!(
                    "container `{}` workspace ownership: not probed (primary service stopped)",
                    policy.name
                )),
                warning: None,
                samples: Vec::new(),
                identity: None,
            };
        }
        Err(error) => {
            return WorkspaceOwnershipDiagnosis {
                status: WorkspaceOwnershipProbeStatus::Unavailable,
                evidence: None,
                warning: Some(format!(
                    "container `{}` workspace ownership probe skipped: {error}",
                    policy.name
                )),
                samples: Vec::new(),
                identity: None,
            };
        }
        Ok(true) => {}
    }
    let plan = match load_workspace_ownership_plan(policy) {
        Ok(plan) => plan,
        Err(error) => {
            return WorkspaceOwnershipDiagnosis {
                status: WorkspaceOwnershipProbeStatus::Unavailable,
                evidence: None,
                warning: Some(format!(
                    "container `{}` workspace ownership probe failed: {error}",
                    policy.name
                )),
                samples: Vec::new(),
                identity: None,
            };
        }
    };
    let identity = match backend.resolve_doctor_identity(user) {
        Ok(identity) => identity,
        Err(error) => {
            return WorkspaceOwnershipDiagnosis {
                status: WorkspaceOwnershipProbeStatus::Unavailable,
                evidence: None,
                warning: Some(format!(
                    "container `{}` workspace ownership verification incomplete (workspace ownership probe failed): {}",
                    policy.name, error.message
                )),
                samples: Vec::new(),
                identity: None,
            };
        }
    };
    let targets = plan
        .targets
        .iter()
        .filter(|target| {
            target.rust_cache.is_some()
                || target.repair_authority == WorkspaceRepairAuthority::OwnedDisposable
        })
        .collect::<Vec<_>>();
    let mut target_samples = vec![Vec::new(); targets.len()];
    let mut root_paths = targets
        .iter()
        .map(|target| target.path.clone())
        .collect::<Vec<_>>();
    root_paths.extend(extra_env_targets.iter().cloned());
    let root_results = match backend.inspect_doctor_paths(&identity, &root_paths) {
        Ok(results) => results,
        Err(error) => {
            return unavailable_workspace_diagnosis(policy, Some(identity), error);
        }
    };
    for (index, result) in root_results.iter().take(targets.len()).enumerate() {
        target_samples[index].extend(result.iter().cloned());
    }
    let mut samples = root_results
        .iter()
        .skip(targets.len())
        .flatten()
        .cloned()
        .collect::<Vec<_>>();

    // Batch each nested depth across clean mounts. These are still the exact
    // bounded Cargo/target paths the doctor has always sampled; missing paths
    // are not walked and a sample on a mount stops its later nested probes.
    let mut pending = targets
        .iter()
        .enumerate()
        .filter(|(index, target)| target_samples[*index].is_empty() && target.rust_cache.is_some())
        .map(|(index, target)| {
            (
                index,
                std::collections::VecDeque::from(rust_nested_probe_paths(
                    &target.path,
                    target.rust_cache,
                )),
            )
        })
        .collect::<Vec<_>>();
    let mut cached_nested = std::collections::HashMap::<String, Vec<String>>::new();
    while !pending.is_empty() {
        let mut next_paths = Vec::new();
        for (_, paths) in &mut pending {
            if let Some(path) = paths.front() {
                if !cached_nested.contains_key(path) && !next_paths.contains(path) {
                    next_paths.push(path.clone());
                }
            }
        }
        if !next_paths.is_empty() {
            let results = match backend.inspect_doctor_paths(&identity, &next_paths) {
                Ok(results) => results,
                Err(error) => {
                    return unavailable_workspace_diagnosis(policy, Some(identity), error);
                }
            };
            cached_nested.extend(next_paths.into_iter().zip(results));
        }

        pending.retain_mut(|(index, paths)| {
            let Some(path) = paths.front() else {
                return false;
            };
            if let Some(found) = cached_nested.get(path) {
                if !found.is_empty() {
                    target_samples[*index].extend(found.iter().cloned());
                    return false;
                }
                paths.pop_front();
                return !paths.is_empty();
            }
            false
        });
    }
    for target_result in target_samples {
        samples.extend(target_result);
    }
    if samples.is_empty() {
        WorkspaceOwnershipDiagnosis {
            status: WorkspaceOwnershipProbeStatus::Clean,
            evidence: Some(format!(
                "container `{}` workspace ownership: clean for user `{user}` (uid={} gid={})",
                policy.name, identity.uid, identity.gid
            )),
            warning: None,
            samples,
            identity: Some(identity),
        }
    } else {
        WorkspaceOwnershipDiagnosis {
            status: WorkspaceOwnershipProbeStatus::Finding,
            evidence: None,
            warning: None,
            samples,
            identity: Some(identity),
        }
    }
}

fn unavailable_workspace_diagnosis(
    policy: &EffectiveContainerPolicy,
    identity: Option<ResolvedWorkspaceIdentity>,
    error: PermissionPrepError,
) -> WorkspaceOwnershipDiagnosis {
    WorkspaceOwnershipDiagnosis {
        status: WorkspaceOwnershipProbeStatus::Unavailable,
        evidence: None,
        warning: Some(format!(
            "container `{}` workspace ownership verification incomplete (workspace ownership probe failed): {}",
            policy.name, error.message
        )),
        samples: Vec::new(),
        identity,
    }
}

fn doctor_path_samples(
    identity: &ResolvedWorkspaceIdentity,
    path: &str,
    inspection: &PathInspection,
    can_read_write: Option<bool>,
) -> Vec<String> {
    if inspection.presence == PathPresence::Missing {
        return Vec::new();
    }
    if inspection.presence == PathPresence::Symlink {
        return vec![format!("{path}\tsymlink")];
    }
    let mut samples = Vec::new();
    if can_read_write == Some(false) {
        samples.push(format!("{path}\tunwritable-by-uid-{}", identity.uid));
    }
    if inspection.uid != Some(identity.uid) || inspection.gid != Some(identity.gid) {
        samples.push(format!("{path}\t{path}"));
    }
    samples
}

pub(in crate::runner) struct ComposeAccessBackend<'a> {
    pub repo_root: &'a Path,
    pub policy: &'a EffectiveContainerPolicy,
    pub deadline: Option<Instant>,
    /// Upper bound for the single bulk chown child, applied even when the
    /// caller supplies no overall deadline.
    pub bulk_timeout: std::time::Duration,
}

/// A volume of tens of thousands of entries repairs in seconds; this only
/// bounds a hung or pathological child.
const BULK_CHOWN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(600);
const DOCTOR_METADATA_BATCH_SCRIPT: &str = "for path do\n  if [ -L \"$path\" ]; then\n    printf 'symlink\\n'\n  elif [ ! -e \"$path\" ]; then\n    printf 'missing\\n'\n  elif [ -d \"$path\" ]; then\n    metadata=\"$(stat -c '%u %g %a' -- \"$path\")\" || exit 1\n    printf 'dir %s\\n' \"$metadata\"\n  else\n    metadata=\"$(stat -c '%u %g %a' -- \"$path\")\" || exit 1\n    printf 'file %s\\n' \"$metadata\"\n  fi\ndone";
const DOCTOR_ACCESS_BATCH_SCRIPT: &str = "for path do\n  if [ -r \"$path\" ] && [ -w \"$path\" ]; then\n    printf 'read-write\\n'\n  else\n    printf 'unwritable\\n'\n  fi\ndone";
const OWNERSHIP_FIND_BATCH_SCRIPT: &str = "uid=$1; gid=$2; shift 2; for path do\n  if [ -L \"$path\" ]; then\n    printf 'symlink\\n'\n  elif [ ! -e \"$path\" ]; then\n    printf 'clean\\n'\n  else\n    first=\"$(find -P \"$path\" -xdev ! -type l ! \\( -user \"$uid\" -a -group \"$gid\" \\) -print -quit)\" || exit 1\n    if [ -n \"$first\" ]; then printf 'dirty\\t%s\\n' \"$first\"; else printf 'clean\\n'; fi\n  fi\ndone";
const WORKSPACE_ACCESS_BATCH_SCRIPT: &str = "while [ \"$#\" -ge 2 ]; do\n  path=$1; directory=$2; shift 2\n  if [ -L \"$path\" ]; then\n    printf 'symlink\\n'\n  elif [ ! -e \"$path\" ]; then\n    printf 'missing\\n'\n  elif [ ! -r \"$path\" ] || [ ! -w \"$path\" ]; then\n    printf 'unwritable\\n'\n  elif [ \"$directory\" = yes ]; then\n    probe=\"${path%/}/.effigy-write-probe-$$\"\n    if touch \"$probe\" && rm -f -- \"$probe\"; then printf 'ready\\n'; else printf 'lock-failed\\n'; fi\n  else\n    printf 'ready\\n'\n  fi\ndone";
const WORKSPACE_MKDIR_BATCH_SCRIPT: &str = "for path do mkdir -p -- \"$path\" || exit 1; done";
const WORKSPACE_CHOWN_SHALLOW_BATCH_SCRIPT: &str =
    "spec=$1; shift; for path do chown -h \"$spec\" -- \"$path\" || exit 1; done";
const WORKSPACE_CHMOD_BATCH_SCRIPT: &str = "while [ \"$#\" -ge 2 ]; do path=$1; kind=$2; shift 2; mode=u+w; [ \"$kind\" = dir ] && mode=u+wx; chmod \"$mode\" -- \"$path\" || exit 1; done";

impl WorkspaceAccessBackend for ComposeAccessBackend<'_> {
    fn resolve_identity(
        &self,
        user: &str,
    ) -> Result<ResolvedWorkspaceIdentity, PermissionPrepError> {
        self.resolve_doctor_identity(user)
    }

    fn resolve_doctor_identity(
        &self,
        user: &str,
    ) -> Result<ResolvedWorkspaceIdentity, PermissionPrepError> {
        let output = self.exec_root(
            &[
                "sh",
                "-c",
                r#"uid="$(id -u "$1")" && gid="$(id -g "$1")" && printf '%s\n%s\n' "$uid" "$gid""#,
                "effigy-workspace-identity",
                user,
            ],
            "workspace identity probe",
        )?;
        let mut lines = output.lines();
        let uid = parse_id_output(lines.next().unwrap_or_default())?;
        let gid = parse_id_output(lines.next().unwrap_or_default())?;
        if lines.next().is_some() {
            return Err(PermissionPrepError::new(
                "workspace identity probe returned unexpected extra output",
            ));
        }
        Ok(ResolvedWorkspaceIdentity {
            user: user.to_owned(),
            uid,
            gid,
        })
    }

    fn inspect(&self, path: &str) -> Result<PathInspection, PermissionPrepError> {
        let output = self.exec_root(
            &[
                "sh",
                "-c",
                r#"if [ -L "$1" ]; then printf 'symlink\n'; elif [ ! -e "$1" ]; then printf 'missing\n'; elif [ -d "$1" ]; then printf 'dir %s\n' "$(stat -c '%u %g %a' "$1")"; elif [ -f "$1" ]; then printf 'file %s\n' "$(stat -c '%u %g %a' "$1")"; else printf 'file %s\n' "$(stat -c '%u %g %a' "$1")"; fi"#,
                "effigy-perm-inspect",
                path,
            ],
            "workspace path inspect",
        )?;
        parse_inspect_output(&output)
    }

    fn inspect_many(
        &mut self,
        paths: &[String],
    ) -> Result<Vec<PathInspection>, PermissionPrepError> {
        if paths.is_empty() {
            return Ok(Vec::new());
        }
        let mut argv = vec![
            "sh",
            "-c",
            DOCTOR_METADATA_BATCH_SCRIPT,
            "effigy-perm-inspect-batch",
        ];
        argv.extend(paths.iter().map(String::as_str));
        let context = paths
            .iter()
            .map(|path| format!("`{path}`"))
            .collect::<Vec<_>>()
            .join(", ");
        let output = self.exec_root(
            &argv,
            &format!("workspace ownership metadata batch for {context}"),
        )?;
        parse_inspect_batch_output(&output, paths.len())
    }

    fn find_unowned_many(
        &mut self,
        paths: &[String],
        uid: u32,
        gid: u32,
    ) -> Result<Vec<Option<String>>, PermissionPrepError> {
        if paths.is_empty() {
            return Ok(Vec::new());
        }
        let uid = uid.to_string();
        let gid = gid.to_string();
        let mut argv = vec![
            "sh",
            "-c",
            OWNERSHIP_FIND_BATCH_SCRIPT,
            "effigy-perm-find-batch",
        ];
        argv.extend([uid.as_str(), gid.as_str()]);
        argv.extend(paths.iter().map(String::as_str));
        let path_context = paths
            .iter()
            .map(|path| format!("`{path}`"))
            .collect::<Vec<_>>()
            .join(", ");
        let label = format!("workspace ownership bounded scan for {path_context}");
        let deadline = Some(match self.deadline {
            Some(outer) => outer.min(Instant::now() + self.bulk_timeout),
            None => Instant::now() + self.bulk_timeout,
        });
        let output = self.exec_as_user_until("0", &argv, &label, false, deadline)?;
        parse_unowned_batch_output(&output.stdout, paths)
    }

    fn mkdir_many(&mut self, paths: &[String]) -> Result<(), PermissionPrepError> {
        if paths.is_empty() {
            return Ok(());
        }
        let mut argv = vec![
            "sh",
            "-c",
            WORKSPACE_MKDIR_BATCH_SCRIPT,
            "effigy-perm-mkdir-batch",
        ];
        argv.extend(paths.iter().map(String::as_str));
        self.exec_root(&argv, "workspace mkdir batch")?;
        Ok(())
    }

    fn chown_shallow_many(
        &mut self,
        paths: &[String],
        uid: u32,
        gid: u32,
    ) -> Result<(), PermissionPrepError> {
        if paths.is_empty() {
            return Ok(());
        }
        let owner = format!("{uid}:{gid}");
        let mut argv = vec![
            "sh",
            "-c",
            WORKSPACE_CHOWN_SHALLOW_BATCH_SCRIPT,
            "effigy-perm-chown-batch",
            owner.as_str(),
        ];
        argv.extend(paths.iter().map(String::as_str));
        self.exec_root(&argv, "workspace shallow chown batch")?;
        Ok(())
    }

    fn chmod_owner_write_many(
        &mut self,
        paths: &[(String, bool)],
    ) -> Result<(), PermissionPrepError> {
        if paths.is_empty() {
            return Ok(());
        }
        let mut argv = vec![
            "sh",
            "-c",
            WORKSPACE_CHMOD_BATCH_SCRIPT,
            "effigy-perm-chmod-batch",
        ];
        for (path, directory) in paths {
            argv.push(path);
            argv.push(if *directory { "dir" } else { "file" });
        }
        self.exec_root(&argv, "workspace owner-write batch")?;
        Ok(())
    }

    fn probe_access_many(
        &mut self,
        identity: &ResolvedWorkspaceIdentity,
        paths: &[(String, bool)],
    ) -> Result<Vec<AccessProbe>, PermissionPrepError> {
        if paths.is_empty() {
            return Ok(Vec::new());
        }
        let mut argv = vec![
            "sh",
            "-c",
            WORKSPACE_ACCESS_BATCH_SCRIPT,
            "effigy-perm-access-batch",
        ];
        for (path, directory) in paths {
            argv.push(path);
            argv.push(if *directory { "yes" } else { "no" });
        }
        let context = paths
            .iter()
            .map(|(path, _)| format!("`{path}`"))
            .collect::<Vec<_>>()
            .join(", ");
        let label = format!(
            "workspace ownership numeric-user access batch (uid={} gid={}) for {context}",
            identity.uid, identity.gid
        );
        let output = self.exec_as(identity, &argv, &label, false)?;
        parse_workspace_access_batch_output(&output.stdout, paths.len())
    }

    fn mkdir_p(&mut self, path: &str) -> Result<(), PermissionPrepError> {
        self.exec_root(&["mkdir", "-p", "--", path], "workspace mkdir")?;
        Ok(())
    }

    fn chown(&mut self, path: &str, uid: u32, gid: u32) -> Result<(), PermissionPrepError> {
        let spec = format!("{uid}:{gid}");
        self.exec_root(&["chown", "-h", &spec, "--", path], "workspace chown")?;
        Ok(())
    }

    fn chown_tree_unowned(
        &mut self,
        path: &str,
        uid: u32,
        gid: u32,
    ) -> Result<(), PermissionPrepError> {
        let argv = bulk_chown_argv(path, uid, gid);
        let argv_refs: Vec<&str> = argv.iter().map(String::as_str).collect();
        let bulk_deadline = Instant::now() + self.bulk_timeout;
        let deadline = Some(match self.deadline {
            Some(outer) => outer.min(bulk_deadline),
            None => bulk_deadline,
        });
        let output =
            self.exec_as_user_until("0", &argv_refs, "workspace bulk chown", false, deadline)?;
        let _ = output;
        Ok(())
    }

    fn chmod_owner_write(
        &mut self,
        path: &str,
        directory: bool,
    ) -> Result<(), PermissionPrepError> {
        let mode = if directory { "u+wx" } else { "u+w" };
        self.exec_root(&["chmod", mode, "--", path], "workspace chmod")?;
        Ok(())
    }

    fn list_unowned(
        &self,
        path: &str,
        uid: u32,
        gid: u32,
    ) -> Result<Vec<String>, PermissionPrepError> {
        let uid = uid.to_string();
        let gid = gid.to_string();
        let output = self.exec_root(
            &[
                "find", "-P", path, "-xdev", "!", "-type", "l", "!", "(", "-user", &uid, "-a",
                "-group", &gid, ")", "-print",
            ],
            "workspace ownership find",
        )?;
        Ok(output
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect())
    }

    fn user_can_read_write(
        &self,
        identity: &ResolvedWorkspaceIdentity,
        path: &str,
    ) -> Result<bool, PermissionPrepError> {
        let output = self.exec_as(
            identity,
            &["test", "-r", path, "-a", "-w", path],
            "workspace access test",
            true,
        )?;
        Ok(output.status_success)
    }

    fn inspect_doctor_paths(
        &mut self,
        identity: &ResolvedWorkspaceIdentity,
        paths: &[String],
    ) -> Result<Vec<Vec<String>>, PermissionPrepError> {
        if paths.is_empty() {
            return Ok(Vec::new());
        }
        let path_context = paths
            .iter()
            .map(|path| format!("`{path}`"))
            .collect::<Vec<_>>()
            .join(", ");
        let metadata_label = format!("workspace ownership metadata batch for {path_context}");
        let mut metadata_argv = vec![
            "sh",
            "-c",
            DOCTOR_METADATA_BATCH_SCRIPT,
            "effigy-workspace-doctor-inspect-batch",
        ];
        metadata_argv.extend(paths.iter().map(String::as_str));
        let metadata_output = self.exec_root(&metadata_argv, &metadata_label)?;
        let inspections = parse_inspect_batch_output(&metadata_output, paths.len())?;

        let access_indexes = inspections
            .iter()
            .enumerate()
            .filter_map(|(index, inspection)| {
                (!matches!(
                    inspection.presence,
                    PathPresence::Missing | PathPresence::Symlink
                ))
                .then_some(index)
            })
            .collect::<Vec<_>>();
        let mut can_read_write = vec![None; paths.len()];
        if !access_indexes.is_empty() {
            let access_paths = access_indexes
                .iter()
                .map(|index| paths[*index].as_str())
                .collect::<Vec<_>>();
            let access_context = access_indexes
                .iter()
                .map(|index| format!("`{}`", paths[*index]))
                .collect::<Vec<_>>()
                .join(", ");
            let access_label = format!(
                "workspace ownership numeric-user access batch (uid={} gid={}) for {access_context}",
                identity.uid, identity.gid
            );
            let mut access_argv = vec![
                "sh",
                "-c",
                DOCTOR_ACCESS_BATCH_SCRIPT,
                "effigy-workspace-doctor-access-batch",
            ];
            access_argv.extend(access_paths);
            let output = self.exec_as(identity, &access_argv, &access_label, false)?;
            let results = parse_access_batch_output(&output.stdout, access_indexes.len())?;
            for (index, result) in access_indexes.into_iter().zip(results) {
                can_read_write[index] = Some(result);
            }
        }

        Ok(paths
            .iter()
            .enumerate()
            .map(|(index, path)| {
                doctor_path_samples(identity, path, &inspections[index], can_read_write[index])
            })
            .collect())
    }

    fn user_create_lock(
        &mut self,
        identity: &ResolvedWorkspaceIdentity,
        directory: &str,
    ) -> Result<(), PermissionPrepError> {
        let output = self.exec_as(
            identity,
            &[
                "sh",
                "-c",
                r#"touch "$1" && rm -f "$1""#,
                "effigy-perm-lock",
                &format!("{directory}/.effigy-write-probe"),
            ],
            "workspace lock probe",
            true,
        )?;
        if output.status_success {
            return Ok(());
        }
        Err(PermissionPrepError::new(
            output
                .stderr
                .trim()
                .split('\n')
                .next()
                .filter(|line| !line.is_empty())
                .unwrap_or("permission denied")
                .to_owned(),
        ))
    }
}

struct ExecOutput {
    stdout: String,
    stderr: String,
    status_success: bool,
}

impl ComposeAccessBackend<'_> {
    fn exec_root(&self, argv: &[&str], label: &str) -> Result<String, PermissionPrepError> {
        let output = self.exec_as_user("0", argv, label, false)?;
        if output.status_success {
            return Ok(output.stdout);
        }
        Err(PermissionPrepError::new(format!(
            "{label} failed: {}",
            output.stderr.trim()
        )))
    }

    fn exec_as(
        &self,
        identity: &ResolvedWorkspaceIdentity,
        argv: &[&str],
        label: &str,
        allow_failure: bool,
    ) -> Result<ExecOutput, PermissionPrepError> {
        self.exec_as_user(
            &format!("{}:{}", identity.uid, identity.gid),
            argv,
            label,
            allow_failure,
        )
    }

    fn exec_as_user(
        &self,
        user: &str,
        argv: &[&str],
        label: &str,
        allow_failure: bool,
    ) -> Result<ExecOutput, PermissionPrepError> {
        self.exec_as_user_until(user, argv, label, allow_failure, self.deadline)
    }

    fn exec_as_user_until(
        &self,
        user: &str,
        argv: &[&str],
        label: &str,
        allow_failure: bool,
        deadline: Option<Instant>,
    ) -> Result<ExecOutput, PermissionPrepError> {
        let service = self.policy.primary_service.as_str();
        let mut args = effigy_containers::compose::compose_args(
            self.policy,
            ["exec", "-T", "-u", user, service],
        );
        args.extend(argv.iter().map(OsString::from));
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(PermissionPrepError::new(format!("{label} timed out")));
        }
        let output = run_compose_exec_with_deadline(
            self.repo_root,
            self.policy,
            &args,
            true,
            label,
            deadline,
        )
        .map_err(|error| PermissionPrepError::new(format!("{label}: {error}")))?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let success = output.status.success();
        if !success && !allow_failure {
            return Err(PermissionPrepError::new(format!(
                "{label} failed: {}",
                stderr.trim()
            )));
        }
        Ok(ExecOutput {
            stdout,
            stderr,
            status_success: success,
        })
    }
}

/// Argument array for the single bulk repair exec. `-execdir` makes `chown` run
/// from a directory fd held by `find` on `./name`, so a path component swapped
/// for a symlink after traversal cannot redirect ownership changes outside the
/// volume. Requires GNU findutils; an image without `-execdir` fails not-ready.
fn bulk_chown_argv(path: &str, uid: u32, gid: u32) -> Vec<String> {
    let spec = format!("{uid}:{gid}");
    let uid = uid.to_string();
    let gid = gid.to_string();
    [
        "find", "-P", path, "-xdev", "!", "-type", "l", "!", "(", "-user", &uid, "-a", "-group",
        &gid, ")", "-execdir", "chown", "-h", &spec, "--", "{}", "+",
    ]
    .iter()
    .map(|part| (*part).to_owned())
    .collect()
}

fn parse_id_output(raw: &str) -> Result<u32, PermissionPrepError> {
    raw.trim().parse::<u32>().map_err(|_| {
        PermissionPrepError::new(format!(
            "workspace identity probe returned non-numeric id `{raw}`"
        ))
    })
}

fn parse_inspect_batch_output(
    raw: &str,
    expected: usize,
) -> Result<Vec<PathInspection>, PermissionPrepError> {
    let lines = raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if lines.len() != expected {
        return Err(PermissionPrepError::new(format!(
            "workspace ownership metadata batch returned {} records for {expected} paths",
            lines.len()
        )));
    }
    lines.into_iter().map(parse_doctor_inspect_record).collect()
}

fn parse_doctor_inspect_record(raw: &str) -> Result<PathInspection, PermissionPrepError> {
    match raw {
        "missing" => {
            return Ok(PathInspection {
                presence: PathPresence::Missing,
                uid: None,
                gid: None,
                mode: None,
            });
        }
        "symlink" => {
            return Ok(PathInspection {
                presence: PathPresence::Symlink,
                uid: None,
                gid: None,
                mode: None,
            });
        }
        _ => {}
    }
    let mut parts = raw.split_whitespace();
    let kind = parts.next().unwrap_or_default();
    let presence = match kind {
        "dir" => PathPresence::Directory,
        "file" => PathPresence::File,
        _ => {
            return Err(PermissionPrepError::new(format!(
                "workspace ownership metadata batch returned invalid record `{raw}`"
            )));
        }
    };
    let uid = parts.next().and_then(|value| value.parse::<u32>().ok());
    let gid = parts.next().and_then(|value| value.parse::<u32>().ok());
    let mode = parts
        .next()
        .and_then(|value| u32::from_str_radix(value, 8).ok());
    if uid.is_none() || gid.is_none() || mode.is_none() || parts.next().is_some() {
        return Err(PermissionPrepError::new(format!(
            "workspace ownership metadata batch returned invalid record `{raw}`"
        )));
    }
    Ok(PathInspection {
        presence,
        uid,
        gid,
        mode,
    })
}

fn parse_access_batch_output(raw: &str, expected: usize) -> Result<Vec<bool>, PermissionPrepError> {
    let lines = raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if lines.len() != expected {
        return Err(PermissionPrepError::new(format!(
            "workspace ownership numeric-user access batch returned {} records for {expected} paths",
            lines.len()
        )));
    }
    lines
        .into_iter()
        .map(|line| match line {
            "read-write" => Ok(true),
            "unwritable" => Ok(false),
            _ => Err(PermissionPrepError::new(format!(
                "workspace ownership numeric-user access batch returned invalid record `{line}`"
            ))),
        })
        .collect()
}

fn parse_unowned_batch_output(
    raw: &str,
    paths: &[String],
) -> Result<Vec<Option<String>>, PermissionPrepError> {
    let lines = raw.lines().map(str::trim_end).collect::<Vec<_>>();
    if lines.len() != paths.len() {
        return Err(PermissionPrepError::new(format!(
            "workspace ownership scan returned {} records for {} paths",
            lines.len(),
            paths.len()
        )));
    }
    lines
        .into_iter()
        .zip(paths)
        .map(|(line, path)| {
            if line == "clean" {
                return Ok(None);
            }
            if line == "symlink" {
                return Err(PermissionPrepError::new(format!(
                    "workspace ownership scope `{path}` changed to a symlink during verification"
                )));
            }
            if let Some(first) = line.strip_prefix("dirty\t") {
                if first.is_empty() {
                    return Err(PermissionPrepError::new(format!(
                        "workspace ownership scan returned an empty finding for `{path}`"
                    )));
                }
                return Ok(Some(first.to_owned()));
            }
            Err(PermissionPrepError::new(format!(
                "workspace ownership scan returned invalid record `{line}` for `{path}`"
            )))
        })
        .collect()
}

fn parse_workspace_access_batch_output(
    raw: &str,
    expected: usize,
) -> Result<Vec<AccessProbe>, PermissionPrepError> {
    let lines = raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if lines.len() != expected {
        return Err(PermissionPrepError::new(format!(
            "workspace ownership numeric-user access batch returned {} records for {expected} paths",
            lines.len()
        )));
    }
    lines
        .into_iter()
        .map(|line| match line {
            "ready" => Ok(AccessProbe::Ready),
            "unwritable" => Ok(AccessProbe::Unwritable),
            "lock-failed" => Ok(AccessProbe::LockFailed),
            "missing" => Ok(AccessProbe::Missing),
            "symlink" => Ok(AccessProbe::Symlink),
            _ => Err(PermissionPrepError::new(format!(
                "workspace ownership numeric-user access batch returned invalid record `{line}`"
            ))),
        })
        .collect()
}

fn parse_inspect_output(raw: &str) -> Result<PathInspection, PermissionPrepError> {
    let line = raw.lines().next().unwrap_or("").trim();
    if line == "missing" {
        return Ok(PathInspection {
            presence: PathPresence::Missing,
            uid: None,
            gid: None,
            mode: None,
        });
    }
    if line == "symlink" {
        return Ok(PathInspection {
            presence: PathPresence::Symlink,
            uid: None,
            gid: None,
            mode: None,
        });
    }
    let mut parts = line.split_whitespace();
    let kind = parts.next().unwrap_or("");
    let uid = parts.next().and_then(|value| value.parse().ok());
    let gid = parts.next().and_then(|value| value.parse().ok());
    let mode = parts
        .next()
        .and_then(|value| u32::from_str_radix(value, 8).ok());
    let presence = if kind == "dir" {
        PathPresence::Directory
    } else {
        PathPresence::File
    };
    Ok(PathInspection {
        presence,
        uid,
        gid,
        mode,
    })
}

#[cfg(test)]
mod memory_backend {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::collections::{BTreeMap, BTreeSet};

    #[derive(Debug, Clone)]
    struct MemoryNode {
        uid: u32,
        gid: u32,
        mode: u32,
        kind: MemoryKind,
    }

    #[derive(Debug, Clone)]
    enum MemoryKind {
        File,
        Directory {
            children: BTreeMap<String, MemoryNode>,
        },
        Symlink {
            #[allow(dead_code)]
            target: String,
        },
    }

    #[derive(Debug, Clone)]
    pub(super) struct MemoryAccessBackend {
        identities: BTreeMap<String, ResolvedWorkspaceIdentity>,
        root: MemoryNode,
        pub chown_log: Vec<String>,
        pub chmod_log: Vec<String>,
        pub created_locks: Vec<String>,
        pub inspect_log: RefCell<Vec<String>>,
        pub list_unowned_calls: Cell<usize>,
        /// Every trait call is one simulated runtime round trip.
        pub exec_calls: Cell<usize>,
        pub bulk_calls: usize,
        pub bulk_fail: Option<String>,
        /// Fails the bulk operation at this path after partial progress.
        pub bulk_fail_at: Option<String>,
        /// Replaces the bulk scope with a symlink mid-operation.
        pub bulk_swap_scope_to_symlink: bool,
        protected: BTreeSet<String>,
        foreign: BTreeSet<String>,
        read_only: BTreeSet<String>,
    }

    impl MemoryAccessBackend {
        pub(super) fn new(identities: Vec<ResolvedWorkspaceIdentity>) -> Self {
            let mut map = BTreeMap::new();
            for identity in identities {
                map.insert(identity.user.clone(), identity);
            }
            Self {
                identities: map,
                root: MemoryNode {
                    uid: 0,
                    gid: 0,
                    mode: 0o755,
                    kind: MemoryKind::Directory {
                        children: BTreeMap::new(),
                    },
                },
                chown_log: Vec::new(),
                chmod_log: Vec::new(),
                created_locks: Vec::new(),
                inspect_log: RefCell::new(Vec::new()),
                list_unowned_calls: Cell::new(0),
                exec_calls: Cell::new(0),
                bulk_calls: 0,
                bulk_fail: None,
                bulk_fail_at: None,
                bulk_swap_scope_to_symlink: false,
                protected: BTreeSet::new(),
                foreign: BTreeSet::new(),
                read_only: BTreeSet::new(),
            }
        }

        pub(super) fn protect(&mut self, path: impl Into<String>) {
            self.protected.insert(path.into());
        }

        pub(super) fn mark_foreign(&mut self, path: impl Into<String>) {
            self.foreign.insert(path.into());
        }

        pub(super) fn mark_read_only(&mut self, path: impl Into<String>) {
            self.read_only.insert(path.into());
        }

        pub(super) fn add_dir(&mut self, path: &str, uid: u32, gid: u32, mode: u32) {
            self.insert(
                path,
                MemoryNode {
                    uid,
                    gid,
                    mode,
                    kind: MemoryKind::Directory {
                        children: BTreeMap::new(),
                    },
                },
                true,
            );
        }

        pub(super) fn add_file(&mut self, path: &str, uid: u32, gid: u32, mode: u32) {
            self.insert(
                path,
                MemoryNode {
                    uid,
                    gid,
                    mode,
                    kind: MemoryKind::File,
                },
                true,
            );
        }

        pub(super) fn add_symlink(&mut self, path: &str, target: &str) {
            self.insert(
                path,
                MemoryNode {
                    uid: 0,
                    gid: 0,
                    mode: 0o777,
                    kind: MemoryKind::Symlink {
                        target: target.to_owned(),
                    },
                },
                true,
            );
        }

        pub(super) fn owner_of(&self, path: &str) -> Option<(u32, u32)> {
            self.node(path).map(|node| (node.uid, node.gid))
        }

        pub(super) fn mode_of(&self, path: &str) -> Option<u32> {
            self.node(path).map(|node| node.mode)
        }

        fn insert(&mut self, path: &str, node: MemoryNode, merge_dirs: bool) {
            let parts = split_path(path);
            insert_node(&mut self.root, &parts, node, merge_dirs);
        }

        fn node(&self, path: &str) -> Option<&MemoryNode> {
            let parts = split_path(path);
            walk(&self.root, &parts)
        }

        fn node_mut(&mut self, path: &str) -> Option<&mut MemoryNode> {
            let parts = split_path(path);
            walk_mut(&mut self.root, &parts)
        }

        fn inspect_path(&self, path: &str) -> PathInspection {
            let Some(node) = self.node(path) else {
                return PathInspection {
                    presence: PathPresence::Missing,
                    uid: None,
                    gid: None,
                    mode: None,
                };
            };
            let presence = match node.kind {
                MemoryKind::Directory { .. } => PathPresence::Directory,
                MemoryKind::File => PathPresence::File,
                MemoryKind::Symlink { .. } => PathPresence::Symlink,
            };
            PathInspection {
                presence,
                uid: Some(node.uid),
                gid: Some(node.gid),
                mode: Some(node.mode),
            }
        }

        fn mkdir_path(&mut self, path: &str) -> Result<(), PermissionPrepError> {
            self.refuse_mutation(path)?;
            match self.inspect_path(path).presence {
                PathPresence::Directory => Ok(()),
                PathPresence::Symlink => Err(PermissionPrepError::new(format!(
                    "refusing to mkdir through symlink `{path}`"
                ))),
                PathPresence::File => Err(PermissionPrepError::new(format!(
                    "refusing to mkdir over file `{path}`"
                ))),
                PathPresence::Missing => {
                    self.add_dir(path, 0, 0, 0o755);
                    Ok(())
                }
            }
        }

        fn refuse_mutation(&self, path: &str) -> Result<(), PermissionPrepError> {
            if self
                .protected
                .iter()
                .any(|prefix| path == prefix || path_is_under(path, prefix))
            {
                return Err(PermissionPrepError::new(format!(
                    "refusing to mutate protected path `{path}`"
                )));
            }
            if self.foreign.contains(path) {
                return Err(PermissionPrepError::new(format!(
                    "refusing to mutate foreign path `{path}`"
                )));
            }
            if self.read_only.contains(path) {
                return Err(PermissionPrepError::new(format!(
                    "read-only mount `{path}`"
                )));
            }
            Ok(())
        }
    }

    fn split_path(path: &str) -> Vec<String> {
        path.trim_end_matches('/')
            .split('/')
            .filter(|part| !part.is_empty())
            .map(str::to_owned)
            .collect()
    }

    fn insert_node(root: &mut MemoryNode, parts: &[String], node: MemoryNode, merge_dirs: bool) {
        if parts.is_empty() {
            return;
        }
        let MemoryKind::Directory { children } = &mut root.kind else {
            return;
        };
        if parts.len() == 1 {
            if merge_dirs {
                if let (
                    Some(existing),
                    MemoryKind::Directory {
                        children: incoming_children,
                    },
                ) = (children.get_mut(&parts[0]), &node.kind)
                {
                    if let MemoryKind::Directory {
                        children: existing_children,
                    } = &mut existing.kind
                    {
                        existing.uid = node.uid;
                        existing.gid = node.gid;
                        existing.mode = node.mode;
                        for (name, child) in incoming_children {
                            existing_children
                                .entry(name.clone())
                                .or_insert_with(|| child.clone());
                        }
                        return;
                    }
                }
            }
            children.insert(parts[0].clone(), node);
            return;
        }
        let child = children
            .entry(parts[0].clone())
            .or_insert_with(|| MemoryNode {
                uid: root.uid,
                gid: root.gid,
                mode: 0o755,
                kind: MemoryKind::Directory {
                    children: BTreeMap::new(),
                },
            });
        insert_node(child, &parts[1..], node, merge_dirs);
    }

    fn walk<'a>(node: &'a MemoryNode, parts: &[String]) -> Option<&'a MemoryNode> {
        if parts.is_empty() {
            return Some(node);
        }
        match &node.kind {
            MemoryKind::Directory { children } => walk(children.get(&parts[0])?, &parts[1..]),
            MemoryKind::Symlink { .. } | MemoryKind::File => None,
        }
    }

    fn walk_mut<'a>(node: &'a mut MemoryNode, parts: &[String]) -> Option<&'a mut MemoryNode> {
        if parts.is_empty() {
            return Some(node);
        }
        match &mut node.kind {
            MemoryKind::Directory { children } => {
                walk_mut(children.get_mut(&parts[0])?, &parts[1..])
            }
            MemoryKind::Symlink { .. } | MemoryKind::File => None,
        }
    }

    impl WorkspaceAccessBackend for MemoryAccessBackend {
        fn resolve_identity(
            &self,
            user: &str,
        ) -> Result<ResolvedWorkspaceIdentity, PermissionPrepError> {
            self.exec_calls.set(self.exec_calls.get() + 1);
            self.identities.get(user).cloned().ok_or_else(|| {
                PermissionPrepError::new(format!("workspace user `{user}` is not present"))
            })
        }

        fn inspect(&self, path: &str) -> Result<PathInspection, PermissionPrepError> {
            self.inspect_log.borrow_mut().push(path.to_owned());
            self.exec_calls.set(self.exec_calls.get() + 1);
            Ok(self.inspect_path(path))
        }

        fn inspect_many(
            &mut self,
            paths: &[String],
        ) -> Result<Vec<PathInspection>, PermissionPrepError> {
            self.exec_calls.set(self.exec_calls.get() + 1);
            self.inspect_log.borrow_mut().extend(paths.iter().cloned());
            Ok(paths.iter().map(|path| self.inspect_path(path)).collect())
        }

        fn mkdir_p(&mut self, path: &str) -> Result<(), PermissionPrepError> {
            self.exec_calls.set(self.exec_calls.get() + 1);
            self.mkdir_path(path)
        }

        fn mkdir_many(&mut self, paths: &[String]) -> Result<(), PermissionPrepError> {
            if paths.is_empty() {
                return Ok(());
            }
            self.exec_calls.set(self.exec_calls.get() + 1);
            for path in paths {
                self.mkdir_path(path)?;
            }
            Ok(())
        }

        fn chown_tree_unowned(
            &mut self,
            path: &str,
            uid: u32,
            gid: u32,
        ) -> Result<(), PermissionPrepError> {
            self.exec_calls.set(self.exec_calls.get() + 1);
            self.bulk_calls += 1;
            if let Some(message) = self.bulk_fail.clone() {
                return Err(PermissionPrepError::new(message));
            }
            let mut targets = Vec::new();
            collect_unowned(self.node(path), path, uid, gid, &mut targets);
            for target in &targets {
                self.refuse_mutation(target)?;
            }
            for target in &targets {
                if self.bulk_fail_at.as_deref() == Some(target.as_str()) {
                    return Err(PermissionPrepError::new(format!(
                        "workspace bulk chown failed: chown: cannot access '{target}': Permission denied"
                    )));
                }
                if let Some(node) = self.node_mut(target) {
                    node.uid = uid;
                    node.gid = gid;
                }
                self.chown_log.push(target.clone());
            }
            if self.bulk_swap_scope_to_symlink {
                self.add_symlink(path, "/etc");
            }
            Ok(())
        }

        fn chown(&mut self, path: &str, uid: u32, gid: u32) -> Result<(), PermissionPrepError> {
            self.exec_calls.set(self.exec_calls.get() + 1);
            self.refuse_mutation(path)?;
            let node = self.node_mut(path).ok_or_else(|| {
                PermissionPrepError::new(format!("chown target `{path}` is missing"))
            })?;
            if matches!(node.kind, MemoryKind::Symlink { .. }) {
                return Err(PermissionPrepError::new(format!(
                    "refusing to chown symlink `{path}`"
                )));
            }
            node.uid = uid;
            node.gid = gid;
            self.chown_log.push(path.to_owned());
            Ok(())
        }

        fn chown_shallow_many(
            &mut self,
            paths: &[String],
            uid: u32,
            gid: u32,
        ) -> Result<(), PermissionPrepError> {
            if paths.is_empty() {
                return Ok(());
            }
            self.exec_calls.set(self.exec_calls.get() + 1);
            for path in paths {
                self.refuse_mutation(path)?;
                let node = self.node_mut(path).ok_or_else(|| {
                    PermissionPrepError::new(format!("chown target `{path}` is missing"))
                })?;
                if matches!(node.kind, MemoryKind::Symlink { .. }) {
                    return Err(PermissionPrepError::new(format!(
                        "refusing to chown symlink `{path}`"
                    )));
                }
                node.uid = uid;
                node.gid = gid;
                self.chown_log.push(path.clone());
            }
            Ok(())
        }

        fn chmod_owner_write(
            &mut self,
            path: &str,
            directory: bool,
        ) -> Result<(), PermissionPrepError> {
            self.refuse_mutation(path)?;
            let node = self.node_mut(path).ok_or_else(|| {
                PermissionPrepError::new(format!("chmod target `{path}` is missing"))
            })?;
            node.mode |= if directory { 0o300 } else { 0o200 };
            self.chmod_log.push(path.to_owned());
            Ok(())
        }

        fn chmod_owner_write_many(
            &mut self,
            paths: &[(String, bool)],
        ) -> Result<(), PermissionPrepError> {
            if paths.is_empty() {
                return Ok(());
            }
            self.exec_calls.set(self.exec_calls.get() + 1);
            for (path, directory) in paths {
                self.refuse_mutation(path)?;
                let node = self.node_mut(path).ok_or_else(|| {
                    PermissionPrepError::new(format!("chmod target `{path}` is missing"))
                })?;
                node.mode |= if *directory { 0o300 } else { 0o200 };
                self.chmod_log.push(path.clone());
            }
            Ok(())
        }

        fn list_unowned(
            &self,
            path: &str,
            uid: u32,
            gid: u32,
        ) -> Result<Vec<String>, PermissionPrepError> {
            self.list_unowned_calls
                .set(self.list_unowned_calls.get() + 1);
            self.exec_calls.set(self.exec_calls.get() + 1);
            let mut out = Vec::new();
            collect_unowned(self.node(path), path, uid, gid, &mut out);
            Ok(out)
        }

        fn find_unowned_many(
            &mut self,
            paths: &[String],
            uid: u32,
            gid: u32,
        ) -> Result<Vec<Option<String>>, PermissionPrepError> {
            if paths.is_empty() {
                return Ok(Vec::new());
            }
            self.list_unowned_calls
                .set(self.list_unowned_calls.get() + 1);
            self.exec_calls.set(self.exec_calls.get() + 1);
            paths
                .iter()
                .map(|path| {
                    let mut entries = Vec::new();
                    collect_unowned(self.node(path), path, uid, gid, &mut entries);
                    Ok(entries.into_iter().next())
                })
                .collect()
        }

        fn user_can_read_write(
            &self,
            identity: &ResolvedWorkspaceIdentity,
            path: &str,
        ) -> Result<bool, PermissionPrepError> {
            let Some(node) = self.node(path) else {
                return Ok(false);
            };
            Ok(mode_allows(node, identity, true, true))
        }

        fn user_create_lock(
            &mut self,
            identity: &ResolvedWorkspaceIdentity,
            directory: &str,
        ) -> Result<(), PermissionPrepError> {
            let Some(node) = self.node(directory) else {
                return Err(PermissionPrepError::new(format!(
                    "lock directory `{directory}` is missing"
                )));
            };
            if !matches!(node.kind, MemoryKind::Directory { .. }) {
                return Err(PermissionPrepError::new(format!(
                    "`{directory}` is not a directory"
                )));
            }
            if !mode_allows(node, identity, true, true) || !mode_allows_exec(node, identity) {
                return Err(PermissionPrepError::new("permission denied".to_owned()));
            }
            let lock = format!("{directory}/.effigy-write-probe");
            if self.protected.contains(&lock) {
                return Err(PermissionPrepError::new(format!(
                    "refusing to mutate protected path `{lock}`"
                )));
            }
            self.add_file(&lock, identity.uid, identity.gid, 0o644);
            self.created_locks.push(lock.clone());
            if let Some(MemoryKind::Directory { children }) =
                self.node_mut(directory).map(|node| &mut node.kind)
            {
                children.remove(".effigy-write-probe");
            }
            Ok(())
        }

        fn probe_access_many(
            &mut self,
            identity: &ResolvedWorkspaceIdentity,
            paths: &[(String, bool)],
        ) -> Result<Vec<AccessProbe>, PermissionPrepError> {
            if paths.is_empty() {
                return Ok(Vec::new());
            }
            self.exec_calls.set(self.exec_calls.get() + 1);
            paths
                .iter()
                .map(|(path, directory)| {
                    let inspection = self.inspect_path(path);
                    if inspection.presence == PathPresence::Symlink {
                        return Ok(AccessProbe::Symlink);
                    }
                    if inspection.presence == PathPresence::Missing {
                        return Ok(AccessProbe::Missing);
                    }
                    if !self.user_can_read_write(identity, path)? {
                        return Ok(AccessProbe::Unwritable);
                    }
                    if *directory {
                        return Ok(if self.user_create_lock(identity, path).is_ok() {
                            AccessProbe::Ready
                        } else {
                            AccessProbe::LockFailed
                        });
                    }
                    Ok(AccessProbe::Ready)
                })
                .collect()
        }
    }

    fn collect_unowned(
        node: Option<&MemoryNode>,
        path: &str,
        uid: u32,
        gid: u32,
        out: &mut Vec<String>,
    ) {
        let Some(node) = node else {
            return;
        };
        if matches!(node.kind, MemoryKind::Symlink { .. }) {
            return;
        }
        if node.uid != uid || node.gid != gid {
            out.push(path.to_owned());
        }
        if let MemoryKind::Directory { children } = &node.kind {
            for (name, child) in children {
                if matches!(child.kind, MemoryKind::Symlink { .. }) {
                    continue;
                }
                let child_path = if path == "/" {
                    format!("/{name}")
                } else {
                    format!("{path}/{name}")
                };
                collect_unowned(Some(child), &child_path, uid, gid, out);
            }
        }
    }

    fn mode_class_bits(node: &MemoryNode, identity: &ResolvedWorkspaceIdentity) -> u32 {
        if identity.uid == node.uid {
            (node.mode >> 6) & 0o7
        } else if identity.gid == node.gid {
            (node.mode >> 3) & 0o7
        } else {
            node.mode & 0o7
        }
    }

    fn mode_allows(
        node: &MemoryNode,
        identity: &ResolvedWorkspaceIdentity,
        read: bool,
        write: bool,
    ) -> bool {
        let bits = mode_class_bits(node, identity);
        (!read || bits & 0o4 != 0) && (!write || bits & 0o2 != 0)
    }

    fn mode_allows_exec(node: &MemoryNode, identity: &ResolvedWorkspaceIdentity) -> bool {
        mode_class_bits(node, identity) & 0o1 != 0
    }
}

#[cfg(test)]
use memory_backend::MemoryAccessBackend;

pub(in crate::runner) fn compose_backend<'a>(
    repo_root: &'a Path,
    policy: &'a EffectiveContainerPolicy,
) -> ComposeAccessBackend<'a> {
    compose_backend_with_deadline(repo_root, policy, None)
}

pub(in crate::runner) fn compose_backend_with_deadline<'a>(
    repo_root: &'a Path,
    policy: &'a EffectiveContainerPolicy,
    deadline: Option<Instant>,
) -> ComposeAccessBackend<'a> {
    ComposeAccessBackend {
        repo_root,
        policy,
        deadline,
        bulk_timeout: BULK_CHOWN_TIMEOUT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::test_support::effective_container_policy;
    use effigy_catalog::volumes::ManagedVolume;
    use effigy_containers::WorkspaceOwnershipTarget;

    fn identity(user: &str, uid: u32, gid: u32) -> ResolvedWorkspaceIdentity {
        ResolvedWorkspaceIdentity {
            user: user.to_owned(),
            uid,
            gid,
        }
    }

    fn context() -> PermissionPrepContext {
        PermissionPrepContext {
            profile: "effigy".to_owned(),
            project_name: "acowtancy-dev".to_owned(),
            container_name: "web".to_owned(),
            service: "workspace".to_owned(),
        }
    }

    fn owned_target(path: &str, rust: Option<WorkspaceRustCacheKind>) -> WorkspaceOwnershipTarget {
        WorkspaceOwnershipTarget {
            path: path.to_owned(),
            mount_kind: WorkspaceMountKind::NamedVolume,
            source: Some(format!(
                "vol-{}",
                path.trim_start_matches('/').replace('/', "-")
            )),
            repair_authority: WorkspaceRepairAuthority::OwnedDisposable,
            rust_cache: rust,
            read_only: false,
        }
    }

    fn verify_target_spec(
        path: &str,
        rust: Option<WorkspaceRustCacheKind>,
    ) -> WorkspaceOwnershipTarget {
        WorkspaceOwnershipTarget {
            path: path.to_owned(),
            mount_kind: WorkspaceMountKind::Bind,
            source: Some("/Users/tom/src/app/target".to_owned()),
            repair_authority: WorkspaceRepairAuthority::VerifyOnly,
            rust_cache: rust,
            read_only: false,
        }
    }

    #[test]
    fn nested_unwritable_lock_on_owned_volume_is_repaired_for_uid_501() {
        let mut backend = MemoryAccessBackend::new(vec![identity("dev", 501, 20)]);
        backend.add_dir("/workspace/target", 501, 20, 0o755);
        backend.add_dir("/workspace/target/debug", 501, 20, 0o755);
        backend.add_file("/workspace/target/debug/.cargo-build-lock", 0, 0, 0o644);
        backend.add_file("/workspace/src/main.rs", 501, 20, 0o644);
        backend.protect("/workspace/src/main.rs");

        let plan = WorkspaceOwnershipPlan {
            targets: vec![owned_target(
                "/workspace/target",
                Some(WorkspaceRustCacheKind::RustTarget),
            )],
        };
        prepare_workspace_permissions("dev", &plan, None, &context(), &mut backend).expect("prep");

        assert_eq!(
            backend.owner_of("/workspace/target/debug/.cargo-build-lock"),
            Some((501, 20))
        );
        assert_eq!(backend.owner_of("/workspace/src/main.rs"), Some((501, 20)));
        assert!(backend
            .created_locks
            .iter()
            .any(|path| path == "/workspace/target/debug/.effigy-write-probe"));
        assert!(!backend
            .chown_log
            .iter()
            .any(|path| path.contains("main.rs")));
    }

    #[test]
    fn cargo_git_and_registry_src_are_repaired_and_verified() {
        let mut backend = MemoryAccessBackend::new(vec![identity("dev", 501, 20)]);
        backend.add_dir("/usr/local/cargo/registry", 0, 0, 0o755);
        backend.add_dir("/usr/local/cargo/registry/src", 0, 0, 0o755);
        backend.add_dir("/usr/local/cargo/git", 0, 0, 0o755);
        backend.add_dir("/usr/local/cargo/git/checkouts", 0, 0, 0o755);
        backend.add_dir("/usr/local/cargo/git/checkouts/foo", 0, 0, 0o755);

        let plan = WorkspaceOwnershipPlan {
            targets: vec![
                owned_target(
                    "/usr/local/cargo/registry",
                    Some(WorkspaceRustCacheKind::CargoRegistry),
                ),
                owned_target(
                    "/usr/local/cargo/git",
                    Some(WorkspaceRustCacheKind::CargoGit),
                ),
            ],
        };
        prepare_workspace_permissions("dev", &plan, None, &context(), &mut backend).expect("prep");
        assert_eq!(
            backend.owner_of("/usr/local/cargo/registry/src"),
            Some((501, 20))
        );
        assert_eq!(
            backend.owner_of("/usr/local/cargo/git/checkouts/foo"),
            Some((501, 20))
        );
    }

    #[test]
    fn already_correct_ownership_is_idempotent() {
        let mut backend = MemoryAccessBackend::new(vec![identity("dev", 501, 20)]);
        backend.add_dir("/usr/local/cargo/registry", 501, 20, 0o755);
        backend.add_dir("/usr/local/cargo/registry/src", 501, 20, 0o755);
        let plan = WorkspaceOwnershipPlan {
            targets: vec![owned_target(
                "/usr/local/cargo/registry",
                Some(WorkspaceRustCacheKind::CargoRegistry),
            )],
        };
        prepare_workspace_permissions("dev", &plan, None, &context(), &mut backend).expect("prep");
        assert!(backend.chown_log.is_empty());
    }

    #[test]
    fn exec_preparation_batches_three_clean_mounts_and_skips_repeat_repairs() {
        let mut backend = MemoryAccessBackend::new(vec![identity("dev", 501, 20)]);
        backend.add_dir("/cargo/registry", 501, 20, 0o755);
        backend.add_dir("/cargo/registry/src", 501, 20, 0o755);
        backend.add_dir("/cargo/git", 501, 20, 0o755);
        backend.add_dir("/cargo/git/checkouts", 501, 20, 0o755);
        backend.add_dir("/workspace/target", 501, 20, 0o755);
        backend.add_dir("/workspace/target/debug", 501, 20, 0o755);
        backend.add_file("/workspace/target/debug/.cargo-build-lock", 501, 20, 0o644);
        let plan = WorkspaceOwnershipPlan {
            targets: vec![
                owned_target(
                    "/cargo/registry",
                    Some(WorkspaceRustCacheKind::CargoRegistry),
                ),
                owned_target("/cargo/git", Some(WorkspaceRustCacheKind::CargoGit)),
                owned_target(
                    "/workspace/target",
                    Some(WorkspaceRustCacheKind::RustTarget),
                ),
            ],
        };

        prepare_workspace_permissions("dev", &plan, None, &context(), &mut backend)
            .expect("first prep");
        let first_calls = backend.exec_calls.get();
        assert_eq!(
            backend.bulk_calls, 0,
            "clean paths need no ownership repair"
        );
        assert!(
            first_calls <= 5,
            "three mounts use bounded batches: {first_calls}"
        );

        prepare_workspace_permissions("dev", &plan, None, &context(), &mut backend)
            .expect("repeat prep");
        assert_eq!(backend.exec_calls.get() - first_calls, first_calls);
        assert_eq!(backend.bulk_calls, 0, "unchanged paths launch no repair");
        assert!(backend.chown_log.is_empty());
    }

    #[test]
    fn exec_preparation_rechecks_nested_permission_changes_without_a_cache() {
        let mut backend = MemoryAccessBackend::new(vec![identity("dev", 501, 20)]);
        backend.add_dir("/workspace/target", 501, 20, 0o755);
        backend.add_dir("/workspace/target/debug", 501, 20, 0o755);
        backend.add_file("/workspace/target/debug/.cargo-build-lock", 501, 20, 0o644);
        let plan = WorkspaceOwnershipPlan {
            targets: vec![owned_target(
                "/workspace/target",
                Some(WorkspaceRustCacheKind::RustTarget),
            )],
        };
        prepare_workspace_permissions("dev", &plan, None, &context(), &mut backend)
            .expect("first prep");

        backend.add_file("/workspace/target/debug/.cargo-build-lock", 0, 0, 0o444);
        prepare_workspace_permissions("dev", &plan, None, &context(), &mut backend)
            .expect("nested ownership and mode drift is repaired");
        assert_eq!(
            backend.owner_of("/workspace/target/debug/.cargo-build-lock"),
            Some((501, 20))
        );
        assert_eq!(
            backend.mode_of("/workspace/target/debug/.cargo-build-lock"),
            Some(0o644)
        );
        assert_eq!(backend.bulk_calls, 1);
    }

    #[test]
    fn exec_preparation_rechecks_external_bind_access_on_each_call() {
        let mut backend = MemoryAccessBackend::new(vec![identity("dev", 501, 20)]);
        backend.add_dir("/workspace/target", 501, 20, 0o755);
        backend.add_dir("/workspace/target/debug", 501, 20, 0o755);
        backend.add_file("/workspace/target/debug/.cargo-build-lock", 501, 20, 0o644);
        backend.protect("/workspace/target");
        let plan = WorkspaceOwnershipPlan {
            targets: vec![verify_target_spec(
                "/workspace/target",
                Some(WorkspaceRustCacheKind::RustTarget),
            )],
        };
        prepare_workspace_permissions("dev", &plan, None, &context(), &mut backend)
            .expect("first prep");

        backend.add_file("/workspace/target/debug/.cargo-build-lock", 501, 20, 0o444);
        let error = prepare_workspace_permissions("dev", &plan, None, &context(), &mut backend)
            .expect_err("changed bind permissions must not use a cached pass");
        assert!(error.message.contains("cannot read/write"));
        assert!(backend.chown_log.is_empty());
        assert_eq!(
            backend.mode_of("/workspace/target/debug/.cargo-build-lock"),
            Some(0o444)
        );
    }

    #[test]
    fn numeric_uid_1000_is_distinct_from_501() {
        let mut backend = MemoryAccessBackend::new(vec![
            identity("dev", 1000, 1000),
            identity("other", 501, 20),
        ]);
        backend.add_dir("/workspace/target", 501, 20, 0o755);
        backend.add_dir("/workspace/target/debug", 501, 20, 0o755);
        let plan = WorkspaceOwnershipPlan {
            targets: vec![owned_target(
                "/workspace/target",
                Some(WorkspaceRustCacheKind::RustTarget),
            )],
        };
        prepare_workspace_permissions("dev", &plan, None, &context(), &mut backend).expect("prep");
        assert_eq!(backend.owner_of("/workspace/target"), Some((1000, 1000)));
        assert_eq!(
            backend.owner_of("/workspace/target/debug"),
            Some((1000, 1000))
        );
    }

    #[test]
    fn bind_mounted_unwritable_lock_does_not_chown_host_source() {
        let mut backend = MemoryAccessBackend::new(vec![identity("dev", 501, 20)]);
        backend.add_dir("/workspace/target", 501, 20, 0o755);
        backend.add_dir("/workspace/target/debug", 501, 20, 0o755);
        backend.add_file("/workspace/target/debug/.cargo-build-lock", 501, 20, 0o444);
        backend.protect("/workspace/target");
        let plan = WorkspaceOwnershipPlan {
            targets: vec![verify_target_spec(
                "/workspace/target",
                Some(WorkspaceRustCacheKind::RustTarget),
            )],
        };
        let error = prepare_workspace_permissions("dev", &plan, None, &context(), &mut backend)
            .expect_err("bind must fail closed");
        assert!(error.message.contains("uid=501"));
        assert!(error.message.contains("gid=20"));
        assert!(error.message.contains("bind"));
        assert!(error.message.contains("isolated_dirs"));
        assert!(backend.chown_log.is_empty());
        assert_eq!(
            backend.mode_of("/workspace/target/debug/.cargo-build-lock"),
            Some(0o444)
        );
    }

    #[test]
    fn symlink_escape_is_not_mutated() {
        let mut backend = MemoryAccessBackend::new(vec![identity("dev", 501, 20)]);
        backend.add_dir("/etc", 0, 0, 0o755);
        backend.add_symlink("/workspace/target", "/etc");
        backend.protect("/etc");
        let plan = WorkspaceOwnershipPlan {
            targets: vec![owned_target(
                "/workspace/target",
                Some(WorkspaceRustCacheKind::RustTarget),
            )],
        };
        let error = prepare_workspace_permissions("dev", &plan, None, &context(), &mut backend)
            .expect_err("symlink");
        assert!(error.message.contains("symlink"));
        assert!(backend.chown_log.is_empty());
        assert_eq!(backend.owner_of("/etc"), Some((0, 0)));
    }

    #[test]
    fn foreign_and_read_only_mounts_are_not_mutated() {
        let mut backend = MemoryAccessBackend::new(vec![identity("dev", 501, 20)]);
        backend.add_dir("/usr/local/cargo/git", 0, 0, 0o755);
        backend.mark_foreign("/usr/local/cargo/git");
        backend.mark_read_only("/usr/local/cargo/git");
        let plan = WorkspaceOwnershipPlan {
            targets: vec![WorkspaceOwnershipTarget {
                path: "/usr/local/cargo/git".to_owned(),
                mount_kind: WorkspaceMountKind::NamedVolume,
                source: Some("shared-cargo-git".to_owned()),
                repair_authority: WorkspaceRepairAuthority::Forbidden,
                rust_cache: Some(WorkspaceRustCacheKind::CargoGit),
                read_only: false,
            }],
        };
        let error = prepare_workspace_permissions("dev", &plan, None, &context(), &mut backend)
            .expect_err("foreign");
        assert!(
            error.message.contains("foreign")
                || error.message.contains("read-only")
                || error.message.contains("shared")
        );
        assert!(backend.chown_log.is_empty());
        assert_eq!(backend.owner_of("/usr/local/cargo/git"), Some((0, 0)));
    }

    #[test]
    fn doctor_distinguishes_stopped_from_clean_and_unwritable_nested() {
        let mut policy = effective_container_policy("web", "demo-web", "workspace", "compose.yml");
        policy.workspace_user = Some("dev".to_owned());
        policy.compose_files = Vec::new();
        policy.managed_volumes = vec![ManagedVolume {
            name: "demo-target".to_owned(),
            service: "workspace".to_owned(),
            persist: false,
            size_bytes: None,
            mount_point: None,
            mount_target: Some("/workspace/target".to_owned()),
        }];

        let stopped = diagnose_workspace_ownership(
            &policy,
            Ok(false),
            &[],
            &mut MemoryAccessBackend::new(vec![identity("dev", 501, 20)]),
        );
        assert_eq!(
            stopped.status,
            WorkspaceOwnershipProbeStatus::NotProbedStopped
        );
        assert!(stopped
            .evidence
            .unwrap_or_default()
            .contains("not probed (primary service stopped)"));

        let mut clean_backend = MemoryAccessBackend::new(vec![identity("dev", 501, 20)]);
        clean_backend.add_dir("/workspace/target", 501, 20, 0o755);
        let clean = diagnose_workspace_ownership(&policy, Ok(true), &[], &mut clean_backend);
        assert_eq!(clean.status, WorkspaceOwnershipProbeStatus::Clean);
        assert!(clean.evidence.unwrap_or_default().contains("uid=501"));

        let mut dirty = MemoryAccessBackend::new(vec![identity("dev", 501, 20)]);
        dirty.add_dir("/workspace/target", 501, 20, 0o755);
        dirty.add_dir("/workspace/target/debug", 0, 0, 0o755);
        let finding = diagnose_workspace_ownership(&policy, Ok(true), &[], &mut dirty);
        assert_eq!(finding.status, WorkspaceOwnershipProbeStatus::Finding);
        assert!(finding
            .samples
            .iter()
            .any(|sample| sample.contains("/workspace/target/debug")));
    }

    #[test]
    fn doctor_marks_unavailable_separately_from_clean() {
        let mut policy = effective_container_policy("web", "demo-web", "workspace", "compose.yml");
        policy.workspace_user = Some("dev".to_owned());
        policy.compose_files = Vec::new();
        let diagnosis = diagnose_workspace_ownership(
            &policy,
            Err("compose ps failed".to_owned()),
            &[],
            &mut MemoryAccessBackend::new(vec![identity("dev", 501, 20)]),
        );
        assert_eq!(diagnosis.status, WorkspaceOwnershipProbeStatus::Unavailable);
        assert!(diagnosis
            .warning
            .unwrap_or_default()
            .contains("compose ps failed"));
        assert!(diagnosis.evidence.is_none());
    }

    #[test]
    fn doctor_does_not_walk_unowned_tree() {
        let mut policy = effective_container_policy("web", "demo-web", "workspace", "compose.yml");
        policy.workspace_user = Some("dev".to_owned());
        policy.compose_files = Vec::new();
        policy.managed_volumes = vec![ManagedVolume {
            name: "demo-cargo-registry".to_owned(),
            service: "workspace".to_owned(),
            persist: true,
            size_bytes: None,
            mount_point: None,
            mount_target: Some("/usr/local/cargo/registry".to_owned()),
        }];
        let mut backend = MemoryAccessBackend::new(vec![identity("dev", 501, 20)]);
        backend.add_dir("/usr/local/cargo/registry", 501, 20, 0o755);
        backend.add_dir("/usr/local/cargo/registry/src", 0, 0, 0o755);
        backend.add_dir("/usr/local/cargo/registry/src/crate-a", 0, 0, 0o755);
        backend.add_file("/usr/local/cargo/registry/src/crate-a/lib.rs", 0, 0, 0o644);
        backend.add_dir("/usr/local/cargo/registry/src/crate-b", 0, 0, 0o755);
        let finding = diagnose_workspace_ownership(&policy, Ok(true), &[], &mut backend);
        assert_eq!(finding.status, WorkspaceOwnershipProbeStatus::Finding);
        assert_eq!(backend.list_unowned_calls.get(), 0);
        let inspected = backend.inspect_log.borrow().clone();
        assert!(inspected.contains(&"/usr/local/cargo/registry".to_owned()));
        assert!(inspected.contains(&"/usr/local/cargo/registry/src".to_owned()));
        assert!(!inspected
            .iter()
            .any(|path| path.contains("crate-a") || path.contains("crate-b")));
    }

    #[test]
    fn doctor_stops_nested_probes_after_first_sample() {
        let mut policy = effective_container_policy("web", "demo-web", "workspace", "compose.yml");
        policy.workspace_user = Some("dev".to_owned());
        policy.compose_files = Vec::new();
        policy.managed_volumes = vec![ManagedVolume {
            name: "demo-target".to_owned(),
            service: "workspace".to_owned(),
            persist: false,
            size_bytes: None,
            mount_point: None,
            mount_target: Some("/workspace/target".to_owned()),
        }];
        let mut backend = MemoryAccessBackend::new(vec![identity("dev", 501, 20)]);
        backend.add_dir("/workspace/target", 0, 0, 0o755);
        backend.add_dir("/workspace/target/debug", 0, 0, 0o755);
        backend.add_file("/workspace/target/debug/.cargo-build-lock", 0, 0, 0o644);
        let finding = diagnose_workspace_ownership(&policy, Ok(true), &[], &mut backend);
        assert_eq!(finding.status, WorkspaceOwnershipProbeStatus::Finding);
        assert_eq!(backend.list_unowned_calls.get(), 0);
        let inspected = backend.inspect_log.borrow().clone();
        assert_eq!(inspected, vec!["/workspace/target".to_owned()]);
        assert!(finding
            .samples
            .iter()
            .any(|sample| sample.contains("/workspace/target")));
    }

    #[test]
    fn doctor_expired_probe_deadline_is_unavailable_not_clean() {
        let mut policy = effective_container_policy("web", "demo-web", "workspace", "compose.yml");
        policy.workspace_user = Some("dev".to_owned());
        policy.compose_files = Vec::new();
        let deadline = Instant::now()
            .checked_sub(std::time::Duration::from_secs(1))
            .expect("deadline");
        let mut backend = compose_backend_with_deadline(Path::new("/tmp"), &policy, Some(deadline));
        let diagnosis = diagnose_workspace_ownership(&policy, Ok(true), &[], &mut backend);
        assert_eq!(diagnosis.status, WorkspaceOwnershipProbeStatus::Unavailable);
        assert!(
            diagnosis
                .warning
                .as_deref()
                .is_some_and(|warning| warning.contains("timed out")),
            "timeout must be unavailable, got {:?}",
            diagnosis.warning
        );
        assert!(diagnosis.evidence.is_none());
    }

    #[test]
    fn shared_named_rust_volume_prep_does_not_chown() {
        let temp = tempfile::tempdir().expect("tempdir");
        let compose = temp.path().join("docker-compose.yml");
        std::fs::write(
            &compose,
            r#"
services:
  workspace:
    volumes:
      - demo-cargo-git:/usr/local/cargo/git
  sidecar:
    volumes:
      - demo-cargo-git:/usr/local/cargo/git
"#,
        )
        .expect("compose");
        let mut policy = effective_container_policy("web", "demo-web", "workspace", compose);
        policy.workspace_user = Some("dev".to_owned());
        policy.repo_root = temp.path().to_path_buf();
        let plan = load_workspace_ownership_plan(&policy).expect("plan");
        let git = plan
            .targets
            .iter()
            .find(|target| target.path == "/usr/local/cargo/git")
            .expect("git");
        assert_eq!(git.repair_authority, WorkspaceRepairAuthority::VerifyOnly);

        let mut backend = MemoryAccessBackend::new(vec![identity("dev", 501, 20)]);
        backend.add_dir("/usr/local/cargo/git", 0, 0, 0o755);
        backend.add_dir("/usr/local/cargo/git/checkouts", 0, 0, 0o755);
        let error = prepare_workspace_permissions("dev", &plan, None, &context(), &mut backend)
            .expect_err("shared rust named volume must fail closed");
        assert!(error.message.contains("uid=501"));
        assert!(error.message.contains("gid=20"));
        assert!(backend.chown_log.is_empty());
        assert_eq!(backend.owner_of("/usr/local/cargo/git"), Some((0, 0)));
        assert_eq!(
            backend.owner_of("/usr/local/cargo/git/checkouts"),
            Some((0, 0))
        );
    }

    #[test]
    fn fresh_missing_owned_target_is_created_and_verified() {
        let mut backend = MemoryAccessBackend::new(vec![identity("dev", 501, 20)]);
        let plan = WorkspaceOwnershipPlan {
            targets: vec![owned_target(
                "/workspace/target",
                Some(WorkspaceRustCacheKind::RustTarget),
            )],
        };
        prepare_workspace_permissions("dev", &plan, None, &context(), &mut backend).expect("prep");
        assert_eq!(backend.owner_of("/workspace/target"), Some((501, 20)));
        assert!(backend
            .created_locks
            .iter()
            .any(|path| path == "/workspace/target/.effigy-write-probe"));
    }

    #[test]
    fn owned_volume_restores_owner_write_without_world_write() {
        let mut backend = MemoryAccessBackend::new(vec![identity("dev", 501, 20)]);
        backend.add_dir("/workspace/target", 501, 20, 0o755);
        backend.add_dir("/workspace/target/debug", 501, 20, 0o555);
        backend.add_file("/workspace/target/debug/.cargo-build-lock", 501, 20, 0o444);
        let plan = WorkspaceOwnershipPlan {
            targets: vec![owned_target(
                "/workspace/target",
                Some(WorkspaceRustCacheKind::RustTarget),
            )],
        };
        prepare_workspace_permissions("dev", &plan, None, &context(), &mut backend).expect("prep");
        assert_eq!(backend.mode_of("/workspace/target/debug"), Some(0o755));
        assert_eq!(
            backend.mode_of("/workspace/target/debug/.cargo-build-lock"),
            Some(0o644)
        );
        assert!(backend
            .chmod_log
            .iter()
            .any(|path| path == "/workspace/target/debug"));
        assert!(backend.mode_of("/workspace/target/debug").unwrap() & 0o002 == 0);
        assert!(
            backend
                .mode_of("/workspace/target/debug/.cargo-build-lock")
                .unwrap()
                & 0o002
                == 0
        );
    }

    #[test]
    fn bind_mounted_writable_target_is_verified_without_chown() {
        let mut backend = MemoryAccessBackend::new(vec![identity("dev", 501, 20)]);
        backend.add_dir("/workspace/target", 501, 20, 0o755);
        backend.add_dir("/workspace/target/debug", 501, 20, 0o755);
        backend.add_file("/workspace/target/debug/.cargo-build-lock", 501, 20, 0o644);
        backend.protect("/workspace/target");
        let plan = WorkspaceOwnershipPlan {
            targets: vec![verify_target_spec(
                "/workspace/target",
                Some(WorkspaceRustCacheKind::RustTarget),
            )],
        };
        prepare_workspace_permissions("dev", &plan, None, &context(), &mut backend).expect("prep");
        assert!(backend.chown_log.is_empty());
        assert!(backend.chmod_log.is_empty());
    }

    #[test]
    fn doctor_marks_no_workspace_user_as_not_probed() {
        let policy = effective_container_policy("web", "demo-web", "workspace", "compose.yml");
        let diagnosis = diagnose_workspace_ownership(
            &policy,
            Ok(true),
            &[],
            &mut MemoryAccessBackend::new(vec![identity("dev", 501, 20)]),
        );
        assert_eq!(
            diagnosis.status,
            WorkspaceOwnershipProbeStatus::NotProbedNoUser
        );
    }

    // ---- bulk ownership repair (papercut dab293be) ----

    const BULK_VOLUMES: [(&str, usize, WorkspaceRustCacheKind); 3] = [
        (
            "/cargo/registry",
            31497,
            WorkspaceRustCacheKind::CargoRegistry,
        ),
        (
            "/workspace/target",
            9709,
            WorkspaceRustCacheKind::RustTarget,
        ),
        ("/cargo/git", 3133, WorkspaceRustCacheKind::CargoGit),
    ];
    const EXEC_LATENCY_MS: u64 = 25;
    fn bulk_fixture(
        scale: usize,
        owner: (u32, u32),
    ) -> (MemoryAccessBackend, WorkspaceOwnershipPlan) {
        let mut backend = MemoryAccessBackend::new(vec![identity("dev", 501, 20)]);
        let mut targets = Vec::new();
        for (path, entries, rust) in BULK_VOLUMES {
            backend.add_dir(path, owner.0, owner.1, 0o755);
            let nested = match rust {
                WorkspaceRustCacheKind::CargoRegistry => format!("{path}/src"),
                WorkspaceRustCacheKind::RustTarget => format!("{path}/debug"),
                _ => format!("{path}/checkouts"),
            };
            backend.add_dir(&nested, owner.0, owner.1, 0o755);
            for index in 0..(entries / scale).saturating_sub(2) {
                backend.add_file(
                    &format!("{nested}/d{}/f{index}", index % 50),
                    owner.0,
                    owner.1,
                    0o644,
                );
            }
            backend.add_file(
                &format!("{path}/debug/.cargo-build-lock"),
                owner.0,
                owner.1,
                0o644,
            );
            targets.push(owned_target(path, Some(rust)));
        }
        (backend, WorkspaceOwnershipPlan { targets })
    }

    /// The pre-fix algorithm: one listing, then inspect+chown round trips per path.
    fn legacy_per_path_repair(backend: &mut MemoryAccessBackend, path: &str, uid: u32, gid: u32) {
        for entry in backend.list_unowned(path, uid, gid).expect("list") {
            let nested = backend.inspect(&entry).expect("inspect");
            if nested.presence == PathPresence::Symlink {
                continue;
            }
            backend.chown(&entry, uid, gid).expect("chown");
        }
    }

    #[test]
    fn exec_preparation_bulk_repair_uses_o_volumes_round_trips_not_o_files() {
        let (mut legacy, _) = bulk_fixture(1, (0, 0));
        let entries: usize = BULK_VOLUMES.iter().map(|(_, n, _)| *n).sum();
        assert!(entries >= 44000);
        for (path, _, _) in BULK_VOLUMES {
            legacy_per_path_repair(&mut legacy, path, 501, 20);
        }
        let legacy_calls = legacy.exec_calls.get();

        let (mut backend, plan) = bulk_fixture(1, (0, 0));
        prepare_workspace_permissions("dev", &plan, None, &context(), &mut backend).expect("prep");
        let bulk_calls = backend.exec_calls.get();
        let first_repair_count = backend.bulk_calls;
        prepare_workspace_permissions("dev", &plan, None, &context(), &mut backend)
            .expect("unchanged repeat prep");
        let repeated_calls = backend.exec_calls.get() - bulk_calls;

        let legacy_ms = legacy_calls as u64 * EXEC_LATENCY_MS;
        let bulk_ms = bulk_calls as u64 * EXEC_LATENCY_MS;
        println!(
            "bulk-ownership throughput: legacy calls={legacy_calls} (~{legacy_ms}ms @ {EXEC_LATENCY_MS}ms/exec) \
             bulk calls={bulk_calls} (~{bulk_ms}ms) bulk_ops={}",
            backend.bulk_calls
        );
        println!(
            "unchanged repeat fixture calls={repeated_calls}; additional repair launches={}",
            backend.bulk_calls - first_repair_count
        );
        assert!(
            legacy_calls > 80000,
            "legacy scales per file: {legacy_calls}"
        );
        assert_eq!(first_repair_count, 3);
        assert_eq!(backend.bulk_calls, first_repair_count);
        assert_eq!(
            repeated_calls, 4,
            "identity, metadata, ownership, and access batches"
        );
        assert!(bulk_calls < 60, "bulk must be O(volumes): {bulk_calls}");
        assert!(backend
            .list_unowned("/cargo/registry", 501, 20)
            .expect("list")
            .is_empty());
    }

    #[test]
    fn bulk_repair_gives_numeric_user_access_and_preserves_contents() {
        let (mut backend, plan) = bulk_fixture(100, (0, 0));
        backend.add_file("/workspace/target/debug/.cargo-build-lock", 0, 0, 0o644);
        backend.add_dir("/cargo/git/checkouts/repo", 1000, 1000, 0o755);
        backend.add_file("/cargo/git/checkouts/repo/a.rs", 1000, 1000, 0o644);
        backend.add_dir("/cargo/registry/src/crate", 501, 20, 0o755);
        let before = backend.mode_of("/cargo/git/checkouts/repo/a.rs");
        prepare_workspace_permissions("dev", &plan, None, &context(), &mut backend).expect("prep");

        let user = identity("dev", 501, 20);
        for path in [
            "/workspace/target/debug",
            "/workspace/target/debug/.cargo-build-lock",
            "/cargo/git/checkouts",
            "/cargo/git/checkouts/repo/a.rs",
            "/cargo/registry/src",
        ] {
            assert_eq!(backend.owner_of(path), Some((501, 20)), "{path}");
            assert!(backend.user_can_read_write(&user, path).expect("access"));
        }
        for dir in [
            "/workspace/target/debug",
            "/cargo/git/checkouts",
            "/cargo/registry/src",
        ] {
            backend.user_create_lock(&user, dir).expect("create lock");
        }
        assert_eq!(backend.mode_of("/cargo/git/checkouts/repo/a.rs"), before);
        assert!(backend.chmod_log.is_empty(), "no chmod widening");
    }

    #[test]
    fn bulk_repair_is_idempotent_on_reused_volumes() {
        let (mut backend, plan) = bulk_fixture(100, (0, 0));
        prepare_workspace_permissions("dev", &plan, None, &context(), &mut backend).expect("first");
        let first = backend.chown_log.len();
        assert!(first > 0);
        prepare_workspace_permissions("dev", &plan, None, &context(), &mut backend)
            .expect("second");
        assert_eq!(backend.chown_log.len(), first, "second run must not chown");
    }

    #[test]
    fn bulk_repair_skips_symlinks_inside_scope() {
        let (mut backend, plan) = bulk_fixture(100, (0, 0));
        backend.add_symlink("/cargo/registry/src/escape", "/etc");
        prepare_workspace_permissions("dev", &plan, None, &context(), &mut backend).expect("prep");
        assert_eq!(backend.owner_of("/cargo/registry/src/escape"), Some((0, 0)));
        assert!(!backend
            .chown_log
            .iter()
            .any(|path| path.ends_with("escape")));
    }

    #[test]
    fn bulk_failure_is_not_ready_and_mutates_nothing() {
        let (mut backend, plan) = bulk_fixture(100, (0, 0));
        backend.bulk_fail = Some("workspace bulk chown timed out".to_owned());
        let error = prepare_workspace_permissions("dev", &plan, None, &context(), &mut backend)
            .expect_err("must fail");
        assert!(error.message.contains("timed out"), "{}", error.message);
        assert!(backend.chown_log.is_empty());
    }

    #[test]
    fn bulk_deep_path_failure_after_partial_progress_names_path_and_stops() {
        let (mut backend, plan) = bulk_fixture(100, (0, 0));
        let deep = "/cargo/git/checkouts/formualizer-f6140fface89ddae/2a8303d/docs-site/content/docs/let-lambda-and-callables.mdx";
        backend.add_file(deep, 0, 0, 0o640);
        backend.bulk_fail_at = Some(deep.to_owned());
        let error = prepare_workspace_permissions("dev", &plan, None, &context(), &mut backend)
            .expect_err("must be not-ready");
        assert!(error.message.contains(deep), "{}", error.message);
        assert!(
            error.message.contains("Permission denied"),
            "{}",
            error.message
        );
        // Partial progress happened, contents untouched, nothing after the failing volume ran.
        assert!(!backend.chown_log.is_empty());
        assert_eq!(backend.owner_of(deep), Some((0, 0)));
        assert_eq!(backend.mode_of(deep), Some(0o640));
        assert!(!backend.chown_log.iter().any(|path| path == deep));
    }

    #[test]
    fn bulk_scope_swapped_to_symlink_mid_operation_is_not_ready() {
        let (mut backend, plan) = bulk_fixture(100, (0, 0));
        backend.bulk_swap_scope_to_symlink = true;
        let error = prepare_workspace_permissions("dev", &plan, None, &context(), &mut backend)
            .expect_err("must fail");
        assert!(
            error.message.contains("changed identity"),
            "{}",
            error.message
        );
    }

    #[test]
    fn bulk_foreign_path_in_scope_refuses_without_partial_mutation() {
        let (mut backend, plan) = bulk_fixture(100, (0, 0));
        backend.mark_foreign("/workspace/target/debug/d1/f1");
        let error = prepare_workspace_permissions("dev", &plan, None, &context(), &mut backend)
            .expect_err("foreign must fail");
        assert!(error.message.contains("foreign"), "{}", error.message);
        assert!(
            backend.chown_log.is_empty()
                || !backend
                    .chown_log
                    .iter()
                    .any(|p| p == "/workspace/target/debug/d1/f1")
        );
        assert_eq!(
            backend.owner_of("/workspace/target/debug/d1/f1"),
            Some((0, 0))
        );
    }

    #[test]
    fn bulk_leaves_bind_shared_and_read_only_scopes_alone() {
        let mut backend = MemoryAccessBackend::new(vec![identity("dev", 501, 20)]);
        backend.add_dir("/workspace/target", 0, 0, 0o755);
        backend.add_file("/workspace/target/debug/x", 0, 0, 0o644);
        let mut shared = owned_target(
            "/workspace/target",
            Some(WorkspaceRustCacheKind::RustTarget),
        );
        shared.repair_authority = WorkspaceRepairAuthority::VerifyOnly;
        let plan = WorkspaceOwnershipPlan {
            targets: vec![shared],
        };
        let _ = prepare_workspace_permissions("dev", &plan, None, &context(), &mut backend);
        assert_eq!(backend.bulk_calls, 0);
        assert!(backend.chown_log.is_empty());
    }

    #[test]
    fn compose_bulk_chown_honors_expired_deadline() {
        let mut policy = effective_container_policy("web", "demo-web", "workspace", "compose.yml");
        policy.workspace_user = Some("dev".to_owned());
        policy.compose_files = Vec::new();
        let deadline = Instant::now()
            .checked_sub(std::time::Duration::from_secs(1))
            .expect("deadline");
        let mut backend = compose_backend_with_deadline(Path::new("/tmp"), &policy, Some(deadline));
        let error = backend
            .chown_tree_unowned("/cargo/registry", 501, 20)
            .expect_err("expired");
        assert!(error.message.contains("timed out"), "{}", error.message);
    }

    #[test]
    fn bulk_argv_is_a_single_quoted_array_using_fd_relative_execdir() {
        let argv = bulk_chown_argv("/cargo/git", 501, 20);
        assert!(argv.iter().any(|part| part == "-execdir"));
        assert!(!argv.iter().any(|part| part == "-exec"));
        assert_eq!(argv.last().map(String::as_str), Some("+"));
        assert!(argv.iter().any(|part| part == "-P"));
        assert!(argv.iter().any(|part| part == "-xdev"));
        assert!(argv.iter().any(|part| part == "501:20"));
    }

    #[cfg(unix)]
    fn run_swap_race(argv: &[String]) -> (bool, String) {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::Builder::new()
            .prefix("effigy-bulk-swap-")
            .tempdir()
            .expect("tempdir");
        let root = std::fs::canonicalize(temp.path()).expect("canon");
        let vol = root.join("vol");
        let outside = root.join("outside");
        let bin = root.join("bin");
        std::fs::create_dir_all(vol.join("a")).expect("vol");
        std::fs::create_dir_all(&outside).expect("outside");
        std::fs::create_dir_all(&bin).expect("bin");
        std::fs::write(vol.join("a/f"), b"inside").expect("f");
        std::fs::write(outside.join("f"), b"outside").expect("of");
        let log = root.join("escaped.log");
        // Fake chown: swaps the intermediate dir for a symlink once, then
        // reports where its path argument physically resolves. No uid change.
        let script = format!(
            "#!/bin/sh\nshift; shift; shift\nif [ ! -e '{flag}' ]; then : > '{flag}'; mv '{vol}/a' '{vol}/a.real'; ln -s '{outside}' '{vol}/a'; fi\nfor p in \"$@\"; do\n  [ \"$p\" = -- ] && continue\n  d=$(cd \"$(dirname \"$p\")\" 2>/dev/null && pwd -P)\n  case \"$d\" in '{outside}'*) printf 'ESCAPED %s\\n' \"$d/$(basename \"$p\")\" >> '{log}';; esac\ndone\n",
            flag = root.join("swapped").display(),
            vol = vol.display(),
            outside = outside.display(),
            log = log.display(),
        );
        let chown = bin.join("chown");
        std::fs::write(&chown, script).expect("chown");
        std::fs::set_permissions(&chown, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        let argv: Vec<String> = argv
            .iter()
            .map(|part| part.replace("/cargo/git", &vol.display().to_string()))
            .collect();
        let path = format!(
            "{}:{}",
            bin.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let out = std::process::Command::new(&argv[0])
            .args(&argv[1..])
            .env("PATH", path)
            .output()
            .expect("find");
        let note = String::from_utf8_lossy(&out.stderr).into_owned();
        (
            std::fs::read_to_string(&log).is_ok_and(|text| text.contains("ESCAPED")),
            note,
        )
    }

    #[cfg(unix)]
    #[test]
    fn bulk_intermediate_symlink_swap_cannot_redirect_bulk_chown_outside_volume() {
        let argv = bulk_chown_argv("/cargo/git", 424242, 424243);
        let (escaped, note) = run_swap_race(&argv);
        assert!(!escaped, "bulk chown resolved outside the volume: {note}");
    }

    #[cfg(unix)]
    #[test]
    fn bulk_negative_control_path_based_exec_is_redirected_by_the_same_swap() {
        let mut legacy = bulk_chown_argv("/cargo/git", 424242, 424243);
        for part in legacy.iter_mut() {
            if part == "-execdir" {
                *part = "-exec".to_owned();
            }
        }
        let (escaped, _) = run_swap_race(&legacy);
        assert!(escaped, "control must reproduce the original escape");
    }

    /// Real filesystem, real `find`: 44339 entries over three volumes. A fake
    /// `chown` records every path it is handed (no uid change is possible
    /// without root) so we can prove traversal reaches every entry in all three
    /// volumes in few batched invocations while contents stay byte-identical.
    #[cfg(unix)]
    #[test]
    fn bulk_real_find_reaches_all_44339_entries_in_batches_and_preserves_contents() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::Builder::new()
            .prefix("effigy-bulk-real-")
            .tempdir()
            .expect("tempdir");
        let root = std::fs::canonicalize(temp.path()).expect("canon");
        let bin = root.join("bin");
        std::fs::create_dir_all(&bin).expect("bin");
        let log = root.join("chown.log");
        let calls = root.join("chown.calls");
        let script = format!(
            "#!/bin/sh\nshift; shift; shift\necho x >> '{calls}'\nfor p in \"$@\"; do printf '%s/%s\\n' \"$(pwd -P)\" \"${{p#./}}\" >> '{log}'; done\n",
            calls = calls.display(),
            log = log.display()
        );
        let chown = bin.join("chown");
        std::fs::write(&chown, script).expect("chown");
        std::fs::set_permissions(&chown, std::fs::Permissions::from_mode(0o755)).expect("chmod");

        // BSD/bfs `find -execdir +` spawns once per entry, so the full-size run
        // is only affordable (and batching only meaningful) with GNU findutils,
        // the canonical workspace image's find. Elsewhere run a 1/40 scale
        // coverage-only fixture.
        let gnu = std::process::Command::new("find")
            .arg("--version")
            .output()
            .is_ok_and(|out| String::from_utf8_lossy(&out.stdout).contains("GNU findutils"));
        let scale = if gnu { 1 } else { 40 };
        let mut expected = 0usize;
        let mut manifest = Vec::new();
        let mut volumes = Vec::new();
        for (name, full_entries, _) in BULK_VOLUMES {
            let entries = full_entries / scale;
            let vol = root.join(name.trim_start_matches('/').replace('/', "_"));
            let dirs = 40usize;
            std::fs::create_dir_all(&vol).expect("vol");
            for d in 0..dirs {
                std::fs::create_dir_all(vol.join(format!("d{d}"))).expect("dir");
            }
            for f in 0..(entries - 1 - dirs) {
                let path = vol.join(format!("d{}/f{f}", f % dirs));
                std::fs::write(&path, format!("{name}-{f}")).expect("file");
                manifest.push((path, format!("{name}-{f}")));
            }
            expected += entries;
            volumes.push(vol);
        }
        if gnu {
            assert_eq!(expected, 44339);
        }

        let path_env = format!(
            "{}:{}",
            bin.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        for vol in &volumes {
            let argv = bulk_chown_argv(&vol.display().to_string(), 424242, 424243);
            let out = std::process::Command::new(&argv[0])
                .args(&argv[1..])
                .env("PATH", &path_env)
                .output()
                .expect("find");
            assert!(
                out.status.success(),
                "find failed on {}: {}",
                vol.display(),
                String::from_utf8_lossy(&out.stderr)
            );
        }
        let handed: std::collections::BTreeSet<String> = std::fs::read_to_string(&log)
            .expect("log")
            .lines()
            .map(str::to_owned)
            .collect();
        let invocations = std::fs::read_to_string(&calls)
            .expect("calls")
            .lines()
            .count();
        println!(
            "real find (gnu={gnu}): {} unique entries handed to chown in {invocations} invocations",
            handed.len()
        );
        // Volume roots are handed over as `<parent>/<root>`; every entry exactly once.
        assert_eq!(handed.len(), expected);
        if gnu {
            assert!(invocations < expected / 50, "batched: {invocations}");
        }
        for (path, body) in &manifest {
            assert_eq!(&std::fs::read_to_string(path).expect("read"), body);
        }
    }

    // ---- full-size GNU + native-chown acceptance in a private container ----
    //
    // Runs only inside a disposable container (no host mounts, no network) on
    // the already-running `effigy` Colima profile from an already-local image.
    // Marked `#[ignore]` so hosted CI (which has no Colima) does not run it; the
    // `test:workspace:rust-ownership:bulk` selector runs it with `--ignored`
    // and it FAILS, never skips, when the runtime or image is unavailable.

    const GNU_ACCEPT_IMAGE: &str = "soundcheck-linux-arm-builder:local";
    const GNU_ACCEPT_STEP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(900);

    /// Installs a counting wrapper for `chown` ahead of /usr/bin on PATH. It
    /// always ends in the real native chown; marker files in /tmp/ctl add a
    /// one-shot intermediate-directory swap or a failure on a deep path.
    const GNU_ACCEPT_SETUP: &str = r#"set -eu
mkdir -p /tmp/ctl /tmp/outside
cat > /usr/local/bin/chown <<'WRAP'
#!/bin/sh
echo x >> /tmp/ctl/calls
if [ -e /tmp/ctl/swap ] && [ ! -e /tmp/ctl/swapped ]; then
  : > /tmp/ctl/swapped
  mv /tmp/race/vol/a /tmp/race/vol/a.real
  ln -s /tmp/race/outside /tmp/race/vol/a
fi
if [ -e /tmp/ctl/fail ]; then
  for p in "$@"; do
    case "$p" in *let-lambda-and-callables.mdx)
      echo "chown: cannot access '$(pwd)/$p': Permission denied" >&2
      exit 1;;
    esac
  done
fi
exec /usr/bin/chown "$@"
WRAP
chmod 755 /usr/local/bin/chown
printf secret > /tmp/outside/secret
/usr/bin/chown 0:12 /tmp/outside/secret
"#;

    /// args: root base total deep(0/1) link(0/1)
    const GNU_ACCEPT_MAKE: &str = r#"set -eu
root=$1; base=$2; total=$3; deep=$4; link=$5
dirs=40
mkdir -p "$root/$base"
i=0; while [ $i -lt $dirs ]; do mkdir "$root/$base/d$i"; i=$((i+1)); done
if [ "$deep" = 1 ]; then
  mkdir -p "$root/$base/formualizer-f6140fface89ddae/2a8303d/docs-site/content/docs"
  printf deep > "$root/$base/formualizer-f6140fface89ddae/2a8303d/docs-site/content/docs/let-lambda-and-callables.mdx"
fi
if [ "$base" = debug ]; then printf lock > "$root/debug/.cargo-build-lock"; fi
if [ "$link" = 1 ]; then ln -s /tmp/outside/secret "$root/$base/escape-link"; fi
have=$(find -P "$root" | wc -l)
f=0; need=$((total-have))
while [ $f -lt $need ]; do
  printf 'content-%s-%s' "$root" "$f" > "$root/$base/d$((f%dirs))/f$f"; f=$((f+1))
done
# mixed owners: d0-d9 already 501:20, d10-d19 foreign 1000:1000, rest root
i=0; while [ $i -lt 10 ]; do /usr/bin/chown -R 501:20 "$root/$base/d$i"; i=$((i+1)); done
while [ $i -lt 20 ]; do /usr/bin/chown -R 1000:1000 "$root/$base/d$i"; i=$((i+1)); done
"#;

    /// args: root uid gid -> entries / content manifest hash / not-owned count
    const GNU_ACCEPT_SNAP: &str = r#"set -eu
root=$1; uid=$2; gid=$3
echo "entries=$(find -P "$root" -xdev | wc -l)"
echo "hash=$( (find -P "$root" -xdev -type f -exec sha256sum {} + ; find -P "$root" -xdev -type l -printf '%p -> %l\n') | sort | sha256sum | cut -d' ' -f1)"
echo "unowned=$(find -P "$root" -xdev ! -type l ! \( -user "$uid" -a -group "$gid" \) | wc -l)"
"#;

    /// args: uid gid dir... -> read/write/create-and-remove as the numeric user
    const GNU_ACCEPT_ACCESS: &str = r#"u=$1; g=$2; shift 2
exec setpriv --reuid "$u" --regid "$g" --clear-groups sh -c 'for d; do
  test -r "$d" && test -w "$d" || exit 1
  : > "$d/.effigy-write-probe" && rm -f "$d/.effigy-write-probe" || exit 2
done' sh "$@""#;

    #[cfg(unix)]
    struct AcceptContainer {
        name: String,
    }

    #[cfg(unix)]
    impl Drop for AcceptContainer {
        fn drop(&mut self) {
            let _ = std::process::Command::new("colima")
                .args(["-p", "effigy", "nerdctl", "--", "rm", "-f", &self.name])
                .output();
        }
    }

    #[cfg(unix)]
    fn nerd(args: &[&str]) -> (bool, String, String, std::time::Duration) {
        let dir = tempfile::tempdir().expect("tmp");
        let out_path = dir.path().join("out");
        let err_path = dir.path().join("err");
        let mut child = std::process::Command::new("colima")
            .args(["-p", "effigy", "nerdctl", "--"])
            .args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::fs::File::create(&out_path).expect("out"))
            .stderr(std::fs::File::create(&err_path).expect("err"))
            .spawn()
            .unwrap_or_else(|error| panic!("BLOCKER: cannot run colima nerdctl: {error}"));
        let started = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().expect("wait") {
                break status;
            }
            if started.elapsed() > GNU_ACCEPT_STEP_TIMEOUT {
                let _ = child.kill();
                let _ = child.wait();
                panic!("acceptance step timed out and its child was reaped: {args:?}");
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        };
        (
            status.success(),
            std::fs::read_to_string(&out_path).unwrap_or_default(),
            std::fs::read_to_string(&err_path).unwrap_or_default(),
            started.elapsed(),
        )
    }

    #[cfg(unix)]
    fn kv(text: &str, key: &str) -> String {
        text.lines()
            .find_map(|line| line.strip_prefix(&format!("{key}=")))
            .unwrap_or_else(|| panic!("missing {key} in {text:?}"))
            .to_owned()
    }

    #[cfg(unix)]
    #[test]
    #[ignore = "private container acceptance; run via test:workspace:rust-ownership:bulk"]
    fn bulk_gnu_container_full_acceptance() {
        let name = format!(
            "effigy-bulk-accept-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        let (ok, _, err, _) = nerd(&[
            "run",
            "-d",
            "--network",
            "none",
            "--name",
            &name,
            GNU_ACCEPT_IMAGE,
            "sleep",
            "3000",
        ]);
        assert!(ok, "BLOCKER: cannot start private fixture container: {err}");
        let _guard = AcceptContainer { name: name.clone() };
        let exec = |argv: &[&str]| {
            let mut args = vec!["exec", name.as_str()];
            args.extend_from_slice(argv);
            nerd(&args)
        };
        let sh = |script: &str, extra: &[&str]| {
            let mut argv = vec!["sh", "-c", script, "sh"];
            argv.extend_from_slice(extra);
            let (ok, out, err, took) = exec(&argv);
            assert!(ok, "script failed: {err}\n{out}");
            (out, took)
        };

        let find_version = sh("find --version | head -1", &[]).0;
        assert!(find_version.contains("GNU findutils"), "{find_version}");
        sh(GNU_ACCEPT_SETUP, &[]);

        let vols: [(&str, &str, usize, &str, &str); 3] = [
            ("/tmp/fx/cargo_registry", "src", 31497, "0", "1"),
            ("/tmp/fx/target", "debug", 9709, "0", "0"),
            ("/tmp/fx/cargo_git", "checkouts", 3133, "1", "0"),
        ];
        let build_started = Instant::now();
        for (root, base, total, deep, link) in vols {
            sh(
                GNU_ACCEPT_MAKE,
                &[root, base, &total.to_string(), deep, link],
            );
        }
        println!("fixture built in {:?}", build_started.elapsed());

        let snap = |root: &str, uid: u32, gid: u32| {
            let out = sh(GNU_ACCEPT_SNAP, &[root, &uid.to_string(), &gid.to_string()]).0;
            (
                kv(&out, "entries").parse::<usize>().expect("entries"),
                kv(&out, "hash"),
                kv(&out, "unowned").parse::<usize>().expect("unowned"),
            )
        };
        let reset_calls = || {
            sh(": > /tmp/ctl/calls", &[]);
        };
        let calls = || -> usize {
            sh("wc -l < /tmp/ctl/calls", &[])
                .0
                .trim()
                .parse()
                .expect("calls")
        };

        let before: Vec<_> = vols.iter().map(|v| snap(v.0, 501, 20)).collect();
        let total_entries: usize = before.iter().map(|b| b.0).sum();
        assert_eq!(total_entries, 44339, "fixture entry count");
        for (vol, b) in vols.iter().zip(&before) {
            assert_eq!(b.0, vol.2);
            assert!(b.2 > 0, "{} must start with unowned entries", vol.0);
        }
        let mixed = sh(
            "find -P /tmp/fx -xdev ! -type l -printf '%U:%G\\n' | sort | uniq -c",
            &[],
        )
        .0;
        println!("owners before:\n{mixed}");
        assert!(mixed.contains("0:0") && mixed.contains("1000:1000") && mixed.contains("501:20"));

        // --- pass A: repair to 501:20, production argv, one exec per volume.
        let run_bulk = |uid: u32, gid: u32| {
            reset_calls();
            let mut elapsed = std::time::Duration::ZERO;
            let mut execs = 0usize;
            for vol in &vols {
                let argv = bulk_chown_argv(vol.0, uid, gid);
                let mut full = vec!["exec", name.as_str()];
                full.extend(argv.iter().map(String::as_str));
                let (ok, out, err, took) = nerd(&full);
                assert!(ok, "bulk find failed on {}: {err}\n{out}", vol.0);
                elapsed += took;
                execs += 1;
            }
            (execs, elapsed, calls())
        };
        let (execs_a, took_a, chowns_a) = run_bulk(501, 20);
        println!(
            "bulk 501:20: runtime execs={execs_a} chown invocations={chowns_a} real elapsed={took_a:?} \
             (legacy model: {} execs ~{}ms @25ms/exec)",
            2 * total_entries,
            2 * total_entries as u64 * 25
        );
        assert_eq!(execs_a, 3);
        assert!(chowns_a < total_entries / 50, "GNU batching: {chowns_a}");
        for (vol, b) in vols.iter().zip(&before) {
            let after = snap(vol.0, 501, 20);
            assert_eq!(after.0, b.0, "entry count preserved {}", vol.0);
            assert_eq!(after.1, b.1, "content manifest preserved {}", vol.0);
            assert_eq!(after.2, 0, "all entries owned 501:20 in {}", vol.0);
        }
        // Numeric 501:20 can read/write/create in the Rust-critical dirs.
        let dirs = [
            "/tmp/fx/target/debug",
            "/tmp/fx/cargo_git/checkouts",
            "/tmp/fx/cargo_git/checkouts/formualizer-f6140fface89ddae/2a8303d/docs-site/content/docs",
            "/tmp/fx/cargo_registry/src",
            "/tmp/fx/cargo_registry/src/d12",
        ];
        let mut access = vec!["sh", "-c", GNU_ACCEPT_ACCESS, "sh", "501", "20"];
        access.extend(dirs);
        let (ok, _, err, _) = exec(&access);
        assert!(ok, "501:20 access/create failed: {err}");
        let (ok, _, _, _) = exec(&[
            "setpriv",
            "--reuid",
            "501",
            "--regid",
            "20",
            "--clear-groups",
            "sh",
            "-c",
            ": >> /tmp/fx/target/debug/.cargo-build-lock",
        ]);
        assert!(ok, "501:20 cannot write build lock");
        // Ownership is real, not world-writable: another uid cannot write.
        let mut other = vec!["sh", "-c", GNU_ACCEPT_ACCESS, "sh", "1000", "1000"];
        other.push("/tmp/fx/target/debug");
        assert!(!exec(&other).0, "uid 1000 must not gain write via repair");
        // Protected symlink target unchanged.
        let secret = sh(
            "stat -c '%u:%g' /tmp/outside/secret; cat /tmp/outside/secret",
            &[],
        )
        .0;
        assert_eq!(secret, "0:12\nsecret", "symlink escape target untouched");

        // --- idempotence: second run does no chown work at all.
        let (_, _, chowns_idem) = run_bulk(501, 20);
        assert_eq!(chowns_idem, 0, "idempotent run must not invoke chown");

        // --- second numeric identity 1000:1000, mixed starting owners.
        let (_, took_b, chowns_b) = run_bulk(1000, 1000);
        println!("bulk 1000:1000: chown invocations={chowns_b} real elapsed={took_b:?}");
        for (vol, b) in vols.iter().zip(&before) {
            let after = snap(vol.0, 1000, 1000);
            assert_eq!((after.0, &after.1, after.2), (b.0, &b.1, 0));
        }
        let mut access = vec!["sh", "-c", GNU_ACCEPT_ACCESS, "sh", "1000", "1000"];
        access.extend(dirs);
        let (ok, _, err, _) = exec(&access);
        assert!(ok, "1000:1000 access/create failed: {err}");

        // --- deep failed path after partial progress (not-ready, content kept).
        sh("/usr/bin/chown -R 0:0 /tmp/fx/cargo_git", &[]);
        let git_before = snap("/tmp/fx/cargo_git", 501, 20);
        sh(": > /tmp/ctl/fail", &[]);
        let argv = bulk_chown_argv("/tmp/fx/cargo_git", 501, 20);
        let mut full = vec!["exec", name.as_str()];
        full.extend(argv.iter().map(String::as_str));
        let (ok, _, err, _) = nerd(&full);
        assert!(!ok, "deep chown failure must fail the bulk exec");
        assert!(err.contains("let-lambda-and-callables.mdx"), "{err}");
        let git_partial = snap("/tmp/fx/cargo_git", 501, 20);
        assert!(
            git_partial.2 > 0,
            "failing volume must still be unowned/not-ready"
        );
        assert!(
            git_partial.2 < git_before.2,
            "some entries repaired before failure"
        );
        assert_eq!(
            (git_partial.0, &git_partial.1),
            (git_before.0, &git_before.1)
        );
        sh("rm -f /tmp/ctl/fail", &[]);
        let (ok, _, err, _) = nerd(&full);
        assert!(ok, "retry after the failure clears: {err}");
        assert_eq!(snap("/tmp/fx/cargo_git", 501, 20).2, 0);

        // --- intermediate directory swap: production argv vs legacy -exec.
        let race = |argv: &[String]| -> String {
            sh(
                "rm -rf /tmp/race /tmp/ctl/swapped; mkdir -p /tmp/race/vol/a /tmp/race/outside; \
                 printf inside > /tmp/race/vol/a/f; printf outside > /tmp/race/outside/f; \
                 /usr/bin/chown 0:12 /tmp/race/outside/f; : > /tmp/ctl/swap",
                &[],
            );
            let mut full = vec!["exec", name.as_str()];
            full.extend(argv.iter().map(String::as_str));
            let _ = nerd(&full);
            sh("rm -f /tmp/ctl/swap", &[]);
            sh(
                "stat -c '%u:%g' /tmp/race/outside/f; cat /tmp/race/outside/f",
                &[],
            )
            .0
        };
        let safe = race(&bulk_chown_argv("/tmp/race/vol", 501, 20));
        assert_eq!(
            safe, "0:12\noutside",
            "execdir must leave outside file untouched"
        );
        let mut legacy = bulk_chown_argv("/tmp/race/vol", 501, 20);
        for part in legacy.iter_mut() {
            if part == "-execdir" {
                *part = "-exec".to_owned();
            }
        }
        let escaped = race(&legacy);
        assert_eq!(
            escaped, "501:20\noutside",
            "negative control must reproduce the escape"
        );
    }

    #[cfg(unix)]
    #[test]
    fn bulk_production_constructor_bounds_and_reaps_a_hung_bulk_child() {
        use crate::contract_test_support::{lock_test, EnvGuard};
        use std::os::unix::fs::PermissionsExt;
        let _lock = lock_test();
        let temp = tempfile::Builder::new()
            .prefix("effigy-bulk-deadline-")
            .tempdir()
            .expect("tempdir");
        let root = std::fs::canonicalize(temp.path()).expect("canon");
        std::fs::write(
            root.join("effigy.toml"),
            "[containers]\ndefault = \"stack\"\n",
        )
        .expect("manifest");
        let bin = root.join("bin");
        std::fs::create_dir_all(&bin).expect("bin");
        let pids = root.join("pids");
        let fake = format!(
            "#!/bin/sh\ncase \"$*\" in *execdir*) echo $$ >> '{}'; exec sleep 60;; esac\nexit 0\n",
            pids.display()
        );
        let docker = bin.join("docker");
        std::fs::write(&docker, fake).expect("docker");
        std::fs::set_permissions(&docker, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        let base = std::env::var("PATH").unwrap_or_default();
        let _env = EnvGuard::set_many(&[
            ("PATH", Some(format!("{}:{base}", bin.display()))),
            ("EFFIGY_COMPOSE_BACKEND", Some("docker".to_owned())),
        ]);
        let mut policy = effective_container_policy(
            "stack",
            "demo-stack",
            "workspace",
            root.join("docker-compose.yml"),
        );
        policy.repo_root = root.clone();
        policy.workspace_user = Some("dev".to_owned());

        // Normal production constructor: no caller deadline, but a finite bulk bound.
        let mut backend = compose_backend(&root, &policy);
        assert!(backend.deadline.is_none());
        assert_eq!(backend.bulk_timeout, BULK_CHOWN_TIMEOUT);
        assert!(backend.bulk_timeout < std::time::Duration::from_secs(3600));
        // Generous so child spawn under host load always precedes expiry; the
        // bound being finite is what is asserted above, not its length.
        backend.bulk_timeout = std::time::Duration::from_secs(5);

        let started = Instant::now();
        let error = backend
            .chown_tree_unowned("/cargo/git", 501, 20)
            .expect_err("hung child must time out");
        assert!(started.elapsed() < std::time::Duration::from_secs(30));
        assert!(
            error.message.contains("timed out") || error.message.contains("deadline"),
            "{}",
            error.message
        );
        let recorded = std::fs::read_to_string(&pids).unwrap_or_else(|_| {
            panic!(
                "fake bulk child never started; error was: {}",
                error.message
            )
        });
        let pid: i32 = recorded
            .lines()
            .next()
            .expect("pid")
            .trim()
            .parse()
            .expect("pid num");
        let gone = Instant::now() + std::time::Duration::from_secs(10);
        while nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None).is_ok() {
            assert!(Instant::now() < gone, "bulk child {pid} was not reaped");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    /// Host-fs seam: observes current-user access on a private fixture.
    /// This process cannot switch to uid 501; numeric uid/gid acceptance is
    /// covered by `MemoryAccessBackend`, not by this OS probe.
    #[cfg(unix)]
    struct HostFsAccessBackend {
        identity: ResolvedWorkspaceIdentity,
        chown_log: Vec<String>,
        chmod_log: Vec<String>,
        created_locks: Vec<String>,
    }

    #[cfg(unix)]
    impl HostFsAccessBackend {
        fn for_current_user(user: &str, sample: &Path) -> Self {
            use std::os::unix::fs::MetadataExt;
            let meta = std::fs::metadata(sample).expect("sample meta");
            Self {
                identity: ResolvedWorkspaceIdentity {
                    user: user.to_owned(),
                    uid: meta.uid(),
                    gid: meta.gid(),
                },
                chown_log: Vec::new(),
                chmod_log: Vec::new(),
                created_locks: Vec::new(),
            }
        }
    }

    #[cfg(unix)]
    impl WorkspaceAccessBackend for HostFsAccessBackend {
        fn resolve_identity(
            &self,
            user: &str,
        ) -> Result<ResolvedWorkspaceIdentity, PermissionPrepError> {
            if user != self.identity.user {
                return Err(PermissionPrepError::new(format!(
                    "host-fs seam only resolves `{user}` as the current process owner"
                )));
            }
            Ok(self.identity.clone())
        }

        fn inspect(&self, path: &str) -> Result<PathInspection, PermissionPrepError> {
            let meta = match std::fs::symlink_metadata(path) {
                Ok(meta) => meta,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    return Ok(PathInspection {
                        presence: PathPresence::Missing,
                        uid: None,
                        gid: None,
                        mode: None,
                    });
                }
                Err(error) => {
                    return Err(PermissionPrepError::new(format!(
                        "inspect `{path}`: {error}"
                    )));
                }
            };
            use std::os::unix::fs::{MetadataExt, PermissionsExt};
            let presence = if meta.file_type().is_symlink() {
                PathPresence::Symlink
            } else if meta.is_dir() {
                PathPresence::Directory
            } else {
                PathPresence::File
            };
            Ok(PathInspection {
                presence,
                uid: Some(meta.uid()),
                gid: Some(meta.gid()),
                mode: Some(meta.permissions().mode() & 0o777),
            })
        }

        fn mkdir_p(&mut self, path: &str) -> Result<(), PermissionPrepError> {
            Err(PermissionPrepError::new(format!(
                "host-fs seam refuses mkdir `{path}`"
            )))
        }

        fn chown_tree_unowned(
            &mut self,
            path: &str,
            _uid: u32,
            _gid: u32,
        ) -> Result<(), PermissionPrepError> {
            self.chown_log.push(path.to_owned());
            Err(PermissionPrepError::new(format!(
                "host-fs seam cannot switch uid; refusing bulk chown `{path}`"
            )))
        }

        fn chown(&mut self, path: &str, _uid: u32, _gid: u32) -> Result<(), PermissionPrepError> {
            self.chown_log.push(path.to_owned());
            Err(PermissionPrepError::new(format!(
                "host-fs seam cannot switch uid; refusing chown `{path}`"
            )))
        }

        fn chmod_owner_write(
            &mut self,
            path: &str,
            _directory: bool,
        ) -> Result<(), PermissionPrepError> {
            self.chmod_log.push(path.to_owned());
            Err(PermissionPrepError::new(format!(
                "host-fs seam refuses chmod `{path}`"
            )))
        }

        fn list_unowned(
            &self,
            path: &str,
            uid: u32,
            gid: u32,
        ) -> Result<Vec<String>, PermissionPrepError> {
            let inspection = self.inspect(path)?;
            if inspection.presence == PathPresence::Missing
                || inspection.presence == PathPresence::Symlink
            {
                return Ok(Vec::new());
            }
            let mut out = Vec::new();
            if inspection.uid != Some(uid) || inspection.gid != Some(gid) {
                out.push(path.to_owned());
            }
            Ok(out)
        }

        fn user_can_read_write(
            &self,
            identity: &ResolvedWorkspaceIdentity,
            path: &str,
        ) -> Result<bool, PermissionPrepError> {
            if identity.uid != self.identity.uid {
                return Ok(false);
            }
            let inspection = self.inspect(path)?;
            let Some(mode) = inspection.mode else {
                return Ok(false);
            };
            Ok(mode & 0o200 != 0)
        }

        fn user_create_lock(
            &mut self,
            identity: &ResolvedWorkspaceIdentity,
            directory: &str,
        ) -> Result<(), PermissionPrepError> {
            if identity.uid != self.identity.uid {
                return Err(PermissionPrepError::new(
                    "host-fs seam cannot switch uid".to_owned(),
                ));
            }
            let probe = std::path::PathBuf::from(directory).join(".effigy-write-probe");
            match std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&probe)
            {
                Ok(_) => {
                    let _ = std::fs::remove_file(&probe);
                    self.created_locks
                        .push(probe.to_string_lossy().into_owned());
                    Ok(())
                }
                Err(error) => Err(PermissionPrepError::new(error.to_string())),
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn host_fs_nested_unwritable_dir_is_observed_without_uid_switch() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().expect("tempdir");
        let target = temp.path().join("target");
        let debug = target.join("debug");
        std::fs::create_dir_all(&debug).expect("mkdir");
        std::fs::write(debug.join(".cargo-build-lock"), b"lock").expect("lock");
        let protected = temp.path().join("src/main.rs");
        std::fs::create_dir_all(protected.parent().expect("parent")).expect("src");
        std::fs::write(&protected, b"fn main() {}").expect("src");
        let before = std::fs::metadata(&protected).expect("meta");

        let mut perms = std::fs::metadata(&debug).expect("debug meta").permissions();
        perms.set_mode(0o555);
        std::fs::set_permissions(&debug, perms).expect("chmod");

        let mut backend = HostFsAccessBackend::for_current_user("dev", temp.path());
        let target_path = target.to_string_lossy().into_owned();
        let plan = WorkspaceOwnershipPlan {
            targets: vec![WorkspaceOwnershipTarget {
                path: target_path.clone(),
                mount_kind: WorkspaceMountKind::Bind,
                source: Some(target_path),
                repair_authority: WorkspaceRepairAuthority::VerifyOnly,
                rust_cache: Some(WorkspaceRustCacheKind::RustTarget),
                read_only: false,
            }],
        };
        let error = prepare_workspace_permissions("dev", &plan, None, &context(), &mut backend)
            .expect_err("unwritable nested debug must fail closed");
        assert!(error.message.contains("uid="));
        assert!(backend.chown_log.is_empty());
        assert!(backend.chmod_log.is_empty());
        let after = std::fs::metadata(&protected).expect("meta after");
        assert_eq!(before.modified().ok(), after.modified().ok());
        assert_eq!(
            std::fs::metadata(&debug)
                .expect("debug after")
                .permissions()
                .mode()
                & 0o777,
            0o555
        );

        let mut restore = std::fs::metadata(&debug).expect("debug meta").permissions();
        restore.set_mode(0o755);
        std::fs::set_permissions(&debug, restore).expect("restore");
    }
}
