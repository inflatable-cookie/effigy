//! Colima lifecycle command specifications.
//!
//! Produces command specs for Colima operations (start, status) and
//! Docker Compose operations (up, down, kill, ps). The caller handles
//! actual process execution.

use crate::compose::compose_args;
use crate::{ContainerBackendDetection, ContainerManager};
use crate::{EffectiveContainerPolicy, DEFAULT_COLIMA_PROFILE};
use effigy_manifest::ManifestContainerShutdownMode;
use serde_yaml::{Mapping, Number, Value};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const FALLBACK_COLIMA_DNS_SERVERS: [&str; 2] = ["1.1.1.1", "8.8.8.8"];
const BYTES_PER_GIB: u64 = 1024 * 1024 * 1024;
const MIN_EFFIGY_MEMORY_GIB: u64 = 4;
const MAX_EFFIGY_MEMORY_GIB: u64 = 32;
const MIN_EFFIGY_SWAP_GIB: u64 = 4;
const MAX_EFFIGY_SWAP_GIB: u64 = 16;
const COLIMA_ARCH_OVERRIDE_ENV: &str = "EFFIGY_COLIMA_ARCH";
const COLIMA_VM_TYPE_OVERRIDE_ENV: &str = "EFFIGY_COLIMA_VM_TYPE";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColimaResourcePlan {
    pub host_memory_gib: Option<u64>,
    pub memory_gib: u64,
    pub swap_gib: u64,
}

/// A command specification to be executed by the runner.
#[derive(Debug, Clone)]
pub struct CommandSpec {
    /// Program to run.
    pub program: String,
    /// Arguments.
    pub args: Vec<String>,
    /// Human-readable label for error messages.
    pub label: String,
    /// Whether failure is acceptable (e.g., status check).
    pub allow_failure: bool,
}

/// Build the command to check if a Colima profile is running.
pub fn colima_status_command(policy: &EffectiveContainerPolicy) -> CommandSpec {
    CommandSpec {
        program: "colima".to_string(),
        args: vec![
            "status".to_string(),
            "--profile".to_string(),
            policy.profile.clone(),
        ],
        label: "colima status".to_string(),
        allow_failure: true,
    }
}

/// Build the command to start a Colima profile.
pub fn colima_start_command(policy: &EffectiveContainerPolicy) -> CommandSpec {
    let runtime = ContainerManager::defaults()
        .colima_start_runtime(&ContainerBackendDetection::from_env_and_path())
        .unwrap_or("containerd");
    let mut args = vec![
        "start".to_string(),
        "--profile".to_string(),
        policy.profile.clone(),
        "--runtime".to_string(),
        runtime.to_string(),
        // Forward the host's SSH agent socket to
        // `/run/host-services/ssh-auth.sock` inside the VM. Without this flag
        // the workspace catalogs' agent-socket bind mount lands on a
        // non-existent source path and Docker autocreates an empty directory
        // in its place, silently breaking `git push` over SSH from inside
        // workspace shells.
        "--ssh-agent".to_string(),
    ];
    if let Some(resources) = managed_colima_profile_resources(&policy.profile) {
        args.push("--memory".to_string());
        args.push(resources.memory_gib.to_string());
    }
    for server in FALLBACK_COLIMA_DNS_SERVERS {
        args.push("--dns".to_string());
        args.push(server.to_string());
    }
    append_colima_platform_overrides(
        &mut args,
        std::env::consts::OS,
        colima_arch_override().as_deref(),
        colima_vm_type_override().as_deref(),
    );
    CommandSpec {
        program: "colima".to_string(),
        args,
        label: "colima start".to_string(),
        allow_failure: false,
    }
}

/// Build the command to stop a Colima profile.
pub fn colima_stop_command(policy: &EffectiveContainerPolicy) -> CommandSpec {
    CommandSpec {
        program: "colima".to_string(),
        args: vec![
            "stop".to_string(),
            "--profile".to_string(),
            policy.profile.clone(),
        ],
        label: "colima stop".to_string(),
        allow_failure: false,
    }
}

/// Parse Colima status output to determine if the profile is running.
pub fn parse_colima_running(stdout: &str, stderr: &str) -> bool {
    let stdout_lower = stdout.to_ascii_lowercase();
    let stderr_lower = stderr.to_ascii_lowercase();
    if stdout_lower.contains("not running") || stderr_lower.contains("not running") {
        return false;
    }
    stdout_lower.contains("running") || stderr_lower.contains("running")
}

pub fn managed_colima_profile_resources(profile: &str) -> Option<ColimaResourcePlan> {
    if profile != DEFAULT_COLIMA_PROFILE {
        return None;
    }
    Some(
        detect_host_memory_bytes()
            .map(resource_plan_for_host_memory_bytes)
            .unwrap_or_else(|| ColimaResourcePlan {
                host_memory_gib: None,
                memory_gib: MIN_EFFIGY_MEMORY_GIB,
                swap_gib: MIN_EFFIGY_SWAP_GIB,
            }),
    )
}

pub fn prepare_managed_colima_profile(policy: &EffectiveContainerPolicy) -> Result<(), String> {
    if policy.profile != DEFAULT_COLIMA_PROFILE {
        return Ok(());
    }
    let Some(resources) = managed_colima_profile_resources(&policy.profile) else {
        return Ok(());
    };
    let config_path = colima_profile_config_path(&policy.profile)?;
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create Colima profile directory `{}`: {error}",
                parent.display()
            )
        })?;
    }
    let existing = if config_path.is_file() {
        fs::read_to_string(&config_path).map_err(|error| {
            format!(
                "failed to read Colima profile config `{}`: {error}",
                config_path.display()
            )
        })?
    } else {
        String::new()
    };
    let mut config = if existing.trim().is_empty() {
        Value::Mapping(Mapping::new())
    } else {
        serde_yaml::from_str::<Value>(&existing).map_err(|error| {
            format!(
                "failed to parse Colima profile config `{}`: {error}",
                config_path.display()
            )
        })?
    };
    let Some(root) = config.as_mapping_mut() else {
        return Err(format!(
            "Colima profile config `{}` must be a YAML mapping",
            config_path.display()
        ));
    };

    root.insert(
        Value::String("memory".to_owned()),
        Value::Number(Number::from(resources.memory_gib)),
    );
    // Forward the host SSH agent into the VM at
    // `/run/host-services/ssh-auth.sock` so the workspace agent-socket bind
    // mount has a real source path. Equivalent to `colima start --ssh-agent`.
    root.insert(Value::String("sshAgent".to_owned()), Value::Bool(true));
    upsert_colima_platform_overrides(
        root,
        std::env::consts::OS,
        colima_arch_override().as_deref(),
        colima_vm_type_override().as_deref(),
    );
    upsert_effigy_swap_provision(root, resources.swap_gib);

    let rendered = serde_yaml::to_string(&config).map_err(|error| {
        format!(
            "failed to render Colima profile config `{}`: {error}",
            config_path.display()
        )
    })?;
    fs::write(&config_path, rendered).map_err(|error| {
        format!(
            "failed to write Colima profile config `{}`: {error}",
            config_path.display()
        )
    })?;
    Ok(())
}

fn resource_plan_for_host_memory_bytes(host_memory_bytes: u64) -> ColimaResourcePlan {
    let host_memory_gib = bytes_to_gib_floor(host_memory_bytes).max(1);
    let quarter_host_gib = bytes_to_gib_ceil(host_memory_bytes / 4);
    let memory_gib = quarter_host_gib.clamp(MIN_EFFIGY_MEMORY_GIB, MAX_EFFIGY_MEMORY_GIB);
    let swap_gib = memory_gib.clamp(MIN_EFFIGY_SWAP_GIB, MAX_EFFIGY_SWAP_GIB);
    ColimaResourcePlan {
        host_memory_gib: Some(host_memory_gib),
        memory_gib,
        swap_gib,
    }
}

fn detect_host_memory_bytes() -> Option<u64> {
    if let Some(override_bytes) = std::env::var("EFFIGY_INTERNAL_HOST_MEMORY_BYTES")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|value| *value > 0)
    {
        return Some(override_bytes);
    }
    detect_host_memory_bytes_macos().or_else(detect_host_memory_bytes_linux)
}

fn detect_host_memory_bytes_macos() -> Option<u64> {
    let output = Command::new("sysctl")
        .args(["-n", "hw.memsize"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
}

fn append_colima_platform_overrides(
    args: &mut Vec<String>,
    host_os: &str,
    arch_override: Option<&str>,
    vm_type_override: Option<&str>,
) {
    if host_os != "macos" {
        return;
    }
    if let Some(arch) = arch_override.filter(|value| !value.trim().is_empty()) {
        args.push("--arch".to_owned());
        args.push(arch.trim().to_owned());
    }
    if let Some(vm_type) = vm_type_override.filter(|value| !value.trim().is_empty()) {
        args.push("--vm-type".to_owned());
        args.push(vm_type.trim().to_owned());
    }
}

fn upsert_colima_platform_overrides(
    root: &mut Mapping,
    host_os: &str,
    arch_override: Option<&str>,
    vm_type_override: Option<&str>,
) {
    if host_os != "macos" {
        return;
    }
    if let Some(arch) = arch_override.filter(|value| !value.trim().is_empty()) {
        root.insert(
            Value::String("arch".to_owned()),
            Value::String(arch.trim().to_owned()),
        );
    }
    if let Some(vm_type) = vm_type_override.filter(|value| !value.trim().is_empty()) {
        root.insert(
            Value::String("vmType".to_owned()),
            Value::String(vm_type.trim().to_owned()),
        );
    }
}

fn colima_arch_override() -> Option<String> {
    std::env::var(COLIMA_ARCH_OVERRIDE_ENV)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn colima_vm_type_override() -> Option<String> {
    std::env::var(COLIMA_VM_TYPE_OVERRIDE_ENV)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn colima_profile_config_path(profile: &str) -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME")
        .ok_or_else(|| "HOME is not set; cannot resolve Colima profile config".to_owned())?;
    Ok(Path::new(&home)
        .join(".colima")
        .join(profile)
        .join("colima.yaml"))
}

fn upsert_effigy_swap_provision(root: &mut Mapping, swap_gib: u64) {
    let provision_key = Value::String("provision".to_owned());
    let provision = root
        .entry(provision_key)
        .or_insert_with(|| Value::Sequence(Vec::new()));
    let Value::Sequence(entries) = provision else {
        *provision = Value::Sequence(Vec::new());
        let Value::Sequence(entries) = provision else {
            return;
        };
        upsert_effigy_swap_provision_entry(entries, swap_gib);
        return;
    };
    upsert_effigy_swap_provision_entry(entries, swap_gib);
}

fn upsert_effigy_swap_provision_entry(entries: &mut Vec<Value>, swap_gib: u64) {
    let marker = "# effigy-managed-swap";
    let script = render_effigy_swap_provision_script(swap_gib);
    if let Some(Value::Mapping(existing)) = entries.iter_mut().find(|entry| {
        entry
            .as_mapping()
            .and_then(|mapping| mapping.get(Value::String("script".to_owned())))
            .and_then(Value::as_str)
            .is_some_and(|value| value.contains(marker))
    }) {
        existing.insert(
            Value::String("mode".to_owned()),
            Value::String("after-boot".to_owned()),
        );
        existing.insert(Value::String("script".to_owned()), Value::String(script));
        return;
    }

    let mut mapping = Mapping::new();
    mapping.insert(
        Value::String("mode".to_owned()),
        Value::String("after-boot".to_owned()),
    );
    mapping.insert(Value::String("script".to_owned()), Value::String(script));
    entries.push(Value::Mapping(mapping));
}

fn render_effigy_swap_provision_script(swap_gib: u64) -> String {
    format!(
        r#"#!/bin/sh
set -eu
{marker}
swap_gib={swap_gib}
swap_bytes=$((swap_gib * 1024 * 1024 * 1024))
current_bytes=0
if [ -f /swapfile ]; then
  current_bytes=$(stat -c%s /swapfile || echo 0)
fi
if [ "$current_bytes" -ne "$swap_bytes" ]; then
  swapoff /swapfile 2>/dev/null || true
  rm -f /swapfile
  fallocate -l "${{swap_gib}}G" /swapfile 2>/dev/null || dd if=/dev/zero of=/swapfile bs=1M count=$((swap_gib * 1024))
  chmod 600 /swapfile
  mkswap /swapfile
fi
grep -q '^/swapfile ' /etc/fstab || echo '/swapfile none swap sw 0 0' >> /etc/fstab
swapon /swapfile 2>/dev/null || true
"#,
        marker = "# effigy-managed-swap",
        swap_gib = swap_gib,
    )
}

fn detect_host_memory_bytes_linux() -> Option<u64> {
    let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
    let line = meminfo.lines().find(|line| line.starts_with("MemTotal:"))?;
    let value_kib = line
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse::<u64>().ok())?;
    Some(value_kib.saturating_mul(1024))
}

fn bytes_to_gib_floor(bytes: u64) -> u64 {
    bytes / BYTES_PER_GIB
}

fn bytes_to_gib_ceil(bytes: u64) -> u64 {
    if bytes == 0 {
        return 0;
    }
    bytes.saturating_add(BYTES_PER_GIB - 1) / BYTES_PER_GIB
}

/// Build the compose args for bringing services up in detached mode.
pub fn compose_up_args(policy: &EffectiveContainerPolicy) -> Vec<OsString> {
    compose_args(policy, ["up", "-d"])
}

/// Build the compose args for checking service status.
pub fn compose_ps_args(policy: &EffectiveContainerPolicy) -> Vec<OsString> {
    compose_args(policy, ["ps"])
}

/// Build the compose args for tearing down with volume removal.
pub fn compose_reset_args(policy: &EffectiveContainerPolicy) -> Vec<OsString> {
    compose_args(policy, ["down", "-v", "--remove-orphans"])
}

/// Build the ordered list of compose args for shutting down a container
/// according to its configured shutdown policy.
///
/// Graceful: `docker compose down --remove-orphans`
/// Immediate: `docker compose kill` then `docker compose down --remove-orphans`
pub fn shutdown_compose_commands(
    policy: &EffectiveContainerPolicy,
) -> Vec<(Vec<OsString>, &'static str)> {
    match policy.shutdown {
        ManifestContainerShutdownMode::Graceful => {
            vec![(
                compose_args(policy, ["down", "--remove-orphans"]),
                "docker compose down",
            )]
        }
        ManifestContainerShutdownMode::Immediate => {
            vec![
                (compose_args(policy, ["kill"]), "docker compose kill"),
                (
                    compose_args(policy, ["down", "--remove-orphans"]),
                    "docker compose down",
                ),
            ]
        }
    }
}

/// Build compose args for following logs of a specific service.
pub fn compose_logs_follow_args(policy: &EffectiveContainerPolicy, service: &str) -> Vec<OsString> {
    compose_args(policy, ["logs", "--follow", service])
}

/// Build compose args for following logs with tail.
pub fn compose_logs_follow_tail_args(
    policy: &EffectiveContainerPolicy,
    service: &str,
    tail: &str,
) -> Vec<OsString> {
    compose_args(policy, ["logs", "--follow", "--tail", tail, service])
}

/// Build compose args for fetching recent logs.
pub fn compose_logs_tail_args<'a>(
    policy: &EffectiveContainerPolicy,
    service: &'a str,
    tail: &'a str,
) -> Vec<OsString> {
    compose_args(policy, ["logs", "--tail", tail, service])
}

/// Build compose args for exec into a service with a shell.
pub fn compose_exec_shell_args(
    policy: &EffectiveContainerPolicy,
    service: &str,
    shell: &str,
) -> Vec<OsString> {
    compose_args(policy, ["exec", service, shell])
}

/// Build compose args for exec into a service with a command.
pub fn compose_exec_command_args(
    policy: &EffectiveContainerPolicy,
    service: &str,
    command: &str,
) -> Vec<OsString> {
    let mut args = compose_args(policy, ["exec", service, "sh", "-lc"]);
    args.push(OsString::from(command));
    args
}

#[cfg(test)]
#[path = "colima/tests.rs"]
mod tests;
