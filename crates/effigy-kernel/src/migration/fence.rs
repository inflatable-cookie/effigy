//! Existing source resource locks fence legacy mutations without changing a byte
//! of their diagnostic files. They do not fence an unmanaged pre-gate writer.
use super::*;
use crate::fs::{self, IoError};
use std::fs::File;

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
