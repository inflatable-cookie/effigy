use std::io::{self, BufRead, IsTerminal, Write};
use std::path::PathBuf;
use std::process::{Command, Output};

use effigy_cli::ContainerProfileSubcommand;
use effigy_containers::colima::{
    colima_start_command_for_profile, colima_start_command_for_profile_with_disk,
    managed_colima_profile_resources, managed_colima_profile_resources_with_disk,
    prepare_managed_colima_profile_name, prepare_managed_colima_profile_name_with_disk,
};
use serde::Deserialize;
use serde_json::json;

use super::data::maybe_confirm_destructive_container_action;
use super::RunnerError;

const DEFAULT_PROFILE: &str = "effigy";
const BYTES_PER_GIB: u64 = 1024 * 1024 * 1024;

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
    if repo_override.is_some() {
        return Err(RunnerError::task_invocation(
            "`effigy container profile` does not accept `--repo`; Colima profiles are machine-level runtime state",
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
    }
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
        return Err(RunnerError::task_invocation(format!(
            "`effigy container profile recreate` only supports the managed `{DEFAULT_PROFILE}` Colima profile; `{profile}` is not managed by Effigy"
        )));
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

    if before.row.is_some() {
        run_colima_allow_failure(&["stop", "--profile", &profile], "colima stop")?;
        run_colima(
            &["delete", "--profile", &profile, "--force", "--data"],
            "colima delete",
        )?;
    }
    prepare_managed_colima_profile_name_with_disk(&profile, disk_gib)
        .map_err(RunnerError::task_invocation)?;
    let start = colima_start_command_for_profile_with_disk(&profile, disk_gib);
    let start_args = start.args.iter().map(String::as_str).collect::<Vec<_>>();
    run_command(&start.program, &start_args, &start.label, false)?;

    let after = inspect_profile_with_disk(&profile, disk_gib)?;
    Ok(render_profile_recreate(&before, &after, output_json))
}

fn run_container_profile_purge(
    profile: Option<&str>,
    yes: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    let profile = resolve_profile(profile);
    if managed_colima_profile_resources(&profile).is_none() {
        return Err(RunnerError::task_invocation(format!(
            "`effigy container profile purge` only supports the managed `{DEFAULT_PROFILE}` Colima profile; `{profile}` is not managed by Effigy"
        )));
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

    if before.row.is_some() {
        run_colima_allow_failure(&["stop", "--profile", &profile], "colima stop")?;
        run_colima(
            &["delete", "--profile", &profile, "--force", "--data"],
            "colima delete",
        )?;
    }

    let after = inspect_profile(&profile)?;
    Ok(render_profile_purge(&before, &after, output_json))
}

fn run_container_profile_resize(
    profile: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let profile = resolve_profile(profile);
    let resources = managed_colima_profile_resources(&profile).ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "`effigy container profile resize` only supports the managed `{DEFAULT_PROFILE}` Colima profile; `{profile}` is not managed by Effigy"
        ))
    })?;
    let before = inspect_profile(&profile)?;
    prepare_managed_colima_profile_name(&profile).map_err(RunnerError::task_invocation)?;

    let was_running = before
        .row
        .as_ref()
        .is_some_and(|row| row.status.eq_ignore_ascii_case("running"));
    if was_running {
        run_colima(&["stop", "--profile", &profile], "colima stop")?;
    }

    let start = colima_start_command_for_profile(&profile);
    let start_args = start.args.iter().map(String::as_str).collect::<Vec<_>>();
    run_command(&start.program, &start_args, &start.label, false)?;

    let after = inspect_profile(&profile)?;
    let resized = after
        .row
        .as_ref()
        .map(|row| bytes_to_gib(row.disk) >= resources.disk_gib)
        .unwrap_or(false);
    if !resized {
        return Err(RunnerError::task_invocation(format!(
            "Colima profile `{profile}` restarted but still reports disk {}GiB below the managed target {}GiB. Try `effigy container profile recreate --disk {} --yes` only if you are prepared to lose local profile runtime data.",
            after.row.as_ref().map(|row| bytes_to_gib(row.disk)).unwrap_or(0),
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
            "invalid Colima profile disk size `{trimmed}`; expected a positive integer GiB value"
        ))
    })?;
    if value == 0 {
        return Err(RunnerError::task_invocation(
            "invalid Colima profile disk size `0`; expected a positive integer GiB value",
        ));
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
                    "failed to parse `colima list --json` row: {error}"
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
                Err(RunnerError::task_invocation(format!(
                    "{label} failed (code {:?})\nstdout:\n{}\nstderr:\n{}",
                    output.status.code(),
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                )))
            }
        })
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
}
