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

#[derive(Debug)]
pub(super) struct LegacyFence {
    _files: Vec<File>,
}
impl LegacyFence {
    pub(super) fn acquire(
        cx: &ObserveContext,
        root: &StateRoot,
        _quiescence: &LegacyQuiescence,
    ) -> Result<Self, MigrationError> {
        let mut files = Vec::new();
        if let Some(gates) = root.read_family(Family::Gates).map_err(StoreError::from)? {
            let mut scopes = gates.list(ENTRY_LIMIT).map_err(StoreError::from)?;
            scopes.sort();
            let reserved: Vec<String> = ["migrate:runtime", "migrate:checkout", "migrate:seal"]
                .into_iter()
                .map(|name| {
                    crate::id::ResourceId::store(crate::id::Scope::Machine, name)
                        .expect("fixed migration identity")
                        .name()
                        .to_string()
                })
                .collect();
            for scope in scopes {
                cx.check("legacy source fencing")
                    .map_err(StoreError::Interrupted)?;
                let scope = scope
                    .to_str()
                    .ok_or_else(|| StoreError::Corrupt("non-UTF8 legacy gate scope".into()))?;
                if let Some(checkout_scope) = scope.strip_suffix(".scope") {
                    if checkout_scope == "machine" {
                        continue;
                    }
                    let name = FileName::new(scope).map_err(StoreError::from)?;
                    let (file, _) = gates
                        .open_file(&name, libc::O_RDONLY, None, Trust::Private)
                        .map_err(StoreError::from)?
                        .ok_or_else(|| StoreError::Corrupt("legacy scope disappeared".into()))?;
                    if !fs::try_flock(&file, libc::LOCK_EX).map_err(|error| {
                        StoreError::Io(IoError::new("legacy scope lock", root.path(), &error))
                    })? {
                        drop(files);
                        return Err(MigrationError::Blocked(inspect(cx, root)?));
                    }
                    files.push(file);
                    continue;
                }
                if scope != "machine" {
                    continue;
                }
                let Some(dir) = gates
                    .child(scope, Trust::Private, false)
                    .map_err(StoreError::from)?
                else {
                    continue;
                };
                let mut names = dir.list(ENTRY_LIMIT).map_err(StoreError::from)?;
                names.sort();
                for name in names {
                    cx.check("legacy resource fencing")
                        .map_err(StoreError::Interrupted)?;
                    let name = name
                        .to_str()
                        .ok_or_else(|| StoreError::Corrupt("non-UTF8 legacy gate".into()))?;
                    if scope == "machine"
                        && name
                            .strip_suffix(".gate")
                            .is_some_and(|name| reserved.iter().any(|reserved| reserved == name))
                    {
                        continue;
                    }
                    let name = FileName::new(name).map_err(StoreError::from)?;
                    let (file, _) = dir
                        .open_file(&name, libc::O_RDONLY, None, Trust::Private)
                        .map_err(StoreError::from)?
                        .ok_or_else(|| {
                            StoreError::Corrupt("legacy gate disappeared while fencing".into())
                        })?;
                    let held = fs::try_flock(&file, libc::LOCK_EX).map_err(|error| {
                        StoreError::Io(IoError::new("legacy resource lock", root.path(), &error))
                    })?;
                    if !held {
                        drop(files);
                        return Err(MigrationError::Blocked(inspect(cx, root)?));
                    }
                    files.push(file);
                }
            }
        }
        Ok(Self { _files: files })
    }
}
