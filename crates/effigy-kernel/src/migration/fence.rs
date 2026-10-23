//! Existing source resource locks fence legacy mutations without changing a byte
//! of their diagnostic files. They do not fence an unmanaged pre-gate writer.
use super::*;
use crate::fs::{self, IoError};
use std::fs::File;

#[derive(Debug, Clone, Serialize)]
pub struct DestinationBlocker {
    pub checkout_root: std::path::PathBuf,
    pub writers: Vec<HolderDiag>,
    pub reason: String,
}

fn directory_scope(root: &StateRoot) -> Result<FileName, StoreError> {
    let identity = root.directory_gate_identity()?;
    Ok(FileName::new(&format!(
        "{}.scope",
        gate::scope_name(crate::id::Scope::Checkout(identity))
    ))?)
}

/// Pure inventory of the directory-wide exclusion used by every checkout
/// run. A successful probe grants no import authority; import fences again.
pub fn destination_blocker(
    cx: &ObserveContext,
    location: &CheckoutLocation,
) -> Result<Option<DestinationBlocker>, StoreError> {
    let root = match StateRoot::observe(cx, &location.root) {
        Observed::Verified(root, _) => root,
        Observed::Absent(_) => return Ok(None),
        other => {
            return Err(StoreError::Corrupt(format!(
                "destination observation: {other:?}"
            )))
        }
    };
    let Some(gates) = root.read_family(Family::Gates)? else {
        return Ok(None);
    };
    let Some((file, _)) = gates.open_file(
        &directory_scope(&root)?,
        libc::O_RDONLY,
        None,
        Trust::Private,
    )?
    else {
        return Ok(None);
    };
    cx.check("migration destination probe")
        .map_err(StoreError::Interrupted)?;
    root.verify_binding()?;
    if fs::try_flock(&file, libc::LOCK_EX).map_err(|error| {
        StoreError::Io(IoError::new(
            "migration destination probe",
            root.path(),
            &error,
        ))
    })? {
        fs::unlock(&file);
        return Ok(None);
    }
    let mut report = DestinationBlocker {
        checkout_root: location.root.clone(),
        writers: Vec::new(),
        reason: "checkout custody holds the directory scope".into(),
    };
    match StateRoot::checkout_observe(cx, location) {
        Observed::Verified(typed, _) => match gate::Gate::foreground_holders(cx, &typed) {
            Observed::Verified(rows, _) => {
                for row in rows {
                    if !report.writers.iter().any(|old| {
                        old.run == row.holder.run
                            && old.pid == row.holder.pid
                            && old.start == row.holder.start
                            && old.boot == row.holder.boot
                    }) {
                        report.writers.push(row.holder);
                    }
                }
            }
            other => {
                report.reason =
                    format!("held checkout scope; holder diagnostics unavailable: {other:?}")
            }
        },
        other => {
            report.reason =
                format!("held checkout scope; checkout authority unavailable: {other:?}")
        }
    }
    root.verify_binding()?;
    Ok(Some(report))
}

#[derive(Debug)]
pub(super) struct DestinationFence {
    _file: File,
}
impl DestinationFence {
    /// Holds the root-directory alias exclusively, including across a Git
    /// metadata generation change. Only transient lock files precede this.
    pub(super) fn acquire(
        cx: &crate::ctx::EffectContext,
        root: &StateRoot,
        location: &CheckoutLocation,
    ) -> Result<Self, MigrationError> {
        cx.operation()
            .check("migration destination fence")
            .map_err(StoreError::Interrupted)?;
        root.verify_binding()?;
        let gates = root
            .family(Family::Gates)?
            .ok_or_else(|| StoreError::Corrupt("destination gate family absent".into()))?;
        let (file, _) = gates
            .open_file(
                &directory_scope(root)?,
                libc::O_RDONLY,
                Some(fs::FILE_MODE),
                Trust::Private,
            )?
            .ok_or_else(|| StoreError::Corrupt("destination scope disappeared".into()))?;
        if !fs::try_flock(&file, libc::LOCK_EX).map_err(|error| {
            StoreError::Io(IoError::new(
                "migration destination fence",
                root.path(),
                &error,
            ))
        })? {
            let observe = cx
                .operation()
                .observe()
                .map_err(|_| StoreError::Unbounded)?;
            let report =
                destination_blocker(&observe, location)?.unwrap_or_else(|| DestinationBlocker {
                    checkout_root: location.root.clone(),
                    writers: Vec::new(),
                    reason: "checkout scope changed during fencing; retry pure planning".into(),
                });
            return Err(MigrationError::DestinationBlocked(report));
        }
        root.verify_binding()?;
        Ok(Self { _file: file })
    }
}

/// The caller has withdrawn legacy launchers and unsupported pre-fence writers.
/// Kernel locks cannot prove that external operational condition. Construct this
/// only after a supported drain/quiesce protocol or explicit operator assertion.
#[derive(Debug)]
pub struct LegacyQuiescence(());
impl LegacyQuiescence {
    pub fn launchers_withdrawn() -> Self {
        Self(())
    }
}

// One legacy machine scope fences every Store writer before journal go. The
// historical per-resource inode count must not consume the descriptor budget.
// Only importer-owned metadata gates may borrow this exclusive description;
// application gates still acquire their ordinary shared scope.
use std::cell::RefCell;
use std::os::unix::fs::MetadataExt;
use std::sync::{Arc, Weak};

thread_local! {
    static FROZEN_SOURCES: RefCell<Vec<Weak<FrozenSource>>> = const { RefCell::new(Vec::new()) };
}
#[derive(Debug)]
struct FrozenSource {
    root: StateRoot,
    scope: Option<File>,
    reserved: Vec<String>,
}
impl FrozenSource {
    fn verify(&self) -> Result<(), crate::fs::FsError> {
        self.root.verify_binding()?;
        if let Some(scope) = &self.scope {
            let gates = self.root.read_family(Family::Gates)?.ok_or_else(|| {
                crate::fs::FsError::untrusted(self.root.path(), "legacy gates disappeared")
            })?;
            let current = gates
                .open_file(
                    &FileName::new("machine.scope")?,
                    libc::O_RDONLY,
                    None,
                    Trust::Private,
                )?
                .ok_or_else(|| {
                    crate::fs::FsError::untrusted(
                        self.root.path(),
                        "legacy writer scope disappeared",
                    )
                })?
                .0;
            if !same_file(&current, scope, self.root.path())? {
                return Err(crate::fs::FsError::untrusted(
                    self.root.path(),
                    "legacy writer scope changed",
                ));
            }
        }
        Ok(())
    }
}
fn same_file(one: &File, two: &File, path: &std::path::Path) -> Result<bool, crate::fs::FsError> {
    let metadata = |file: &File| {
        file.metadata()
            .map_err(|error| crate::fs::FsError::io("legacy scope identity", path, &error))
    };
    let one = metadata(one)?;
    let two = metadata(two)?;
    Ok((one.dev(), one.ino()) == (two.dev(), two.ino()))
}
fn frozen_sources() -> Vec<Arc<FrozenSource>> {
    FROZEN_SOURCES.with(|entries| {
        let mut entries = entries.borrow_mut();
        entries.retain(|entry| entry.strong_count() != 0);
        entries.iter().filter_map(Weak::upgrade).collect()
    })
}
pub(crate) fn source_scope_owned(file: &File) -> Result<bool, crate::fs::FsError> {
    for source in frozen_sources() {
        if let Some(scope) = &source.scope {
            if same_file(file, scope, source.root.path())? {
                source.verify()?;
                return Ok(true);
            }
        }
    }
    Ok(false)
}
pub(crate) fn borrow_source_scope(
    file: &File,
    ids: &[ResourceId],
) -> Result<Option<File>, crate::fs::FsError> {
    for source in frozen_sources() {
        if !ids.iter().all(|id| {
            id.scope() == crate::id::Scope::Machine
                && id.kind() == crate::id::ResourceKind::Store
                && source.reserved.contains(&id.name().to_string())
        }) {
            continue;
        }
        if let Some(scope) = &source.scope {
            if same_file(file, scope, source.root.path())? {
                source.verify()?;
                return scope.try_clone().map(Some).map_err(|error| {
                    crate::fs::FsError::io("borrow migration scope", source.root.path(), &error)
                });
            }
        }
    }
    Ok(None)
}
pub(crate) fn own_import_holder(
    root: &StateRoot,
    name: &str,
    holder: &Observed<HolderDiag>,
) -> bool {
    let Observed::Verified(holder, _) = holder else {
        return false;
    };
    let Observed::Verified(current, _) = ident::process_identity(std::process::id()) else {
        return false;
    };
    if holder.pid != current.pid
        || holder.start != Some(current.start.text())
        || holder.boot.as_deref() != Some(current.boot.as_str())
    {
        return false;
    }
    frozen_sources().iter().any(|source| {
        source.reserved.iter().any(|reserved| reserved == name)
            && source.verify().is_ok()
            && matches!((source.root.dir().identity(), root.dir().identity()),
                (Ok(expected), Ok(current)) if expected == current)
    })
}

#[derive(Debug)]
pub(super) struct LegacyFence {
    source: Arc<FrozenSource>,
}
impl LegacyFence {
    pub(super) fn acquire(
        cx: &ObserveContext,
        root: &StateRoot,
        _quiescence: &LegacyQuiescence,
    ) -> Result<Self, MigrationError> {
        cx.check("legacy writer fencing")
            .map_err(StoreError::Interrupted)?;
        let scope = match root.read_family(Family::Gates)? {
            Some(gates) => gates
                .open_file(
                    &FileName::new("machine.scope")?,
                    libc::O_RDONLY,
                    None,
                    Trust::Private,
                )?
                .map(|(file, _)| file),
            None => None,
        };
        if let Some(file) = &scope {
            if !fs::try_flock(file, libc::LOCK_EX).map_err(|error| {
                StoreError::Io(IoError::new(
                    "legacy writer scope lock",
                    root.path(),
                    &error,
                ))
            })? {
                let mut report = inspect(cx, root)?;
                if report.ready() {
                    let reason = UnknownReason::Inconsistent(
                        "legacy machine writer scope is held; drain legacy launchers and retry"
                            .into(),
                    );
                    report.blockers.push(Blocker {
                        scope: "machine".into(),
                        resource: "machine.scope".into(),
                        run: None,
                        role: "writer scope".into(),
                        process: Observed::Unknown(reason.clone()),
                        state: Observed::Unknown(reason),
                    });
                }
                return Err(MigrationError::Blocked(report));
            }
        }
        let source = Arc::new(FrozenSource {
            root: root.clone(),
            scope,
            reserved: ["migrate:runtime", "migrate:checkout", "migrate:seal"]
                .into_iter()
                .map(|name| {
                    ResourceId::store(crate::id::Scope::Machine, name)
                        .expect("fixed migration identity")
                        .name()
                        .to_string()
                })
                .collect(),
        });
        FROZEN_SOURCES.with(|entries| {
            let mut entries = entries.borrow_mut();
            entries.retain(|entry| entry.strong_count() != 0);
            if entries.len() >= 16 {
                return Err(StoreError::Corrupt(
                    "nested source freezes exceed 16".into(),
                ));
            }
            entries.push(Arc::downgrade(&source));
            Ok(())
        })?;
        let guard = Self { source };
        // Catch an owner admitted between preflight and the writer freeze.
        // Old pre-fence launchers remain covered by explicit withdrawal.
        let report = inspect(cx, root)?;
        if !report.ready() {
            return Err(MigrationError::Blocked(report));
        }
        guard.verify()?;
        Ok(guard)
    }
    pub(super) fn verify(&self) -> Result<(), StoreError> {
        self.source.verify().map_err(StoreError::from)
    }
}
