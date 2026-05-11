use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::runner::error::RunnerError;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestDeployProviderConfig {
    pub source: DeployProviderSource,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub(super) enum DeployProviderSource {
    Path {
        dir: String,
    },
    Git {
        url: String,
        #[serde(default)]
        r#ref: Option<String>,
    },
    Oci {
        url: String,
    },
}

#[derive(Debug, Clone)]
pub(super) struct DeployProviderPackage {
    pub root: PathBuf,
    pub descriptor: DeployProviderDescriptor,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DeployProviderDescriptor {
    pub provider: DeployProviderMetadata,
    #[serde(default)]
    pub capabilities: DeployProviderCapabilities,
    #[serde(default)]
    pub policy: DeployProviderPolicy,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DeployProviderMetadata {
    pub schema: String,
    pub name: String,
    pub display_name: String,
    pub version: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(super) struct DeployProviderCapabilities {
    #[serde(default)]
    pub checklist: Option<String>,
    #[serde(default)]
    pub export: Option<String>,
    #[serde(default)]
    pub validate: Option<String>,
    #[serde(default)]
    pub preflight: Option<String>,
    #[serde(default)]
    pub apply: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(super) struct DeployProviderPolicy {
    #[serde(default)]
    pub creates_projects: bool,
    #[serde(default)]
    pub creates_services: bool,
    #[serde(default)]
    pub creates_resources: bool,
    #[serde(default)]
    pub creates_variables: bool,
    #[serde(default)]
    pub creates_domains: bool,
    #[serde(default)]
    pub prints_secret_values: bool,
}

pub(super) fn resolve_provider_package(
    repo_root: &Path,
    provider_name: &str,
    providers: &BTreeMap<String, ManifestDeployProviderConfig>,
) -> Result<Option<DeployProviderPackage>, RunnerError> {
    let Some(config) = providers.get(provider_name) else {
        return Ok(None);
    };
    let root = match &config.source {
        DeployProviderSource::Path { dir } => repo_root.join(dir),
        DeployProviderSource::Git { url, r#ref } => {
            resolve_git_provider_source(repo_root, provider_name, url, r#ref.as_deref())?
        }
        DeployProviderSource::Oci { url } => {
            return Err(RunnerError::task_invocation(format!(
                "deploy provider `{provider_name}` uses OCI source `{url}`, but provider OCI materialization is not implemented yet"
            )));
        }
    };
    let descriptor = read_provider_descriptor(&root)?;
    if descriptor.provider.schema != "effigy.deploy-provider.v1" {
        return Err(RunnerError::task_invocation(format!(
            "deploy provider `{provider_name}` descriptor has unsupported schema `{}`",
            descriptor.provider.schema
        )));
    }
    if descriptor.provider.name != provider_name {
        return Err(RunnerError::task_invocation(format!(
            "deploy provider `{provider_name}` resolved descriptor for `{}`",
            descriptor.provider.name
        )));
    }
    validate_capability_paths(&root, provider_name, &descriptor.capabilities)?;
    Ok(Some(DeployProviderPackage { root, descriptor }))
}

fn read_provider_descriptor(root: &Path) -> Result<DeployProviderDescriptor, RunnerError> {
    let path = root.join("provider.toml");
    let raw = std::fs::read_to_string(&path).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to read deploy provider descriptor {}: {error}",
            path.display()
        ))
    })?;
    toml::from_str(&raw).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to parse deploy provider descriptor {}: {error}",
            path.display()
        ))
    })
}

fn validate_capability_paths(
    root: &Path,
    provider_name: &str,
    capabilities: &DeployProviderCapabilities,
) -> Result<(), RunnerError> {
    for (phase, path) in [
        ("checklist", &capabilities.checklist),
        ("export", &capabilities.export),
        ("validate", &capabilities.validate),
        ("preflight", &capabilities.preflight),
        ("apply", &capabilities.apply),
        ("status", &capabilities.status),
    ] {
        let Some(path) = path else {
            continue;
        };
        let resolved = root.join(path);
        if !resolved.is_file() {
            return Err(RunnerError::task_invocation(format!(
                "deploy provider `{provider_name}` capability `{phase}` points to missing script {}",
                resolved.display()
            )));
        }
    }
    Ok(())
}

fn resolve_git_provider_source(
    repo_root: &Path,
    provider_name: &str,
    url: &str,
    reference: Option<&str>,
) -> Result<PathBuf, RunnerError> {
    let reference = reference
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("main");
    let local_path = repo_root
        .join(".effigy")
        .join("cache/providers/git")
        .join(sha256_hex(canonical_git_cache_identity(url).as_bytes()))
        .join(sanitize_cache_segment(reference));
    ensure_git_checkout(repo_root, provider_name, url, reference, &local_path)?;
    Ok(local_path)
}

fn ensure_git_checkout(
    repo_root: &Path,
    provider_name: &str,
    url: &str,
    reference: &str,
    local_path: &Path,
) -> Result<(), RunnerError> {
    if !local_path.join(".git").is_dir() {
        if local_path.exists() && !local_path.is_dir() {
            return Err(RunnerError::task_invocation(format!(
                "deploy provider `{provider_name}` cache path exists but is not a directory: {}",
                local_path.display()
            )));
        }
        if let Some(parent) = local_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                RunnerError::task_invocation(format!(
                    "failed to create deploy provider cache directory {}: {error}",
                    parent.display()
                ))
            })?;
        }
        run_git(
            repo_root,
            None,
            &["clone", "--no-checkout", url, &local_path.to_string_lossy()],
        )?;
    }
    run_git(
        local_path,
        Some(local_path),
        &["remote", "set-url", "origin", url],
    )?;
    run_git(
        local_path,
        Some(local_path),
        &["fetch", "--depth", "1", "origin", reference],
    )?;
    run_git(
        local_path,
        Some(local_path),
        &["checkout", "--detach", "FETCH_HEAD"],
    )?;
    Ok(())
}

fn run_git(manifest_root: &Path, cwd: Option<&Path>, args: &[&str]) -> Result<(), RunnerError> {
    let mut command = ProcessCommand::new("git");
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let output = command.args(args).output().map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to run git for deploy provider package in {}: {error}",
            manifest_root.display()
        ))
    })?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    Err(RunnerError::task_invocation(if stderr.is_empty() {
        format!("git {} failed for deploy provider package", args.join(" "))
    } else {
        format!(
            "git {} failed for deploy provider package: {stderr}",
            args.join(" ")
        )
    }))
}

fn canonical_git_cache_identity(url: &str) -> String {
    let trimmed = url.trim();
    if let Some(rest) = trimmed.strip_prefix("git@") {
        if let Some((host, path)) = rest.split_once(':') {
            return format!(
                "{}/{}",
                host.to_ascii_lowercase(),
                normalize_git_repo_path(path)
            );
        }
    }
    for prefix in ["ssh://", "https://", "http://", "git://"] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            let rest = rest.split('@').next_back().unwrap_or(rest);
            if let Some((host, path)) = rest.split_once('/') {
                return format!(
                    "{}/{}",
                    host.to_ascii_lowercase(),
                    normalize_git_repo_path(path)
                );
            }
        }
    }
    if let Some(path) = trimmed.strip_prefix("file://") {
        return format!("local/{}", normalize_local_git_path(path));
    }
    format!("local/{}", normalize_local_git_path(trimmed))
}

fn normalize_git_repo_path(path: &str) -> String {
    path.trim_start_matches('/')
        .trim_end_matches(".git")
        .trim_end_matches('/')
        .to_ascii_lowercase()
        .to_owned()
}

fn normalize_local_git_path(path: &str) -> String {
    let raw = Path::new(path);
    std::fs::canonicalize(raw)
        .unwrap_or_else(|_| raw.to_path_buf())
        .to_string_lossy()
        .replace('\\', "/")
}

fn sanitize_cache_segment(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn sha256_hex(input: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input);
    format!("{:x}", hasher.finalize())
}
