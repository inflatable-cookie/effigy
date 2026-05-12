use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use effigy_artifacts::{
    ArtifactSourceRef, OciArtifactAdapter, OciArtifactPullRequest, OrasCliArtifactAdapter,
};
use sha2::{Digest, Sha256};

use crate::ManifestError;

use super::BundleSourceType;

// `version_hint` and `stale` are part of the locked source-materialization
// shape but are not consumed by every call site.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(super) struct ResolvedBundleSource {
    pub source_type: BundleSourceType,
    pub local_path: PathBuf,
    pub source_path: PathBuf,
    pub version_hint: Option<String>,
    pub stale: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct BundleSyncReport {
    pub source_type: BundleSourceType,
    pub source_path: PathBuf,
    pub local_path: Option<PathBuf>,
    pub version_hint: Option<String>,
    pub changed: bool,
    pub applicable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct BundleSourceInspectReport {
    pub source_type: BundleSourceType,
    pub source_path: PathBuf,
    pub local_path: PathBuf,
    pub version_hint: Option<String>,
    pub stale: bool,
}

pub(super) enum BundleSelection {
    Path {
        path: PathBuf,
    },
    Git {
        url: String,
        reference: Option<String>,
    },
    Oci {
        url: String,
    },
}

pub(super) fn resolve_bundle_selection(
    manifest_path: &Path,
    bundle: &crate::config_sections::ManifestBundleConfig,
) -> Result<BundleSelection, ManifestError> {
    match bundle.base.as_ref() {
        Some(crate::config_sections::ManifestBundleBase::Path { dir })
            if !dir.trim().is_empty() =>
        {
            let manifest_dir = manifest_path.parent().unwrap_or_else(|| Path::new("."));
            let path = Path::new(dir.trim());
            let resolved = if path.is_absolute() {
                path.to_path_buf()
            } else {
                manifest_dir.join(path)
            };
            Ok(BundleSelection::Path { path: resolved })
        }
        Some(crate::config_sections::ManifestBundleBase::Git { url, r#ref })
            if !url.trim().is_empty() =>
        {
            Ok(BundleSelection::Git {
                url: url.trim().to_owned(),
                reference: r#ref.clone().filter(|value| !value.trim().is_empty()),
            })
        }
        Some(crate::config_sections::ManifestBundleBase::Oci { url }) if !url.trim().is_empty() => {
            Ok(BundleSelection::Oci {
                url: url.trim().to_owned(),
            })
        }
        _ => Err(ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: "`[bundle]` must set `base` to a typed bundle source block".to_owned(),
        }),
    }
}

pub(super) fn resolve_materialized_bundle_source(
    manifest_path: &Path,
    selection: &BundleSelection,
) -> Result<ResolvedBundleSource, ManifestError> {
    resolve_materialized_bundle_source_with_options(manifest_path, selection, false)
}

fn resolve_materialized_bundle_source_with_options(
    manifest_path: &Path,
    selection: &BundleSelection,
    refresh_remote: bool,
) -> Result<ResolvedBundleSource, ManifestError> {
    match selection {
        BundleSelection::Path { path } => Ok(ResolvedBundleSource {
            source_type: BundleSourceType::Path,
            local_path: path.clone(),
            source_path: path.clone(),
            version_hint: None,
            stale: false,
        }),
        BundleSelection::Git { url, reference } => {
            resolve_git_bundle_source(manifest_path, url, reference.as_deref(), refresh_remote)
        }
        BundleSelection::Oci { url } => {
            resolve_oci_bundle_source(manifest_path, url, refresh_remote)
        }
    }
}

fn resolve_git_bundle_source(
    manifest_path: &Path,
    url: &str,
    reference: Option<&str>,
    refresh_remote: bool,
) -> Result<ResolvedBundleSource, ManifestError> {
    let local_path = git_bundle_cache_path(manifest_path, url, reference)?;
    let remote_status_path = git_bundle_remote_status_path(&local_path);
    let reference = reference
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("main");

    let cache_exists = local_path.join(".git").is_dir();

    if !cache_exists {
        emit_git_bundle_status_line(&format!(
            "cloning git bundle {}@{}",
            render_git_bundle_source_label(url),
            reference
        ));
        ensure_git_bundle_checkout(manifest_path, url, reference, &local_path)?;
    } else if refresh_remote {
        emit_git_bundle_status_line(&format!(
            "refreshing git bundle {}@{}",
            render_git_bundle_source_label(url),
            reference
        ));
        ensure_git_bundle_checkout(manifest_path, url, reference, &local_path)?;
    }
    let mut version_hint = if local_path.join(".git").is_dir() {
        Some(git_head_revision(manifest_path, &local_path)?)
    } else {
        None
    };

    if refresh_remote || !cache_exists {
        write_cached_git_bundle_remote_status(&remote_status_path, version_hint.as_deref())?;
    }

    if cache_exists && !refresh_remote {
        let cached_remote_commit = read_cached_git_bundle_remote_status(&remote_status_path)?
            .filter(|status| git_bundle_remote_status_is_fresh(status))
            .map(|status| status.remote_commit);
        let remote_commit = if let Some(remote_commit) = cached_remote_commit {
            Some(remote_commit)
        } else if let Ok(remote_commit) = git_ls_remote(manifest_path, url, reference, &local_path)
        {
            write_cached_git_bundle_remote_status(&remote_status_path, Some(&remote_commit))?;
            Some(remote_commit)
        } else {
            None
        };
        if let Some(remote_commit) = remote_commit {
            let local_commit = version_hint.clone().unwrap_or_default();
            if remote_commit != local_commit {
                emit_git_bundle_status_line(&format!(
                    "updating git bundle {}@{} ({} -> {})",
                    render_git_bundle_source_label(url),
                    reference,
                    abbreviate_revision(&local_commit),
                    abbreviate_revision(&remote_commit)
                ));
                ensure_git_bundle_checkout(manifest_path, url, reference, &local_path)?;
                version_hint = Some(git_head_revision(manifest_path, &local_path)?);
                write_cached_git_bundle_remote_status(
                    &remote_status_path,
                    version_hint.as_deref(),
                )?;
            }
        }
    }

    Ok(ResolvedBundleSource {
        source_type: BundleSourceType::Git,
        local_path,
        source_path: PathBuf::from(url),
        version_hint,
        stale: false,
    })
}

fn resolve_oci_bundle_source(
    manifest_path: &Path,
    url: &str,
    refresh_remote: bool,
) -> Result<ResolvedBundleSource, ManifestError> {
    #[cfg(test)]
    if let Some(adapter) = test_oci_artifact_adapter() {
        return resolve_oci_bundle_source_with_adapter(
            manifest_path,
            url,
            adapter.as_ref(),
            refresh_remote,
        );
    }
    let adapter = OrasCliArtifactAdapter::default();
    resolve_oci_bundle_source_with_adapter(manifest_path, url, &adapter, refresh_remote)
}

fn resolve_oci_bundle_source_with_adapter(
    manifest_path: &Path,
    url: &str,
    adapter: &dyn OciArtifactAdapter,
    refresh_remote: bool,
) -> Result<ResolvedBundleSource, ManifestError> {
    let parsed = ArtifactSourceRef::parse(format!("oci://{url}")).map_err(|error| {
        ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!("invalid OCI bundle source `{url}`: {error}"),
        }
    })?;
    let ArtifactSourceRef::Oci(reference) = parsed else {
        return Err(ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!("invalid OCI bundle source `{url}`"),
        });
    };

    let descriptor = adapter
        .inspect(&effigy_artifacts::OciArtifactInspectRequest {
            reference: reference.clone(),
        })
        .map_err(|error| ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: error.to_string(),
        })?;
    let digest = descriptor.digest.clone();
    let local_path = oci_bundle_cache_path(manifest_path, reference.reference())?;
    let metadata_path = oci_bundle_metadata_path(&local_path);
    let cached_digest = read_cached_oci_bundle_digest(&metadata_path)?;
    let needs_pull = !local_path.is_dir() || cached_digest.is_none();
    let stale = !needs_pull && cached_digest.as_ref() != digest.as_ref();

    if needs_pull || (refresh_remote && stale) {
        let report = adapter
            .pull(&OciArtifactPullRequest {
                reference: reference.clone(),
                destination_root: local_path
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .to_path_buf(),
            })
            .map_err(|error| ManifestError::Compose {
                path: manifest_path.to_path_buf(),
                detail: error.to_string(),
            })?;
        replace_bundle_cache_root(&report.pulled_root, &local_path, manifest_path)?;
        write_cached_oci_bundle_digest(&metadata_path, digest.as_deref())?;
    }

    Ok(ResolvedBundleSource {
        source_type: BundleSourceType::Oci,
        local_path,
        source_path: PathBuf::from(format!("oci://{url}")),
        version_hint: digest,
        stale: stale && !refresh_remote,
    })
}

fn ensure_git_bundle_checkout(
    manifest_path: &Path,
    url: &str,
    reference: &str,
    local_path: &Path,
) -> Result<(), ManifestError> {
    if !local_path.join(".git").is_dir() {
        if local_path.exists() && !local_path.is_dir() {
            return Err(ManifestError::Compose {
                path: manifest_path.to_path_buf(),
                detail: format!(
                    "git bundle cache path exists but is not a directory: {}",
                    local_path.display()
                ),
            });
        }
        if let Some(parent) = local_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| ManifestError::Read {
                path: parent.to_path_buf(),
                error,
            })?;
        }
        let destination = local_path.to_string_lossy().to_string();
        run_git(
            manifest_path,
            None,
            &["clone", "--no-checkout", url, &destination],
            local_path.parent(),
        )?;
    }

    run_git(
        manifest_path,
        Some(local_path),
        &["remote", "set-url", "origin", url],
        None,
    )?;
    run_git(
        manifest_path,
        Some(local_path),
        &["fetch", "--depth", "1", "origin", reference],
        None,
    )?;
    run_git(
        manifest_path,
        Some(local_path),
        &["checkout", "--detach", "FETCH_HEAD"],
        None,
    )?;
    Ok(())
}

fn git_head_revision(manifest_path: &Path, local_path: &Path) -> Result<String, ManifestError> {
    let output = run_git(
        manifest_path,
        Some(local_path),
        &["rev-parse", "HEAD"],
        None,
    )?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn git_ls_remote(
    manifest_path: &Path,
    url: &str,
    reference: &str,
    local_path: &Path,
) -> Result<String, ManifestError> {
    let output = run_git(
        manifest_path,
        Some(local_path),
        &["ls-remote", url, reference],
        None,
    )?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let commit = stdout
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().next())
        .unwrap_or("")
        .to_owned();
    if commit.is_empty() {
        return Err(ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!("git ls-remote returned no commit for {url} {reference}"),
        });
    }
    Ok(commit)
}

fn run_git(
    manifest_path: &Path,
    cwd: Option<&Path>,
    args: &[&str],
    create_dir: Option<&Path>,
) -> Result<std::process::Output, ManifestError> {
    if let Some(dir) = create_dir {
        std::fs::create_dir_all(dir).map_err(|error| ManifestError::Read {
            path: dir.to_path_buf(),
            error,
        })?;
    }
    let mut command = Command::new("git");
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    command.args(args);
    let output = command.output().map_err(|error| ManifestError::Read {
        path: manifest_path.to_path_buf(),
        error,
    })?;
    if output.status.success() {
        return Ok(output);
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    let detail = if stderr.is_empty() {
        format!("git {} failed", args.join(" "))
    } else {
        format!("git {} failed: {stderr}", args.join(" "))
    };
    Err(ManifestError::Compose {
        path: manifest_path.to_path_buf(),
        detail,
    })
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

fn emit_git_bundle_status_line(message: &str) {
    if !std::io::stderr().is_terminal() || std::env::var_os("CI").is_some() {
        return;
    }
    let mut stderr = std::io::stderr().lock();
    let _ = writeln!(stderr, "[bundle] {message}");
}

fn render_git_bundle_source_label(url: &str) -> String {
    canonical_git_cache_identity(url)
}

fn abbreviate_revision(revision: &str) -> &str {
    revision.get(..7).unwrap_or(revision)
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

fn sanitize_bundle_cache_segment(value: &str) -> String {
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

fn git_bundle_cache_path(
    manifest_path: &Path,
    url: &str,
    reference: Option<&str>,
) -> Result<PathBuf, ManifestError> {
    let cache_root = bundle_cache_home_dir(manifest_path)?.join("cache/bundles/git");
    let identity = canonical_git_cache_identity(url);
    let cache_key = sha256_hex(identity.as_bytes());
    let reference = reference
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("main");
    Ok(cache_root
        .join(cache_key)
        .join(sanitize_bundle_cache_segment(reference)))
}

fn oci_bundle_cache_path(manifest_path: &Path, reference: &str) -> Result<PathBuf, ManifestError> {
    let locator = oci_bundle_cache_locator(reference);
    Ok(bundle_cache_home_dir(manifest_path)?
        .join("cache/bundles/oci")
        .join(locator.registry)
        .join(locator.repository_path)
        .join(locator.version_segment))
}

fn oci_bundle_metadata_path(local_path: &Path) -> PathBuf {
    local_path.join(".effigy-bundle-source.digest")
}

fn git_bundle_remote_status_path(local_path: &Path) -> PathBuf {
    local_path.join(".effigy-bundle-source.git-remote")
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GitBundleRemoteStatus {
    checked_at_ms: u64,
    remote_commit: String,
}

fn git_bundle_remote_check_ttl() -> Duration {
    let ttl_secs = std::env::var("EFFIGY_GIT_BUNDLE_REMOTE_CHECK_TTL_SECS")
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .unwrap_or(60);
    Duration::from_secs(ttl_secs)
}

fn current_unix_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn git_bundle_remote_status_is_fresh(status: &GitBundleRemoteStatus) -> bool {
    let age_ms = current_unix_timestamp_ms().saturating_sub(status.checked_at_ms);
    age_ms
        <= git_bundle_remote_check_ttl()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX)
}

fn read_cached_git_bundle_remote_status(
    metadata_path: &Path,
) -> Result<Option<GitBundleRemoteStatus>, ManifestError> {
    let Ok(raw) = std::fs::read_to_string(metadata_path) else {
        return Ok(None);
    };
    let mut lines = raw.lines();
    let checked_at_ms = lines
        .next()
        .and_then(|value| value.trim().parse::<u64>().ok());
    let remote_commit = lines
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    match (checked_at_ms, remote_commit) {
        (Some(checked_at_ms), Some(remote_commit)) => Ok(Some(GitBundleRemoteStatus {
            checked_at_ms,
            remote_commit,
        })),
        _ => Ok(None),
    }
}

fn write_cached_git_bundle_remote_status(
    metadata_path: &Path,
    remote_commit: Option<&str>,
) -> Result<(), ManifestError> {
    let Some(parent) = metadata_path.parent() else {
        return Ok(());
    };
    std::fs::create_dir_all(parent).map_err(|error| ManifestError::Read {
        path: parent.to_path_buf(),
        error,
    })?;
    let remote_commit = remote_commit.unwrap_or_default().trim();
    if remote_commit.is_empty() {
        return Ok(());
    }
    let rendered = format!("{}\n{}\n", current_unix_timestamp_ms(), remote_commit);
    std::fs::write(metadata_path, rendered).map_err(|error| ManifestError::Read {
        path: metadata_path.to_path_buf(),
        error,
    })
}

fn read_cached_oci_bundle_digest(metadata_path: &Path) -> Result<Option<String>, ManifestError> {
    let Ok(raw) = std::fs::read_to_string(metadata_path) else {
        return Ok(None);
    };
    let digest = raw.trim().to_owned();
    if digest.is_empty() {
        return Ok(None);
    }
    Ok(Some(digest))
}

fn write_cached_oci_bundle_digest(
    metadata_path: &Path,
    digest: Option<&str>,
) -> Result<(), ManifestError> {
    let Some(parent) = metadata_path.parent() else {
        return Ok(());
    };
    std::fs::create_dir_all(parent).map_err(|error| ManifestError::Read {
        path: parent.to_path_buf(),
        error,
    })?;
    std::fs::write(metadata_path, digest.unwrap_or_default()).map_err(|error| ManifestError::Read {
        path: metadata_path.to_path_buf(),
        error,
    })
}

fn replace_bundle_cache_root(
    pulled_root: &Path,
    local_path: &Path,
    manifest_path: &Path,
) -> Result<(), ManifestError> {
    if let Some(parent) = local_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| ManifestError::Read {
            path: parent.to_path_buf(),
            error,
        })?;
    }
    if local_path.exists() {
        std::fs::remove_dir_all(local_path).map_err(|error| ManifestError::Read {
            path: local_path.to_path_buf(),
            error,
        })?;
    }
    copy_dir_all(pulled_root, local_path, manifest_path)?;
    std::fs::remove_dir_all(pulled_root).map_err(|error| ManifestError::Read {
        path: pulled_root.to_path_buf(),
        error,
    })
}

fn copy_dir_all(
    source: &Path,
    destination: &Path,
    manifest_path: &Path,
) -> Result<(), ManifestError> {
    std::fs::create_dir_all(destination).map_err(|error| ManifestError::Read {
        path: destination.to_path_buf(),
        error,
    })?;
    for entry in std::fs::read_dir(source).map_err(|error| ManifestError::Read {
        path: source.to_path_buf(),
        error,
    })? {
        let entry = entry.map_err(|error| ManifestError::Read {
            path: source.to_path_buf(),
            error,
        })?;
        let entry_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if entry_path.is_dir() {
            copy_dir_all(&entry_path, &destination_path, manifest_path)?;
        } else if entry_path.is_file() {
            std::fs::copy(&entry_path, &destination_path).map_err(|error| {
                ManifestError::Compose {
                    path: manifest_path.to_path_buf(),
                    detail: format!(
                        "failed to materialize OCI bundle cache file {}: {error}",
                        destination_path.display()
                    ),
                }
            })?;
        }
    }
    Ok(())
}

struct OciBundleCacheLocator {
    registry: String,
    repository_path: String,
    version_segment: String,
}

fn oci_bundle_cache_locator(reference: &str) -> OciBundleCacheLocator {
    let (without_digest, version_segment) = if let Some((path, digest)) = reference.rsplit_once('@')
    {
        (path, sanitize_bundle_cache_segment(digest))
    } else {
        let slash_index = reference.rfind('/').unwrap_or(0);
        let tag_index = reference[slash_index..]
            .rfind(':')
            .map(|index| slash_index + index);
        if let Some(tag_index) = tag_index {
            (
                &reference[..tag_index],
                sanitize_bundle_cache_segment(&reference[tag_index + 1..]),
            )
        } else {
            (reference, "latest".to_owned())
        }
    };

    let (registry, repository_path) = without_digest
        .split_once('/')
        .map(|(registry, path)| (registry.to_owned(), path.to_owned()))
        .unwrap_or_else(|| ("oci".to_owned(), without_digest.to_owned()));

    OciBundleCacheLocator {
        registry: sanitize_bundle_cache_segment(&registry),
        repository_path: repository_path
            .split('/')
            .map(sanitize_bundle_cache_segment)
            .collect::<Vec<_>>()
            .join("/"),
        version_segment,
    }
}

fn sha256_hex(input: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input);
    format!("{:x}", hasher.finalize())
}

fn bundle_cache_home_dir(manifest_path: &Path) -> Result<PathBuf, ManifestError> {
    if let Some(path) = test_bundle_home_dir() {
        return Ok(path.join(".effigy"));
    }
    // Store bundle caches inside the project's .effigy directory so they're
    // available inside workspace containers (which mount the project root).
    let project_root = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    Ok(project_root.join(".effigy"))
}

pub fn sync_bundle_source(manifest_path: &Path) -> Result<Option<BundleSyncReport>, ManifestError> {
    let Some(bundle) = crate::composition::load_manifest_bundle_config(manifest_path)? else {
        return Ok(None);
    };
    let selection = resolve_bundle_selection(manifest_path, &bundle)?;
    match &selection {
        BundleSelection::Path { path } => Ok(Some(BundleSyncReport {
            source_type: BundleSourceType::Path,
            source_path: path.clone(),
            local_path: Some(path.clone()),
            version_hint: None,
            changed: false,
            applicable: false,
        })),
        BundleSelection::Git { url, reference } => {
            let before = read_cached_git_bundle_version(manifest_path, url, reference.as_deref())?;
            let resolved =
                resolve_materialized_bundle_source_with_options(manifest_path, &selection, true)?;
            Ok(Some(BundleSyncReport {
                source_type: BundleSourceType::Git,
                source_path: resolved.source_path,
                local_path: Some(resolved.local_path),
                changed: before != resolved.version_hint,
                version_hint: resolved.version_hint,
                applicable: true,
            }))
        }
        BundleSelection::Oci { .. } => {
            let before = read_cached_oci_bundle_version(manifest_path, &selection)?;
            let resolved =
                resolve_materialized_bundle_source_with_options(manifest_path, &selection, true)?;
            Ok(Some(BundleSyncReport {
                source_type: BundleSourceType::Oci,
                source_path: resolved.source_path,
                local_path: Some(resolved.local_path),
                changed: before != resolved.version_hint,
                version_hint: resolved.version_hint,
                applicable: true,
            }))
        }
    }
}

pub fn inspect_bundle_source(
    manifest_path: &Path,
) -> Result<Option<BundleSourceInspectReport>, ManifestError> {
    let Some(bundle) = crate::composition::load_manifest_bundle_config(manifest_path)? else {
        return Ok(None);
    };
    let selection = resolve_bundle_selection(manifest_path, &bundle)?;
    let resolved =
        resolve_materialized_bundle_source_with_options(manifest_path, &selection, false)?;
    Ok(Some(BundleSourceInspectReport {
        source_type: resolved.source_type,
        source_path: resolved.source_path,
        local_path: resolved.local_path,
        version_hint: resolved.version_hint,
        stale: resolved.stale,
    }))
}

fn read_cached_git_bundle_version(
    manifest_path: &Path,
    url: &str,
    reference: Option<&str>,
) -> Result<Option<String>, ManifestError> {
    let local_path = git_bundle_cache_path(manifest_path, url, reference)?;
    if !local_path.join(".git").is_dir() {
        return Ok(None);
    }
    git_head_revision(manifest_path, &local_path).map(Some)
}

fn read_cached_oci_bundle_version(
    manifest_path: &Path,
    selection: &BundleSelection,
) -> Result<Option<String>, ManifestError> {
    let BundleSelection::Oci { url } = selection else {
        return Ok(None);
    };
    let local_path = oci_bundle_cache_path(manifest_path, url)?;
    read_cached_oci_bundle_digest(&oci_bundle_metadata_path(&local_path))
}

#[cfg(test)]
thread_local! {
    static TEST_BUNDLE_HOME: std::cell::RefCell<Option<PathBuf>> = const {
        std::cell::RefCell::new(None)
    };
    static TEST_OCI_ARTIFACT_ADAPTER:
        std::cell::RefCell<Option<std::rc::Rc<dyn OciArtifactAdapter>>> = const {
            std::cell::RefCell::new(None)
        };
}

#[cfg(test)]
fn test_bundle_home_dir() -> Option<PathBuf> {
    TEST_BUNDLE_HOME.with(|slot| slot.borrow().clone())
}

#[cfg(test)]
#[allow(dead_code)]
fn test_oci_artifact_adapter() -> Option<std::rc::Rc<dyn OciArtifactAdapter>> {
    TEST_OCI_ARTIFACT_ADAPTER.with(|slot| slot.borrow().clone())
}

#[cfg(not(test))]
fn test_bundle_home_dir() -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use effigy_artifacts::{
        OciArtifactDescriptor, OciArtifactError, OciArtifactInspectRequest, OciArtifactPullReport,
        OciArtifactPullRequest, OciArtifactPushReport, OciArtifactPushRequest,
    };
    use std::cell::Cell;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct TestBundleHomeGuard(Option<PathBuf>);
    struct TestOciArtifactAdapterGuard(Option<Rc<dyn OciArtifactAdapter>>);

    impl Drop for TestBundleHomeGuard {
        fn drop(&mut self) {
            TEST_BUNDLE_HOME.with(|slot| {
                *slot.borrow_mut() = self.0.take();
            });
        }
    }

    impl Drop for TestOciArtifactAdapterGuard {
        fn drop(&mut self) {
            TEST_OCI_ARTIFACT_ADAPTER.with(|slot| {
                *slot.borrow_mut() = self.0.take();
            });
        }
    }

    fn with_test_bundle_home(path: &Path) -> TestBundleHomeGuard {
        let previous = TEST_BUNDLE_HOME.with(|slot| slot.borrow_mut().replace(path.to_path_buf()));
        TestBundleHomeGuard(previous)
    }

    fn with_test_oci_adapter(adapter: Rc<dyn OciArtifactAdapter>) -> TestOciArtifactAdapterGuard {
        let previous = TEST_OCI_ARTIFACT_ADAPTER.with(|slot| slot.borrow_mut().replace(adapter));
        TestOciArtifactAdapterGuard(previous)
    }

    fn git(args: &[&str], cwd: &Path) {
        let status = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .status()
            .expect("run git");
        assert!(
            status.success(),
            "git {:?} failed in {}",
            args,
            cwd.display()
        );
    }

    fn write_local_bundle_repo(repo: &Path) {
        std::fs::create_dir_all(repo).expect("mkdir repo");
        std::fs::write(
            repo.join("bundle.toml"),
            r#"
[bundle]
name = "acme"

[[inputs]]
name = "host"
type = "string"
required = true
"#,
        )
        .expect("write descriptor");
        std::fs::write(
            repo.join("effigy.toml"),
            r#"
[tasks.dev]
run = "serve {{ inputs.host }}"
"#,
        )
        .expect("write defaults");
        git(&["init"], repo);
        git(&["config", "user.email", "effigy@example.test"], repo);
        git(&["config", "user.name", "Effigy Tests"], repo);
        git(&["add", "."], repo);
        git(&["commit", "-m", "init"], repo);
        git(&["branch", "-M", "main"], repo);
    }

    fn write_local_bundle_files(root: &Path) {
        std::fs::create_dir_all(root).expect("mkdir bundle root");
        std::fs::write(
            root.join("bundle.toml"),
            r#"
[bundle]
name = "acme"

[[inputs]]
name = "host"
type = "string"
required = true
"#,
        )
        .expect("write descriptor");
        std::fs::write(
            root.join("effigy.toml"),
            r#"
[tasks.dev]
run = "serve {{ inputs.host }}"
"#,
        )
        .expect("write defaults");
    }

    struct FakeOciBundleAdapter {
        digest: String,
        pulls: Cell<u32>,
    }

    impl OciArtifactAdapter for FakeOciBundleAdapter {
        fn inspect(
            &self,
            request: &OciArtifactInspectRequest,
        ) -> Result<OciArtifactDescriptor, OciArtifactError> {
            Ok(OciArtifactDescriptor::new(&request.reference).with_digest(self.digest.clone()))
        }

        fn pull(
            &self,
            _request: &OciArtifactPullRequest,
        ) -> Result<OciArtifactPullReport, OciArtifactError> {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            self.pulls.set(self.pulls.get() + 1);
            let pulled_root = std::env::temp_dir().join(format!(
                "effigy-oci-bundle-pull-{}-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed),
                self.pulls.get()
            ));
            write_local_bundle_files(&pulled_root);
            Ok(OciArtifactPullReport {
                descriptor: OciArtifactDescriptor {
                    reference: "oci://ghcr.io/acme/bundle:v1".to_owned(),
                    redacted_reference: "ghcr.io/acme/bundle:v1".to_owned(),
                    digest: Some(self.digest.clone()),
                    media_type: None,
                    size: None,
                },
                pulled_root,
                primary_files: vec![PathBuf::from("bundle.toml"), PathBuf::from("effigy.toml")],
            })
        }

        fn push(
            &self,
            _request: &OciArtifactPushRequest,
        ) -> Result<OciArtifactPushReport, OciArtifactError> {
            unreachable!("push not used in bundle source tests")
        }
    }

    struct FailingPullOciBundleAdapter;

    impl OciArtifactAdapter for FailingPullOciBundleAdapter {
        fn inspect(
            &self,
            request: &OciArtifactInspectRequest,
        ) -> Result<OciArtifactDescriptor, OciArtifactError> {
            Ok(OciArtifactDescriptor::new(&request.reference).with_digest("sha256:pullfail"))
        }

        fn pull(
            &self,
            request: &OciArtifactPullRequest,
        ) -> Result<OciArtifactPullReport, OciArtifactError> {
            Err(OciArtifactError::PullFailed {
                reference: request.reference.redacted(),
                message: "unauthorized; authenticate first with `oras login ghcr.io` and retry"
                    .to_owned(),
            })
        }

        fn push(
            &self,
            _request: &OciArtifactPushRequest,
        ) -> Result<OciArtifactPushReport, OciArtifactError> {
            unreachable!("push not used in bundle source tests")
        }
    }

    #[test]
    fn canonical_git_cache_identity_normalizes_common_remote_forms() {
        let ssh = canonical_git_cache_identity("git@github.com:Acme/Bundle.git");
        let https = canonical_git_cache_identity("https://github.com/acme/bundle.git");
        assert_eq!(ssh, https);
        assert_eq!(ssh, "github.com/acme/bundle");
    }

    #[test]
    fn git_bundle_source_materializes_into_shared_cache_root() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let bundle_home = tmp.path().join("bundle-home");
        let _home = with_test_bundle_home(&bundle_home);

        let source_repo = tmp.path().join("bundle-source");
        write_local_bundle_repo(&source_repo);

        let consumer = tmp.path().join("consumer");
        std::fs::create_dir_all(&consumer).expect("mkdir consumer");
        let manifest_path = consumer.join("effigy.toml");
        std::fs::write(
            &manifest_path,
            format!(
                "[bundle]\nbase = {{ type = \"git\", url = {:?}, ref = \"main\" }}\nhost = \"acme.test\"\n",
                source_repo.display().to_string()
            ),
        )
        .expect("write manifest");

        let loaded =
            crate::load_task_manifest_with_inspection(&manifest_path).expect("load manifest");
        let task = loaded.manifest.tasks.get("dev").expect("task");
        assert!(matches!(
            task.run.as_ref().expect("run"),
            crate::ManifestManagedRun::Command(command) if command == "serve acme.test"
        ));
        let bundle_root = loaded.bundle_root.expect("bundle root");
        assert!(bundle_root.starts_with(bundle_home.join(".effigy/cache/bundles/git")));
        assert!(bundle_root.join("bundle.toml").exists());
        assert!(bundle_root.join("effigy.toml").exists());
    }

    #[test]
    fn oci_bundle_source_materializes_into_shared_cache_root() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let bundle_home = tmp.path().join("bundle-home");
        let _home = with_test_bundle_home(&bundle_home);
        let _adapter = with_test_oci_adapter(Rc::new(FakeOciBundleAdapter {
            digest: "sha256:abc123".to_owned(),
            pulls: Cell::new(0),
        }));

        let consumer = tmp.path().join("consumer");
        std::fs::create_dir_all(&consumer).expect("mkdir consumer");
        let manifest_path = consumer.join("effigy.toml");
        std::fs::write(
            &manifest_path,
            "[bundle]\nbase = { type = \"oci\", url = \"ghcr.io/acme/bundle:v1\" }\nhost = \"acme.test\"\n",
        )
        .expect("write manifest");

        let loaded =
            crate::load_task_manifest_with_inspection(&manifest_path).expect("load manifest");
        let task = loaded.manifest.tasks.get("dev").expect("task");
        assert!(matches!(
            task.run.as_ref().expect("run"),
            crate::ManifestManagedRun::Command(command) if command == "serve acme.test"
        ));
        let bundle_root = loaded.bundle_root.expect("bundle root");
        assert!(bundle_root
            .starts_with(bundle_home.join(".effigy/cache/bundles/oci/ghcr.io/acme/bundle/v1")));
        assert!(bundle_root.join("bundle.toml").exists());
        assert!(bundle_root.join("effigy.toml").exists());
    }

    #[test]
    fn oci_bundle_source_marks_cached_bundle_stale_when_digest_changes() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let bundle_home = tmp.path().join("bundle-home");
        let _home = with_test_bundle_home(&bundle_home);
        let manifest_path = tmp.path().join("consumer.toml");
        std::fs::write(
            &manifest_path,
            "[bundle]\nbase = { type = \"oci\", url = \"ghcr.io/acme/bundle:v1\" }\n",
        )
        .expect("write manifest");

        let _adapter = with_test_oci_adapter(Rc::new(FakeOciBundleAdapter {
            digest: "sha256:abc123".to_owned(),
            pulls: Cell::new(0),
        }));
        let first = resolve_materialized_bundle_source(
            &manifest_path,
            &BundleSelection::Oci {
                url: "ghcr.io/acme/bundle:v1".to_owned(),
            },
        )
        .expect("first resolve");
        assert!(!first.stale);

        let _adapter = with_test_oci_adapter(Rc::new(FakeOciBundleAdapter {
            digest: "sha256:def456".to_owned(),
            pulls: Cell::new(0),
        }));
        let second = resolve_materialized_bundle_source(
            &manifest_path,
            &BundleSelection::Oci {
                url: "ghcr.io/acme/bundle:v1".to_owned(),
            },
        )
        .expect("second resolve");
        assert!(second.stale);
        assert_eq!(second.version_hint.as_deref(), Some("sha256:def456"));
        assert_eq!(first.local_path, second.local_path);
    }

    #[test]
    fn oci_bundle_source_surfaces_pull_failures() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let bundle_home = tmp.path().join("bundle-home");
        let _home = with_test_bundle_home(&bundle_home);
        let _adapter = with_test_oci_adapter(Rc::new(FailingPullOciBundleAdapter));
        let manifest_path = tmp.path().join("consumer.toml");
        std::fs::write(
            &manifest_path,
            "[bundle]\nbase = { type = \"oci\", url = \"ghcr.io/acme/bundle:v1\" }\n",
        )
        .expect("write manifest");

        let error = resolve_materialized_bundle_source(
            &manifest_path,
            &BundleSelection::Oci {
                url: "ghcr.io/acme/bundle:v1".to_owned(),
            },
        )
        .expect_err("reject pull failure");
        let rendered = error.to_string();
        assert!(rendered.contains("unauthorized"));
        assert!(rendered.contains("oras login ghcr.io"));
    }

    #[test]
    fn sync_git_bundle_source_reports_ref_changes() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let bundle_home = tmp.path().join("bundle-home");
        let _home = with_test_bundle_home(&bundle_home);

        let source_repo = tmp.path().join("bundle-source");
        write_local_bundle_repo(&source_repo);

        let manifest_path = tmp.path().join("consumer.toml");
        std::fs::write(
            &manifest_path,
            format!(
                "[bundle]\nbase = {{ type = \"git\", url = {:?}, ref = \"main\" }}\n",
                source_repo.display().to_string()
            ),
        )
        .expect("write manifest");

        let first = sync_bundle_source(&manifest_path)
            .expect("sync source")
            .expect("bundle sync report");
        assert!(first.applicable);
        assert!(first.changed);

        std::fs::write(source_repo.join("README.md"), "next").expect("write next revision");
        git(&["add", "."], &source_repo);
        git(&["commit", "-m", "next"], &source_repo);

        let second = sync_bundle_source(&manifest_path)
            .expect("sync source")
            .expect("bundle sync report");
        assert!(second.applicable);
        assert!(second.changed);
        assert_ne!(first.version_hint, second.version_hint);

        let third = sync_bundle_source(&manifest_path)
            .expect("sync source")
            .expect("bundle sync report");
        assert!(third.applicable);
        assert!(!third.changed);
        assert_eq!(third.version_hint, second.version_hint);
    }

    #[test]
    fn git_bundle_source_refreshes_stale_cache_root_on_manifest_load() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let bundle_home = tmp.path().join("bundle-home");
        let _home = with_test_bundle_home(&bundle_home);

        let source_repo = tmp.path().join("bundle-source");
        write_local_bundle_repo(&source_repo);

        let manifest_path = tmp.path().join("consumer.toml");
        std::fs::write(
            &manifest_path,
            format!(
                "[bundle]\nbase = {{ type = \"git\", url = {:?}, ref = \"main\" }}\n",
                source_repo.display().to_string()
            ),
        )
        .expect("write manifest");

        let first = resolve_materialized_bundle_source(
            &manifest_path,
            &BundleSelection::Git {
                url: source_repo.display().to_string(),
                reference: Some("main".to_owned()),
            },
        )
        .expect("first resolve");

        std::fs::write(
            source_repo.join("export.toml"),
            "[manifest]\nextend = [\"bundle.meta\"]\n\n[bundle.meta]\nsource = \"next\"\n",
        )
        .expect("update export template");
        git(&["add", "."], &source_repo);
        git(&["commit", "-m", "next"], &source_repo);

        let second = resolve_materialized_bundle_source(
            &manifest_path,
            &BundleSelection::Git {
                url: source_repo.display().to_string(),
                reference: Some("main".to_owned()),
            },
        )
        .expect("second resolve");

        assert_eq!(first.local_path, second.local_path);
        assert_eq!(
            first.version_hint, second.version_hint,
            "fresh remote-check cache should skip repeated remote probes"
        );
        assert!(
            !second.local_path.join("export.toml").is_file(),
            "fresh remote-check cache should leave the cached bundle unchanged until the check window expires"
        );

        let remote_status_path = git_bundle_remote_status_path(&second.local_path);
        std::fs::write(
            &remote_status_path,
            format!(
                "0\n{}\n",
                first.version_hint.clone().expect("first version")
            ),
        )
        .expect("backdate remote status");

        let third = resolve_materialized_bundle_source(
            &manifest_path,
            &BundleSelection::Git {
                url: source_repo.display().to_string(),
                reference: Some("main".to_owned()),
            },
        )
        .expect("third resolve");

        assert_eq!(first.local_path, second.local_path);
        assert_eq!(first.local_path, third.local_path);
        assert_ne!(first.version_hint, third.version_hint);
        let cached_export =
            std::fs::read_to_string(third.local_path.join("export.toml")).expect("read cache");
        assert!(
            cached_export.contains("source = \"next\""),
            "expected refreshed git bundle cache to pick up the new commit"
        );
        assert!(!third.stale);
    }

    #[test]
    fn sync_oci_bundle_source_refreshes_stale_cache_root() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let bundle_home = tmp.path().join("bundle-home");
        let _home = with_test_bundle_home(&bundle_home);
        let manifest_path = tmp.path().join("consumer.toml");
        std::fs::write(
            &manifest_path,
            "[bundle]\nbase = { type = \"oci\", url = \"ghcr.io/acme/bundle:v1\" }\n",
        )
        .expect("write manifest");

        let _adapter = with_test_oci_adapter(Rc::new(FakeOciBundleAdapter {
            digest: "sha256:abc123".to_owned(),
            pulls: Cell::new(0),
        }));
        let first = sync_bundle_source(&manifest_path)
            .expect("sync source")
            .expect("bundle sync report");
        assert!(first.applicable);
        assert!(first.changed);
        assert_eq!(first.version_hint.as_deref(), Some("sha256:abc123"));

        let _adapter = with_test_oci_adapter(Rc::new(FakeOciBundleAdapter {
            digest: "sha256:def456".to_owned(),
            pulls: Cell::new(0),
        }));
        let second = sync_bundle_source(&manifest_path)
            .expect("sync source")
            .expect("bundle sync report");
        assert!(second.applicable);
        assert!(second.changed);
        assert_eq!(second.version_hint.as_deref(), Some("sha256:def456"));

        let third = sync_bundle_source(&manifest_path)
            .expect("sync source")
            .expect("bundle sync report");
        assert!(third.applicable);
        assert!(!third.changed);
        assert_eq!(third.version_hint.as_deref(), Some("sha256:def456"));
    }
}
