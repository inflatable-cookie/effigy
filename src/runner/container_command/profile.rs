use std::path::PathBuf;
use std::process::{Command, Output};

use effigy_cli::ContainerProfileSubcommand;
use effigy_containers::colima::{
    colima_start_command_for_profile, managed_colima_profile_resources,
    prepare_managed_colima_profile_name,
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
        ContainerProfileSubcommand::Recreate { profile, yes } => {
            run_container_profile_recreate(profile.as_deref(), *yes, output_json)
        }
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
    yes: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    let profile = resolve_profile(profile);
    if managed_colima_profile_resources(&profile).is_none() {
        return Err(RunnerError::task_invocation(format!(
            "`effigy container profile recreate` only supports the managed `{DEFAULT_PROFILE}` Colima profile; `{profile}` is not managed by Effigy"
        )));
    }
    let before = inspect_profile(&profile)?;
    maybe_confirm_destructive_container_action(
        "`effigy container profile recreate`",
        &format!(
            "Recreate Colima profile `{profile}` with Effigy's managed sizing. This deletes the profile VM, container runtime state, images, containers, and volumes. Export database/object-store data first if you need it."
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
    prepare_managed_colima_profile_name(&profile).map_err(RunnerError::task_invocation)?;
    let start = colima_start_command_for_profile(&profile);
    let start_args = start.args.iter().map(String::as_str).collect::<Vec<_>>();
    run_command(&start.program, &start_args, &start.label, false)?;

    let after = inspect_profile(&profile)?;
    Ok(render_profile_recreate(&before, &after, output_json))
}

fn resolve_profile(profile: Option<&str>) -> String {
    profile
        .map(str::to_owned)
        .or_else(effigy_containers::user_global_colima_profile)
        .unwrap_or_else(|| DEFAULT_PROFILE.to_owned())
}

fn inspect_profile(profile: &str) -> Result<ProfileStatus, RunnerError> {
    let rows = list_colima_profiles()?;
    let row = rows.into_iter().find(|row| row.name == profile);
    let resources = managed_colima_profile_resources(profile);
    Ok(ProfileStatus {
        profile: profile.to_owned(),
        row,
        target_memory_gib: resources.map(|value| value.memory_gib),
        target_swap_gib: resources.map(|value| value.swap_gib),
        target_disk_gib: resources.map(|value| value.disk_gib),
    })
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
            "next: `effigy container profile recreate --yes` will rebuild `{}` at the managed target and delete local profile data",
            status.profile
        ));
    }
    lines.join("\n")
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
    fn profile_status_renders_recreate_hint_when_disk_is_small() {
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
        assert!(rendered.contains("profile recreate --yes"));
    }
}
