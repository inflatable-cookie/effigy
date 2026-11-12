//! Plan and prepare permissions for the declared workspace ownership targets.

use crate::runner::workspace_command::{emit_workspace_info, WorkspaceOpProgress};
use crate::runner::workspace_command::{permissions, PROBE_PERMISSION_PREP};
use crate::runner::RunnerError;
use effigy_containers::{load_workspace_ownership_plan, EffectiveContainerPolicy};
use effigy_ui::steps::ProgressFrame;
use std::path::{Path, PathBuf};
pub(super) fn ensure_workspace_permissions_ready(
    workspace_repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    container_name: Option<&str>,
    repo_override: Option<PathBuf>,
) -> Result<(), RunnerError> {
    let Some(_user) = policy.workspace_user.as_deref() else {
        return Ok(());
    };
    let plan = load_workspace_ownership_plan(policy)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    if plan.targets.is_empty() {
        return Ok(());
    }
    // Mounts are part of the service configuration the one apply reconciles,
    // so permission repair no longer waits on a separate drift probe.
    let allow_repair = true;
    let perm_plan = plan_workspace_permission_prep(&plan.owned_disposable_paths());
    let progress_label = render_workspace_permission_progress_label(&perm_plan);
    if repo_override.is_none()
        && (!std::io::IsTerminal::is_terminal(&std::io::stderr())
            || effigy_ui::theme::is_ci_environment())
    {
        emit_workspace_info(&progress_label, false);
    }
    let progress =
        WorkspaceOpProgress::start(repo_override.is_some(), PROBE_PERMISSION_PREP, false);
    progress.update(|_| {
        ProgressFrame::new(progress_label).count(Some((0, perm_plan.targets.len() as u64)))
    });
    let mut backend = permissions::compose_backend(workspace_repo_root, policy);
    match permissions::ensure_workspace_permissions_ready_with(
        policy,
        container_name,
        allow_repair,
        &mut backend,
    ) {
        Ok(()) => {
            progress.finish_clear();
            Ok(())
        }
        Err(error) => {
            progress.fail_error(&error);
            Err(error)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WorkspacePermissionMode {
    Recursive,
    Shallow,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct WorkspacePermissionTarget {
    pub(super) path: String,
    pub(super) mode: WorkspacePermissionMode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct WorkspacePermissionPlan {
    pub(super) targets: Vec<WorkspacePermissionTarget>,
    pub(super) skipped_nested: usize,
    pub(super) shallow_dependency_trees: usize,
}

pub(super) fn plan_workspace_permission_prep(targets: &[String]) -> WorkspacePermissionPlan {
    let mut sorted = targets.to_vec();
    sorted.sort_by(|left, right| {
        path_depth(left)
            .cmp(&path_depth(right))
            .then_with(|| left.cmp(right))
    });

    let mut kept: Vec<WorkspacePermissionTarget> = Vec::new();
    let mut skipped_nested = 0usize;
    let mut shallow_dependency_trees = 0usize;

    for path in sorted {
        if kept.iter().any(|existing| {
            existing.mode == WorkspacePermissionMode::Recursive
                && path_is_under(&path, &existing.path)
        }) {
            skipped_nested += 1;
            continue;
        }

        let mode = if is_dependency_tree_path(&path)
            && kept.iter().any(|existing| {
                existing.mode == WorkspacePermissionMode::Recursive
                    && is_dependency_tree_path(&existing.path)
                    && dependency_tree_is_under_authoritative_root(&path, &existing.path)
            }) {
            shallow_dependency_trees += 1;
            WorkspacePermissionMode::Shallow
        } else {
            WorkspacePermissionMode::Recursive
        };

        kept.push(WorkspacePermissionTarget { path, mode });
    }

    WorkspacePermissionPlan {
        targets: kept,
        skipped_nested,
        shallow_dependency_trees,
    }
}

fn render_workspace_permission_progress_label(plan: &WorkspacePermissionPlan) -> String {
    let recursive = plan
        .targets
        .iter()
        .filter(|target| target.mode == WorkspacePermissionMode::Recursive)
        .count();
    let shallow = plan
        .targets
        .iter()
        .filter(|target| target.mode == WorkspacePermissionMode::Shallow)
        .count();
    if plan.skipped_nested == 0 && plan.shallow_dependency_trees == 0 {
        return format!(
            "preparing workspace permissions ({recursive} path{})",
            if recursive == 1 { "" } else { "s" }
        );
    }
    format!(
        "preparing workspace permissions ({recursive} recursive, {shallow} shallow; skipped {} nested)",
        plan.skipped_nested
    )
}

pub(super) fn is_dependency_tree_path(path: &str) -> bool {
    let trimmed = path.trim_end_matches('/');
    matches!(
        Path::new(trimmed)
            .file_name()
            .and_then(|name| name.to_str()),
        Some("node_modules" | "vendor")
    )
}

fn dependency_tree_is_under_authoritative_root(candidate: &str, authoritative: &str) -> bool {
    let Some(root_parent) = parent_unix_path(authoritative) else {
        return false;
    };
    let Some(candidate_parent) = parent_unix_path(candidate) else {
        return false;
    };
    // Only a strictly nested tree (candidate parent inside the authoritative
    // tree's parent scope) may rely on the authoritative recursive scan.
    // Same-parent siblings (Composer `vendor` next to JS `node_modules` as
    // separate volumes) are distinct mount scopes: neither scan descends into
    // the other, so each sibling keeps its own recursive repair.
    path_is_under(candidate_parent, root_parent)
}

fn parent_unix_path(path: &str) -> Option<&str> {
    let trimmed = path.trim_end_matches('/');
    match trimmed.rsplit_once('/') {
        Some(("", _)) => Some("/"),
        Some((parent, _)) if !parent.is_empty() => Some(parent),
        _ => None,
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

fn path_depth(path: &str) -> usize {
    path.trim_matches('/')
        .split('/')
        .filter(|part| !part.is_empty())
        .count()
}
