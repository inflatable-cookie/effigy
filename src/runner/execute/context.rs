use std::path::{Path, PathBuf};

use crate::resolver::ResolvedTarget;

use super::preflight::ExecutionPreflight;
use crate::runner::model::catalog::{TaskSelection, TaskSelector};
use crate::runner::render::render_task_resolution_trace;

pub(super) struct ExecutionTaskContext<'a> {
    pub(super) resolved: &'a ResolvedTarget,
    pub(super) selector: &'a TaskSelector,
    pub(super) selection: &'a TaskSelection<'a>,
    pub(super) resolved_root: &'a Path,
    pub(super) repo_for_task: PathBuf,
    pub(super) command: String,
}

impl<'a> ExecutionTaskContext<'a> {
    pub(super) fn new(
        preflight: &'a ExecutionPreflight,
        selection: &'a TaskSelection<'a>,
        command: String,
    ) -> Self {
        Self {
            resolved: &preflight.resolved,
            selector: &preflight.selector,
            selection,
            resolved_root: &preflight.resolved.resolved_root,
            repo_for_task: selection.catalog.catalog_root.clone(),
            command,
        }
    }

    pub(super) fn command(&self) -> &str {
        &self.command
    }

    pub(super) fn repo_for_task(&self) -> &Path {
        &self.repo_for_task
    }

    pub(super) fn render_resolution_trace(&self) -> String {
        render_task_resolution_trace(
            self.resolved,
            self.selector,
            self.selection,
            self.repo_for_task(),
            self.command(),
        )
    }
}
