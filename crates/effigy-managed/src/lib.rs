//! Managed task orchestration for Effigy.
//!
//! Moved out of the runner binary in batch 240. Owns the managed task
//! scheduler, run-spec rendering, env resolution, references, profiles,
//! the runtime session loop, and the plan-shape structs.

use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use effigy_core::path_error_text::{
    failed_to_parse_path, failed_to_read_path, failed_to_render_path, failed_to_write_path,
};
use effigy_env::schema_support::SchemaSupportError;
use effigy_process::ProcessManagerError;
use effigy_ui::{OutputMode, PlainRenderer, UiError};

pub mod command;
pub mod plan;
pub mod presentation;
pub mod profiles;
pub mod references;
pub mod render_support;
pub mod run_spec;
pub mod runtime;
pub mod scheduler;

pub use command::resolve_managed_task_plan;
pub use plan::{
    invalid_managed_process_definition, resolve_managed_concurrent_task_plan,
    ManagedConcurrentPlanInput,
};
pub use presentation::{managed_execution_mode, run_or_render_managed_task, ManagedExecutionMode};
pub use profiles::{
    available_concurrent_profiles, has_concurrent_profile, has_concurrent_schema,
    resolve_concurrent_profile, select_concurrent_profile, ResolvedConcurrentProfile,
    DEFAULT_MANAGED_PROFILE,
};
pub use run_spec::{
    render_builtin_reference_invocation, render_run_step_sequence, render_step_command_template,
    render_task_run_spec, resolve_run_step_env, wrap_command_with_env,
    wrap_reference_command_in_cwd, RunSpecContext, StepEnvAccumulator,
};
pub use scheduler::build_run_sequence_schedule;

/// Pure-data plan shapes moved in from `runner::model::managed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedProcessRole {
    Standard,
    Lifecycle,
    Shell,
}

#[derive(Debug, Clone)]
pub struct ManagedProcessSpec {
    pub name: String,
    pub role: ManagedProcessRole,
    pub run: String,
    pub setup: Option<String>,
    pub cwd: PathBuf,
    pub service: Option<String>,
    pub start_after_ms: u64,
    pub shutdown_on_exit: bool,
}

#[derive(Debug)]
pub struct ManagedTaskPlan {
    pub mode: String,
    pub profile: String,
    pub processes: Vec<ManagedProcessSpec>,
    pub tab_order: Vec<String>,
    pub fail_on_non_zero: bool,
    pub passthrough: Vec<String>,
    pub gateway_auto_start: bool,
    pub readiness: ManagedTaskReadiness,
}

#[derive(Debug, Clone, Default)]
pub struct ManagedTaskReadiness {
    pub health_wait: bool,
    pub ready_message: Option<String>,
}

/// Default shell command used when a managed profile's `shell` entry
/// omits `run`. Mirrors the runner-local default from before the
/// extraction.
pub const DEFAULT_MANAGED_SHELL_RUN: &str = "exec ${SHELL:-/bin/zsh} -i";

/// Builtin task names managed's reference parser recognises without
/// catalog resolution. Duplicated from `runner::model::constants`
/// during batch 240 — when routing-core extracts, both copies collapse
/// back to one.
pub const BUILTIN_TASKS: [(&str, &str); 13] = [
    ("test", "effigy-test"),
    ("scan", "effigy-scan"),
    ("doctor", "effigy-doctor"),
    ("contracts", "effigy-contracts"),
    ("bootstrap", "effigy-bootstrap"),
    ("changelog", "effigy-changelog"),
    ("container", "effigy-container"),
    ("demo", "effigy-demo"),
    ("distribution", "effigy-distribution"),
    ("docs", "effigy-docs"),
    ("release", "effigy-release"),
    ("script", "effigy-script"),
    ("tasks", "effigy-tasks"),
];

/// Error raised by the managed orchestration crate.
///
/// The runner provides `From<ManagedError> for RunnerError` so managed
/// failures surface with the same shape they had when managed code
/// lived inside the runner binary.
#[derive(Debug)]
pub enum ManagedError {
    Cwd(std::io::Error),
    TaskInvocation(String),
    Ui(String),
    Process(ProcessManagerError),
    TaskManagedUnsupportedMode {
        task: String,
        mode: String,
    },
    TaskManagedProfileNotFound {
        task: String,
        profile: String,
        available: Vec<String>,
    },
    TaskManagedProfileEmpty {
        task: String,
        profile: String,
    },
    TaskManagedProcessInvalidDefinition {
        task: String,
        process: String,
        detail: String,
    },
    TaskManagedTaskReferenceInvalid {
        task: String,
        process: String,
        reference: String,
        detail: String,
    },
    TaskManagedNonZeroExit {
        task: String,
        profile: String,
        processes: Vec<(String, String)>,
    },
}

impl ManagedError {
    pub fn task_invocation(message: impl Into<String>) -> Self {
        Self::TaskInvocation(message.into())
    }

    pub fn task_invocation_failed_read(path: &Path, error: impl std::fmt::Display) -> Self {
        Self::task_invocation(failed_to_read_path(path, error))
    }

    pub fn task_invocation_failed_parse(path: &Path, error: impl std::fmt::Display) -> Self {
        Self::task_invocation(failed_to_parse_path(path, error))
    }

    pub fn task_invocation_failed_write(path: &Path, error: impl std::fmt::Display) -> Self {
        Self::task_invocation(failed_to_write_path(path, error))
    }

    pub fn task_invocation_failed_render(path: &Path, error: impl std::fmt::Display) -> Self {
        Self::task_invocation(failed_to_render_path(path, error))
    }
}

impl std::fmt::Display for ManagedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cwd(error) => write!(f, "failed to read current directory: {error}"),
            Self::TaskInvocation(message) => write!(f, "{message}"),
            Self::Ui(message) => write!(f, "{message}"),
            Self::Process(error) => write!(f, "{error}"),
            Self::TaskManagedUnsupportedMode { task, mode } => {
                write!(f, "task '{task}' has unsupported managed mode '{mode}'")
            }
            Self::TaskManagedProfileNotFound {
                task,
                profile,
                available,
            } => write!(
                f,
                "task '{task}' has no managed profile '{profile}' (available: {})",
                available.join(", ")
            ),
            Self::TaskManagedProfileEmpty { task, profile } => write!(
                f,
                "task '{task}' managed profile '{profile}' has no processes"
            ),
            Self::TaskManagedProcessInvalidDefinition {
                task,
                process,
                detail,
            } => write!(
                f,
                "task '{task}' managed process '{process}' is invalid: {detail}"
            ),
            Self::TaskManagedTaskReferenceInvalid {
                task,
                process,
                reference,
                detail,
            } => write!(
                f,
                "task '{task}' managed process '{process}' has invalid task reference '{reference}': {detail}"
            ),
            Self::TaskManagedNonZeroExit {
                task,
                profile,
                processes,
            } => {
                let summaries = processes
                    .iter()
                    .map(|(name, detail)| format!("{name}: {detail}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(
                    f,
                    "task '{task}' managed profile '{profile}' exited non-zero ({summaries})"
                )
            }
        }
    }
}

impl std::error::Error for ManagedError {}

impl From<UiError> for ManagedError {
    fn from(value: UiError) -> Self {
        Self::Ui(value.to_string())
    }
}

impl From<ProcessManagerError> for ManagedError {
    fn from(value: ProcessManagerError) -> Self {
        Self::Process(value)
    }
}

impl From<SchemaSupportError> for ManagedError {
    fn from(value: SchemaSupportError) -> Self {
        Self::task_invocation(value.to_string())
    }
}

// -- small render utilities duplicated from `runner::render` --

pub(crate) fn text_renderer() -> PlainRenderer<Vec<u8>> {
    PlainRenderer::new(Vec::<u8>::new(), text_color_enabled())
}

pub(crate) fn text_color_enabled() -> bool {
    resolve_text_color_enabled(
        OutputMode::from_env(),
        std::io::stdout().is_terminal(),
        std::env::var_os("NO_COLOR").is_some(),
    )
}

fn resolve_text_color_enabled(mode: OutputMode, is_tty: bool, no_color: bool) -> bool {
    match mode {
        OutputMode::Always => true,
        OutputMode::Never => false,
        OutputMode::Auto => is_tty && !no_color,
    }
}

pub(crate) fn render_utf8(out: Vec<u8>) -> Result<String, ManagedError> {
    String::from_utf8(out)
        .map_err(|error| ManagedError::Ui(format!("invalid utf-8 in rendered output: {error}")))
}

/// Adapter that shapes `effigy-env`'s `resolve_catalog_env_schema`
/// against a `ManifestEnvSchemaConfig`. Managed code used to reach
/// `runner::env_schema_support` for this; now the shared resolver is
/// public, and this adapter lives where managed can reach it.
pub(crate) fn resolve_catalog_env_schema_from_manifest(
    catalog_root: &Path,
    config: Option<&effigy_manifest::ManifestEnvSchemaConfig>,
    runtime_override: Option<&Path>,
) -> Result<Option<effigy_env::resolver::ResolvedEnv>, ManagedError> {
    let shared_config = effigy_env::schema_support::SchemaSupportConfig {
        config_schema: config.and_then(|c| c.schema.as_deref()),
        config_enabled: config.and_then(|c| c.enabled),
        config_exec_timeout_secs: config.and_then(|c| c.exec_timeout),
    };
    effigy_env::schema_support::resolve_catalog_env_schema(
        catalog_root,
        shared_config,
        runtime_override,
    )
    .map_err(Into::into)
}
