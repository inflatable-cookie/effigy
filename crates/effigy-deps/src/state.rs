use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use fs2::FileExt;
use serde::{Deserialize, Serialize};

use crate::{BunRegistrationIndex, DepsError, DesiredDependencyLink};

pub const REPO_LINK_STATE_SCHEMA: &str = "effigy.deps.links.v1";
pub const REPO_LINK_STATE_SCHEMA_VERSION: u32 = 1;
pub const BUN_REGISTRATION_INDEX_SCHEMA: &str = "effigy.deps.bun-registrations.v1";
pub const BUN_REGISTRATION_INDEX_SCHEMA_VERSION: u32 = 1;

const REPO_LINK_STATE_PATH: &str = ".effigy/local/dependency-links.json";
const BUN_REGISTRATION_INDEX_PATH: &str = ".effigy/deps/bun-registrations.json";
const BUN_REGISTRATION_LOCK_FILE: &str = "bun-registrations.lock";
static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepoLinkState {
    pub schema: String,
    pub schema_version: u32,
    pub links: Vec<DesiredDependencyLink>,
}

impl RepoLinkState {
    pub fn empty() -> Self {
        Self {
            schema: REPO_LINK_STATE_SCHEMA.to_owned(),
            schema_version: REPO_LINK_STATE_SCHEMA_VERSION,
            links: Vec::new(),
        }
    }

    pub fn normalize(&mut self) {
        for link in &mut self.links {
            link.normalize();
        }
        self.links.sort_by(|left, right| left.key.cmp(&right.key));
    }
}

impl Default for RepoLinkState {
    fn default() -> Self {
        Self::empty()
    }
}

impl BunRegistrationIndex {
    pub fn empty() -> Self {
        Self {
            schema: BUN_REGISTRATION_INDEX_SCHEMA.to_owned(),
            schema_version: BUN_REGISTRATION_INDEX_SCHEMA_VERSION,
            registrations: Vec::new(),
        }
    }
}

impl Default for BunRegistrationIndex {
    fn default() -> Self {
        Self::empty()
    }
}

#[derive(Debug, Clone)]
pub struct RepoLinkStateStore {
    path: PathBuf,
}

impl RepoLinkStateStore {
    pub fn for_repo(repo_root: impl AsRef<Path>) -> Self {
        Self {
            path: repo_root.as_ref().join(REPO_LINK_STATE_PATH),
        }
    }

    /// The ledger for the checkout that owns `path`.
    ///
    /// Machine-local link state is one file per checkout, so a nested Bun
    /// package root and its checkout resolve to the same store. Use this
    /// wherever a repo path — rather than an exact ledger path — selects the
    /// store, so reads and writes cannot drift apart.
    pub fn for_checkout(path: impl AsRef<Path>) -> Self {
        Self::for_repo(repo_state_root(path.as_ref()))
    }

    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn read(&self) -> Result<RepoLinkState, DepsError> {
        let Some(raw) = read_optional(&self.path)? else {
            return Ok(RepoLinkState::empty());
        };
        let mut state: RepoLinkState = serde_json::from_slice(&raw)
            .map_err(|error| DepsError::json("parse", &self.path, error))?;
        validate_repo_state(&state, &self.path)?;
        state.normalize();
        Ok(state)
    }

    pub fn write(&self, state: &RepoLinkState) -> Result<(), DepsError> {
        let mut state = state.clone();
        state.normalize();
        validate_repo_state(&state, &self.path)?;
        write_json_atomic(&self.path, &state, false)
    }
}

#[derive(Debug, Clone)]
pub struct BunRegistrationIndexStore {
    path: PathBuf,
    lock_path: PathBuf,
}

impl BunRegistrationIndexStore {
    pub fn for_home(home: impl AsRef<Path>) -> Self {
        Self::at(home.as_ref().join(BUN_REGISTRATION_INDEX_PATH))
    }

    pub fn at(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let lock_path = path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(BUN_REGISTRATION_LOCK_FILE);
        Self { path, lock_path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn lock_path(&self) -> &Path {
        &self.lock_path
    }

    pub fn read(&self) -> Result<BunRegistrationIndex, DepsError> {
        read_bun_index(&self.path)
    }

    pub fn update<T>(
        &self,
        update: impl FnOnce(&mut BunRegistrationIndex) -> Result<T, DepsError>,
    ) -> Result<T, DepsError> {
        let _guard = StateLock::acquire(&self.lock_path)?;
        let mut index = self.read()?;
        let result = update(&mut index)?;
        index.normalize();
        validate_bun_index(&index, &self.path)?;
        write_json_atomic(&self.path, &index, true)?;
        Ok(result)
    }

    pub(crate) fn update_exact<T>(
        &self,
        expected_before: Option<&str>,
        update: impl FnOnce(
            &BunRegistrationIndex,
        ) -> Result<(Option<BunRegistrationIndex>, T), DepsError>,
    ) -> Result<T, DepsError> {
        let _guard = StateLock::acquire(&self.lock_path)?;
        let current_raw = read_optional_string(&self.path)?;
        if current_raw.as_deref() != expected_before {
            return Err(DepsError::invalid(
                &self.path,
                "planned Bun registration-index before-state is stale; no manager process was run",
            ));
        }
        let current = read_bun_index(&self.path)?;
        let (next, result) = update(&current)?;
        if let Some(mut next) = next {
            next.normalize();
            validate_bun_index(&next, &self.path)?;
            write_json_atomic(&self.path, &next, true)?;
        }
        Ok(result)
    }

    pub(crate) fn replace_exact(
        &self,
        expected_before: Option<&str>,
        after: Option<&str>,
    ) -> Result<(), DepsError> {
        let _guard = StateLock::acquire(&self.lock_path)?;
        let current = read_optional_string(&self.path)?;
        if current.as_deref() != expected_before {
            return Err(DepsError::invalid(
                &self.path,
                "Bun registration-index rollback refused because the file changed after apply",
            ));
        }
        match after {
            Some(after) => {
                let mut index: BunRegistrationIndex = serde_json::from_str(after)
                    .map_err(|error| DepsError::json("parse rollback", &self.path, error))?;
                index.normalize();
                validate_bun_index(&index, &self.path)?;
                write_json_atomic(&self.path, &index, true)
            }
            None => match fs::remove_file(&self.path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(DepsError::io("remove", &self.path, error)),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IgnoreFileChange {
    None,
    Create,
    Append,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalStateIgnorePlan {
    pub gitignore_path: PathBuf,
    pub covered: bool,
    pub pattern: &'static str,
    pub change: IgnoreFileChange,
}

pub fn plan_repo_local_state_ignore(
    repo_root: impl AsRef<Path>,
) -> Result<LocalStateIgnorePlan, DepsError> {
    let gitignore_path = repo_root.as_ref().join(".gitignore");
    let raw = match fs::read_to_string(&gitignore_path) {
        Ok(raw) => Some(raw),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(DepsError::io("read", &gitignore_path, error)),
    };
    let covered = raw.as_deref().is_some_and(repo_effigy_ignore_rule_present);
    let change = if covered {
        IgnoreFileChange::None
    } else if raw.is_some() {
        IgnoreFileChange::Append
    } else {
        IgnoreFileChange::Create
    };
    Ok(LocalStateIgnorePlan {
        gitignore_path,
        covered,
        pattern: ".effigy/",
        change,
    })
}

/// The checkout that owns machine-local Effigy state for a path.
///
/// Bun links are keyed by package root, which can sit below the checkout —
/// `studio/` in a repo with no root manifest. The ledger, `.gitignore`, and
/// backups still belong to the enclosing checkout, so both a bare invocation
/// and `--repo <nested-root>` resolve to the same state location. A path
/// outside any checkout owns its own state.
pub fn repo_state_root(path: &Path) -> PathBuf {
    path.ancestors()
        .find(|ancestor| ancestor.join(".git").exists())
        .map(|ancestor| fs::canonicalize(ancestor).unwrap_or_else(|_| ancestor.to_path_buf()))
        .unwrap_or_else(|| fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()))
}

/// Whether `path` lies inside `repo_root` without crossing a nested checkout.
///
/// A vendored clone carries its own `.git`; its packages and its link state
/// belong to it. A lexical prefix test would let a parent-level invocation
/// plan Bun processes and `node_modules` changes inside it while writing the
/// ledger to the parent.
pub(crate) fn contained_in_checkout(repo_root: &Path, path: &Path) -> bool {
    path.starts_with(repo_root)
        && path
            .ancestors()
            .take_while(|ancestor| *ancestor != repo_root)
            .all(|ancestor| !ancestor.join(".git").exists())
}

/// Whether two paths are owned by the same checkout.
///
/// A Bun link is keyed by package root, so one checkout can hold several —
/// `studio/` and `harness/` are siblings that share a ledger, and either may
/// be the path a command was pointed at. Resolved checkout identity covers
/// that; an independently nested checkout resolves to itself and is excluded.
///
/// Outside a checkout there is no `.git` to resolve to, so every directory
/// would resolve to itself. Containment without crossing a checkout boundary
/// keeps a non-git project and its nested Bun root together.
pub(crate) fn shares_checkout(left: &Path, right: &Path) -> bool {
    let left = fs::canonicalize(left).unwrap_or_else(|_| left.to_path_buf());
    let right = fs::canonicalize(right).unwrap_or_else(|_| right.to_path_buf());
    repo_state_root(&left) == repo_state_root(&right)
        || contained_in_checkout(&right, &left)
        || contained_in_checkout(&left, &right)
}

pub fn canonical_existing_path(path: impl AsRef<Path>) -> Result<PathBuf, DepsError> {
    fs::canonicalize(path.as_ref())
        .map_err(|error| DepsError::io("canonicalize", path.as_ref(), error))
}

fn repo_effigy_ignore_rule_present(raw: &str) -> bool {
    raw.lines().map(str::trim).any(|line| {
        matches!(
            line,
            ".effigy/"
                | "/.effigy/"
                | ".effigy"
                | "/.effigy"
                | ".effigy/**"
                | "/.effigy/**"
                | "**/.effigy/"
        )
    })
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, DepsError> {
    match fs::read(path) {
        Ok(raw) => Ok(Some(raw)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(DepsError::io("read", path, error)),
    }
}

fn read_optional_string(path: &Path) -> Result<Option<String>, DepsError> {
    match fs::read_to_string(path) {
        Ok(raw) => Ok(Some(raw)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(DepsError::io("read", path, error)),
    }
}

fn read_bun_index(path: &Path) -> Result<BunRegistrationIndex, DepsError> {
    let Some(raw) = read_optional(path)? else {
        return Ok(BunRegistrationIndex::empty());
    };
    let mut index: BunRegistrationIndex =
        serde_json::from_slice(&raw).map_err(|error| DepsError::json("parse", path, error))?;
    validate_bun_index(&index, path)?;
    index.normalize();
    Ok(index)
}

fn validate_repo_state(state: &RepoLinkState, path: &Path) -> Result<(), DepsError> {
    validate_schema(
        path,
        &state.schema,
        state.schema_version,
        REPO_LINK_STATE_SCHEMA,
        REPO_LINK_STATE_SCHEMA_VERSION,
    )?;
    let mut keys = BTreeSet::new();
    for link in &state.links {
        if !keys.insert(&link.key) {
            return Err(DepsError::invalid(
                path,
                format!(
                    "duplicate link key for `{}`",
                    link.key.library_path.display()
                ),
            ));
        }
        if link.mechanism.manager() != link.key.manager {
            return Err(DepsError::invalid(
                path,
                "link mechanism does not match package manager",
            ));
        }
        match (link.mechanism, link.cargo_ownership) {
            (crate::LinkMechanism::CargoPatch, None) => {
                return Err(DepsError::invalid(
                    path,
                    "Cargo dependency link is missing safe-unlink ownership metadata",
                ));
            }
            (crate::LinkMechanism::BunLink, Some(_)) => {
                return Err(DepsError::invalid(
                    path,
                    "Bun dependency link cannot carry Cargo ownership metadata",
                ));
            }
            _ => {}
        }
        if link.mechanism == crate::LinkMechanism::BunLink && !link.cargo_resolutions.is_empty() {
            return Err(DepsError::invalid(
                path,
                "Bun dependency link cannot carry Cargo resolution metadata",
            ));
        }
        require_absolute(path, "consumer repo", &link.key.consumer_repo)?;
        require_absolute(path, "library", &link.key.library_path)?;
        let mut package_names = BTreeSet::new();
        for package in &link.packages {
            if !package_names.insert(&package.name) {
                return Err(DepsError::invalid(
                    path,
                    format!(
                        "duplicate package `{}` in one dependency link",
                        package.name
                    ),
                ));
            }
            require_absolute(path, "package", &package.local_path)?;
        }
        for root in &link.consumer_roots {
            require_absolute(path, "consumer root", &root.canonical_path)?;
        }
        for resolution in &link.cargo_resolutions {
            require_absolute(path, "Cargo consumer root", &resolution.consumer_root)?;
            require_absolute(path, "Cargo package", &resolution.local_path)?;
            if resolution.committed_source.kind != crate::CommittedSourceKind::Git {
                return Err(DepsError::invalid(
                    path,
                    "Cargo resolution metadata must name a committed Git source",
                ));
            }
        }
    }
    Ok(())
}

fn validate_bun_index(index: &BunRegistrationIndex, path: &Path) -> Result<(), DepsError> {
    validate_schema(
        path,
        &index.schema,
        index.schema_version,
        BUN_REGISTRATION_INDEX_SCHEMA,
        BUN_REGISTRATION_INDEX_SCHEMA_VERSION,
    )?;
    let mut package_names = BTreeSet::new();
    for registration in &index.registrations {
        if !package_names.insert(&registration.package_name) {
            return Err(DepsError::invalid(
                path,
                format!("duplicate Bun registration `{}`", registration.package_name),
            ));
        }
        if registration.consumers.is_empty() {
            return Err(DepsError::invalid(
                path,
                format!(
                    "Bun registration `{}` has no desired consumers",
                    registration.package_name
                ),
            ));
        }
        require_absolute(path, "Bun package", &registration.package_path)?;
        for consumer in &registration.consumers {
            require_absolute(path, "consumer repo", &consumer.consumer_repo)?;
            require_absolute(path, "library", &consumer.library_path)?;
        }
    }
    Ok(())
}

fn validate_schema(
    path: &Path,
    actual_schema: &str,
    actual_version: u32,
    expected_schema: &'static str,
    expected_version: u32,
) -> Result<(), DepsError> {
    if actual_schema == expected_schema && actual_version == expected_version {
        return Ok(());
    }
    Err(DepsError::UnsupportedSchema {
        path: path.to_path_buf(),
        expected_schema,
        expected_version,
        actual_schema: actual_schema.to_owned(),
        actual_version,
    })
}

fn require_absolute(path: &Path, kind: &str, value: &Path) -> Result<(), DepsError> {
    if value.is_absolute() {
        Ok(())
    } else {
        Err(DepsError::invalid(
            path,
            format!(
                "{kind} path `{}` is not canonical/absolute",
                value.display()
            ),
        ))
    }
}

fn write_json_atomic<T: Serialize>(
    path: &Path,
    value: &T,
    owner_only: bool,
) -> Result<(), DepsError> {
    let mut rendered =
        serde_json::to_vec_pretty(value).map_err(|error| DepsError::json("render", path, error))?;
    rendered.push(b'\n');
    write_atomic(path, &rendered, owner_only)
}

pub(crate) fn write_atomic(
    path: &Path,
    contents: &[u8],
    owner_only: bool,
) -> Result<(), DepsError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| DepsError::io("create directory", parent, error))?;
    let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("state");
    let temp_path = parent.join(format!(".{file_name}.tmp-{}-{counter}", std::process::id()));
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    set_owner_only_mode(&mut options, owner_only);
    let mut file = options
        .open(&temp_path)
        .map_err(|error| DepsError::io("create temporary state", &temp_path, error))?;
    let result = (|| {
        file.write_all(contents)
            .map_err(|error| DepsError::io("write temporary state", &temp_path, error))?;
        file.sync_all()
            .map_err(|error| DepsError::io("sync temporary state", &temp_path, error))?;
        fs::rename(&temp_path, path).map_err(|error| DepsError::io("replace", path, error))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

#[cfg(unix)]
fn set_owner_only_mode(options: &mut OpenOptions, owner_only: bool) {
    use std::os::unix::fs::OpenOptionsExt;

    if owner_only {
        options.mode(0o600);
    }
}

#[cfg(not(unix))]
fn set_owner_only_mode(_options: &mut OpenOptions, _owner_only: bool) {}

#[derive(Debug, Serialize, Deserialize)]
struct LockRecord {
    pid: u32,
    created_unix_ms: u64,
}

struct StateLock {
    file: File,
}

impl StateLock {
    fn acquire(path: &Path) -> Result<Self, DepsError> {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)
            .map_err(|error| DepsError::io("create lock directory", parent, error))?;
        let mut options = OpenOptions::new();
        options.create(true).read(true).write(true);
        set_owner_only_mode(&mut options, true);
        let mut file = options
            .open(path)
            .map_err(|error| DepsError::io("open lock", path, error))?;
        file.try_lock_exclusive().map_err(|error| {
            if error.kind() == io::ErrorKind::WouldBlock {
                DepsError::LockHeld {
                    path: path.to_path_buf(),
                    owner_pid: lock_owner_pid(path),
                }
            } else {
                DepsError::io("acquire lock", path, error)
            }
        })?;
        let record = LockRecord {
            pid: std::process::id(),
            created_unix_ms: now_unix_ms()?,
        };
        let body = serde_json::to_vec(&record)
            .map_err(|error| DepsError::json("render lock", path, error))?;
        file.set_len(0)
            .map_err(|error| DepsError::io("truncate lock", path, error))?;
        file.write_all(&body)
            .map_err(|error| DepsError::io("write lock", path, error))?;
        file.sync_all()
            .map_err(|error| DepsError::io("sync lock", path, error))?;
        Ok(Self { file })
    }
}

impl Drop for StateLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

fn lock_owner_pid(path: &Path) -> Option<u32> {
    fs::read(path)
        .ok()
        .and_then(|raw| serde_json::from_slice::<LockRecord>(&raw).ok())
        .map(|record| record.pid)
}

fn now_unix_ms() -> Result<u64, DepsError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| DepsError::Clock {
            operation: "timestamp dependency state",
        })?;
    Ok(duration.as_millis().min(u128::from(u64::MAX)) as u64)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::mpsc;
    use std::thread;

    use tempfile::tempdir;

    use super::{
        now_unix_ms, plan_repo_local_state_ignore, BunRegistrationIndexStore, IgnoreFileChange,
        LockRecord, RepoLinkState, RepoLinkStateStore, BUN_REGISTRATION_INDEX_SCHEMA,
        BUN_REGISTRATION_INDEX_SCHEMA_VERSION, REPO_LINK_STATE_SCHEMA,
        REPO_LINK_STATE_SCHEMA_VERSION,
    };
    use crate::{
        canonical_existing_path, BunConsumerReference, BunReferenceRelease, CargoLinkOwnership,
        CommittedSource, CommittedSourceKind, ConsumerRoot, DependencyLinkKey, DependencyPackage,
        DesiredDependencyLink, LinkMechanism, PackageManager,
    };

    fn absolute(path: &Path) -> PathBuf {
        assert!(path.is_absolute());
        path.to_path_buf()
    }

    fn consumer(repo: &Path, library: &Path) -> BunConsumerReference {
        BunConsumerReference {
            consumer_repo: absolute(repo),
            library_path: absolute(library),
        }
    }

    fn desired_link(
        manager: PackageManager,
        repo: &Path,
        library: &Path,
        package_name: &str,
    ) -> DesiredDependencyLink {
        let mechanism = match manager {
            PackageManager::Cargo => LinkMechanism::CargoPatch,
            PackageManager::Bun => LinkMechanism::BunLink,
        };
        DesiredDependencyLink {
            key: DependencyLinkKey {
                manager,
                consumer_repo: absolute(repo),
                library_path: absolute(library),
            },
            mechanism,
            consumer_roots: vec![ConsumerRoot {
                canonical_path: absolute(repo),
            }],
            packages: vec![DependencyPackage {
                name: package_name.to_owned(),
                local_path: absolute(library),
                committed_sources: vec![CommittedSource {
                    kind: CommittedSourceKind::Git,
                    identity: "https://example.invalid/library".to_owned(),
                }],
            }],
            cargo_resolutions: Vec::new(),
            cargo_ownership: (manager == PackageManager::Cargo).then_some(CargoLinkOwnership {
                config_created_by_effigy: true,
                cargo_dir_created_by_effigy: true,
            }),
        }
    }

    #[test]
    fn missing_repo_state_reads_as_empty() {
        let temp = tempdir().expect("tempdir");
        let store = RepoLinkStateStore::for_repo(temp.path());
        let state = store.read().expect("read missing state");
        assert_eq!(state, RepoLinkState::empty());
        assert!(!store.path().exists());
    }

    #[test]
    fn repo_state_roundtrips_multiple_links_deterministically() {
        let temp = tempdir().expect("tempdir");
        let repo = canonical_existing_path(temp.path()).expect("canonical repo");
        let cargo_library = repo.join("cargo-library");
        let bun_library = repo.join("bun-library");
        fs::create_dir_all(&cargo_library).expect("cargo library");
        fs::create_dir_all(&bun_library).expect("bun library");
        let store = RepoLinkStateStore::for_repo(&repo);
        let state = RepoLinkState {
            schema: REPO_LINK_STATE_SCHEMA.to_owned(),
            schema_version: REPO_LINK_STATE_SCHEMA_VERSION,
            links: vec![
                desired_link(PackageManager::Bun, &repo, &bun_library, "@scope/ui"),
                desired_link(PackageManager::Cargo, &repo, &cargo_library, "signal"),
            ],
        };

        store.write(&state).expect("write state");
        let first = fs::read(store.path()).expect("first encoding");
        let roundtrip = store.read().expect("roundtrip");
        store.write(&roundtrip).expect("rewrite state");
        let second = fs::read(store.path()).expect("second encoding");

        assert_eq!(first, second);
        assert_eq!(roundtrip.links.len(), 2);
        assert_eq!(roundtrip.links[0].key.manager, PackageManager::Cargo);
    }

    #[test]
    fn malformed_and_future_repo_state_are_not_overwritten() {
        let temp = tempdir().expect("tempdir");
        let store = RepoLinkStateStore::for_repo(temp.path());
        fs::create_dir_all(store.path().parent().expect("state parent")).expect("state parent");
        fs::write(store.path(), b"{not-json\n").expect("malformed state");
        assert!(store.read().is_err());
        assert_eq!(
            fs::read(store.path()).expect("malformed unchanged"),
            b"{not-json\n"
        );

        let future = format!(
            "{{\"schema\":\"{REPO_LINK_STATE_SCHEMA}\",\"schema_version\":{},\"links\":[]}}\n",
            REPO_LINK_STATE_SCHEMA_VERSION + 1
        );
        fs::write(store.path(), future.as_bytes()).expect("future state");
        assert!(store.read().is_err());
        assert_eq!(
            fs::read_to_string(store.path()).expect("future unchanged"),
            future
        );
    }

    #[test]
    fn malformed_and_future_bun_state_are_not_overwritten() {
        let temp = tempdir().expect("tempdir");
        let store = BunRegistrationIndexStore::for_home(temp.path());
        fs::create_dir_all(store.path().parent().expect("state parent")).expect("state parent");
        fs::write(store.path(), b"{not-json\n").expect("malformed state");
        assert!(store.read().is_err());
        assert_eq!(
            fs::read(store.path()).expect("malformed unchanged"),
            b"{not-json\n"
        );

        let future = format!(
            "{{\"schema\":\"{BUN_REGISTRATION_INDEX_SCHEMA}\",\"schema_version\":{},\"registrations\":[]}}\n",
            BUN_REGISTRATION_INDEX_SCHEMA_VERSION + 1
        );
        fs::write(store.path(), future.as_bytes()).expect("future state");
        assert!(store.read().is_err());
        assert_eq!(
            fs::read_to_string(store.path()).expect("future unchanged"),
            future
        );
    }

    #[test]
    fn ignore_plan_reports_create_append_and_covered_without_writing() {
        let temp = tempdir().expect("tempdir");
        let plan = plan_repo_local_state_ignore(temp.path()).expect("missing ignore plan");
        assert_eq!(plan.change, IgnoreFileChange::Create);
        assert!(!plan.gitignore_path.exists());

        fs::write(&plan.gitignore_path, "target/\n").expect("gitignore");
        let plan = plan_repo_local_state_ignore(temp.path()).expect("append ignore plan");
        assert_eq!(plan.change, IgnoreFileChange::Append);
        assert_eq!(
            fs::read_to_string(&plan.gitignore_path).expect("unchanged"),
            "target/\n"
        );

        fs::write(&plan.gitignore_path, "target/\n/.effigy/\n").expect("covered gitignore");
        let plan = plan_repo_local_state_ignore(temp.path()).expect("covered ignore plan");
        assert!(plan.covered);
        assert_eq!(plan.change, IgnoreFileChange::None);
    }

    #[test]
    fn canonical_existing_path_resolves_relative_components() {
        let temp = tempdir().expect("tempdir");
        let nested = temp.path().join("nested");
        fs::create_dir(&nested).expect("nested");
        let input = nested.join("..").join("nested");
        assert_eq!(
            canonical_existing_path(&input).expect("canonical"),
            fs::canonicalize(nested).expect("canonical nested")
        );
    }

    #[test]
    fn foreign_registration_is_not_claimed_and_conflicts_are_refused() {
        let temp = tempdir().expect("tempdir");
        let repo = absolute(temp.path());
        let library = repo.join("library");
        let other = repo.join("other");
        let store = BunRegistrationIndexStore::for_home(&repo);
        let reference = consumer(&repo, &library);

        store
            .update(|index| {
                index.add_reference("@scope/ui", library.clone(), false, reference.clone())?;
                index.add_reference("@scope/ui", library.clone(), true, reference.clone())
            })
            .expect("record foreign registration");
        let index = store.read().expect("read index");
        assert!(!index.registrations[0].effigy_created);

        let error = store
            .update(|index| {
                index.add_reference("@scope/ui", other.clone(), true, reference.clone())
            })
            .expect_err("conflict");
        assert!(error.to_string().contains("already points"));
        assert_eq!(store.read().expect("unchanged index"), index);
    }

    #[test]
    fn stale_references_roundtrip_and_release_preserves_ownership() {
        let temp = tempdir().expect("tempdir");
        let home = absolute(temp.path());
        let missing_repo = home.join("missing-repo");
        let missing_library = home.join("missing-library");
        let shared_repo = home.join("shared-repo");
        let reference = consumer(&missing_repo, &missing_library);
        let shared = consumer(&shared_repo, &missing_library);
        let store = BunRegistrationIndexStore::for_home(&home);

        store
            .update(|index| {
                index.add_reference(
                    "@scope/ui",
                    missing_library.clone(),
                    true,
                    reference.clone(),
                )?;
                index.add_reference("@scope/ui", missing_library.clone(), true, shared.clone())
            })
            .expect("write stale references");
        let index = store.read().expect("read stale references");
        assert_eq!(index.registrations[0].consumers.len(), 2);

        let first = store
            .update(|index| Ok(index.release_reference("@scope/ui", &reference)))
            .expect("release first");
        assert_eq!(first, BunReferenceRelease::RetainedShared);
        let second = store
            .update(|index| Ok(index.release_reference("@scope/ui", &shared)))
            .expect("release second");
        assert_eq!(second, BunReferenceRelease::RemoveOwned);
        assert!(store.read().expect("empty index").registrations.is_empty());
    }

    #[test]
    fn stale_lock_is_recovered_and_live_lock_fails_without_corruption() {
        let temp = tempdir().expect("tempdir");
        let home = absolute(temp.path());
        let library = home.join("library");
        let reference = consumer(&home, &library);
        let store = BunRegistrationIndexStore::for_home(&home);
        fs::create_dir_all(store.lock_path().parent().expect("lock parent")).expect("lock parent");
        let stale = LockRecord {
            pid: 999_999,
            created_unix_ms: now_unix_ms().expect("now").saturating_sub(1_000),
        };
        fs::write(
            store.lock_path(),
            serde_json::to_vec(&stale).expect("stale lock"),
        )
        .expect("write stale lock");
        store
            .update(|index| {
                index.add_reference("@scope/ui", library.clone(), true, reference.clone())
            })
            .expect("recover stale lock");

        let (locked_tx, locked_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let thread_store = store.clone();
        let thread_reference = reference.clone();
        let thread_library = library.clone();
        let handle = thread::spawn(move || {
            thread_store.update(|index| {
                locked_tx.send(()).expect("signal lock");
                release_rx.recv().expect("release lock");
                index.add_reference("@scope/other", thread_library, true, thread_reference)
            })
        });
        locked_rx.recv().expect("lock acquired");
        let error = store.update(|_| Ok(())).expect_err("live lock must fail");
        assert!(error.to_string().contains("is held"));
        release_tx.send(()).expect("release worker");
        handle.join().expect("worker join").expect("worker update");
        let index = store.read().expect("valid index");
        assert_eq!(index.registrations.len(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn machine_state_uses_owner_only_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempdir().expect("tempdir");
        let home = absolute(temp.path());
        let library = home.join("library");
        let reference = consumer(&home, &library);
        let store = BunRegistrationIndexStore::for_home(&home);
        store
            .update(|index| index.add_reference("@scope/ui", library, true, reference))
            .expect("write index");
        let mode = fs::metadata(store.path())
            .expect("index metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
        let lock_mode = fs::metadata(store.lock_path())
            .expect("lock metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(lock_mode, 0o600);
    }

    #[test]
    fn schema_constants_match_empty_state() {
        let repo = RepoLinkState::empty();
        assert_eq!(repo.schema, REPO_LINK_STATE_SCHEMA);
        assert_eq!(repo.schema_version, REPO_LINK_STATE_SCHEMA_VERSION);
        let bun = crate::BunRegistrationIndex::empty();
        assert_eq!(bun.schema, BUN_REGISTRATION_INDEX_SCHEMA);
        assert_eq!(bun.schema_version, BUN_REGISTRATION_INDEX_SCHEMA_VERSION);
    }
}
