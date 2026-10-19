//! Journalled supervisor socket custody. The Run owns every row mutation.
use super::*;
use crate::fs::{DirFd, FileName, Trust};
use crate::id::{ResourceId, ResourceKind, RunId};
use crate::root::{Family, StateRoot};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;

pub(super) const KIND: &str = "kernel:control-socket";
#[derive(Debug, Serialize, Deserialize)]
struct Receipt {
    device: u64,
    inode: u64,
}
impl Run<'_> {
    pub(crate) fn control_socket(&mut self) -> Result<UnixListener, RunError> {
        let ticket = self.intend_inner(EffectSpec {
            target: self.owner.clone(),
            kind: KIND.into(),
            payload: Value::Null,
            settlement: Settlement::Release(Value::Null),
            required: false,
        })?;
        let result = (|| {
            crate::fault_point!(SocketBind)?;
            let dir = directory(self.root, &self.owner.resource)
                .map_err(StoreError::from)?
                .ok_or_else(|| RunError::Invalid("socket directory missing".into()))?;
            let name = FileName::new(&format!("{}.sock", self.id)).map_err(StoreError::from)?;
            if dir.entry(&name).map_err(StoreError::from)?.is_some() {
                return Err(RunError::Invalid("socket path already exists".into()));
            }
            let listener = UnixListener::bind(dir.path().join(name.as_str()))?;
            let stat = dir
                .entry(&name)
                .map_err(StoreError::from)?
                .ok_or_else(|| RunError::Invalid("socket vanished".into()))?;
            let cname = std::ffi::CString::new(name.as_str())
                .map_err(|_| RunError::Invalid("socket name".into()))?;
            // SAFETY: live verified private directory and a single NUL-terminated
            // component. NOFOLLOW refuses a substituted symlink; no global umask.
            if unsafe {
                libc::fchmodat(dir.raw(), cname.as_ptr(), 0o600, libc::AT_SYMLINK_NOFOLLOW)
            } != 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
            listener.set_nonblocking(true)?;
            self.applied(
                &ticket,
                serde_json::to_value(Receipt {
                    device: device(&stat),
                    inode: stat.st_ino,
                })?,
            )?;
            Ok(listener)
        })();
        result
    }
    pub(super) fn settle_socket(
        &mut self,
        effect: &Effect,
        cx: &EffectContext,
    ) -> Result<(), RunError> {
        let name = FileName::new(&format!("{}.sock", self.id)).map_err(StoreError::from)?;
        let disposition = (|| -> Result<_, RunError> {
            let Some(dir) = directory(self.root, &self.owner.resource).map_err(StoreError::from)?
            else {
                return Ok(EffectState::Released);
            };
            let Some(stat) = dir.entry(&name).map_err(StoreError::from)? else {
                return Ok(EffectState::Released);
            };
            let receipt: Receipt = serde_json::from_value(
                effect
                    .receipt
                    .clone()
                    .ok_or_else(|| RunError::Invalid("socket identity unrecorded".into()))?,
            )?;
            if device(&stat) != receipt.device
                || stat.st_ino != receipt.inode
                || stat.st_uid != crate::fs::operator_uid()
                || stat.st_mode & libc::S_IFMT != libc::S_IFSOCK
            {
                return Err(RunError::Invalid("socket generation unverified".into()));
            }
            dir.remove(&name, false).map_err(StoreError::from)?;
            Ok(EffectState::Released)
        })();
        let (state, reason) = match disposition {
            Ok(state) => (state, None),
            Err(error) => (EffectState::Residue, Some(error.to_string())),
        };
        self.store.transaction(cx, |txn| {
            journal::write_effect(
                txn,
                self.id,
                effect.sequence,
                state,
                None,
                reason.as_deref(),
            )
        })
    }
}

fn device(stat: &libc::stat) -> u64 {
    #[cfg(target_os = "linux")]
    {
        stat.st_dev
    }
    // macOS dev_t is signed; the durable wire field preserves its numeric bits.
    #[cfg(target_os = "macos")]
    {
        stat.st_dev as u64
    }
}

/// Pure socket coordinates; opening/creating the directory stays inside the
/// owning run. Gateway sockets live in its operator-owned runtime directory.
pub fn control_socket_path(root: &StateRoot, owner: &ResourceId, run: RunId) -> PathBuf {
    let parent = if owner.kind() == ResourceKind::Gateway {
        root.path()
            .parent()
            .expect("state root has parent")
            .join("gateway/runtime")
    } else {
        root.path().join("sock")
    };
    parent.join(format!("{run}.sock"))
}
fn directory(root: &StateRoot, owner: &ResourceId) -> Result<Option<DirFd>, crate::fs::FsError> {
    if owner.kind() != ResourceKind::Gateway {
        return root.family(Family::Sock);
    }
    let effigy = DirFd::open(root.path().parent().expect("state root has parent"))?;
    let Some(gateway) = effigy.child("gateway", Trust::Operator, root.writable())? else {
        return Ok(None);
    };
    gateway.child("runtime", Trust::Private, root.writable())
}
