use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use effigy_artifacts::{
    ArtifactSourceRef, OciArtifactAdapter, OciArtifactPullRequest, OrasCliArtifactAdapter,
};
use sha2::{Digest, Sha256};
use toml::Value;

use crate::ManifestError;

mod export;
mod specs;

use export::{materialize_shipped_bundle_assets, shipped_bundle_export_files};
use specs::{
    decodelabs_library_spec, decodelabs_spec, resolve_decodelabs_bundle,
    resolve_decodelabs_library_bundle, resolve_underlay_bundle, underlay_spec,
};

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct BundleSpec {
    pub name: String,
    pub description: String,
    pub inputs: Vec<BundleInputSpec>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct BundleInputSpec {
    pub name: String,
    pub value_type: BundleInputType,
    pub required: bool,
    pub description: String,
    pub default: Option<Value>,
    pub example: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BundleInputType {
    String,
    Integer,
    Bool,
    List,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct BundleExport {
    pub bundle: String,
    pub path: PathBuf,
    pub files: Vec<String>,
}

// The remote-source variants are introduced here so later git/OCI batches can
// widen the same source seam without another model break.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BundleSourceType {
    Shipped,
    Path,
    Git,
    Oci,
}

// `version_hint` and `stale` are part of the locked source-materialization
// shape but are not consumed until the remote resolver and inspect/sync batches.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct ResolvedBundleSource {
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

pub(crate) fn apply_bundle_defaults(
    manifest_path: &Path,
    current: &mut Value,
    extend_paths: &[String],
) -> Result<Option<AppliedBundleDefaults>, ManifestError> {
    let Some(bundle): Option<crate::config_sections::ManifestBundleConfig> = current
        .as_table()
        .and_then(|table| table.get("bundle"))
        .cloned()
        .map(|value| {
            value.try_into().map_err(|error| ManifestError::Compose {
                path: manifest_path.to_path_buf(),
                detail: format!("invalid `[bundle]` section: {error}"),
            })
        })
        .transpose()?
    else {
        return Ok(None);
    };

    let selection = resolve_bundle_selection(manifest_path, &bundle)?;
    let mut normalized_inputs = bundle.inputs.clone();
    let bundle_name = match &selection {
        BundleSelection::Shipped { name } => name.as_str(),
        BundleSelection::Path { .. } => "",
        BundleSelection::Git { .. } => "",
        BundleSelection::Oci { .. } => "",
    };
    if !bundle_name.is_empty() {
        normalize_database_bundle_inputs(manifest_path, bundle_name, &mut normalized_inputs)?;
        normalize_bundle_specific_inputs(manifest_path, bundle_name, &mut normalized_inputs)?;
    }
    let resolved_source = resolve_materialized_bundle_source(manifest_path, &selection)?;
    let (mut defaults, source_path) = resolve_bundle_defaults_from_source(
        manifest_path,
        current,
        &selection,
        &resolved_source,
        &normalized_inputs,
    )?;
    let bundle_extend_paths = take_bundle_extend_paths(manifest_path, &mut defaults)?;
    let existing_extend_paths = combined_bundle_extend_paths(extend_paths, &bundle_extend_paths);
    let existing_bundle_values = existing_extend_paths
        .iter()
        .map(|path| (path.clone(), lookup_value_at_path(current, path).is_some()))
        .collect::<BTreeMap<_, _>>();
    merge_missing_values(current, &defaults);
    apply_bundle_extend_paths(
        manifest_path,
        current,
        &defaults,
        &existing_extend_paths,
        &existing_bundle_values,
    )?;
    Ok(Some(AppliedBundleDefaults {
        source_path,
        bundle_root: resolved_source.local_path,
    }))
}

pub(crate) fn bundle_source_path(name: &str) -> PathBuf {
    PathBuf::from(format!("<bundle:{name}>"))
}

fn resolve_materialized_bundle_source(
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
        BundleSelection::Shipped { name } => Ok(ResolvedBundleSource {
            source_type: BundleSourceType::Shipped,
            local_path: materialize_shipped_bundle_assets(manifest_path, name)?,
            source_path: bundle_source_path(name),
            version_hint: None,
            stale: false,
        }),
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

fn resolve_bundle_defaults_from_source(
    manifest_path: &Path,
    current: &Value,
    selection: &BundleSelection,
    source: &ResolvedBundleSource,
    normalized_inputs: &BTreeMap<String, Value>,
) -> Result<(Value, PathBuf), ManifestError> {
    match (selection, source.source_type) {
        (BundleSelection::Shipped { name }, BundleSourceType::Shipped) => Ok((
            resolve_bundle_defaults(manifest_path, current, name, normalized_inputs)?,
            source.source_path.clone(),
        )),
        (BundleSelection::Path { .. }, BundleSourceType::Path)
        | (BundleSelection::Git { .. }, BundleSourceType::Git)
        | (BundleSelection::Oci { .. }, BundleSourceType::Oci) => {
            resolve_local_bundle_defaults(manifest_path, &source.local_path, normalized_inputs)
        }
        _ => Err(ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: "bundle source resolution mismatch between selection and materialization"
                .to_owned(),
        }),
    }
}

fn resolve_git_bundle_source(
    manifest_path: &Path,
    url: &str,
    reference: Option<&str>,
    _refresh_remote: bool,
) -> Result<ResolvedBundleSource, ManifestError> {
    let local_path = git_bundle_cache_path(manifest_path, url, reference)?;
    let reference = reference
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("main");
    ensure_git_bundle_checkout(manifest_path, url, reference, &local_path)?;
    let version_hint = Some(git_head_revision(manifest_path, &local_path)?);
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
    let home = std::env::var_os("HOME").ok_or_else(|| ManifestError::Compose {
        path: manifest_path.to_path_buf(),
        detail: "HOME is not set; cannot resolve bundle cache path".to_owned(),
    })?;
    Ok(PathBuf::from(home).join(".effigy"))
}

pub fn sync_bundle_source(manifest_path: &Path) -> Result<Option<BundleSyncReport>, ManifestError> {
    let Some(bundle) = crate::composition::load_manifest_bundle_config(manifest_path)? else {
        return Ok(None);
    };
    let selection = resolve_bundle_selection(manifest_path, &bundle)?;
    match &selection {
        BundleSelection::Shipped { name } => Ok(Some(BundleSyncReport {
            source_type: BundleSourceType::Shipped,
            source_path: bundle_source_path(name),
            local_path: None,
            version_hint: None,
            changed: false,
            applicable: false,
        })),
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

#[derive(Debug, Clone)]
pub(crate) struct AppliedBundleDefaults {
    pub source_path: PathBuf,
    pub bundle_root: PathBuf,
}

enum BundleSelection {
    Shipped {
        name: String,
    },
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

fn resolve_bundle_selection(
    manifest_path: &Path,
    bundle: &crate::config_sections::ManifestBundleConfig,
) -> Result<BundleSelection, ManifestError> {
    match bundle.base.as_ref() {
        Some(crate::config_sections::ManifestBundleBase::Shipped { name })
            if !name.trim().is_empty() =>
        {
            Ok(BundleSelection::Shipped {
                name: name.trim().to_owned(),
            })
        }
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
            detail: "`[bundle]` must set `base` to a shipped preset or a typed bundle source block"
                .to_owned(),
        }),
    }
}

pub fn list_bundles() -> Vec<BundleSpec> {
    vec![
        decodelabs_spec(),
        decodelabs_library_spec(),
        underlay_spec(),
    ]
}

pub fn get_bundle(name: &str) -> Option<BundleSpec> {
    list_bundles()
        .into_iter()
        .find(|bundle| bundle.name == name)
}

pub fn render_bundle_defaults(
    name: &str,
    inputs: &BTreeMap<String, Value>,
) -> Result<Value, ManifestError> {
    let mut normalized_inputs = inputs.clone();
    normalize_database_bundle_inputs(&bundle_source_path(name), name, &mut normalized_inputs)?;
    normalize_bundle_specific_inputs(&bundle_source_path(name), name, &mut normalized_inputs)?;
    resolve_bundle_defaults(
        &bundle_source_path(name),
        &Value::Table(Default::default()),
        name,
        &normalized_inputs,
    )
}

pub fn list_bundle_default_paths(name: &str) -> Result<Vec<String>, ManifestError> {
    let spec = get_bundle(name).ok_or_else(|| ManifestError::Compose {
        path: bundle_source_path(name),
        detail: format!("unknown bundle `{name}`"),
    })?;
    let example_inputs = spec
        .inputs
        .iter()
        .map(|input| {
            (
                input.name.clone(),
                input
                    .default
                    .clone()
                    .or_else(|| input.example.clone())
                    .unwrap_or_else(|| Value::String(format!("<{}>", input.name))),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let defaults = render_bundle_defaults(name, &example_inputs)?;
    let mut paths = Vec::new();
    collect_value_paths("", &defaults, &mut paths);
    Ok(paths)
}

pub fn export_bundle(name: &str, target_dir: &Path) -> Result<BundleExport, ManifestError> {
    let files = shipped_bundle_export_files(name)?;
    if target_dir.exists() && !target_dir.is_dir() {
        return Err(ManifestError::Compose {
            path: target_dir.to_path_buf(),
            detail: "bundle export path exists but is not a directory".to_owned(),
        });
    }
    std::fs::create_dir_all(target_dir).map_err(|error| ManifestError::Read {
        path: target_dir.to_path_buf(),
        error,
    })?;

    for file in &files {
        let path = target_dir.join(file.path);
        if path.exists() {
            return Err(ManifestError::Compose {
                path,
                detail:
                    "bundle export refuses to overwrite existing files; choose an empty directory"
                        .to_owned(),
            });
        }
    }

    let mut written = Vec::new();
    for file in files {
        let path = target_dir.join(file.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| ManifestError::Read {
                path: parent.to_path_buf(),
                error,
            })?;
        }
        std::fs::write(&path, file.contents).map_err(|error| ManifestError::Read {
            path: path.clone(),
            error,
        })?;
        written.push(file.path.to_owned());
    }

    Ok(BundleExport {
        bundle: name.to_owned(),
        path: target_dir.to_path_buf(),
        files: written,
    })
}

fn resolve_bundle_defaults(
    manifest_path: &Path,
    current: &Value,
    bundle_name: &str,
    inputs: &BTreeMap<String, Value>,
) -> Result<Value, ManifestError> {
    match bundle_name {
        "decodelabs" => resolve_decodelabs_bundle(manifest_path, inputs),
        "decodelabs-library" => resolve_decodelabs_library_bundle(manifest_path, inputs),
        "underlay" => resolve_underlay_bundle(manifest_path, current, inputs),
        other => Err(ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!("unknown bundle `{other}`"),
        }),
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LocalBundleDescriptor {
    bundle: LocalBundleMetadata,
    #[serde(default)]
    inputs: Vec<LocalBundleInputDescriptor>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LocalBundleMetadata {
    name: String,
    #[serde(default, rename = "description")]
    _description: String,
    #[serde(default = "default_local_bundle_defaults_file")]
    defaults: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LocalBundleInputDescriptor {
    name: String,
    #[serde(rename = "type")]
    value_type: BundleInputType,
    #[serde(default)]
    required: bool,
    #[serde(default, rename = "description")]
    _description: String,
    #[serde(default)]
    default: Option<Value>,
    #[serde(default)]
    example: Option<Value>,
}

fn default_local_bundle_defaults_file() -> String {
    "effigy.toml".to_owned()
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct BundleManifestSectionConfig {
    #[serde(default)]
    extend: Vec<String>,
}

pub(super) fn parse_bundle_descriptor_source(
    path: &Path,
    source: &str,
) -> Result<LocalBundleDescriptor, ManifestError> {
    toml::from_str::<LocalBundleDescriptor>(source).map_err(|error| ManifestError::Parse {
        path: path.to_path_buf(),
        error,
    })
}

pub(super) fn bundle_spec_from_descriptor(descriptor: &LocalBundleDescriptor) -> BundleSpec {
    BundleSpec {
        name: descriptor.bundle.name.clone(),
        description: descriptor.bundle._description.clone(),
        inputs: descriptor
            .inputs
            .iter()
            .map(|input| BundleInputSpec {
                name: input.name.clone(),
                value_type: input.value_type,
                required: input.required,
                description: input._description.clone(),
                default: input.default.clone(),
                example: input.example.clone(),
            })
            .collect(),
    }
}

fn resolve_local_bundle_defaults(
    manifest_path: &Path,
    bundle_dir: &Path,
    inputs: &BTreeMap<String, Value>,
) -> Result<(Value, PathBuf), ManifestError> {
    if !bundle_dir.is_dir() {
        return Err(ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!(
                "`[bundle].base = {{ type = \"path\", dir = ... }}` must point at a directory, got {}",
                bundle_dir.display()
            ),
        });
    }

    let descriptor_path = bundle_dir.join("bundle.toml");
    let descriptor_source =
        std::fs::read_to_string(&descriptor_path).map_err(|error| ManifestError::Read {
            path: descriptor_path.clone(),
            error,
        })?;
    let descriptor = parse_bundle_descriptor_source(&descriptor_path, &descriptor_source)?;
    validate_local_bundle_descriptor(manifest_path, &descriptor)?;
    let resolved_inputs = resolve_local_bundle_inputs(manifest_path, &descriptor, inputs)?;

    let defaults_path = bundle_dir.join(&descriptor.bundle.defaults);
    let defaults_template =
        std::fs::read_to_string(&defaults_path).map_err(|error| ManifestError::Read {
            path: defaults_path.clone(),
            error,
        })?;
    let rendered = render_bundle_template_with_inputs(
        manifest_path,
        &descriptor.bundle.name,
        bundle_dir,
        &defaults_template,
        &resolved_inputs,
    )?;
    let defaults = toml::from_str::<Value>(&rendered).map_err(|error| ManifestError::Parse {
        path: defaults_path,
        error,
    })?;
    Ok((defaults, bundle_dir.join(&descriptor.bundle.defaults)))
}

fn validate_local_bundle_descriptor(
    manifest_path: &Path,
    descriptor: &LocalBundleDescriptor,
) -> Result<(), ManifestError> {
    let name = descriptor.bundle.name.trim();
    if name.is_empty() {
        return Err(ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: "local bundle `bundle.name` must not be empty".to_owned(),
        });
    }
    if descriptor.bundle.defaults.trim().is_empty() {
        return Err(ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!("local bundle `{name}` `bundle.defaults` must not be empty"),
        });
    }

    let mut seen = std::collections::BTreeSet::new();
    for input in &descriptor.inputs {
        let input_name = input.name.trim();
        if input_name.is_empty() {
            return Err(ManifestError::Compose {
                path: manifest_path.to_path_buf(),
                detail: format!("local bundle `{name}` has an empty input name"),
            });
        }
        if matches!(input_name, "base" | "name" | "base_path") {
            return Err(ManifestError::Compose {
                path: manifest_path.to_path_buf(),
                detail: format!(
                    "local bundle `{name}` input `{input_name}` collides with a reserved `[bundle]` selector key"
                ),
            });
        }
        if !seen.insert(input_name.to_owned()) {
            return Err(ManifestError::Compose {
                path: manifest_path.to_path_buf(),
                detail: format!(
                    "local bundle `{name}` declares input `{input_name}` more than once"
                ),
            });
        }
        if let Some(default) = &input.default {
            validate_bundle_input_type(manifest_path, name, input_name, input.value_type, default)?;
        }
        if let Some(example) = &input.example {
            validate_bundle_input_type(manifest_path, name, input_name, input.value_type, example)?;
        }
    }
    Ok(())
}

fn resolve_local_bundle_inputs(
    manifest_path: &Path,
    descriptor: &LocalBundleDescriptor,
    provided: &BTreeMap<String, Value>,
) -> Result<BTreeMap<String, Value>, ManifestError> {
    let bundle_name = descriptor.bundle.name.trim();
    let declared = descriptor
        .inputs
        .iter()
        .map(|input| (input.name.as_str(), input))
        .collect::<BTreeMap<_, _>>();
    for key in bundle_input_paths(provided) {
        if !declared.contains_key(key.as_str()) {
            return Err(ManifestError::Compose {
                path: manifest_path.to_path_buf(),
                detail: format!("local bundle `{bundle_name}` does not declare input `{key}`"),
            });
        }
    }

    let mut resolved = BTreeMap::new();
    for input in &descriptor.inputs {
        let key = input.name.as_str();
        let value = bundle_input_value(provided, key)
            .cloned()
            .or_else(|| input.default.clone());
        match (value, input.required) {
            (Some(value), _) => {
                validate_bundle_input_type(
                    manifest_path,
                    bundle_name,
                    key,
                    input.value_type,
                    &value,
                )?;
                insert_bundle_input_value(&mut resolved, key, value);
            }
            (None, true) => {
                return Err(ManifestError::Compose {
                    path: manifest_path.to_path_buf(),
                    detail: format!("local bundle `{bundle_name}` requires input `{key}`"),
                });
            }
            (None, false) => {}
        }
    }
    normalize_database_bundle_inputs(manifest_path, bundle_name, &mut resolved)?;
    normalize_bundle_specific_inputs(manifest_path, bundle_name, &mut resolved)?;
    Ok(resolved)
}

fn normalize_bundle_specific_inputs(
    manifest_path: &Path,
    bundle_name: &str,
    inputs: &mut BTreeMap<String, Value>,
) -> Result<(), ManifestError> {
    if bundle_name == "underlay" {
        ensure_optional_bundle_string_inputs(
            inputs,
            &[
                "dirs.docs",
                "dirs.api",
                "dirs.client",
                "dirs.ui",
                "dirs.front",
                "dirs.admin",
                "routes.front",
                "routes.admin",
                "routes.api",
                "sources.underlay",
                "sources.poodle",
            ],
        );
        let host = required_bundle_string(manifest_path, bundle_name, inputs, "host")?;
        for (output, input, default_label) in [
            ("front_route_domain", "routes.front", None),
            ("admin_route_domain", "routes.admin", Some("admin")),
            ("api_route_domain", "routes.api", Some("api")),
        ] {
            insert_bundle_input_value(
                inputs,
                output,
                Value::String(underlay_route_domain(
                    &host,
                    optional_bundle_string(inputs, input)
                        .as_deref()
                        .or(default_label),
                )),
            );
        }
        return Ok(());
    }

    if bundle_name == "decodelabs-library" {
        let shared_root_path = bundle_shared_root_path(manifest_path, bundle_name, inputs)?;
        inputs.insert(
            "shared_root".to_owned(),
            Value::String(shared_root_path.display().to_string()),
        );
        if !inputs.contains_key("workspace_subdir") {
            let workspace_subdir = derive_bundle_workspace_subdir(
                manifest_path,
                &shared_root_path.display().to_string(),
            )?;
            inputs.insert(
                "workspace_subdir".to_owned(),
                Value::String(workspace_subdir),
            );
        }
    }

    Ok(())
}

fn normalize_database_bundle_inputs(
    manifest_path: &Path,
    bundle_name: &str,
    inputs: &mut BTreeMap<String, Value>,
) -> Result<(), ManifestError> {
    if inputs.contains_key("database") {
        return Err(ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!(
                "bundle `{bundle_name}` input `database` has been removed; use `databases = [\"app\"]` instead"
            ),
        });
    }

    let Some(databases) =
        normalize_database_value(manifest_path, bundle_name, "databases", inputs)?
    else {
        return Ok(());
    };

    if !inputs.contains_key("databases") {
        inputs.insert("databases".to_owned(), Value::Array(databases.clone()));
    }
    let Some(primary) = databases.first().and_then(|value| value.as_str()) else {
        return Err(ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!(
                "bundle `{bundle_name}` normalized `databases` but found no primary database entry"
            ),
        });
    };
    inputs.insert("database".to_owned(), Value::String(primary.to_owned()));
    Ok(())
}

fn normalize_database_value(
    manifest_path: &Path,
    bundle_name: &str,
    field_name: &str,
    inputs: &BTreeMap<String, Value>,
) -> Result<Option<Vec<Value>>, ManifestError> {
    match inputs.get("databases") {
        Some(Value::Array(values)) => {
            if values.is_empty() {
                return Err(ManifestError::Compose {
                    path: manifest_path.to_path_buf(),
                    detail: format!("bundle `{bundle_name}` input `{field_name}` must contain at least one database name"),
                });
            }
            let mut normalized = Vec::with_capacity(values.len());
            for value in values {
                let Some(name) = value
                    .as_str()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                else {
                    return Err(ManifestError::Compose {
                        path: manifest_path.to_path_buf(),
                        detail: format!("bundle `{bundle_name}` input `{field_name}` must be a list of non-empty strings"),
                    });
                };
                normalized.push(Value::String(name.to_owned()));
            }
            Ok(Some(normalized))
        }
        Some(_) => Err(ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!(
                "bundle `{bundle_name}` input `{field_name}` must be a list of non-empty strings"
            ),
        }),
        None => Ok(None),
    }
}

fn ensure_optional_bundle_string_inputs(inputs: &mut BTreeMap<String, Value>, keys: &[&str]) {
    for key in keys {
        if bundle_input_value(inputs, key).is_none() {
            insert_bundle_input_value(inputs, key, Value::String(String::new()));
        }
    }
}

pub(super) fn render_bundle_template_with_inputs(
    manifest_path: &Path,
    bundle_name: &str,
    bundle_root: &Path,
    template: &str,
    inputs: &BTreeMap<String, Value>,
) -> Result<String, ManifestError> {
    let mut env = minijinja::Environment::new();
    env.add_template("bundle", template)
        .map_err(|error| ManifestError::Render {
            path: manifest_path.to_path_buf(),
            detail: format!("bundle `{bundle_name}` template parse error: {error}"),
        })?;
    let template = env
        .get_template("bundle")
        .map_err(|error| ManifestError::Render {
            path: manifest_path.to_path_buf(),
            detail: format!("bundle `{bundle_name}` template load error: {error}"),
        })?;
    let bundle_root = bundle_root.display().to_string();
    let context = LocalBundleTemplateContext {
        inputs,
        bundle: LocalBundleTemplateBundle {
            name: bundle_name,
            root: &bundle_root,
        },
    };
    template
        .render(context)
        .map_err(|error| ManifestError::Render {
            path: manifest_path.to_path_buf(),
            detail: format!("bundle `{bundle_name}` template render error: {error}"),
        })
}

#[derive(serde::Serialize)]
struct LocalBundleTemplateContext<'a> {
    inputs: &'a BTreeMap<String, Value>,
    bundle: LocalBundleTemplateBundle<'a>,
}

#[derive(serde::Serialize)]
pub(super) struct LocalBundleTemplateBundle<'a> {
    pub(super) name: &'a str,
    pub(super) root: &'a str,
}

pub(super) fn required_bundle_string(
    manifest_path: &Path,
    bundle_name: &str,
    inputs: &BTreeMap<String, Value>,
    key: &str,
) -> Result<String, ManifestError> {
    let Some(value) = bundle_input_value(inputs, key) else {
        return Err(ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!("bundle `{bundle_name}` requires string input `{key}`"),
        });
    };
    let Some(value) = value.as_str() else {
        return Err(ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!("bundle `{bundle_name}` input `{key}` must be a string"),
        });
    };
    let value = value.trim();
    if value.is_empty() {
        return Err(ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!("bundle `{bundle_name}` input `{key}` must not be empty"),
        });
    }
    Ok(value.to_owned())
}

pub(super) fn bundle_shared_root_input(
    _manifest_path: &Path,
    bundle_name: &str,
    inputs: &BTreeMap<String, Value>,
) -> Result<String, ManifestError> {
    Ok(
        optional_bundle_string(inputs, "shared_root").unwrap_or_else(|| {
            bundle_default_input_string(bundle_name, "shared_root")
                .unwrap_or_else(|| "../".to_owned())
        }),
    )
}

pub(super) fn bundle_shared_root_path(
    manifest_path: &Path,
    bundle_name: &str,
    inputs: &BTreeMap<String, Value>,
) -> Result<PathBuf, ManifestError> {
    let shared_root = bundle_shared_root_input(manifest_path, bundle_name, inputs)?;
    let shared_root_path = resolve_bundle_host_path(manifest_path, &shared_root);
    Ok(shared_root_path.canonicalize().unwrap_or(shared_root_path))
}

pub(super) fn bundle_default_input_string(bundle_name: &str, key: &str) -> Option<String> {
    list_bundles()
        .into_iter()
        .find(|spec| spec.name == bundle_name)
        .and_then(|spec| spec.inputs.into_iter().find(|input| input.name == key))
        .and_then(|input| input.default)
        .and_then(|value| value.as_str().map(str::to_owned))
}

pub(super) fn optional_bundle_integer(inputs: &BTreeMap<String, Value>, key: &str) -> Option<i64> {
    bundle_input_value(inputs, key).and_then(Value::as_integer)
}

pub(super) fn optional_bundle_string(
    inputs: &BTreeMap<String, Value>,
    key: &str,
) -> Option<String> {
    let value = bundle_input_value(inputs, key)?.as_str()?.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
}

pub(super) fn render_toml_string_list(inputs: &BTreeMap<String, Value>, key: &str) -> String {
    let Some(values) = bundle_input_value(inputs, key).and_then(Value::as_array) else {
        return "[]".to_owned();
    };
    let encoded = values
        .iter()
        .filter_map(Value::as_str)
        .map(|value| format!("{value:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{encoded}]")
}

pub(super) fn render_toml_string_array_lines(values: &[&str], indent: &str) -> String {
    values
        .iter()
        .map(|value| format!("{indent}{value:?},"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn derive_bundle_workspace_subdir(
    manifest_path: &Path,
    shared_root: &str,
) -> Result<String, ManifestError> {
    let manifest_root = manifest_path
        .parent()
        .ok_or_else(|| ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: "bundle workspace subdir derivation requires a manifest parent directory"
                .to_owned(),
        })?;
    let shared_root = resolve_bundle_host_path(manifest_path, shared_root);
    let manifest_root = manifest_root
        .canonicalize()
        .unwrap_or_else(|_| manifest_root.to_path_buf());
    let shared_root = shared_root.canonicalize().unwrap_or(shared_root);
    let relative = manifest_root
        .strip_prefix(&shared_root)
        .map_err(|_| ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!(
                "bundle `decodelabs-library` could not derive `workspace_subdir` because repo root {} is not under shared_root {}",
                manifest_root.display(),
                shared_root.display()
            ),
        })?;
    if relative.as_os_str().is_empty() {
        return Err(ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: "bundle `decodelabs-library` requires `workspace_subdir` when the repo root equals `shared_root`".to_owned(),
        });
    }
    Ok(relative.display().to_string())
}

pub(super) fn resolve_bundle_host_path(manifest_path: &Path, path: &str) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else {
        manifest_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(path)
    }
}

pub(super) fn underlay_route_domain(host: &str, label: Option<&str>) -> String {
    let label = label.map(str::trim).unwrap_or_default();
    if label.is_empty() {
        host.to_owned()
    } else {
        format!("{label}.{host}")
    }
}

pub(super) fn bundle_input_value<'a>(
    inputs: &'a BTreeMap<String, Value>,
    key: &str,
) -> Option<&'a Value> {
    let mut segments = key.split('.');
    let first = segments.next()?;
    let mut current = inputs.get(first)?;
    for segment in segments {
        current = current.as_table()?.get(segment)?;
    }
    Some(current)
}

pub(super) fn bundle_input_paths(inputs: &BTreeMap<String, Value>) -> Vec<String> {
    let mut paths = Vec::new();
    for (key, value) in inputs {
        collect_bundle_input_paths(key, value, &mut paths);
    }
    paths
}

fn collect_bundle_input_paths(prefix: &str, value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Table(table) => {
            for (key, child) in table {
                let child_prefix = if prefix.is_empty() {
                    key.to_owned()
                } else {
                    format!("{prefix}.{key}")
                };
                collect_bundle_input_paths(&child_prefix, child, out);
            }
        }
        _ => out.push(prefix.to_owned()),
    }
}

pub(super) fn insert_bundle_input_value(
    inputs: &mut BTreeMap<String, Value>,
    key: &str,
    value: Value,
) {
    fn insert_nested_segments(
        table: &mut toml::map::Map<String, Value>,
        segments: &[&str],
        value: Value,
    ) {
        if let Some((head, tail)) = segments.split_first() {
            if tail.is_empty() {
                table.insert((*head).to_owned(), value);
                return;
            }
            let entry = table
                .entry((*head).to_owned())
                .or_insert_with(|| Value::Table(toml::map::Map::new()));
            let nested = entry
                .as_table_mut()
                .expect("bundle input path prefixes must be tables");
            insert_nested_segments(nested, tail, value);
        }
    }

    let segments = key.split('.').collect::<Vec<_>>();
    if let Some((head, tail)) = segments.split_first() {
        if tail.is_empty() {
            inputs.insert((*head).to_owned(), value);
            return;
        }
        let entry = inputs
            .entry((*head).to_owned())
            .or_insert_with(|| Value::Table(toml::map::Map::new()));
        let nested = entry
            .as_table_mut()
            .expect("bundle input path prefixes must be tables");
        insert_nested_segments(nested, tail, value);
    }
}

pub(super) fn validate_bundle_input_type(
    manifest_path: &Path,
    bundle_name: &str,
    key: &str,
    expected: BundleInputType,
    value: &Value,
) -> Result<(), ManifestError> {
    let ok = match expected {
        BundleInputType::String => value.is_str(),
        BundleInputType::Integer => value.is_integer(),
        BundleInputType::Bool => value.is_bool(),
        BundleInputType::List => value.is_array(),
    };
    if ok {
        return Ok(());
    }
    Err(ManifestError::Compose {
        path: manifest_path.to_path_buf(),
        detail: format!(
            "bundle `{bundle_name}` input `{key}` must be {}, got {}",
            bundle_input_type_name(expected),
            toml_type_name(value)
        ),
    })
}

fn bundle_input_type_name(value_type: BundleInputType) -> &'static str {
    match value_type {
        BundleInputType::String => "a string",
        BundleInputType::Integer => "an integer",
        BundleInputType::Bool => "a bool",
        BundleInputType::List => "a list",
    }
}

fn toml_type_name(value: &Value) -> &'static str {
    match value {
        Value::String(_) => "string",
        Value::Integer(_) => "integer",
        Value::Float(_) => "float",
        Value::Boolean(_) => "bool",
        Value::Datetime(_) => "datetime",
        Value::Array(_) => "list",
        Value::Table(_) => "table",
    }
}

pub(super) fn merge_missing_values(current: &mut Value, defaults: &Value) {
    if let (Some(current_table), Some(defaults_table)) =
        (current.as_table_mut(), defaults.as_table())
    {
        for (key, default_value) in defaults_table {
            match current_table.get_mut(key) {
                Some(current_value) => merge_missing_values(current_value, default_value),
                None => {
                    current_table.insert(key.clone(), default_value.clone());
                }
            }
        }
    }
}

fn take_bundle_extend_paths(
    manifest_path: &Path,
    defaults: &mut Value,
) -> Result<Vec<String>, ManifestError> {
    let Some(defaults_table) = defaults.as_table_mut() else {
        return Ok(Vec::new());
    };
    let Some(section) = defaults_table.remove("manifest") else {
        return Ok(Vec::new());
    };
    let config: BundleManifestSectionConfig =
        section.try_into().map_err(|error| ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!("invalid bundle `[manifest]` section: {error}"),
        })?;
    Ok(config.extend)
}

pub(super) fn apply_bundle_extend_paths(
    manifest_path: &Path,
    current: &mut Value,
    defaults: &Value,
    extend_paths: &[String],
    existing_values: &BTreeMap<String, bool>,
) -> Result<(), ManifestError> {
    for path in extend_paths {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            return Err(ManifestError::Compose {
                path: manifest_path.to_path_buf(),
                detail: "invalid `[bundle]` section: `extend[]` must not contain empty paths"
                    .to_owned(),
            });
        }
        if !existing_values.get(trimmed).copied().unwrap_or(false) {
            continue;
        }
        apply_bundle_extend_path(manifest_path, current, defaults, trimmed)?;
    }
    Ok(())
}

fn combined_bundle_extend_paths(base: &[String], incoming: &[String]) -> Vec<String> {
    let mut combined = base.to_vec();
    for path in incoming {
        if !combined.contains(path) {
            combined.push(path.clone());
        }
    }
    combined
}

fn apply_bundle_extend_path(
    manifest_path: &Path,
    current: &mut Value,
    defaults: &Value,
    path: &str,
) -> Result<(), ManifestError> {
    let Some(default_value) = lookup_value_at_path(defaults, path) else {
        return Ok(());
    };
    let Some(current_value) = lookup_value_at_path_mut(current, path) else {
        return Ok(());
    };
    let default_array = default_value
        .as_array()
        .ok_or_else(|| ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!(
            "invalid `[bundle]` section: extend path `{path}` requires an array in bundle defaults"
        ),
        })?;
    let current_array = current_value
        .as_array_mut()
        .ok_or_else(|| ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!(
                "invalid `[bundle]` section: extend path `{path}` requires an array in the manifest"
            ),
        })?;
    let mut combined = default_array.clone();
    combined.extend(current_array.iter().cloned());
    *current_array = combined;
    Ok(())
}

pub(super) fn lookup_value_at_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = value;
    for segment in path.split('.') {
        current = current.as_table()?.get(segment)?;
    }
    Some(current)
}

pub(super) fn lookup_value_at_path_mut<'a>(
    value: &'a mut Value,
    path: &str,
) -> Option<&'a mut Value> {
    let mut current = value;
    for segment in path.split('.') {
        current = current.as_table_mut()?.get_mut(segment)?;
    }
    Some(current)
}

fn collect_value_paths(path: &str, value: &Value, out: &mut Vec<String>) {
    if !path.is_empty() {
        out.push(path.to_owned());
    }
    if let Some(table) = value.as_table() {
        for (key, child) in table {
            let child_path = if path.is_empty() {
                key.clone()
            } else {
                format!("{path}.{key}")
            };
            collect_value_paths(&child_path, child, out);
        }
    }
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
