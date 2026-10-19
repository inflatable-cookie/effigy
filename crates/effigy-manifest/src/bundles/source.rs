//! Source resolution is local-only. The Prepare owner lives in `prepare`.
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use effigy_core::git_source::{canonical_git_cache_identity, sanitize_cache_segment, sha256_hex};
use effigy_kernel::{
    ctx::{CancelToken, Deadline, EffectClass, Limits, OperationContext, Placement},
    fs::{DirFd, FileName},
    observe::Observed,
    record::{read_record, Record},
};
use serde::{Deserialize, Serialize};

use super::BundleSourceType;
use crate::{ManifestBundleBase, ManifestError};
mod prepare;
pub use prepare::{
    prepare_bundle_sources, sync_bundle_source, SourcePreparation, SourceSettlement,
};

const LOCK_FILE: &str = "sources.lock.json";
const LOCK_MAX: u64 = 1024 * 1024;

#[derive(Debug, Clone)]
pub(super) struct ResolvedBundleSource {
    pub source_type: BundleSourceType,
    pub local_path: PathBuf,
    pub source_path: PathBuf,
    pub version_hint: Option<String>,
    pub stale: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BundleSyncReport {
    pub source_type: BundleSourceType,
    pub source_path: PathBuf,
    pub local_path: Option<PathBuf>,
    pub version_hint: Option<String>,
    pub changed: bool,
    pub applicable: bool,
    pub sources: BTreeMap<String, BundleSourceInspectReport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BundleSourceInspectReport {
    pub source_type: BundleSourceType,
    pub source_path: PathBuf,
    pub local_path: PathBuf,
    pub version_hint: Option<String>,
    pub stale: bool,
}

#[derive(Debug)]
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

impl BundleSelection {
    fn from_spec(manifest: &Path, spec: &ManifestBundleBase) -> Result<Self, ManifestError> {
        match spec {
            ManifestBundleBase::Path { dir } if !dir.trim().is_empty() => {
                let path = Path::new(dir.trim());
                Ok(Self::Path {
                    path: if path.is_absolute() {
                        path.into()
                    } else {
                        parent(manifest).join(path)
                    },
                })
            }
            ManifestBundleBase::Git { url, r#ref } if !url.trim().is_empty() => Ok(Self::Git {
                url: url.trim().into(),
                reference: r#ref
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned),
            }),
            ManifestBundleBase::Oci { url } if !url.trim().is_empty() => Ok(Self::Oci {
                url: url.trim().into(),
            }),
            _ => Err(error(
                manifest,
                "source must declare a non-empty path, git URL or OCI URL",
            )),
        }
    }
    fn spec(&self) -> ManifestBundleBase {
        match self {
            Self::Path { path } => ManifestBundleBase::Path {
                dir: path.to_string_lossy().into(),
            },
            Self::Git { url, reference } => ManifestBundleBase::Git {
                url: url.clone(),
                r#ref: reference.clone(),
            },
            Self::Oci { url } => ManifestBundleBase::Oci { url: url.clone() },
        }
    }
    pub(super) fn kind(&self) -> BundleSourceType {
        match self {
            Self::Path { .. } => BundleSourceType::Path,
            Self::Git { .. } => BundleSourceType::Git,
            Self::Oci { .. } => BundleSourceType::Oci,
        }
    }
    pub(super) fn label(&self) -> PathBuf {
        match self {
            Self::Path { path } => path.clone(),
            Self::Git { url, .. } => url.into(),
            Self::Oci { url } => format!("oci://{url}").into(),
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceLock {
    version: u8,
    sources: BTreeMap<String, SourcePin>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourcePin {
    declaration: ManifestBundleBase,
    object: PathBuf,
    version: String,
    #[serde(default)]
    revision: Option<String>,
}
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(transparent)]
struct KnownVersions(BTreeMap<String, String>);
impl Record for KnownVersions {
    const MAX_BYTES: u64 = LOCK_MAX;
    fn decode(bytes: &[u8]) -> Result<Self, String> {
        let known: Self = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        if known.0.len() > 4096 {
            return Err("source hint inventory exceeds limit".into());
        }
        Ok(known)
    }
}
fn source_key(selection: &BundleSelection) -> Result<String, serde_json::Error> {
    let mut spec = selection.spec();
    if let ManifestBundleBase::Git { url, .. } = &mut spec {
        *url = canonical_git_cache_identity(url);
    }
    serde_json::to_vec(&spec).map(|bytes| sha256_hex(&bytes))
}
fn known_version(selection: &BundleSelection) -> Option<String> {
    if effigy_core::container_detection::process_is_inside_effigy_workspace_container() {
        return None;
    }
    let home = effigy_kernel::root::StateRoot::home_from_env()?;
    let op = observe_context();
    let cx = op.observe().ok()?;
    let Observed::Verified(root, _) = effigy_kernel::root::StateRoot::observe(&cx, &home) else {
        return None;
    };
    let dir = DirFd::open(root.path()).ok()?;
    let Observed::Verified(known, _) =
        read_record::<KnownVersions>(&cx, &dir, &FileName::new("sources-known.json").ok()?)
    else {
        return None;
    };
    known.0.get(&source_key(selection).ok()?).cloned()
}
impl Record for SourceLock {
    const MAX_BYTES: u64 = LOCK_MAX;
    fn decode(bytes: &[u8]) -> Result<Self, String> {
        let lock: Self = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        if lock.version != 1 {
            return Err("unsupported source lock version".into());
        }
        for pin in lock.sources.values() {
            if !pin.object.starts_with("cache/sources")
                || !pin
                    .object
                    .components()
                    .all(|part| matches!(part, Component::Normal(_)))
            {
                return Err("source lock object must stay inside cache/sources".into());
            }
        }
        Ok(lock)
    }
}
fn parent(manifest: &Path) -> &Path {
    manifest.parent().unwrap_or_else(|| Path::new("."))
}
fn error(manifest: &Path, detail: impl Into<String>) -> ManifestError {
    ManifestError::Compose {
        path: manifest.into(),
        detail: detail.into(),
    }
}
fn io_error(path: &Path, error: std::io::Error) -> ManifestError {
    ManifestError::Read {
        path: path.into(),
        error,
    }
}
fn observe_context() -> OperationContext {
    OperationContext::new(
        Deadline::At(Instant::now() + Duration::from_secs(5)),
        CancelToken::default(),
        Limits {
            capture_tail: 1024,
            log_rotate: 4096,
            log_keep: 1,
            line_max: 4096,
            queue_bytes: 4096,
            probe_concurrency: 1,
        },
        EffectClass::Observe,
        Placement::Host,
    )
}
fn read_lock(manifest: &Path) -> Result<Option<SourceLock>, ManifestError> {
    let directory = parent(manifest).join(".effigy");
    if !directory.exists() {
        return Ok(None);
    }
    let dir = DirFd::open(&directory).map_err(|err| error(manifest, err.to_string()))?;
    let op = observe_context();
    let cx = op
        .observe()
        .map_err(|err| error(manifest, format!("{err:?}")))?;
    let file = FileName::new(LOCK_FILE).map_err(|err| error(manifest, err.to_string()))?;
    match read_record(&cx, &dir, &file) {
        Observed::Verified(lock, _) => Ok(Some(lock)),
        Observed::Absent(_) => Ok(None),
        Observed::Unknown(reason) => Err(error(
            manifest,
            format!(
                "source lock unavailable: {reason:?}; restore the lock or run `effigy bundle sync`"
            ),
        )),
    }
}

pub(super) fn resolve_bundle_selection(
    manifest: &Path,
    bundle: &crate::ManifestBundleConfig,
) -> Result<BundleSelection, ManifestError> {
    BundleSelection::from_spec(
        manifest,
        bundle
            .base
            .as_ref()
            .ok_or_else(|| error(manifest, "`[bundle]` must set a typed `base` source"))?,
    )
}
pub(super) fn resolve_materialized_bundle_source(
    manifest: &Path,
    selection: &BundleSelection,
) -> Result<ResolvedBundleSource, ManifestError> {
    resolve_source(manifest, "bundle", selection)
}
fn resolve_source(
    manifest: &Path,
    name: &str,
    selection: &BundleSelection,
) -> Result<ResolvedBundleSource, ManifestError> {
    let lock = read_lock(manifest)?;
    if let Some(pin) = lock.as_ref().and_then(|lock| lock.sources.get(name)) {
        let root = parent(manifest).join(".effigy").join(&pin.object);
        if !root.is_dir() {
            return Err(error(
                manifest,
                format!("pinned source `{name}` is unavailable; run `effigy bundle sync`"),
            ));
        }
        let stale = pin.declaration != selection.spec()
            || known_version(selection).is_some_and(|version| version != pin.version);
        if stale {
            notice("bundle behind: run `effigy bundle sync`");
        }
        let selected = BundleSelection::from_spec(manifest, &pin.declaration)?;
        return Ok(ResolvedBundleSource {
            source_type: selected.kind(),
            source_path: selected.label(),
            local_path: root,
            version_hint: pin.revision.clone().or_else(|| Some(pin.version.clone())),
            stale,
        });
    }
    // Read-only migration view. Prepare imports these exact bytes before any
    // first-run acquisition. Discovery never records a pin or refreshes them.
    if let Some(root) = legacy_root(manifest, selection)
        .filter(|root| root.is_dir() || matches!(selection, BundleSelection::Path { .. }))
    {
        return Ok(ResolvedBundleSource {
            source_type: selection.kind(),
            source_path: selection.label(),
            version_hint: legacy_version(&root, selection),
            local_path: root,
            stale: false,
        });
    }
    Err(error(manifest, format!("source `{name}` is not prepared; run `effigy bundle sync` (or start `effigy dev` / `effigy bootstrap`)")))
}
fn inspect(resolved: ResolvedBundleSource) -> BundleSourceInspectReport {
    BundleSourceInspectReport {
        source_type: resolved.source_type,
        source_path: resolved.source_path,
        local_path: resolved.local_path,
        version_hint: resolved.version_hint,
        stale: resolved.stale,
    }
}
/// Pure preflight for bootstrap/dev, before opening any preparation context.
pub fn bundle_sources_need_preparation(manifest: &Path) -> Result<bool, ManifestError> {
    let (bundle, catalogs) = crate::composition::load_manifest_source_declarations(manifest)?;
    if bundle.is_none() && catalogs.is_empty() {
        return Ok(false);
    }
    let lock = read_lock(manifest)?;
    Ok(lock.is_none_or(|lock| {
        (bundle.is_some() && !lock.sources.contains_key("bundle"))
            || catalogs
                .keys()
                .any(|name| !lock.sources.contains_key(&format!("catalog:{name}")))
    }))
}
pub fn inspect_bundle_source(
    manifest: &Path,
) -> Result<Option<BundleSourceInspectReport>, ManifestError> {
    let (bundle, _) = crate::composition::load_manifest_source_declarations(manifest)?;
    bundle
        .as_ref()
        .map(|bundle| {
            resolve_bundle_selection(manifest, bundle)
                .and_then(|selection| resolve_source(manifest, "bundle", &selection))
                .map(inspect)
        })
        .transpose()
}
/// Resolve a declared catalog source from the same checkout lock as bundles.
/// This does no acquisition and is available to the catalog fragment resolver.
pub fn resolve_catalog_source(
    manifest: &Path,
    name: &str,
    source: &ManifestBundleBase,
) -> Result<BundleSourceInspectReport, ManifestError> {
    let selection = BundleSelection::from_spec(manifest, source)?;
    resolve_source(manifest, &format!("catalog:{name}"), &selection).map(inspect)
}
fn notice(message: &str) {
    // A single bounded diagnostic per process; nested manifest discovery can
    // resolve the same checkout repeatedly (completion and graph in particular).
    static EMITTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if !EMITTED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        eprintln!("[bundle] {message}");
    }
}
fn legacy_root(manifest: &Path, selection: &BundleSelection) -> Option<PathBuf> {
    let cache = parent(manifest).join(".effigy/cache/bundles");
    Some(match selection {
        BundleSelection::Path { path } => path.clone(),
        BundleSelection::Git { url, reference } => cache
            .join("git")
            .join(sha256_hex(canonical_git_cache_identity(url).as_bytes()))
            .join(sanitize_cache_segment(
                reference.as_deref().unwrap_or("main"),
            )),
        BundleSelection::Oci { url } => {
            let locator = oci_bundle_cache_locator(url);
            cache
                .join("oci")
                .join(locator.registry)
                .join(locator.repository_path)
                .join(locator.version_segment)
        }
    })
}
fn read_small(path: &Path, max: u64) -> Option<String> {
    let mut bytes = String::new();
    let file = std::fs::File::open(path).ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    file.take(max + 1).read_to_string(&mut bytes).ok()?;
    (bytes.len() as u64 <= max).then_some(bytes)
}
fn legacy_version(root: &Path, selection: &BundleSelection) -> Option<String> {
    match selection {
        BundleSelection::Git { .. } => {
            let head = read_small(&root.join(".git/HEAD"), 4096)?;
            let head = head.trim();
            if let Some(reference) = head.strip_prefix("ref: ") {
                if !reference.starts_with("refs/")
                    || !Path::new(reference)
                        .components()
                        .all(|part| matches!(part, Component::Normal(_)))
                {
                    return None;
                }
                read_small(&root.join(".git").join(reference), 4096)
                    .map(|value| value.trim().into())
            } else {
                Some(head.into())
            }
        }
        BundleSelection::Oci { .. } => read_small(&root.join(".effigy-bundle-source.digest"), 4096)
            .map(|value| value.trim().into()),
        BundleSelection::Path { .. } => None,
    }
}
struct OciBundleCacheLocator {
    registry: String,
    repository_path: String,
    version_segment: String,
}

fn oci_bundle_cache_locator(reference: &str) -> OciBundleCacheLocator {
    let (without_digest, version_segment) = if let Some((path, digest)) = reference.rsplit_once('@')
    {
        (path, sanitize_cache_segment(digest))
    } else {
        let slash_index = reference.rfind('/').unwrap_or(0);
        let tag_index = reference[slash_index..]
            .rfind(':')
            .map(|index| slash_index + index);
        if let Some(tag_index) = tag_index {
            (
                &reference[..tag_index],
                sanitize_cache_segment(&reference[tag_index + 1..]),
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
        registry: sanitize_cache_segment(&registry),
        repository_path: repository_path
            .split('/')
            .map(sanitize_cache_segment)
            .collect::<Vec<_>>()
            .join("/"),
        version_segment,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_remote_source_is_a_pure_error() {
        let home = tempfile::tempdir().unwrap();
        let manifest = home.path().join("effigy.toml");
        std::fs::write(
            &manifest,
            r#"[bundle]
base = { type = "git", url = "https://invalid.invalid/bundle" }
"#,
        )
        .unwrap();
        let source = BundleSelection::Git {
            url: "https://invalid.invalid/bundle".into(),
            reference: None,
        };
        assert!(resolve_materialized_bundle_source(&manifest, &source)
            .unwrap_err()
            .to_string()
            .contains("effigy bundle sync"));
        assert!(!home.path().join(".effigy").exists());
    }
    #[test]
    fn lock_rejects_escaping_objects_and_unknown_versions() {
        for object in ["../outside", "/outside", "cache/sources/../../outside"] {
            let bytes = serde_json::to_vec(&SourceLock {
                version: 1,
                sources: BTreeMap::from([(
                    "bundle".into(),
                    SourcePin {
                        declaration: ManifestBundleBase::Path {
                            dir: "bundle".into(),
                        },
                        object: object.into(),
                        version: "v1".into(),
                        revision: None,
                    },
                )]),
            })
            .unwrap();
            assert!(SourceLock::decode(&bytes).is_err());
        }
        assert!(SourceLock::decode(br#"{"version":2,"sources":{}}"#).is_err());
    }
}
