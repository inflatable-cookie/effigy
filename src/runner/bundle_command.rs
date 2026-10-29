use effigy_cli::{BundleArgs, BundleSubcommand};
use effigy_manifest::{
    inspect_bundle_source, sync_bundle_source, BundleSourceType, SourceProgress,
    SourceProgressEvent, TASK_MANIFEST_FILE,
};
use effigy_ui::steps::{ProgressSource, Step};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use super::error::RunnerError;

/// Syntax-only classification shared by binary signal setup and dev dispatch.
/// It does not resolve a manifest or acquire source authority. `container up`
/// is a Prepare path too, but it prepares in its own entry point rather than
/// owning the process signal router from startup.
pub(crate) fn is_source_prepare_command(command: &effigy_cli::Command) -> bool {
    match command {
        effigy_cli::Command::Bundle(args) => matches!(args.subcommand, BundleSubcommand::Sync),
        effigy_cli::Command::Bootstrap(_) => true,
        effigy_cli::Command::Task(task) => {
            task.name.rsplit('/').next() == Some("dev")
                && !is_read_only_task_invocation(task)
                && !is_task_local_session_invocation(task)
        }
        _ => false,
    }
}

/// First-argument `status`, `logs` or `stop` on a task. On a managed TUI
/// task this inspects or stops the session; on any other task those words
/// are ordinary arguments and the run still acquires a missing pin. A CLI
/// invocation joins Effigy flags with
/// [`effigy_core::task_args::TASK_ARGS_BOUNDARY`] before the task's
/// arguments; the verb is the first task-side argument after that split.
pub(crate) fn is_task_local_session_invocation(task: &effigy_cli::TaskInvocation) -> bool {
    first_task_arg(&task.args).is_some_and(|arg| matches!(arg, "status" | "logs" | "stop"))
}

/// Managed TUI companions (`mode = "tui"`) with first-argument status, logs
/// or stop. They skip missing-pin acquisition. The selected task's own
/// overlay, included fragments, and catalog file are read as TOML so a
/// missing bundle pin is not cloned just to classify it; a prefixed catalog
/// name uses that catalog's graph, not the root leaf of the same name.
pub(crate) fn is_managed_tui_session_invocation(
    task: &effigy_cli::TaskInvocation,
    manifest_path: &Path,
) -> bool {
    is_task_local_session_invocation(task)
        && declared_task_mode(manifest_path, &task.name).as_deref() == Some("tui")
}

fn declared_task_mode(manifest_path: &Path, task_name: &str) -> Option<String> {
    let parent = manifest_path.parent()?;
    let (catalog_root, leaf) = match task_name.split_once('/') {
        Some((alias, leaf)) => (catalog_member_dir(parent, manifest_path, alias)?, leaf),
        None => (parent.to_path_buf(), task_name),
    };
    let overlay = catalog_root.join(effigy_core::repo_markers::LOCAL_OVERLAY_FILE);
    task_mode_from_toml_file(&overlay, leaf)
        .or_else(|| task_mode_from_include_graph(&catalog_root.join(TASK_MANIFEST_FILE), leaf))
}

/// Membership is the committed root `[catalog.members]` table, not an overlay.
/// A handle that is missing or whose directory is not here fails closed.
fn catalog_member_dir(root: &Path, manifest_path: &Path, alias: &str) -> Option<PathBuf> {
    let text = std::fs::read_to_string(manifest_path).ok()?;
    let value: toml::Value = toml::from_str(&text).ok()?;
    let relative = value.get("catalog")?.get("members")?.get(alias)?.as_str()?;
    if relative.trim().is_empty() {
        return None;
    }
    Some(root.join(relative))
}

/// Parent file first, then each `[manifest].include` in order, matching
/// composition: a later fragment overwrites a declared mode. A cycle or a
/// missing required include fails closed.
fn task_mode_from_include_graph(path: &Path, name: &str) -> Option<String> {
    fn walk(path: &Path, name: &str, visiting: &mut BTreeSet<PathBuf>) -> Option<Option<String>> {
        let identity = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        if !visiting.insert(identity.clone()) {
            return None;
        }
        let value = read_toml_value(path)?;
        let mut mode = task_mode_from_value(&value, name);
        for include in include_entries(&value) {
            if include.path.trim().is_empty() {
                visiting.remove(&identity);
                return None;
            }
            let Some(parent) = path.parent() else {
                visiting.remove(&identity);
                return None;
            };
            let resolved = if Path::new(&include.path).is_absolute() {
                PathBuf::from(&include.path)
            } else {
                parent.join(&include.path)
            };
            if !resolved.is_file() {
                if include.optional {
                    continue;
                }
                visiting.remove(&identity);
                return None;
            }
            match walk(&resolved, name, visiting) {
                None => {
                    visiting.remove(&identity);
                    return None;
                }
                Some(Some(child)) => mode = Some(child),
                Some(None) => {}
            }
        }
        visiting.remove(&identity);
        Some(mode)
    }
    walk(path, name, &mut BTreeSet::new()).flatten()
}

struct IncludePeek {
    path: String,
    optional: bool,
}

fn include_entries(value: &toml::Value) -> Vec<IncludePeek> {
    let Some(entries) = value
        .get("manifest")
        .and_then(|manifest| manifest.get("include"))
        .and_then(toml::Value::as_array)
    else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|entry| {
            if let Some(path) = entry.as_str() {
                return Some(IncludePeek {
                    path: path.to_owned(),
                    optional: false,
                });
            }
            let table = entry.as_table()?;
            Some(IncludePeek {
                path: table.get("path")?.as_str()?.to_owned(),
                optional: table
                    .get("optional")
                    .and_then(toml::Value::as_bool)
                    .unwrap_or(false),
            })
        })
        .collect()
}

fn read_toml_value(path: &Path) -> Option<toml::Value> {
    let text = std::fs::read_to_string(path).ok()?;
    toml::from_str(&text).ok()
}

fn task_mode_from_toml_file(path: &Path, name: &str) -> Option<String> {
    task_mode_from_value(&read_toml_value(path)?, name)
}

fn task_mode_from_value(value: &toml::Value, name: &str) -> Option<String> {
    value
        .get("tasks")?
        .get(name)?
        .get("mode")?
        .as_str()
        .map(str::to_owned)
}

fn first_task_arg(args: &[String]) -> Option<&str> {
    let mut rest = match args
        .iter()
        .position(|arg| arg == effigy_core::task_args::TASK_ARGS_BOUNDARY)
    {
        Some(index) => &args[index + 1..],
        None => args,
    };
    while rest
        .first()
        .is_some_and(|arg| effigy_core::task_args::COMPAT_TRAILING_FLAGS.contains(&arg.as_str()))
    {
        rest = &rest[1..];
    }
    rest.first().map(String::as_str)
}

/// A task invocation that only reads: `--plan`, `--help` or `-h` before any
/// `--`. It still fills a missing pin like any other run, but it is not a
/// Prepare path, so it never refreshes one.
pub(crate) fn is_read_only_task_invocation(task: &effigy_cli::TaskInvocation) -> bool {
    task.args
        .iter()
        .take_while(|arg| arg.as_str() != "--")
        .any(|arg| matches!(arg.as_str(), "--plan" | "--help" | "-h"))
}

pub(super) fn run_bundle(args: BundleArgs) -> Result<String, RunnerError> {
    match args.subcommand {
        BundleSubcommand::Inspect => {
            run_active_bundle_inspect(args.repo_override.as_deref(), args.output_json)
        }
        BundleSubcommand::Sync => run_bundle_sync(args.output_json),
    }
}

fn run_active_bundle_inspect(
    repo_override: Option<&Path>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let manifest_path = match repo_override {
        Some(repo_root) => discover_bundle_manifest_path(repo_root)?,
        None => {
            let cwd = crate::runner::command_context::active_invocation_cwd()?;
            discover_bundle_manifest_path(&cwd)?
        }
    };
    let Some(report) = inspect_bundle_source(&manifest_path)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?
    else {
        return Err(RunnerError::task_invocation(
            "current repo does not declare a `[bundle]` source".to_owned(),
        ));
    };

    if output_json {
        return Ok(json!({
            "schema": "effigy.bundle.inspect.v1",
            "schema_version": 1,
            "ok": true,
            "mode": "active-source",
            "bundle": null,
            "source": {
                "source_type": report.source_type,
                "source_path": report.source_path,
                "local_path": report.local_path,
                "version_hint": report.version_hint,
                "stale": report.stale,
                "manifest_path": manifest_path,
            }
        })
        .to_string());
    }

    Ok([
        format!("[bundle] source {}", source_type_label(report.source_type)),
        format!("source={}", report.source_path.display()),
        format!("manifest={}", manifest_path.display()),
        format!("local_path={}", report.local_path.display()),
        format!(
            "version={}",
            report.version_hint.as_deref().unwrap_or("unavailable")
        ),
        format!("stale={}", report.stale),
    ]
    .join("\n"))
}

fn run_bundle_sync(output_json: bool) -> Result<String, RunnerError> {
    let cwd = crate::runner::command_context::active_invocation_cwd()?;
    let manifest_path = discover_bundle_manifest_path(&cwd)?;
    let Some(report) = sync_bundle_source(&manifest_path, &source_preparation()?)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?
    else {
        return Err(RunnerError::task_invocation(
            "current repo does not declare a `[bundle]` source or catalog sources".to_owned(),
        ));
    };

    if output_json {
        return Ok(json!({
            "schema": "effigy.bundle.sync.v1",
            "schema_version": 1,
            "ok": true,
            "source_type": report.source_type,
            "source_path": report.source_path,
            "local_path": report.local_path,
            "version_hint": report.version_hint,
            "changed": report.changed,
            "applicable": report.applicable,
            "manifest_path": manifest_path,
            "sources": report.sources,
        })
        .to_string());
    }

    let mut lines = vec![format!(
        "[bundle] sync {}",
        source_type_label(report.source_type)
    )];
    lines.push(format!("source={}", report.source_path.display()));
    lines.push(format!("manifest={}", manifest_path.display()));
    if let Some(local_path) = &report.local_path {
        lines.push(format!("local_path={}", local_path.display()));
    }
    if let Some(version_hint) = &report.version_hint {
        lines.push(format!("version={version_hint}"));
    }
    if !report.applicable {
        lines.push("status=not-applicable".to_owned());
        lines.push("current bundle source is local; nothing to refresh".to_owned());
        return Ok(lines.join("\n"));
    }
    lines.push(format!(
        "status={}",
        if report.changed {
            "refreshed"
        } else {
            "unchanged"
        }
    ));
    Ok(lines.join("\n"))
}

static SOURCE_CANCEL: std::sync::OnceLock<effigy_kernel::ctx::CancelToken> =
    std::sync::OnceLock::new();

pub(crate) fn set_source_cancel(token: effigy_kernel::ctx::CancelToken) {
    let _ = SOURCE_CANCEL.set(token);
}
fn source_preparation() -> Result<effigy_manifest::SourcePreparation, RunnerError> {
    let home = match crate::runner::command_context::active_runtime_context() {
        Some(context) => context.paths().home.clone(),
        None => effigy_kernel::root::StateRoot::home_from_env(),
    }
    .ok_or_else(|| {
        RunnerError::task_invocation("source preparation requires an absolute HOME".to_owned())
    })?;
    let executable =
        std::env::current_exe().map_err(|err| RunnerError::task_invocation(err.to_string()))?;
    let next = effigy_kernel::outcome::Invocation::validated(
        vec!["bundle".into(), "sync".into()],
        |args| args == ["bundle", "sync"],
    )
    .map_err(|err| RunnerError::task_invocation(format!("{err:?}")))?;
    Ok(effigy_manifest::SourcePreparation {
        home,
        executable,
        cancel: SOURCE_CANCEL.get().cloned().unwrap_or_default(),
        next,
        budget: std::time::Duration::from_secs(120),
        progress: source_progress_steps(),
    })
}
/// A source's live step, and whether it snapshots a local path rather than
/// fetching over the network.
struct RunningSource {
    step: Step,
    local: bool,
}

/// Draw each source acquisition as a live step: `fetching bundle decodelabs
/// main`, then one line saying what now holds. Warnings and notes print above
/// any spinner. The engine reports the events; this plane decides how they
/// look, so `--json` and piped output stay silent on the step lines.
fn source_progress_steps() -> SourceProgress {
    let running: Mutex<BTreeMap<String, RunningSource>> = Mutex::new(BTreeMap::new());
    SourceProgress::to(move |event| {
        let mut running = running.lock().unwrap_or_else(PoisonError::into_inner);
        draw_source_event(&mut running, event);
    })
}

/// Turn one engine event into what the operator sees: a step started, a step
/// ended, or a line above any spinner.
fn draw_source_event(running: &mut BTreeMap<String, RunningSource>, event: SourceProgressEvent) {
    match event {
        SourceProgressEvent::Started {
            source,
            subject,
            revision,
            local,
        } => {
            let progress = source_progress(&source, &subject, &revision, local);
            let step = effigy_ui::steps::live_step(progress);
            running.insert(source, RunningSource { step, local });
        }
        SourceProgressEvent::Finished {
            source,
            subject,
            kept,
            failure,
        } => {
            let running = running.remove(&source);
            let local = running.as_ref().is_some_and(|running| running.local);
            let (ok, summary) = finished_summary(
                &source,
                &subject,
                kept.as_deref(),
                failure.as_deref(),
                local,
            );
            match running {
                Some(running) if ok => running.step.succeed(&summary),
                Some(running) => running.step.fail(&summary),
                None if !ok => effigy_ui::steps::print_line(&format!("[bundle] {summary}")),
                None => {}
            }
        }
        SourceProgressEvent::Warning { message } | SourceProgressEvent::Note { message } => {
            effigy_ui::steps::print_line(&format!("[bundle] {message}"));
        }
    }
}

/// The live state of one source acquisition. The engine reports only a start
/// and a finish, so the step waits on the exact source and revision it was
/// handed and shows the elapsed wait until the finish event arrives.
pub(in crate::runner) fn source_progress(
    source: &str,
    subject: &str,
    revision: &str,
    local: bool,
) -> ProgressSource {
    ProgressSource::waiting_for(source_awaiting(source, subject, revision, local))
}

/// What a source acquisition waits on, named as the operator would name it:
/// `bundle `decodelabs` at main`, or the snapshot of a local path.
pub(in crate::runner) fn source_awaiting(
    source: &str,
    subject: &str,
    revision: &str,
    local: bool,
) -> String {
    let noun = if source == "bundle" {
        "bundle"
    } else {
        "catalog"
    };
    if local {
        return format!("the local snapshot of {noun} `{subject}`");
    }
    format!("{noun} `{subject}` at {revision}")
}

/// The one line a finished acquisition leaves: what was fetched or snapshotted,
/// or why it was not and which pin still holds.
pub(in crate::runner) fn finished_summary(
    source: &str,
    subject: &str,
    kept: Option<&str>,
    failure: Option<&str>,
    local: bool,
) -> (bool, String) {
    let (done, failed) = if local {
        ("snapshotted local", "could not snapshot local")
    } else {
        ("fetched", "could not fetch")
    };
    match (failure, kept) {
        (Some(failure), _) => (false, format!("{failed} {source} `{subject}`: {failure}")),
        (None, Some(pin)) => (true, format!("kept {source} `{subject}` at {pin}")),
        (None, None) => (true, format!("{done} {source} `{subject}`")),
    }
}

/// Prepare the nearest manifest at or above `root` (contract 020).
pub(in crate::runner) fn prepare_sources_at(root: &Path) -> Result<(), RunnerError> {
    let Some(manifest) = root
        .ancestors()
        .map(|root| root.join(TASK_MANIFEST_FILE))
        .find(|manifest| manifest.is_file())
    else {
        return Ok(());
    };
    prepare_sources_for_manifest(&manifest)
}

/// Acquire missing bundle and catalog sources and refresh a stale pin through
/// the kernel Prepare owner (contract 020). Nothing to do is the cheap path:
/// `prepare_bundle_sources` returns before any gate or file creation.
pub(in crate::runner) fn prepare_bundle_sources_for_manifest(
    manifest: &Path,
) -> Result<(), RunnerError> {
    effigy_manifest::prepare_bundle_sources(manifest, &source_preparation()?)
        .map_err(|err| RunnerError::task_invocation(err.to_string()))?;
    Ok(())
}

/// Clone each missing pinned `[sources.<name>]` working copy. An existing
/// checkout is inspected and never reset, fetched forward or cleaned.
pub(in crate::runner) fn prepare_pinned_sources_for_manifest(
    manifest: &Path,
) -> Result<(), RunnerError> {
    effigy_manifest::prepare_pinned_sources(manifest)
        .map_err(|err| RunnerError::task_invocation(err.to_string()))?;
    Ok(())
}

/// Clone the pinned `[sources.<name>]` working copies that are missing, and
/// nothing else: a source this checkout already has costs no `git` call.
fn prepare_missing_pinned_sources_for_manifest(manifest: &Path) -> Result<(), RunnerError> {
    effigy_manifest::prepare_missing_pinned_sources(manifest)
        .map_err(|err| RunnerError::task_invocation(err.to_string()))
}

/// Prepare every missing pinned source for bootstrap and the first `dev`
/// (contract 020): bundle and catalog sources first, then extra sources.
pub(in crate::runner) fn prepare_sources_for_manifest(manifest: &Path) -> Result<(), RunnerError> {
    prepare_bundle_sources_for_manifest(manifest)?;
    prepare_pinned_sources_for_manifest(manifest)
}

/// Any other task run acquires only what this checkout is missing and never
/// moves a pin (contract 020 §Behaviour rule 1): a fresh worktree runs its
/// tasks, and lists them, instead of refusing on an unprepared source.
pub(in crate::runner) fn prepare_missing_sources_for_manifest(
    manifest: &Path,
) -> Result<(), RunnerError> {
    effigy_manifest::prepare_missing_bundle_sources(manifest, &source_preparation()?)
        .map_err(|err| RunnerError::task_invocation(err.to_string()))?;
    prepare_missing_pinned_sources_for_manifest(manifest)
}

fn discover_bundle_manifest_path(root_or_cwd: impl AsRef<Path>) -> Result<PathBuf, RunnerError> {
    let root_or_cwd = root_or_cwd.as_ref();
    if root_or_cwd.is_dir() {
        let direct = root_or_cwd.join(TASK_MANIFEST_FILE);
        if direct.is_file() {
            return Ok(direct);
        }
    }
    for ancestor in root_or_cwd.ancestors() {
        let manifest_path = ancestor.join(TASK_MANIFEST_FILE);
        if manifest_path.is_file() {
            return Ok(manifest_path);
        }
    }
    Err(RunnerError::task_invocation(format!(
        "no `effigy.toml` found at or above {}",
        root_or_cwd.display()
    )))
}

fn source_type_label(source_type: BundleSourceType) -> &'static str {
    match source_type {
        BundleSourceType::Path => "path",
        BundleSourceType::Git => "git",
        BundleSourceType::Oci => "oci",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use effigy_context::{CapturedEnv, EffigyRuntimeContext};

    #[test]
    fn task_local_status_logs_and_stop_are_not_prepare_or_pin_paths() {
        use effigy_cli::{Command, TaskInvocation};

        fn task(name: &str, args: &[&str]) -> TaskInvocation {
            TaskInvocation {
                name: name.to_owned(),
                args: args.iter().map(|arg| (*arg).to_owned()).collect(),
            }
        }

        for verb in ["status", "logs", "stop"] {
            for name in ["dev", "api/dev", "serve"] {
                let invocation = task(name, &[verb]);
                assert!(
                    is_task_local_session_invocation(&invocation),
                    "{name} {verb} is a session companion"
                );
                assert!(
                    !is_source_prepare_command(&Command::Task(invocation)),
                    "{name} {verb} is not a Prepare path"
                );

                let joined = TaskInvocation {
                    name: name.to_owned(),
                    args: effigy_core::task_args::join_task_args(Vec::new(), vec![verb.to_owned()]),
                };
                assert!(
                    is_task_local_session_invocation(&joined),
                    "{name} {verb} after the CLI boundary is a session companion"
                );
                assert!(
                    !is_source_prepare_command(&Command::Task(joined)),
                    "{name} {verb} after the CLI boundary is not a Prepare path"
                );
            }
        }

        let trailing_json = TaskInvocation {
            name: "dev".to_owned(),
            args: effigy_core::task_args::join_task_args(
                Vec::new(),
                vec!["--json".to_owned(), "status".to_owned()],
            ),
        };
        assert!(is_task_local_session_invocation(&trailing_json));
        assert!(!is_source_prepare_command(&Command::Task(trailing_json)));

        let hello = task("hello", &[]);
        assert!(!is_task_local_session_invocation(&hello));
        assert!(!is_source_prepare_command(&Command::Task(hello)));

        let dev = task("dev", &[]);
        assert!(!is_task_local_session_invocation(&dev));
        assert!(is_source_prepare_command(&Command::Task(dev)));

        let joined_dev = TaskInvocation {
            name: "dev".to_owned(),
            args: effigy_core::task_args::join_task_args(Vec::new(), Vec::new()),
        };
        assert!(!is_task_local_session_invocation(&joined_dev));
        assert!(is_source_prepare_command(&Command::Task(joined_dev)));
    }

    #[test]
    fn only_a_tui_task_skips_missing_pin_acquisition_for_session_verbs() {
        use effigy_cli::TaskInvocation;

        let directory = tempfile::tempdir().expect("fixture directory");
        let manifest = directory.path().join("effigy.toml");
        std::fs::write(
            &manifest,
            "[catalog.members]\napi = \"api\"\n\n[tasks.dev]\nmode = \"tui\"\n\n[tasks.need]\nrun = \"sh -c 'cat book/README'\"\n\n[tasks.serve]\nrun = \"echo serve\"\n",
        )
        .expect("write overlay");
        let api = directory.path().join("api");
        std::fs::create_dir(&api).expect("api catalog");
        std::fs::write(
            api.join("effigy.toml"),
            "[catalog]\nalias = \"api\"\n\n[tasks.dev]\nrun = \"echo api-dev-ran\"\n",
        )
        .expect("write member catalog");

        let status = TaskInvocation {
            name: "dev".to_owned(),
            args: vec!["status".to_owned()],
        };
        assert!(
            is_managed_tui_session_invocation(&status, &manifest),
            "dev status on a tui overlay skips the missing pin"
        );

        let joined = TaskInvocation {
            name: "dev".to_owned(),
            args: effigy_core::task_args::join_task_args(Vec::new(), vec!["status".to_owned()]),
        };
        assert!(is_managed_tui_session_invocation(&joined, &manifest));

        let need_status = TaskInvocation {
            name: "need".to_owned(),
            args: vec!["status".to_owned()],
        };
        assert!(is_task_local_session_invocation(&need_status));
        assert!(!is_managed_tui_session_invocation(&need_status, &manifest));

        let serve_status = TaskInvocation {
            name: "serve".to_owned(),
            args: vec!["status".to_owned()],
        };
        assert!(is_task_local_session_invocation(&serve_status));
        assert!(!is_managed_tui_session_invocation(&serve_status, &manifest));

        let overlay = directory
            .path()
            .join(effigy_core::repo_markers::LOCAL_OVERLAY_FILE);
        std::fs::write(&overlay, "[tasks.serve]\nmode = \"tui\"\n").expect("write local overlay");
        assert!(is_managed_tui_session_invocation(&serve_status, &manifest));

        let api_status = TaskInvocation {
            name: "api/dev".to_owned(),
            args: vec!["status".to_owned()],
        };
        assert!(is_task_local_session_invocation(&api_status));
        assert!(
            !is_managed_tui_session_invocation(&api_status, &manifest),
            "a prefixed non-tui catalog task does not inherit the root leaf's tui mode"
        );
        let joined_api = TaskInvocation {
            name: "api/dev".to_owned(),
            args: effigy_core::task_args::join_task_args(Vec::new(), vec!["status".to_owned()]),
        };
        assert!(!is_managed_tui_session_invocation(&joined_api, &manifest));

        std::fs::write(
            api.join(effigy_core::repo_markers::LOCAL_OVERLAY_FILE),
            "[tasks.dev]\nmode = \"tui\"\n",
        )
        .expect("write member overlay");
        assert!(
            is_managed_tui_session_invocation(&api_status, &manifest),
            "the selected catalog's own tui overlay skips the missing pin"
        );
    }

    #[test]
    fn an_included_fragment_tui_task_skips_missing_pin_acquisition_for_session_verbs() {
        use effigy_cli::TaskInvocation;

        let directory = tempfile::tempdir().expect("fixture directory");
        let infra = directory.path().join("infra");
        std::fs::create_dir(&infra).expect("infra");
        std::fs::write(
            infra.join("tasks.toml"),
            "[tasks.dev]\nmode = \"tui\"\nconcurrent = [{ name = \"api\", run = \"true\", start = 1 }]\n",
        )
        .expect("write fragment");
        let manifest = directory.path().join("effigy.toml");
        std::fs::write(
            &manifest,
            "[manifest]\ninclude = [\"infra/tasks.toml\"]\n\n[tasks.need]\nrun = \"echo need\"\n",
        )
        .expect("write root");

        let status = TaskInvocation {
            name: "dev".to_owned(),
            args: vec!["status".to_owned()],
        };
        assert!(
            is_managed_tui_session_invocation(&status, &manifest),
            "dev status on an included tui fragment skips the missing pin"
        );
        let joined = TaskInvocation {
            name: "dev".to_owned(),
            args: effigy_core::task_args::join_task_args(Vec::new(), vec!["status".to_owned()]),
        };
        assert!(is_managed_tui_session_invocation(&joined, &manifest));

        let need_status = TaskInvocation {
            name: "need".to_owned(),
            args: vec!["status".to_owned()],
        };
        assert!(is_task_local_session_invocation(&need_status));
        assert!(!is_managed_tui_session_invocation(&need_status, &manifest));
    }

    #[test]
    fn source_frames_name_each_source_and_its_elapsed_wait() {
        let bundle = source_progress("bundle", "decodelabs", "main", false);
        let catalog = source_progress("catalog:acme", "acme", "main", false);
        let snapshot = source_progress("bundle", "decodelabs", "path", true);
        let seconds = std::time::Duration::from_secs;
        assert_eq!(
            bundle.frame_at(seconds(0)),
            "waiting for bundle `decodelabs` at main; 0s elapsed"
        );
        assert_eq!(
            catalog.frame_at(seconds(3)),
            "waiting for catalog `acme` at main; 3s elapsed"
        );
        assert_eq!(
            bundle.frame_at(seconds(9)),
            "waiting for bundle `decodelabs` at main; 9s elapsed"
        );
        assert_eq!(
            snapshot.frame_at(seconds(7)),
            "waiting for the local snapshot of bundle `decodelabs`; 7s elapsed"
        );
    }

    #[test]
    fn source_steps_name_the_bundle_its_revision_and_the_outcome() {
        assert_eq!(
            source_awaiting("bundle", "decodelabs", "main", false),
            "bundle `decodelabs` at main"
        );
        assert_eq!(
            source_awaiting("catalog:acme", "acme", "main", false),
            "catalog `acme` at main"
        );
        assert_eq!(
            finished_summary("bundle", "decodelabs", None, None, false),
            (true, "fetched bundle `decodelabs`".to_owned())
        );
        assert_eq!(
            finished_summary(
                "bundle",
                "decodelabs",
                Some("1a2b3c4d"),
                Some("bundle update failed: the remote refused the fetch"),
                false,
            ),
            (
                false,
                "could not fetch bundle `decodelabs`: bundle update failed: the remote refused the fetch"
                    .to_owned()
            )
        );
    }

    #[test]
    fn a_local_path_source_is_a_snapshot_not_a_fetch() {
        assert_eq!(
            source_awaiting("bundle", "decodelabs", "path", true),
            "the local snapshot of bundle `decodelabs`"
        );
        assert_eq!(
            finished_summary("bundle", "decodelabs", None, None, true),
            (true, "snapshotted local bundle `decodelabs`".to_owned())
        );
        assert_eq!(
            finished_summary(
                "catalog:extra",
                "extra",
                None,
                Some("the directory is gone"),
                true
            ),
            (
                false,
                "could not snapshot local catalog:extra `extra`: the directory is gone".to_owned()
            )
        );
    }

    #[test]
    fn a_source_step_ends_once_with_a_check_or_a_cross() {
        let ((), lines) = effigy_ui::steps::capture_lines(|| {
            let mut running = BTreeMap::new();
            draw_source_event(
                &mut running,
                SourceProgressEvent::Started {
                    source: "bundle".to_owned(),
                    subject: "decodelabs".to_owned(),
                    revision: "path".to_owned(),
                    local: true,
                },
            );
            draw_source_event(
                &mut running,
                SourceProgressEvent::Finished {
                    source: "bundle".to_owned(),
                    subject: "decodelabs".to_owned(),
                    kept: None,
                    failure: None,
                },
            );
            draw_source_event(
                &mut running,
                SourceProgressEvent::Started {
                    source: "catalog:extra".to_owned(),
                    subject: "extra".to_owned(),
                    revision: "path".to_owned(),
                    local: true,
                },
            );
            draw_source_event(
                &mut running,
                SourceProgressEvent::Finished {
                    source: "catalog:extra".to_owned(),
                    subject: "extra".to_owned(),
                    kept: None,
                    failure: Some("the directory is gone".to_owned()),
                },
            );
        });
        assert_eq!(
            lines,
            [
                "✓ snapshotted local bundle `decodelabs`",
                "✗ could not snapshot local catalog:extra `extra`: the directory is gone",
            ]
        );
    }

    #[test]
    fn bundle_sync_reports_missing_bundle_config() {
        let directory = tempfile::tempdir().expect("fixture directory");
        let tmp = directory.path().to_path_buf();
        std::fs::create_dir_all(&tmp).expect("mkdir");
        std::fs::write(tmp.join("effigy.toml"), "[tasks.dev]\nrun = \"echo hi\"\n").expect("write");
        let context = EffigyRuntimeContext::builder()
            .cwd_override(Some(tmp.clone()))
            .captured_env(CapturedEnv {
                home: Some(tmp.as_os_str().into()),
                ..CapturedEnv::default()
            })
            .capture_lossy()
            .expect("capture context");
        let error = crate::runner::command_context::with_runtime_context(&context, || {
            run_bundle_sync(false)
        })
        .expect_err("reject missing bundle");
        assert!(error
            .to_string()
            .contains("does not declare a `[bundle]` source"));
        let _ = std::fs::remove_dir_all(tmp);
    }

    #[test]
    fn bundle_sync_json_prepares_path_bundle() {
        let directory = tempfile::tempdir().expect("fixture directory");
        let tmp = directory.path().to_path_buf();
        std::fs::create_dir_all(&tmp).expect("mkdir");
        std::fs::write(
            tmp.join("effigy.toml"),
            "[bundle]\nbase = { type = \"path\", dir = \"bundles/acme\" }\n",
        )
        .expect("write");
        let bundle = tmp.join("bundles/acme");
        std::fs::create_dir_all(&bundle).expect("bundle directory");
        std::fs::write(bundle.join("bundle.toml"), "[bundle]\nname = \"test\"\n")
            .expect("descriptor");
        std::fs::write(
            bundle.join("effigy.toml"),
            "[tasks.hello]\nrun = \"echo hello\"\n",
        )
        .expect("defaults");
        let context = EffigyRuntimeContext::builder()
            .cwd_override(Some(tmp.clone()))
            .captured_env(CapturedEnv {
                home: Some(tmp.as_os_str().into()),
                ..CapturedEnv::default()
            })
            .capture_lossy()
            .expect("capture context");
        let rendered = crate::runner::command_context::with_runtime_context(&context, || {
            run_bundle_sync(true)
        })
        .expect("bundle sync");
        assert!(rendered.contains("\"schema\":\"effigy.bundle.sync.v1\""));
        assert!(rendered.contains("\"source_type\":\"path\""));
        assert!(rendered.contains("\"applicable\":true"));
        let _ = std::fs::remove_dir_all(tmp);
    }

    #[test]
    fn bundle_inspect_reports_active_source_metadata() {
        let directory = tempfile::tempdir().expect("fixture directory");
        let tmp = directory.path().to_path_buf();
        std::fs::create_dir_all(&tmp).expect("mkdir");
        std::fs::write(
            tmp.join("effigy.toml"),
            "[bundle]\nbase = { type = \"path\", dir = \"bundles/acme\" }\n",
        )
        .expect("write");
        let context = EffigyRuntimeContext::builder()
            .cwd_override(Some(tmp.clone()))
            .captured_env(CapturedEnv {
                home: Some(tmp.as_os_str().into()),
                ..CapturedEnv::default()
            })
            .capture_lossy()
            .expect("capture context");
        let rendered = crate::runner::command_context::with_runtime_context(&context, || {
            run_active_bundle_inspect(None, true)
        })
        .expect("bundle inspect");
        assert!(rendered.contains("\"mode\":\"active-source\""));
        assert!(rendered.contains("\"source_type\":\"path\""));
        assert!(rendered.contains("\"stale\":false"));
        let _ = std::fs::remove_dir_all(tmp);
    }
}
