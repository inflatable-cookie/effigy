//! Volume lifecycle management.
//!
//! Docker named volumes persist data across container restarts. This module
//! provides the logic layer for volume management operations:
//!
//! - **List**: Enumerate volumes with sizes and metadata.
//! - **Export**: Archive a volume's contents to a tar file.
//! - **Import**: Restore a volume from a tar archive.
//! - **Classification**: Determine which volumes are data (persist across
//!   reset) vs ephemeral.
//!
//! Actual Docker CLI invocations are described as command specs — the caller
//! is responsible for execution. This keeps the module testable without
//! requiring Docker.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::assembly::VolumeInfo;

/// A volume as seen by the management layer (enriched with runtime info).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedVolume {
    /// Docker volume name.
    pub name: String,

    /// Which service declared this volume.
    pub service: String,

    /// Whether this volume should survive `container reset --keep-data`.
    pub persist: bool,

    /// Size in bytes, if known (populated from Docker inspect).
    pub size_bytes: Option<u64>,

    /// Mount point inside the container.
    pub mount_point: Option<String>,

    /// Declared target path inside the container, when known.
    pub mount_target: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CacheVolumeKind {
    RustTarget,
    NodeModules,
    PnpmStore,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeVolumeMetadata {
    pub name: String,
    pub mount_point: Option<String>,
    pub size_bytes: Option<u64>,
    pub labels: BTreeMap<String, String>,
}

impl ManagedVolume {
    /// Create from a VolumeInfo (catalog assembly output).
    pub fn from_volume_info(info: &VolumeInfo) -> Self {
        Self {
            name: info.name.clone(),
            service: info.service.clone(),
            persist: info.persist,
            size_bytes: None,
            mount_point: None,
            mount_target: info.mount.clone(),
        }
    }

    pub fn cache_kind(&self) -> Option<CacheVolumeKind> {
        let target = self.mount_target.as_deref()?.trim_end_matches('/');
        if target.ends_with("/target") || target == "target" {
            return Some(CacheVolumeKind::RustTarget);
        }
        if target.ends_with("/node_modules") || target == "node_modules" {
            return Some(CacheVolumeKind::NodeModules);
        }
        if target.ends_with("/pnpm/store") || target == "/home/dev/.local/share/pnpm/store" {
            return Some(CacheVolumeKind::PnpmStore);
        }
        None
    }
}

/// Classification of volumes for reset operations.
#[derive(Debug, Clone)]
pub struct VolumeClassification {
    /// Volumes to remove (ephemeral).
    pub remove: Vec<String>,

    /// Volumes to keep (persistent data).
    pub keep: Vec<String>,
}

/// Classify volumes for a reset operation.
///
/// `keep_data` corresponds to the `--keep-data` flag on `container reset`.
/// When true, volumes marked `persist = true` are kept.
pub fn classify_for_reset(volumes: &[ManagedVolume], keep_data: bool) -> VolumeClassification {
    let mut remove = Vec::new();
    let mut keep = Vec::new();

    for vol in volumes {
        if keep_data && vol.persist {
            keep.push(vol.name.clone());
        } else {
            remove.push(vol.name.clone());
        }
    }

    VolumeClassification { remove, keep }
}

/// A command specification for Docker CLI volume operations.
///
/// These are pure data descriptions of what command to run. The caller
/// executes them via `std::process::Command` or equivalent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockerCommand {
    /// The command program (typically "docker").
    pub program: String,

    /// Command arguments.
    pub args: Vec<String>,

    /// Human-readable description for progress reporting.
    pub description: String,
}

impl DockerCommand {
    fn docker(args: Vec<String>, description: impl Into<String>) -> Self {
        Self {
            program: "docker".to_string(),
            args,
            description: description.into(),
        }
    }
}

const VOLUME_USAGE_PROGRAM: &str = "__effigy_volume_usage";
const VOLUME_USAGE_BATCH_PROGRAM: &str = "__effigy_volume_usage_batch";

/// Build the command to list Docker volumes matching a project prefix.
pub fn list_volumes_command(project_name: &str) -> DockerCommand {
    DockerCommand::docker(
        vec![
            "volume".to_string(),
            "ls".to_string(),
            "--filter".to_string(),
            format!("name={project_name}-"),
            "--format".to_string(),
            "{{.Name}}\t{{.Driver}}\t{{.Labels}}".to_string(),
        ],
        format!("List volumes for project '{project_name}'"),
    )
}

/// Build the command to list all Docker volumes.
pub fn list_all_volumes_command() -> DockerCommand {
    DockerCommand::docker(
        vec![
            "volume".to_string(),
            "ls".to_string(),
            "--format".to_string(),
            "{{.Name}}\t{{.Driver}}\t{{.Labels}}".to_string(),
        ],
        "List all Docker volumes",
    )
}

/// Build the command to inspect a Docker volume (for size and metadata).
pub fn inspect_volume_command(volume_name: &str) -> DockerCommand {
    DockerCommand::docker(
        vec![
            "volume".to_string(),
            "inspect".to_string(),
            volume_name.to_string(),
        ],
        format!("Inspect volume '{volume_name}'"),
    )
}

pub fn volume_usage_command(mount_point: &str) -> DockerCommand {
    DockerCommand {
        program: VOLUME_USAGE_PROGRAM.to_owned(),
        args: vec![mount_point.to_owned()],
        description: format!("Measure volume usage under '{mount_point}'"),
    }
}

pub fn volume_usage_batch_command(mount_points: &[String]) -> DockerCommand {
    DockerCommand {
        program: VOLUME_USAGE_BATCH_PROGRAM.to_owned(),
        args: mount_points.to_vec(),
        description: "Measure volume usage for multiple mount points".to_owned(),
    }
}

/// Build the command to export a Docker volume to a tar file.
///
/// Uses a temporary Alpine container to tar the volume contents.
pub fn export_volume_command(volume_name: &str, output_path: &Path) -> DockerCommand {
    let output_dir = output_path
        .parent()
        .unwrap_or(Path::new("."))
        .to_string_lossy()
        .to_string();
    let output_filename = output_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    DockerCommand::docker(
        vec![
            "run".to_string(),
            "--rm".to_string(),
            "-v".to_string(),
            format!("{volume_name}:/source:ro"),
            "-v".to_string(),
            format!("{output_dir}:/output"),
            "alpine:latest".to_string(),
            "tar".to_string(),
            "czf".to_string(),
            format!("/output/{output_filename}"),
            "-C".to_string(),
            "/source".to_string(),
            ".".to_string(),
        ],
        format!("Export volume '{volume_name}' to {}", output_path.display()),
    )
}

/// Build the command to import a tar file into a Docker volume.
///
/// Uses a temporary Alpine container to extract the tar into the volume.
pub fn import_volume_command(volume_name: &str, input_path: &Path) -> DockerCommand {
    let input_dir = input_path
        .parent()
        .unwrap_or(Path::new("."))
        .to_string_lossy()
        .to_string();
    let input_filename = input_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    DockerCommand::docker(
        vec![
            "run".to_string(),
            "--rm".to_string(),
            "-v".to_string(),
            format!("{volume_name}:/target"),
            "-v".to_string(),
            format!("{input_dir}:/input:ro"),
            "alpine:latest".to_string(),
            "tar".to_string(),
            "xzf".to_string(),
            format!("/input/{input_filename}"),
            "-C".to_string(),
            "/target".to_string(),
        ],
        format!(
            "Import {} into volume '{volume_name}'",
            input_path.display()
        ),
    )
}

/// Build the command to remove a Docker volume.
pub fn remove_volume_command(volume_name: &str) -> DockerCommand {
    DockerCommand::docker(
        vec![
            "volume".to_string(),
            "rm".to_string(),
            volume_name.to_string(),
        ],
        format!("Remove volume '{volume_name}'"),
    )
}

/// Build commands for a reset operation based on volume classification.
pub fn reset_commands(classification: &VolumeClassification) -> Vec<DockerCommand> {
    classification
        .remove
        .iter()
        .map(|name| remove_volume_command(name))
        .collect()
}

pub fn parse_listed_volume_names(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|line| line.split('\t').next())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect()
}

pub fn parse_inspect_volume_metadata(output: &str) -> Option<RuntimeVolumeMetadata> {
    let parsed = serde_json::from_str::<JsonValue>(output).ok()?;
    let first = parsed.as_array()?.first()?;
    let name = first.get("Name")?.as_str()?.to_owned();
    let mount_point = first
        .get("Mountpoint")
        .and_then(JsonValue::as_str)
        .map(str::to_owned);
    let size_bytes = first
        .get("UsageData")
        .and_then(|value| value.get("Size"))
        .and_then(JsonValue::as_u64);
    Some(RuntimeVolumeMetadata {
        name,
        mount_point,
        size_bytes,
        labels: first
            .get("Labels")
            .and_then(JsonValue::as_object)
            .map(|labels| {
                labels
                    .iter()
                    .filter_map(|(key, value)| {
                        value.as_str().map(|value| (key.clone(), value.to_owned()))
                    })
                    .collect()
            })
            .unwrap_or_default(),
    })
}

pub fn parse_volume_usage_bytes(output: &str) -> Option<u64> {
    let raw = output.lines().next()?.split_whitespace().next()?;
    let kibibytes = raw.parse::<u64>().ok()?;
    Some(kibibytes.saturating_mul(1024))
}

pub fn parse_volume_usage_bytes_map(output: &str) -> BTreeMap<String, u64> {
    output
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let kibibytes = parts.next()?.parse::<u64>().ok()?;
            let mount_point = parts.next()?.to_owned();
            Some((mount_point, kibibytes.saturating_mul(1024)))
        })
        .collect()
}

pub fn merge_runtime_volume_metadata(
    volumes: &[ManagedVolume],
    runtime: &[RuntimeVolumeMetadata],
) -> Vec<ManagedVolume> {
    let runtime_by_name = runtime
        .iter()
        .map(|entry| (entry.name.as_str(), entry))
        .collect::<BTreeMap<_, _>>();

    volumes
        .iter()
        .cloned()
        .map(|mut volume| {
            if let Some(metadata) = runtime_by_name.get(volume.name.as_str()) {
                volume.mount_point = metadata.mount_point.clone();
                volume.size_bytes = metadata.size_bytes;
            }
            volume
        })
        .collect()
}

#[cfg(test)]
#[path = "volumes/tests.rs"]
mod tests;
