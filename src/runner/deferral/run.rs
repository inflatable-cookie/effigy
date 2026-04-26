use std::collections::{BTreeMap, HashMap};
use std::ffi::OsString;
use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use effigy_cli::TaskInvocation;

use effigy_core::runtime_dir::ensure_effigy_ignored_in_git_root;
use effigy_core::shell::{shell_quote, with_local_node_bin_path};
use effigy_manifest::{load_task_manifest, ManifestTask, ManifestTaskRunIn};
use effigy_ui::style_text;
use effigy_ui::theme::is_ci_environment;
use effigy_ui::theme::{resolve_color_enabled, Theme};
use effigy_ui::{OutputMode, PlainRenderer, Renderer, SpinnerHandle};

use super::policy::DEFER_DEPTH_ENV;
use super::trace::render_deferral_trace;
use crate::runner::container_command::support::validate_running_container_runtime_match;
use crate::runner::error::RunnerError;
use crate::runner::exec_command::append_color_exec_env;
use crate::runner::exec_command::run_compose_exec;
use crate::runner::execute::api::{resolve_container_execution_binding, ContainerExecutionBinding};
use crate::runner::host_container_lease::{
    has_active_host_container_lease, host_container_lease_timeout_duration,
    refresh_host_container_lease,
};
use effigy_manifest::DeferredCommand;
use effigy_tasks::TaskRuntimeArgs;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ComposerHomeCacheKey {
    composer_path: PathBuf,
    composer_home_env: Option<String>,
    home_env: Option<String>,
}

fn composer_home_cache() -> &'static Mutex<HashMap<ComposerHomeCacheKey, PathBuf>> {
    static CACHE: OnceLock<Mutex<HashMap<ComposerHomeCacheKey, PathBuf>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

const IMPLICIT_DEFERRAL_CACHE_FILE: &str = "implicit-deferral-v1.json";

enum DeferredExecutionPlan {
    HostCommand(String),
    Completed(String),
}

struct DeferredStartupProgress {
    spinner: Option<Box<dyn SpinnerHandle>>,
}

impl DeferredStartupProgress {
    fn start(label: &str) -> Self {
        let enabled = std::io::stderr().is_terminal() && !is_ci_environment();
        if !enabled {
            eprintln!("{label}");
            return Self { spinner: None };
        }

        let mut renderer = PlainRenderer::stderr(OutputMode::from_env());
        let spinner = renderer.spinner(label).ok();
        Self { spinner }
    }

    fn finish_success(mut self) {
        if let Some(spinner) = self.spinner.take() {
            spinner.finish_clear();
        }
    }

    fn finish_error(mut self, message: &str) {
        if let Some(spinner) = self.spinner.take() {
            spinner.finish_error(message);
        } else {
            eprintln!("{message}");
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ImplicitDeferralCacheStore {
    #[serde(default = "implicit_deferral_cache_schema")]
    schema: String,
    #[serde(default = "implicit_deferral_cache_schema_version")]
    schema_version: u8,
    #[serde(default)]
    entries: BTreeMap<String, String>,
}

pub(in crate::runner) fn run_deferred_request(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    deferral: &DeferredCommand,
    cause: &RunnerError,
) -> Result<String, RunnerError> {
    let current_depth = std::env::var(DEFER_DEPTH_ENV)
        .ok()
        .and_then(|value| value.parse::<u8>().ok())
        .unwrap_or(0);
    if current_depth >= 1 {
        return Err(RunnerError::DeferLoopDetected {
            depth: current_depth,
        });
    }

    match deferral.run_in {
        ManifestTaskRunIn::Host => run_deferred_request_on_host(
            task,
            runtime_args,
            deferral,
            cause,
            current_depth,
            &build_deferred_command(task, runtime_args, deferral, &deferral.working_dir)?,
        ),
        ManifestTaskRunIn::Container | ManifestTaskRunIn::Either => {
            match run_deferred_request_with_binding(
                task,
                runtime_args,
                deferral,
                cause,
                current_depth,
            )? {
                DeferredExecutionPlan::HostCommand(command) => run_deferred_request_on_host(
                    task,
                    runtime_args,
                    deferral,
                    cause,
                    current_depth,
                    &command,
                ),
                DeferredExecutionPlan::Completed(output) => Ok(output),
            }
        }
    }
}

fn run_deferred_request_on_host(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    deferral: &DeferredCommand,
    cause: &RunnerError,
    current_depth: u8,
    command: &str,
) -> Result<String, RunnerError> {
    let shell = std::env::var("SHELL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "sh".to_owned());
    let shell_arg = if shell.ends_with("zsh") || shell.ends_with("bash") {
        "-ic"
    } else {
        "-c"
    };
    let mut process = ProcessCommand::new(&shell);
    process
        .arg(shell_arg)
        .arg(command)
        .current_dir(&deferral.working_dir)
        .env(DEFER_DEPTH_ENV, (current_depth + 1).to_string());
    with_local_node_bin_path(&mut process, &deferral.working_dir);
    let status = process
        .status()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: command.to_owned(),
            error,
        })?;

    if status.success() {
        if runtime_args.verbose_root {
            return Ok(render_deferral_trace(task, deferral, command, cause));
        }
        return Ok(String::new());
    }

    Err(RunnerError::TaskCommandFailure {
        command: command.to_owned(),
        code: status.code(),
        stdout: String::new(),
        stderr: String::new(),
    })
}

fn run_deferred_request_with_binding(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    deferral: &DeferredCommand,
    cause: &RunnerError,
    current_depth: u8,
) -> Result<DeferredExecutionPlan, RunnerError> {
    let manifest_path = deferral.working_dir.join("effigy.toml");
    if !manifest_path.is_file() {
        return match deferral.run_in {
            ManifestTaskRunIn::Either => Ok(DeferredExecutionPlan::HostCommand(
                build_deferred_command(task, runtime_args, deferral, &deferral.working_dir)?,
            )),
            ManifestTaskRunIn::Container => Err(RunnerError::task_invocation(format!(
                "deferred command from {} sets `[defer].run_in = \"container\"`, but {} does not contain an `effigy.toml` manifest to resolve a workspace container binding",
                deferral.source,
                deferral.working_dir.display()
            ))),
            ManifestTaskRunIn::Host => unreachable!(),
        };
    }

    let manifest = load_task_manifest(&manifest_path)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let binding_task = ManifestTask {
        run_in: Some(deferral.run_in),
        ..Default::default()
    };
    let binding = resolve_container_execution_binding(
        None,
        manifest.systems.as_ref(),
        manifest.containers.as_ref(),
        "defer",
        &binding_task,
        "deferral",
    )?;

    match binding {
        ContainerExecutionBinding::None | ContainerExecutionBinding::Host => {
            if deferral.run_in == ManifestTaskRunIn::Either {
                return Ok(DeferredExecutionPlan::HostCommand(build_deferred_command(
                    task,
                    runtime_args,
                    deferral,
                    &deferral.working_dir,
                )?));
            }
            Err(RunnerError::task_invocation(format!(
                "deferred command from {} sets `[defer].run_in = \"container\"`, but no default workspace container binding could be resolved from {}",
                deferral.source,
                manifest_path.display()
            )))
        }
        ContainerExecutionBinding::Container { .. } | ContainerExecutionBinding::Inline { .. } => {
            let policy = binding
                .load_effective_policy(&deferral.working_dir)?
                .ok_or_else(|| {
                    RunnerError::task_invocation(format!(
                        "deferred command from {} resolved a container binding, but no effective container policy was available",
                        deferral.source
                    ))
                })?;
            let had_active_lease = has_active_host_container_lease(&deferral.working_dir, &policy)?;
            effigy_containers::validate_container_policy(&deferral.working_dir, &policy)
                .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
            effigy_containers::validate_compose_backend_runtime(&deferral.working_dir, &policy)
                .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
            let was_running = crate::runner::system_command::is_primary_service_running(
                &deferral.working_dir,
                &policy,
            )?;
            let _colima_started =
                effigy_containers::exec::ensure_colima_running(&policy, &deferral.working_dir)?;
            validate_running_container_runtime_match(&deferral.working_dir, &policy)?;
            if !was_running {
                let progress = DeferredStartupProgress::start(&format!(
                    "Starting container environment `{}` for deferred task",
                    policy.name
                ));
                let startup_result = effigy_containers::exec::run_docker_capture(
                    &deferral.working_dir,
                    &policy,
                    &effigy_containers::compose::compose_up_args(&policy),
                    "docker compose up",
                );
                match startup_result {
                    Ok(_) => progress.finish_success(),
                    Err(error) => {
                        progress.finish_error("container environment startup failed");
                        return Err(error.into());
                    }
                }
            }
            if had_active_lease || !was_running {
                refresh_host_container_lease(&deferral.working_dir, &policy)?;
            }
            let exec_working_dir = binding
                .exec_working_dir(&deferral.working_dir)?
                .ok_or_else(|| {
                    RunnerError::task_invocation(format!(
                        "deferred command from {} resolved a container binding, but no exec working directory was available",
                        deferral.source
                    ))
                })?;
            let command = build_deferred_command(task, runtime_args, deferral, &exec_working_dir)?;
            let tty = std::io::stdout().is_terminal() || std::io::stderr().is_terminal();
            let args = build_deferred_container_command_args(
                &policy,
                &command,
                &exec_working_dir,
                current_depth,
                tty,
            );
            let output = run_compose_exec(
                &deferral.working_dir,
                &policy,
                &args,
                false,
                "docker compose exec",
            )?;
            if output.status.success() {
                if had_active_lease || !was_running {
                    emit_deferred_lease_notice(&policy.name);
                }
                if runtime_args.verbose_root {
                    return Ok(DeferredExecutionPlan::Completed(render_deferral_trace(
                        task, deferral, &command, cause,
                    )));
                }
                return Ok(DeferredExecutionPlan::Completed(String::new()));
            }
            Err(RunnerError::TaskCommandFailure {
                command,
                code: output.status.code(),
                stdout: String::new(),
                stderr: String::new(),
            })
        }
    }
}

fn build_deferred_container_command_args(
    policy: &effigy_containers::EffectiveContainerPolicy,
    command: &str,
    exec_working_dir: &Path,
    current_depth: u8,
    tty: bool,
) -> Vec<OsString> {
    let mut args = effigy_containers::compose::compose_args(policy, ["exec"]);
    if !tty {
        args.push(OsString::from("-T"));
    }
    args.push(OsString::from("-w"));
    args.push(OsString::from(exec_working_dir));
    if let Some(user) = policy.workspace_user.as_deref() {
        args.push(OsString::from("-u"));
        args.push(OsString::from(user));
    }
    if let Some(home) = policy.workspace_home.as_deref() {
        args.push(OsString::from("-e"));
        args.push(OsString::from(format!("HOME={home}")));
    }
    append_color_exec_env(&mut args, tty);
    args.push(OsString::from("-e"));
    args.push(OsString::from(format!(
        "{}={}",
        DEFER_DEPTH_ENV,
        current_depth + 1
    )));
    args.push(OsString::from(policy.primary_service.as_str()));
    args.push(OsString::from("sh"));
    args.push(OsString::from("-lc"));
    args.push(OsString::from(render_container_deferral_command(command)));
    args
}

fn render_container_deferral_command(command: &str) -> String {
    format!(
        "unset NO_COLOR; export EFFIGY_COLOR=always CLICOLOR_FORCE=1 FORCE_COLOR=3 PATH={}:$PATH; {command}",
        shell_quote("/usr/local/bin")
    )
}

fn emit_deferred_lease_notice(container_name: &str) {
    let timeout = format_duration_short(host_container_lease_timeout_duration());
    let color_enabled =
        resolve_color_enabled(OutputMode::from_env(), std::io::stderr().is_terminal());
    eprintln!(
        "{} temporary container lease active for `{container_name}`; idle shutdown in {timeout} unless reused or kept up explicitly",
        style_text(color_enabled, Theme::default().label, "[info]")
    );
}

fn format_duration_short(duration: std::time::Duration) -> String {
    let secs = duration.as_secs();
    if secs.is_multiple_of(60) && secs >= 60 {
        let mins = secs / 60;
        if mins == 1 {
            return "1 minute".to_owned();
        }
        return format!("{mins} minutes");
    }
    if secs == 1 {
        return "1 second".to_owned();
    }
    format!("{secs} seconds")
}

fn build_deferred_command(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    deferral: &DeferredCommand,
    repo_root: &Path,
) -> Result<String, RunnerError> {
    let args_rendered = runtime_args.passthrough.join(" ");
    let request_rendered = task.name.clone();
    let repo_rendered = shell_quote(&repo_root.display().to_string());
    let mut template = deferral.template.clone();
    if template.contains("{composer_global_effigy}") {
        let legacy_effigy = resolve_composer_global_effigy_path(&deferral.working_dir)?;
        template = template.replace(
            "{composer_global_effigy}",
            &shell_quote(&legacy_effigy.display().to_string()),
        );
    }
    Ok(template
        .replace("{request}", &request_rendered)
        .replace("{args}", &args_rendered)
        .replace("{repo}", &repo_rendered))
}

fn resolve_composer_global_effigy_path(workspace_root: &Path) -> Result<PathBuf, RunnerError> {
    let composer_path = resolve_path_executable("composer")?;
    let cache_key = ComposerHomeCacheKey {
        composer_path: composer_path.clone(),
        composer_home_env: std::env::var("COMPOSER_HOME").ok(),
        home_env: std::env::var("HOME").ok(),
    };

    if let Some(cached) = composer_home_cache()
        .lock()
        .expect("composer home cache poisoned")
        .get(&cache_key)
        .cloned()
    {
        return Ok(cached);
    }

    let persistent_key = composer_home_cache_store_key(&cache_key);
    if let Some(cached) = load_persisted_composer_home_cache(workspace_root, &persistent_key) {
        composer_home_cache()
            .lock()
            .expect("composer home cache poisoned")
            .insert(cache_key, cached.clone());
        return Ok(cached);
    }

    let output = ProcessCommand::new(&composer_path)
        .args(["global", "config", "home", "--absolute"])
        .output()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: "composer global config home --absolute".to_owned(),
            error,
        })?;
    if !output.status.success() {
        return Err(RunnerError::TaskCommandFailure {
            command: "composer global config home --absolute".to_owned(),
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }

    let home = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if home.is_empty() {
        return Err(RunnerError::task_invocation(
            "composer global config home --absolute returned an empty path",
        ));
    }

    let legacy_effigy = PathBuf::from(home).join("vendor/bin/effigy");
    composer_home_cache()
        .lock()
        .expect("composer home cache poisoned")
        .insert(cache_key, legacy_effigy.clone());
    save_persisted_composer_home_cache(workspace_root, &persistent_key, &legacy_effigy);
    Ok(legacy_effigy)
}

fn resolve_path_executable(name: &str) -> Result<PathBuf, RunnerError> {
    let path_value = std::env::var_os("PATH")
        .ok_or_else(|| RunnerError::task_invocation(format!("`{name}` requires PATH to be set")))?;
    for dir in std::env::split_paths(&path_value) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(RunnerError::task_invocation(format!(
        "implicit deferral requires `{name}` on PATH"
    )))
}

fn composer_home_cache_store_key(key: &ComposerHomeCacheKey) -> String {
    format!(
        "composer={}\ncomposer_home={}\nhome={}",
        key.composer_path.display(),
        key.composer_home_env.as_deref().unwrap_or(""),
        key.home_env.as_deref().unwrap_or(""),
    )
}

fn load_persisted_composer_home_cache(workspace_root: &Path, key: &str) -> Option<PathBuf> {
    let path = implicit_deferral_cache_path(workspace_root);
    let raw = fs::read_to_string(&path).ok()?;
    let store = serde_json::from_str::<ImplicitDeferralCacheStore>(&raw).ok()?;
    let cached = PathBuf::from(store.entries.get(key)?);
    cached.is_file().then_some(cached)
}

fn save_persisted_composer_home_cache(workspace_root: &Path, key: &str, value: &Path) {
    let cache_root = workspace_root.join(".effigy/cache");
    if ensure_effigy_ignored_in_git_root(workspace_root).is_err() {
        return;
    }
    if fs::create_dir_all(&cache_root).is_err() {
        return;
    }

    let path = implicit_deferral_cache_path(workspace_root);
    let mut store = fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str::<ImplicitDeferralCacheStore>(&raw).ok())
        .unwrap_or_default();
    store
        .entries
        .insert(key.to_owned(), value.display().to_string());
    let Ok(encoded) = serde_json::to_string_pretty(&store) else {
        return;
    };
    let _ = fs::write(path, encoded);
}

fn implicit_deferral_cache_path(workspace_root: &Path) -> PathBuf {
    workspace_root
        .join(".effigy/cache")
        .join(IMPLICIT_DEFERRAL_CACHE_FILE)
}

fn implicit_deferral_cache_schema() -> String {
    "effigy.deferral.implicit-cache.v1".to_owned()
}

fn implicit_deferral_cache_schema_version() -> u8 {
    1
}

impl Default for ImplicitDeferralCacheStore {
    fn default() -> Self {
        Self {
            schema: implicit_deferral_cache_schema(),
            schema_version: implicit_deferral_cache_schema_version(),
            entries: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
pub(crate) fn reset_composer_home_cache_for_tests() {
    composer_home_cache()
        .lock()
        .expect("composer home cache poisoned")
        .clear();
}
