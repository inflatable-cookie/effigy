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
mod hold;
mod prepare;
mod progress;
pub use hold::{hold_selected_sources, SourceHold};
pub use prepare::{
    bundle_update_notes, prepare_bundle_sources, prepare_missing_bundle_sources,
    sync_bundle_source, SourcePreparation, SourceSettlement,
};
pub use progress::{SourceProgress, SourceProgressEvent};

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
    /// One line per source this Prepare selected new bytes for, also printed on
    /// stderr and carried into a JSON envelope's `notes`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    /// A refresh this Prepare could not complete. The pin it kept is still
    /// selected, so the caller carries on (contract 020 §Failure).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
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

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct SourceLock {
    version: u8,
    sources: BTreeMap<String, SourcePin>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourcePin {
    declaration: ManifestBundleBase,
    object: PathBuf,
    version: String,
    #[serde(default)]
    revision: Option<String>,
}
fn state_location(
    manifest: &Path,
) -> Result<effigy_kernel::location::CheckoutLocation, ManifestError> {
    let root = effigy_core::resolver::resolve_target_root(parent(manifest).into(), None)
        .map_err(|err| error(manifest, err.to_string()))?
        .resolved_root;
    // Prepared sources belong to the checkout that owns the manifest, never to
    // a nested catalog's directory (contract 052 §7.1).
    Ok(effigy_kernel::location::CheckoutLocation {
        root: effigy_core::worktree_scope::checkout_root(&root),
    })
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
/// Declarations match when they select the same source. A path source compares
/// by its real directory, so a lock written through one alias of the checkout
/// (a symlinked temp root, say) still matches a read through another.
fn same_declaration(declaration: &ManifestBundleBase, selection: &BundleSelection) -> bool {
    let selected = selection.spec();
    if declaration == &selected {
        return true;
    }
    match (declaration, &selected) {
        (ManifestBundleBase::Path { dir: pinned }, ManifestBundleBase::Path { dir }) => {
            real_dir(Path::new(pinned.trim())) == real_dir(Path::new(dir.trim()))
        }
        _ => false,
    }
}

fn real_dir(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
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
        .map_err(|err| error(manifest, err.to_string()))?;
    let file = FileName::new(LOCK_FILE).map_err(|err| error(manifest, err.to_string()))?;
    match read_record(&cx, &dir, &file) {
        Observed::Verified(lock, _) => Ok(Some(lock)),
        Observed::Absent(_) => Ok(None),
        Observed::Unknown(reason) => Err(error(
            manifest,
            format!("source lock unavailable: {reason}"),
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
        let stale = !same_declaration(&pin.declaration, selection);
        if stale {
            // Reads stay network-free and never select new bytes (contract 020).
            notice(&behind_notice(name, pin));
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
/// Pinned catalog source directories, in resolver precedence order below the
/// project catalog. Local reads only: nothing is acquired, and an unprepared
/// source is an error naming `effigy bundle sync`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CatalogSourceRoots {
    /// The bundle's shipped `catalog/` directory, when the bundle has one.
    pub bundle: Option<PathBuf>,
    /// Declared `[catalog.sources.<name>]` roots, in handle order.
    pub declared: Vec<(String, PathBuf)>,
}

pub fn catalog_source_roots(manifest: &Path) -> Result<CatalogSourceRoots, ManifestError> {
    if !manifest.is_file() {
        return Ok(CatalogSourceRoots::default());
    }
    let (bundle, catalogs) = crate::composition::load_manifest_source_declarations(manifest)?;
    let bundle = match bundle.as_ref().filter(|bundle| bundle.base.is_some()) {
        Some(bundle) => {
            let selection = resolve_bundle_selection(manifest, bundle)?;
            Some(
                resolve_source(manifest, "bundle", &selection)?
                    .local_path
                    .join("catalog"),
            )
            .filter(|root| root.is_dir())
        }
        None => None,
    };
    let declared = catalogs
        .iter()
        .map(|(name, source)| {
            resolve_catalog_source(manifest, name, &source.base)
                .map(|pin| (name.clone(), pin.local_path))
        })
        .collect::<Result<_, _>>()?;
    Ok(CatalogSourceRoots { bundle, declared })
}
fn notice(message: &str) {
    // A single bounded diagnostic per process; nested manifest discovery can
    // resolve the same checkout repeatedly (completion in particular).
    static EMITTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if !EMITTED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        let _ = effigy_core::output_cap::write(
            effigy_core::output_cap::Stream::Stderr,
            format!("[bundle] {message}\n").as_bytes(),
        );
    }
}
/// A revision or content digest, short enough for one diagnostic line.
fn short_version(value: &str) -> String {
    let value = value.strip_prefix("sha256:").unwrap_or(value);
    let short: String = value.chars().take(8).collect();
    if short.is_empty() {
        "unknown".to_owned()
    } else {
        short
    }
}

/// The `<source> behind` line for a pin whose declaration changed. Reads never
/// select new bytes; the next Prepare path acquires the declared source.
fn behind_notice(name: &str, pin: &SourcePin) -> String {
    format!(
        "{name} behind: locked {}, declaration changed; the next `effigy dev` or `effigy container up` will update",
        short_version(pin.revision.as_deref().unwrap_or(&pin.version))
    )
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
    use super::hold::held_generations;
    use super::*;
    use std::collections::BTreeSet;
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
    fn catalog_source_handles_reject_reserved_and_qualified_names() {
        let home = tempfile::tempdir().unwrap();
        let manifest = home.path().join("effigy.toml");
        for name in ["project", "bundle", "baseline", "\"a/b\"", "\"a:b\""] {
            std::fs::write(
                &manifest,
                format!("[catalog.sources.{name}]\ntype = \"path\"\ndir = \"catalog\"\n"),
            )
            .unwrap();
            let error = crate::load_task_manifest(&manifest)
                .unwrap_err()
                .to_string();
            assert!(error.contains("needs a plain handle"), "{name}: {error}");
        }
    }
    #[test]
    fn catalog_source_roots_read_pins_without_acquiring() {
        let home = tempfile::tempdir().unwrap();
        let manifest = home.path().join("effigy.toml");
        assert_eq!(
            catalog_source_roots(&manifest).unwrap(),
            CatalogSourceRoots::default()
        );
        std::fs::create_dir_all(home.path().join("extra/queue")).unwrap();
        std::fs::write(
            &manifest,
            "[catalog.sources.extra]\ntype = \"path\"\ndir = \"extra\"\n",
        )
        .unwrap();
        let roots = catalog_source_roots(&manifest).unwrap();
        assert_eq!(roots.bundle, None);
        assert_eq!(
            roots.declared,
            vec![("extra".to_owned(), home.path().join("extra"))]
        );
        std::fs::write(
            &manifest,
            "[catalog.sources.remote]\ntype = \"git\"\nurl = \"https://invalid.invalid/catalog\"\n",
        )
        .unwrap();
        assert!(catalog_source_roots(&manifest)
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

    fn pin(revision: Option<&str>, version: &str) -> SourcePin {
        SourcePin {
            declaration: ManifestBundleBase::Git {
                url: "https://example.invalid/bundle".into(),
                r#ref: None,
            },
            object: "cache/sources/one".into(),
            version: version.into(),
            revision: revision.map(str::to_owned),
        }
    }

    #[test]
    fn behind_notice_names_a_changed_declaration_by_the_locked_revision() {
        let locked = pin(Some("544c0cc9b30c2a5d0b6e6c5d4f0d1c2b3a4f5e60"), "ignored");
        assert_eq!(
            behind_notice("bundle", &locked),
            "bundle behind: locked 544c0cc9, declaration changed; the next `effigy dev` or `effigy container up` will update"
        );
    }

    /// A checkout whose lock selects `bundle` from `object`.
    fn pinned_checkout(object: &str) -> (tempfile::TempDir, PathBuf) {
        let home = tempfile::tempdir().unwrap();
        let manifest = home.path().join("effigy.toml");
        std::fs::write(
            &manifest,
            "[bundle]\nbase = { type = \"path\", dir = \"bundle\" }\n",
        )
        .unwrap();
        std::fs::create_dir(home.path().join(".effigy")).unwrap();
        std::fs::write(
            home.path().join(".effigy").join(LOCK_FILE),
            serde_json::to_vec(&SourceLock {
                version: 1,
                sources: BTreeMap::from([(
                    "bundle".to_owned(),
                    SourcePin {
                        declaration: ManifestBundleBase::Path {
                            dir: "bundle".into(),
                        },
                        object: object.into(),
                        version: "sha256:selected".into(),
                        revision: None,
                    },
                )]),
            })
            .unwrap(),
        )
        .unwrap();
        (home, manifest)
    }

    /// A lock that cannot be read reports the kernel's own sentence — the
    /// evidence and the next action — instead of an `Unparsable(…)` `Debug`
    /// structure (effigy#402, Q-017).
    #[test]
    fn an_unreadable_source_lock_names_its_cause_in_words() {
        let (home, manifest) = pinned_checkout("cache/sources/generation-b/bundle-object");
        std::fs::write(
            home.path().join(".effigy").join(LOCK_FILE),
            b"{\"version\": 1, \"sources\":",
        )
        .unwrap();
        let error = read_lock(&manifest).expect_err("a corrupt lock is a failure");
        let rendered = error.to_string();
        assert!(
            rendered.contains("source lock unavailable"),
            "the failure names the lock: {rendered}"
        );
        assert!(
            rendered.contains("could not tell: a recorded value could not be parsed"),
            "the cause is the kernel's sentence: {rendered}"
        );
        assert!(
            rendered.contains("run `effigy doctor`"),
            "the next action survives: {rendered}"
        );
        assert!(
            !rendered.contains("Unparsable"),
            "no variant name reaches a person: {rendered}"
        );
    }

    #[test]
    fn a_hold_keeps_its_selected_generation_until_it_is_released() {
        let (home, manifest) = pinned_checkout("cache/sources/generation-a/bundle-object");
        let held = hold_selected_sources(&manifest)
            .expect("hold")
            .expect("a pinned checkout holds its selection");
        assert_eq!(
            held_generations(&manifest),
            BTreeSet::from(["generation-a".to_owned()])
        );
        drop(held);
        assert!(held_generations(&manifest).is_empty());
        drop(home);
    }

    #[test]
    fn a_hold_left_by_a_dead_process_is_removed_and_keeps_nothing() {
        let (home, manifest) = pinned_checkout("cache/sources/generation-a/bundle-object");
        let holds = home.path().join(".effigy/cache/source-holds");
        std::fs::create_dir_all(&holds).unwrap();
        // No process has this pid, so the link no longer holds anything.
        std::os::unix::fs::symlink(
            "../../cache/sources/generation-a/bundle-object",
            holds.join("999999999.0"),
        )
        .unwrap();
        assert!(held_generations(&manifest).is_empty());
        assert!(!holds.join("999999999.0").exists());
    }
}
