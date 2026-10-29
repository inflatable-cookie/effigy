use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Instant;

use effigy_cli::ContainerProfileSubcommand;
use effigy_containers::colima::{
    colima_start_command_for_profile, colima_start_command_for_profile_with_disk,
    managed_colima_profile_resources, managed_colima_profile_resources_with_disk,
    prepare_managed_colima_profile_name, prepare_managed_colima_profile_name_with_disk,
};
use effigy_containers::exec::{recover_colima_runtime, reset_colima_runtime, ColimaRecoveryReport};
use serde::Deserialize;
use serde_json::json;

use super::data::maybe_confirm_destructive_container_action;
use super::RunnerError;
use crate::runner::command_context::resolve_active_repo_root;

const DEFAULT_PROFILE: &str = "effigy";
const BYTES_PER_GIB: u64 = 1024 * 1024 * 1024;
/// One absolute deadline for the prune's three bounded VM probes (inspect,
/// prune, inspect). Pruning a profile that holds the leak takes seconds.
/// The next step after a disk size the recreate prompt or flag cannot use.
const DISK_NEXT: &str = "rerun with `--disk <GiB>` set to a positive whole number, or press Enter at the prompt for the managed default";
const PROFILE_PRUNE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(90);

#[derive(Debug, Clone, Deserialize)]
struct ColimaListRow {
    name: String,
    status: String,
    cpus: u64,
    memory: u64,
    disk: u64,
    #[serde(default)]
    runtime: Option<String>,
}

#[derive(Debug, Clone)]
struct ProfileStatus {
    profile: String,
    row: Option<ColimaListRow>,
    target_memory_gib: Option<u64>,
    target_swap_gib: Option<u64>,
    target_disk_gib: Option<u64>,
}

pub(super) fn run_container_profile_command(
    repo_override: Option<PathBuf>,
    subcommand: &ContainerProfileSubcommand,
    output_json: bool,
) -> Result<String, RunnerError> {
    // Recovery selects the environment through the manifest, so it keeps the
    // repository and system selection the retired `effigy system` verbs had.
    match subcommand {
        ContainerProfileSubcommand::Repair { system } => {
            return run_container_profile_runtime_recovery(
                repo_override,
                system.as_deref(),
                output_json,
                true,
            );
        }
        ContainerProfileSubcommand::ResetRuntime { system } => {
            return run_container_profile_runtime_recovery(
                repo_override,
                system.as_deref(),
                output_json,
                false,
            );
        }
        _ => {}
    }
    if repo_override.is_some() {
        return Err(RunnerError::task_invocation(
            "`effigy container profile` does not accept `--repo`; Colima profiles are machine-level runtime state\nnext: drop `--repo` and rerun the profile command, for example `effigy container profile status`",
        ));
    }
    match subcommand {
        ContainerProfileSubcommand::Status { profile } => {
            run_container_profile_status(profile.as_deref(), output_json)
        }
        ContainerProfileSubcommand::Resize { profile } => {
            run_container_profile_resize(profile.as_deref(), output_json)
        }
        ContainerProfileSubcommand::Purge { profile, yes } => {
            run_container_profile_purge(profile.as_deref(), *yes, output_json)
        }
        ContainerProfileSubcommand::Recreate {
            profile,
            disk_gib,
            yes,
        } => run_container_profile_recreate(profile.as_deref(), *disk_gib, *yes, output_json),
        ContainerProfileSubcommand::PruneHealthPidFiles { profile } => {
            run_container_profile_prune_health_pid_files(profile.as_deref(), output_json)
        }
        ContainerProfileSubcommand::Repair { .. }
        | ContainerProfileSubcommand::ResetRuntime { .. } => {
            unreachable!("recovery verbs return before the machine-level profile check")
        }
    }
}

/// `container profile prune-health-pid-files`: delete only the stale nerdctl
/// health-check pid files a running profile leaked into `/run`, and report the
/// counts around it (effigy#291).
///
/// This is machine-level runtime maintenance like `purge`, so it takes a
/// profile and never a repository. It refuses a profile that is not running:
/// `colima ssh` would either start nothing or fail, and a stopped VM keeps no
/// `/run` pressure. Nothing outside the exact containerd task directory and
/// pid-file pattern is read or removed.
fn run_container_profile_prune_health_pid_files(
    profile: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let profile = resolve_profile(profile);
    let status = inspect_profile(&profile)?;
    if status
        .row
        .as_ref()
        .is_none_or(|row| row.status != "Running")
    {
        let state = status
            .row
            .as_ref()
            .map_or("not listed by `colima list`".to_owned(), |row| {
                format!("reported as {}", row.status)
            });
        return Err(RunnerError::task_invocation(format!(
            "`effigy container profile prune-health-pid-files` needs Colima profile `{profile}` running; the profile is {state}\nnext: start it with `colima start --profile {profile}` or `effigy container up`, then rerun this command"
        )));
    }
    let deadline = Instant::now() + PROFILE_PRUNE_TIMEOUT;
    let progress = ProfileProgress::new(
        output_json,
        profile_probe(&profile),
        format!("pruning stale health-check pid files in Colima profile `{profile}`"),
    );
    progress.observe(&status);
    let result = effigy_containers::exec::prune_stale_health_check_pidfiles(
        &profile,
        Path::new("."),
        deadline,
    )
    .map_err(|error| RunnerError::task_invocation(error.to_string()))
    .map(|prune| render_profile_prune_health_pid_files(&profile, prune, output_json));
    match &result {
        Ok(_) => progress.finish_clear(),
        Err(_) => progress.fail(&format!(
            "stale health-check pid files were not pruned from Colima profile `{profile}`; `effigy container profile status` shows what it reports now"
        )),
    }
    result
}

/// `container profile repair` / `reset-runtime`: the Colima runtime belonging
/// to the environment's profile, recovered in place or stopped without
/// deleting it.
fn run_container_profile_runtime_recovery(
    repo_override: Option<PathBuf>,
    system: Option<&str>,
    output_json: bool,
    repair: bool,
) -> Result<String, RunnerError> {
    let resolved = resolve_active_repo_root(repo_override)?;
    let repo_root = resolved.resolved_root;
    let surface = if repair {
        "container profile repair"
    } else {
        "container profile reset-runtime"
    };
    let container_name = crate::runner::workspace_command::resolve_public_workspace_container(
        &repo_root, system, None, surface,
    )?;
    let policy =
        super::policy::load_resolved_container_policy(&repo_root, container_name.as_deref())?;
    // The recovery's own stop, pidfile, start and health steps run inside
    // `effigy-containers` and report nothing while they run, so this frame
    // carries the profile operation, the state Colima last reported for the
    // profile and the elapsed wait, and falls back to naming the exact probe
    // (effigy#461).
    let progress = ProfileProgress::new(
        output_json,
        profile_probe(&policy.profile),
        format!(
            "{} the Colima runtime of profile `{}`",
            if repair { "recovering" } else { "stopping" },
            policy.profile
        ),
    );
    if let Ok(status) = inspect_profile(&policy.profile) {
        progress.observe(&status);
    }
    let result = if repair {
        render_profile_runtime_recovery(
            recover_colima_runtime(&policy, &repo_root).map_err(Into::<RunnerError>::into),
            output_json,
        )
    } else {
        render_profile_runtime_recovery(
            reset_colima_runtime(&policy, &repo_root).map_err(Into::<RunnerError>::into),
            output_json,
        )
    };
    match &result {
        Ok(_) => progress.succeed(&format!(
            "{} the Colima runtime of profile `{}`",
            if repair { "recovered" } else { "stopped" },
            policy.profile
        )),
        Err(_) => progress.fail(&format!(
            "the Colima runtime of profile `{}` did not {}; run `effigy container profile status` to see what is left",
            policy.profile,
            if repair { "recover" } else { "stop" },
        )),
    }
    result
}

/// The live progress of one profile command (effigy#461): the profile
/// operation now running, how far through its Colima calls it is, and the last
/// state `colima list --json` reported for the profile.
struct ProfileProgress {
    source: Option<effigy_ui::steps::ProgressSource>,
    step: Option<effigy_ui::steps::Step>,
}

/// The exact probe every profile frame falls back to when the command has
/// published no state of its own: the one command that reports the profile.
fn profile_probe(profile: &str) -> String {
    format!("`colima list --json` to answer for profile `{profile}`")
}

impl ProfileProgress {
    /// `waiting_for` names the exact probe that reports this profile, and
    /// `operation` is what is running until the command says otherwise.
    fn new(output_json: bool, waiting_for: String, operation: String) -> Self {
        if output_json {
            return Self {
                source: None,
                step: None,
            };
        }
        let source = effigy_ui::steps::ProgressSource::waiting_for(waiting_for);
        source.update(operation, None, None);
        let step = effigy_ui::steps::live_step(source.clone());
        Self {
            source: Some(source),
            step: Some(step),
        }
    }

    /// The profile operation now running, with a known count when the command
    /// runs a known number of Colima calls.
    fn phase(&self, operation: impl Into<String>, count: Option<(u64, u64)>) {
        if let Some(source) = &self.source {
            source.update(operation, count, None);
        }
    }

    /// What `colima list --json` last reported for this profile.
    fn observe(&self, status: &ProfileStatus) {
        if let Some(source) = &self.source {
            source.event(profile_observation(status));
        }
    }

    /// A one-line ending: what now holds.
    fn succeed(&self, summary: &str) {
        if let Some(step) = &self.step {
            step.succeed(summary);
        }
    }

    /// A one-line ending naming what is wrong.
    fn fail(&self, summary: &str) {
        if let Some(step) = &self.step {
            step.fail(summary);
        }
    }

    /// Ends the step without a line: the command's own rendered block is the
    /// ending the operator reads.
    fn finish_clear(&self) {
        if let Some(step) = &self.step {
            step.finish_clear();
        }
    }

    /// The frame this progress renders after `elapsed`. Production passes the
    /// wall clock through its steps; a proof passes the time it means to show.
    #[cfg(test)]
    fn frame_at(&self, elapsed: std::time::Duration) -> String {
        self.source
            .as_ref()
            .expect("a drawn progress has a source")
            .frame_at(elapsed)
    }
}

/// What `colima list --json` last reported for the profile: its status, sizing
/// and the managed target when one applies.
fn profile_observation(status: &ProfileStatus) -> String {
    let Some(row) = &status.row else {
        return format!(
            "profile `{}` is not listed by `colima list --json`",
            status.profile
        );
    };
    format!(
        "profile `{}` reported {} with {} CPUs, {}GiB memory and {}GiB disk{}",
        status.profile,
        row.status,
        row.cpus,
        bytes_to_gib(row.memory),
        bytes_to_gib(row.disk),
        target_suffix(status.target_disk_gib),
    )
}

fn render_profile_runtime_recovery(
    result: Result<ColimaRecoveryReport, RunnerError>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let report = result?;
    if output_json {
        return serde_json::to_string_pretty(&serde_json::json!({
            "schema": "effigy.system.recover.v1",
            "schema_version": 1,
            "ok": true,
            "profile": report.profile,
            "steps": report.steps,
        }))
        .map_err(|error| RunnerError::task_invocation(error.to_string()));
    }
    Ok(report.steps.join("\n"))
}

fn run_container_profile_status(
    profile: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let profile = resolve_profile(profile);
    let status = inspect_profile(&profile)?;
    Ok(render_profile_status(&status, output_json))
}

fn run_container_profile_recreate(
    profile: Option<&str>,
    disk_gib: Option<u64>,
    yes: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    let profile = resolve_profile(profile);
    let Some(default_resources) = managed_colima_profile_resources(&profile) else {
        return Err(not_managed_profile_error("recreate", &profile));
    };
    let disk_gib = resolve_profile_recreate_disk_gib(
        &profile,
        disk_gib,
        default_resources.disk_gib,
        output_json,
        io::stdin().is_terminal(),
        io::stdout().is_terminal(),
    )?;
    let before = inspect_profile_with_disk(&profile, disk_gib)?;
    maybe_confirm_destructive_container_action(
        "`effigy container profile recreate`",
        &format!(
            "Recreate Colima profile `{profile}` with Effigy's managed sizing ({disk}GiB disk target). This deletes the profile VM, container runtime state, images, containers, and volumes. Export database/object-store data first if you need it.",
            disk = disk_gib.unwrap_or(default_resources.disk_gib)
        ),
        output_json,
        yes,
    )?;

    let progress = ProfileProgress::new(
        output_json,
        profile_probe(&profile),
        format!("reading the state Colima reports for profile `{profile}`"),
    );
    progress.observe(&before);
    let result = (|| -> Result<String, RunnerError> {
        let existed = before.row.is_some();
        if existed {
            progress.phase(format!("stopping Colima profile `{profile}`"), Some((1, 3)));
            run_colima_allow_failure(&["stop", "--profile", &profile], "colima stop")?;
            progress.phase(
                format!("deleting Colima profile `{profile}` and its runtime data"),
                Some((2, 3)),
            );
            run_colima(
                &["delete", "--profile", &profile, "--force", "--data"],
                "colima delete",
            )?;
        }
        progress.phase(
            format!("starting Colima profile `{profile}` with the managed sizing"),
            Some(if existed { (3, 3) } else { (1, 1) }),
        );
        prepare_managed_colima_profile_name_with_disk(&profile, disk_gib)
            .map_err(RunnerError::task_invocation)?;
        let start = colima_start_command_for_profile_with_disk(&profile, disk_gib);
        let start_args = start.args.iter().map(String::as_str).collect::<Vec<_>>();
        run_command(&start.program, &start_args, &start.label, false)?;

        progress.phase(
            format!("reading the state Colima reports for profile `{profile}`"),
            None,
        );
        let after = inspect_profile_with_disk(&profile, disk_gib)?;
        progress.observe(&after);
        Ok(render_profile_recreate(&before, &after, output_json))
    })();
    match &result {
        Ok(_) => progress.finish_clear(),
        Err(_) => progress.fail(&format!(
            "Colima profile `{profile}` was not recreated; `effigy container profile status` shows what Colima reports now"
        )),
    }
    result
}

fn run_container_profile_purge(
    profile: Option<&str>,
    yes: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    let profile = resolve_profile(profile);
    if managed_colima_profile_resources(&profile).is_none() {
        return Err(not_managed_profile_error("purge", &profile));
    }
    let before = inspect_profile(&profile)?;
    maybe_confirm_destructive_container_action(
        "`effigy container profile purge`",
        &format!(
            "Purge Colima profile `{profile}`. This deletes the profile VM, container runtime state, images, containers, and volumes. Effigy will not recreate or restart the profile."
        ),
        output_json,
        yes,
    )?;

    let progress = ProfileProgress::new(
        output_json,
        profile_probe(&profile),
        format!("reading the state Colima reports for profile `{profile}`"),
    );
    progress.observe(&before);
    let result = (|| -> Result<String, RunnerError> {
        if before.row.is_some() {
            progress.phase(format!("stopping Colima profile `{profile}`"), Some((1, 2)));
            run_colima_allow_failure(&["stop", "--profile", &profile], "colima stop")?;
            progress.phase(
                format!("deleting Colima profile `{profile}` and its runtime data"),
                Some((2, 2)),
            );
            run_colima(
                &["delete", "--profile", &profile, "--force", "--data"],
                "colima delete",
            )?;
        }

        progress.phase(
            format!("reading the state Colima reports for profile `{profile}`"),
            None,
        );
        let after = inspect_profile(&profile)?;
        progress.observe(&after);
        Ok(render_profile_purge(&before, &after, output_json))
    })();
    match &result {
        Ok(_) => progress.finish_clear(),
        Err(_) => progress.fail(&format!(
            "Colima profile `{profile}` was not purged; `effigy container profile status` shows what Colima reports now"
        )),
    }
    result
}

fn run_container_profile_resize(
    profile: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let profile = resolve_profile(profile);
    let resources = managed_colima_profile_resources(&profile)
        .ok_or_else(|| not_managed_profile_error("resize", &profile))?;
    let progress = ProfileProgress::new(
        output_json,
        profile_probe(&profile),
        format!("reading the state Colima reports for profile `{profile}`"),
    );
    let result = (|| -> Result<String, RunnerError> {
        let before = inspect_profile(&profile)?;
        progress.observe(&before);
        progress.phase(
            format!("preparing the managed configuration for Colima profile `{profile}`"),
            None,
        );
        prepare_managed_colima_profile_name(&profile).map_err(RunnerError::task_invocation)?;

        let was_running = before
            .row
            .as_ref()
            .is_some_and(|row| row.status.eq_ignore_ascii_case("running"));
        if was_running {
            progress.phase(format!("stopping Colima profile `{profile}`"), Some((1, 2)));
            run_colima(&["stop", "--profile", &profile], "colima stop")?;
        }

        progress.phase(
            format!("starting Colima profile `{profile}` with the managed sizing"),
            Some(if was_running { (2, 2) } else { (1, 1) }),
        );
        let start = colima_start_command_for_profile(&profile);
        let start_args = start.args.iter().map(String::as_str).collect::<Vec<_>>();
        run_command(&start.program, &start_args, &start.label, false)?;

        progress.phase(
            format!("reading the state Colima reports for profile `{profile}`"),
            None,
        );
        let after = inspect_profile(&profile)?;
        progress.observe(&after);
        let resized = after
            .row
            .as_ref()
            .map(|row| bytes_to_gib(row.disk) >= resources.disk_gib)
            .unwrap_or(false);
        if !resized {
            let disk = after
                .row
                .as_ref()
                .map(|row| bytes_to_gib(row.disk))
                .unwrap_or(0);
            return Err(RunnerError::task_invocation(format!(
            "Colima profile `{profile}` restarted but still reports disk {disk}GiB, below the managed target {}GiB\nnext: check `effigy container profile status`; `effigy container profile recreate --disk {} --yes` deletes local profile runtime data, so run it only if that is acceptable",
            resources.disk_gib,
            resources.disk_gib,
        )));
        }
        Ok(render_profile_resize(
            &before,
            &after,
            was_running,
            output_json,
        ))
    })();
    match &result {
        Ok(_) => progress.finish_clear(),
        Err(_) => progress.fail(&format!(
            "Colima profile `{profile}` was not resized; `effigy container profile status` shows what Colima reports now"
        )),
    }
    result
}

/// A profile command on a profile Effigy does not manage. The managed profile
/// is the only one these verbs may change, so the next step names it.
fn not_managed_profile_error(verb: &str, profile: &str) -> RunnerError {
    RunnerError::task_invocation(format!(
        "`effigy container profile {verb}` only supports the managed `{DEFAULT_PROFILE}` Colima profile; `{profile}` is not managed by Effigy\nnext: rerun it with `--profile {DEFAULT_PROFILE}`"
    ))
}

fn resolve_profile(profile: Option<&str>) -> String {
    profile
        .map(str::to_owned)
        .or_else(effigy_containers::user_global_colima_profile)
        .unwrap_or_else(|| DEFAULT_PROFILE.to_owned())
}

fn inspect_profile(profile: &str) -> Result<ProfileStatus, RunnerError> {
    inspect_profile_with_disk(profile, None)
}

fn inspect_profile_with_disk(
    profile: &str,
    disk_gib: Option<u64>,
) -> Result<ProfileStatus, RunnerError> {
    let rows = list_colima_profiles()?;
    let row = rows.into_iter().find(|row| row.name == profile);
    let resources = managed_colima_profile_resources_with_disk(profile, disk_gib);
    Ok(ProfileStatus {
        profile: profile.to_owned(),
        row,
        target_memory_gib: resources.map(|value| value.memory_gib),
        target_swap_gib: resources.map(|value| value.swap_gib),
        target_disk_gib: resources.map(|value| value.disk_gib),
    })
}

fn resolve_profile_recreate_disk_gib(
    profile: &str,
    explicit_disk_gib: Option<u64>,
    default_disk_gib: u64,
    output_json: bool,
    stdin_is_tty: bool,
    stdout_is_tty: bool,
) -> Result<Option<u64>, RunnerError> {
    if explicit_disk_gib.is_some() || output_json || !stdin_is_tty || !stdout_is_tty {
        return Ok(explicit_disk_gib);
    }

    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    prompt_profile_recreate_disk_gib(profile, default_disk_gib, &mut stdin, &mut stdout).map(Some)
}

fn prompt_profile_recreate_disk_gib<R: BufRead, W: Write>(
    profile: &str,
    default_disk_gib: u64,
    input: &mut R,
    output: &mut W,
) -> Result<u64, RunnerError> {
    write!(
        output,
        "Disk size for Colima profile `{profile}` in GiB [{default_disk_gib}]: "
    )
    .map_err(|error| RunnerError::task_invocation(format!("failed to write prompt: {error}")))?;
    output.flush().map_err(|error| {
        RunnerError::task_invocation(format!("failed to flush prompt: {error}"))
    })?;

    let mut line = String::new();
    input
        .read_line(&mut line)
        .map_err(|error| RunnerError::task_invocation(format!("failed to read prompt: {error}")))?;
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(default_disk_gib);
    }
    let value = trimmed.parse::<u64>().map_err(|_| {
        RunnerError::task_invocation(format!(
            "invalid Colima profile disk size `{trimmed}`; expected a positive integer GiB value\nnext: {DISK_NEXT}"
        ))
    })?;
    if value == 0 {
        return Err(RunnerError::task_invocation(format!(
            "invalid Colima profile disk size `0`; expected a positive integer GiB value\nnext: {DISK_NEXT}"
        )));
    }
    Ok(value)
}

fn list_colima_profiles() -> Result<Vec<ColimaListRow>, RunnerError> {
    let output = run_colima(&["list", "--json"], "colima list")?;
    parse_colima_list(&String::from_utf8_lossy(&output.stdout))
}

fn parse_colima_list(stdout: &str) -> Result<Vec<ColimaListRow>, RunnerError> {
    stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<ColimaListRow>(line).map_err(|error| {
                RunnerError::task_invocation(format!(
                    "failed to parse `colima list --json` row: {error}\nnext: run `colima list` to check that Colima answers, then rerun the command"
                ))
            })
        })
        .collect()
}

fn render_profile_status(status: &ProfileStatus, output_json: bool) -> String {
    let json = profile_status_json(status);
    if output_json {
        return json.to_string();
    }
    let Some(row) = &status.row else {
        return format!(
            "[info] Colima profile `{}` does not exist\nmanaged: {}",
            status.profile,
            yes_no(status.target_disk_gib.is_some()),
        );
    };
    let mut lines = vec![
        format!("[ok] Colima profile `{}` is {}", status.profile, row.status),
        format!("runtime: {}", row.runtime.as_deref().unwrap_or("unknown")),
        format!("cpus: {}", row.cpus),
        format!(
            "memory: {}GiB{}",
            bytes_to_gib(row.memory),
            target_suffix(status.target_memory_gib)
        ),
        format!(
            "disk: {}GiB{}",
            bytes_to_gib(row.disk),
            target_suffix(status.target_disk_gib)
        ),
        format!("managed: {}", yes_no(status.target_disk_gib.is_some())),
    ];
    if status_below_target(row.memory, status.target_memory_gib)
        || status_below_target(row.disk, status.target_disk_gib)
    {
        lines.push(format!(
            "next: `effigy container profile resize` will stop and restart `{}` with the managed sizing without deleting local profile data",
            status.profile
        ));
    }
    lines.join("\n")
}

fn render_profile_resize(
    before: &ProfileStatus,
    after: &ProfileStatus,
    was_running: bool,
    output_json: bool,
) -> String {
    let json = json!({
        "schema": "effigy.container.profile-resize.v1",
        "schema_version": 1,
        "ok": true,
        "profile": after.profile,
        "was_running": was_running,
        "before": profile_status_json(before),
        "after": profile_status_json(after),
    });
    if output_json {
        return json.to_string();
    }
    let Some(row) = &after.row else {
        return format!(
            "[warning] resized Colima profile `{}` but it was not found in `colima list --json` afterward",
            after.profile
        );
    };
    format!(
        "[ok] resized Colima profile `{}` in place\nstatus: {}\nmemory: {}GiB{}\ndisk: {}GiB{}",
        after.profile,
        row.status,
        bytes_to_gib(row.memory),
        target_suffix(after.target_memory_gib),
        bytes_to_gib(row.disk),
        target_suffix(after.target_disk_gib),
    )
}

fn render_profile_recreate(
    before: &ProfileStatus,
    after: &ProfileStatus,
    output_json: bool,
) -> String {
    let json = json!({
        "schema": "effigy.container.profile-recreate.v1",
        "schema_version": 1,
        "ok": true,
        "profile": after.profile,
        "before": profile_status_json(before),
        "after": profile_status_json(after),
    });
    if output_json {
        return json.to_string();
    }
    let Some(row) = &after.row else {
        return format!(
            "[warning] recreated Colima profile `{}` but it was not found in `colima list --json` afterward",
            after.profile
        );
    };
    format!(
        "[ok] recreated Colima profile `{}`\nstatus: {}\nmemory: {}GiB{}\ndisk: {}GiB{}",
        after.profile,
        row.status,
        bytes_to_gib(row.memory),
        target_suffix(after.target_memory_gib),
        bytes_to_gib(row.disk),
        target_suffix(after.target_disk_gib),
    )
}

fn render_profile_purge(
    before: &ProfileStatus,
    after: &ProfileStatus,
    output_json: bool,
) -> String {
    let json = json!({
        "schema": "effigy.container.profile-purge.v1",
        "schema_version": 1,
        "ok": true,
        "profile": after.profile,
        "before": profile_status_json(before),
        "after": profile_status_json(after),
    });
    if output_json {
        return json.to_string();
    }
    if before.row.is_none() {
        return format!(
            "[info] Colima profile `{}` was already absent",
            after.profile
        );
    }
    if after.row.is_some() {
        return format!(
            "[warning] purged Colima profile `{}` but it still appears in `colima list --json`",
            after.profile
        );
    }
    format!(
        "[ok] purged Colima profile `{}` and deleted profile runtime data",
        after.profile
    )
}

fn render_profile_prune_health_pid_files(
    profile: &str,
    prune: effigy_containers::exec::HealthCheckPidFilePrune,
    output_json: bool,
) -> String {
    let json = json!({
        "schema": "effigy.container.profile-prune-health-pid-files.v1",
        "schema_version": 1,
        "ok": true,
        "profile": profile,
        "prune": {
            "before": prune.before,
            "after": prune.after,
            "deleted": prune.deleted(),
        },
        "stale_after_minutes": effigy_containers::exec::HEALTHCHECK_PIDFILE_STALE_MINUTES,
        "task_root": effigy_containers::exec::HEALTHCHECK_TASK_ROOT,
    });
    if output_json {
        return json.to_string();
    }
    format!(
        "[ok] Colima profile `{profile}`: {}; {} health-check pid file(s) remain, all younger than {} minutes",
        prune.summary(),
        prune.after,
        effigy_containers::exec::HEALTHCHECK_PIDFILE_STALE_MINUTES,
    )
}

fn profile_status_json(status: &ProfileStatus) -> serde_json::Value {
    let memory_gib = status.row.as_ref().map(|row| bytes_to_gib(row.memory));
    let disk_gib = status.row.as_ref().map(|row| bytes_to_gib(row.disk));
    json!({
        "schema": "effigy.container.profile-status.v1",
        "schema_version": 1,
        "ok": true,
        "profile": status.profile,
        "exists": status.row.is_some(),
        "status": status.row.as_ref().map(|row| row.status.clone()),
        "runtime": status.row.as_ref().and_then(|row| row.runtime.clone()),
        "cpus": status.row.as_ref().map(|row| row.cpus),
        "memory_gib": memory_gib,
        "disk_gib": disk_gib,
        "target_memory_gib": status.target_memory_gib,
        "target_swap_gib": status.target_swap_gib,
        "target_disk_gib": status.target_disk_gib,
        "managed": status.target_disk_gib.is_some(),
        "memory_below_target": memory_gib.zip(status.target_memory_gib).is_some_and(|(actual, target)| actual < target),
        "disk_below_target": disk_gib.zip(status.target_disk_gib).is_some_and(|(actual, target)| actual < target),
    })
}

fn run_colima(args: &[&str], label: &str) -> Result<Output, RunnerError> {
    run_command("colima", args, label, false)
}

fn run_colima_allow_failure(args: &[&str], label: &str) -> Result<Output, RunnerError> {
    run_command("colima", args, label, true)
}

fn run_command(
    program: &str,
    args: &[&str],
    label: &str,
    allow_failure: bool,
) -> Result<Output, RunnerError> {
    Command::new(program)
        .args(args)
        .output()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: format!("{label} ({program} {})", args.join(" ")),
            error,
        })
        .and_then(|output| {
            if output.status.success() || allow_failure {
                Ok(output)
            } else {
                Err(command_failure(
                    label,
                    args,
                    output.status.code(),
                    &output.stdout,
                    &output.stderr,
                ))
            }
        })
}

/// A Colima command that failed, named by its profile when it has one. Its own
/// output is the cause, so it stays in the message after the next step.
fn command_failure(
    label: &str,
    args: &[&str],
    code: Option<i32>,
    stdout: &[u8],
    stderr: &[u8],
) -> RunnerError {
    let subject = match profile_flag(args) {
        Some(profile) => format!("{label} for profile `{profile}`"),
        None => label.to_owned(),
    };
    RunnerError::task_invocation(format!(
        "{subject} failed ({})\nnext: read the Colima output below, check `effigy container profile status`, then rerun the command\nstdout:\n{}\nstderr:\n{}",
        effigy_ui::human::exit_code(code),
        String::from_utf8_lossy(stdout),
        String::from_utf8_lossy(stderr)
    ))
}

/// The profile a Colima command names with `--profile`, when it names one.
fn profile_flag<'a>(args: &[&'a str]) -> Option<&'a str> {
    args.windows(2)
        .find(|pair| pair[0] == "--profile")
        .map(|pair| pair[1])
}

fn bytes_to_gib(bytes: u64) -> u64 {
    bytes / BYTES_PER_GIB
}

fn status_below_target(bytes: u64, target_gib: Option<u64>) -> bool {
    target_gib.is_some_and(|target| bytes_to_gib(bytes) < target)
}

fn target_suffix(target: Option<u64>) -> String {
    target
        .map(|value| format!(" (target={value}GiB)"))
        .unwrap_or_default()
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_colima_list_reads_profile_rows() {
        let rows = parse_colima_list(
            r#"{"name":"effigy","status":"Running","cpus":2,"memory":34359738368,"disk":107374182400,"runtime":"containerd"}"#,
        )
        .expect("parse");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "effigy");
        assert_eq!(bytes_to_gib(rows[0].disk), 100);
    }

    #[test]
    fn profile_status_renders_resize_hint_when_disk_is_small() {
        let status = ProfileStatus {
            profile: "effigy".to_owned(),
            row: Some(ColimaListRow {
                name: "effigy".to_owned(),
                status: "Running".to_owned(),
                cpus: 2,
                memory: 32 * BYTES_PER_GIB,
                disk: 100 * BYTES_PER_GIB,
                runtime: Some("containerd".to_owned()),
            }),
            target_memory_gib: Some(32),
            target_swap_gib: Some(16),
            target_disk_gib: Some(300),
        };

        let rendered = render_profile_status(&status, false);

        assert!(rendered.contains("disk: 100GiB (target=300GiB)"));
        assert!(rendered.contains("profile resize"));
        assert!(!rendered.contains("profile recreate --disk"));
    }

    #[test]
    fn profile_resize_renders_in_place_result() {
        let before = ProfileStatus {
            profile: "effigy".to_owned(),
            row: Some(ColimaListRow {
                name: "effigy".to_owned(),
                status: "Running".to_owned(),
                cpus: 2,
                memory: 32 * BYTES_PER_GIB,
                disk: 100 * BYTES_PER_GIB,
                runtime: Some("containerd".to_owned()),
            }),
            target_memory_gib: Some(32),
            target_swap_gib: Some(16),
            target_disk_gib: Some(300),
        };
        let after = ProfileStatus {
            profile: "effigy".to_owned(),
            row: Some(ColimaListRow {
                name: "effigy".to_owned(),
                status: "Running".to_owned(),
                cpus: 2,
                memory: 32 * BYTES_PER_GIB,
                disk: 300 * BYTES_PER_GIB,
                runtime: Some("containerd".to_owned()),
            }),
            target_memory_gib: Some(32),
            target_swap_gib: Some(16),
            target_disk_gib: Some(300),
        };

        let rendered = render_profile_resize(&before, &after, true, false);

        assert!(rendered.contains("resized Colima profile `effigy` in place"));
        assert!(rendered.contains("disk: 300GiB (target=300GiB)"));
    }

    #[test]
    fn profile_purge_renders_deleted_result() {
        let before = ProfileStatus {
            profile: "effigy".to_owned(),
            row: Some(ColimaListRow {
                name: "effigy".to_owned(),
                status: "Stopped".to_owned(),
                cpus: 2,
                memory: 32 * BYTES_PER_GIB,
                disk: 300 * BYTES_PER_GIB,
                runtime: Some("containerd".to_owned()),
            }),
            target_memory_gib: Some(32),
            target_swap_gib: Some(16),
            target_disk_gib: Some(300),
        };
        let after = ProfileStatus {
            profile: "effigy".to_owned(),
            row: None,
            target_memory_gib: Some(32),
            target_swap_gib: Some(16),
            target_disk_gib: Some(300),
        };

        let rendered = render_profile_purge(&before, &after, false);

        assert!(rendered.contains("purged Colima profile `effigy`"));
    }

    #[test]
    fn profile_purge_renders_absent_as_info() {
        let status = ProfileStatus {
            profile: "effigy".to_owned(),
            row: None,
            target_memory_gib: Some(32),
            target_swap_gib: Some(16),
            target_disk_gib: Some(300),
        };

        let rendered = render_profile_purge(&status, &status, false);

        assert!(rendered.contains("already absent"));
    }

    #[test]
    fn profile_recreate_disk_prompt_uses_default_on_blank() {
        let mut input = std::io::Cursor::new(b"\n".as_slice());
        let mut output = Vec::new();

        let disk = prompt_profile_recreate_disk_gib("effigy", 180, &mut input, &mut output)
            .expect("prompt");

        assert_eq!(disk, 180);
        assert!(String::from_utf8(output)
            .expect("utf8")
            .contains("Disk size for Colima profile `effigy` in GiB [180]:"));
    }

    #[test]
    fn profile_recreate_disk_prompt_accepts_positive_value() {
        let mut input = std::io::Cursor::new(b"220\n".as_slice());
        let mut output = Vec::new();

        let disk = prompt_profile_recreate_disk_gib("effigy", 300, &mut input, &mut output)
            .expect("prompt");

        assert_eq!(disk, 220);
    }

    #[test]
    fn an_unmanaged_profile_refusal_names_the_managed_profile_to_use() {
        let error = run_container_profile_purge(Some("work"), true, false)
            .expect_err("an unmanaged profile is refused before any Colima call");
        let human = error.human_error();

        assert_eq!(
            human.what_failed(),
            "`effigy container profile purge` only supports the managed `effigy` Colima profile; `work` is not managed by Effigy"
        );
        assert_eq!(
            human.next_action_text(),
            Some("rerun it with `--profile effigy`")
        );

        let error = run_container_profile_resize(Some("work"), false)
            .expect_err("an unmanaged profile is refused before any Colima call");
        assert!(error
            .human_error()
            .next_action_text()
            .is_some_and(|action| action.contains("--profile effigy")));
    }

    #[test]
    fn the_repo_refusal_points_at_a_profile_command_without_it() {
        let error = run_container_profile_command(
            Some(PathBuf::from(".")),
            &ContainerProfileSubcommand::Status { profile: None },
            false,
        )
        .expect_err("profiles are machine-level, so --repo is refused");

        assert!(error
            .human_error()
            .next_action_text()
            .is_some_and(|action| action.contains("effigy container profile status")));
    }

    #[test]
    fn a_failed_colima_command_names_its_profile_its_output_and_the_next_step() {
        let error = command_failure(
            "colima start",
            &["start", "--profile", "effigy"],
            Some(1),
            b"",
            b"error: vm not found\n",
        );
        let human = error.human_error();

        assert!(
            human
                .what_failed()
                .starts_with("colima start for profile `effigy` failed"),
            "{}",
            human.what_failed()
        );
        assert!(
            human
                .evidence_lines()
                .iter()
                .any(|line| line.contains("error: vm not found")),
            "{:?}",
            human.evidence_lines()
        );
        assert!(human
            .next_action_text()
            .is_some_and(|action| action.contains("check `effigy container profile status`")));

        let unnamed = command_failure("colima list", &["list", "--json"], Some(1), b"", b"");
        assert!(unnamed
            .human_error()
            .what_failed()
            .starts_with("colima list failed"));
    }

    #[test]
    fn a_bad_disk_size_names_the_value_and_the_valid_form() {
        let mut input = std::io::Cursor::new(b"lots\n".as_slice());
        let mut output = Vec::new();
        let error = prompt_profile_recreate_disk_gib("effigy", 180, &mut input, &mut output)
            .expect_err("a non-number is not a disk size");
        let human = error.human_error();

        assert_eq!(
            human.what_failed(),
            "invalid Colima profile disk size `lots`; expected a positive integer GiB value"
        );
        assert!(human
            .next_action_text()
            .is_some_and(|action| action.contains("--disk <GiB>")));
    }

    #[test]
    fn an_unreadable_colima_list_row_names_the_command_to_check() {
        let error = parse_colima_list("not json").expect_err("a row must parse");

        assert!(error
            .human_error()
            .next_action_text()
            .is_some_and(|action| action.contains("`colima list`")));
    }
}

#[cfg(test)]
mod progress_tests {
    use super::*;
    use std::time::Duration;

    fn running_profile(disk_gib: u64) -> ProfileStatus {
        ProfileStatus {
            profile: "effigy".to_owned(),
            row: Some(ColimaListRow {
                name: "effigy".to_owned(),
                status: "Running".to_owned(),
                cpus: 2,
                memory: 32 * BYTES_PER_GIB,
                disk: disk_gib * BYTES_PER_GIB,
                runtime: Some("containerd".to_owned()),
            }),
            target_memory_gib: Some(32),
            target_swap_gib: Some(16),
            target_disk_gib: Some(300),
        }
    }

    /// effigy#461: a profile frame names the operation now running and the
    /// state Colima last reported, with the elapsed wait. Before there is any
    /// observation it names the exact probe, never a canned label.
    #[test]
    fn fake_clock_frames_name_the_profile_operation_and_its_state() {
        let progress = ProfileProgress::new(
            false,
            profile_probe("effigy"),
            "repairing the Colima runtime of profile `effigy`".to_owned(),
        );
        assert_eq!(
            progress.frame_at(Duration::from_secs(3)),
            "repairing the Colima runtime of profile `effigy`; waiting for `colima list --json` to answer for profile `effigy`; 3s elapsed"
        );

        progress.observe(&running_profile(100));
        let frame = progress.frame_at(Duration::from_secs(42));
        assert_eq!(
            frame,
            "repairing the Colima runtime of profile `effigy`; last: profile `effigy` reported Running with 2 CPUs, 32GiB memory and 100GiB disk (target=300GiB); 42s elapsed"
        );

        progress.phase(
            "starting Colima profile `effigy` with the managed sizing",
            Some((2, 2)),
        );
        let frame = progress.frame_at(Duration::from_secs(57));
        assert!(
            frame.starts_with("starting Colima profile `effigy` with the managed sizing (2/2)"),
            "{frame}"
        );
        assert!(frame.contains("100GiB disk (target=300GiB)"), "{frame}");
        assert!(frame.ends_with("; 57s elapsed"), "{frame}");

        progress.observe(&running_profile(300));
        let frame = progress.frame_at(Duration::from_secs(61));
        assert!(frame.contains("300GiB disk (target=300GiB)"), "{frame}");
        progress.succeed("recovered the Colima runtime of profile `effigy`");
    }

    /// A profile the probe does not list says so, and a JSON run draws nothing.
    #[test]
    fn an_unlisted_profile_and_json_output_stay_honest() {
        let absent = ProfileStatus {
            profile: "effigy".to_owned(),
            row: None,
            target_memory_gib: Some(32),
            target_swap_gib: Some(16),
            target_disk_gib: Some(300),
        };
        let progress = ProfileProgress::new(
            false,
            profile_probe("effigy"),
            "reading the state Colima reports for profile `effigy`".to_owned(),
        );
        progress.observe(&absent);
        assert_eq!(
            progress.frame_at(Duration::from_secs(1)),
            "reading the state Colima reports for profile `effigy`; last: profile `effigy` is not listed by `colima list --json`; 1s elapsed"
        );

        let quiet = ProfileProgress::new(
            true,
            profile_probe("effigy"),
            "reading the state Colima reports for profile `effigy`".to_owned(),
        );
        assert!(quiet.source.is_none() && quiet.step.is_none());
        quiet.observe(&running_profile(300));
        quiet.fail("this line is never drawn under --json");
    }
}
