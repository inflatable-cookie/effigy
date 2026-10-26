//! Journalled supervisor socket custody. The Run owns every row mutation.
use super::*;
use crate::fs::{DirFd, FileName, Trust};
use crate::id::{ResourceId, ResourceKind, RunId};
use crate::root::{Family, StateRoot};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;

pub(super) const KIND: &str = "kernel:control-socket";
/// The socket file this run bound. Only its inode is recorded: the device
/// number macOS reports is assigned at mount and changes at every reboot, while
/// the inode still names the file this run bound (effigy#337).
#[derive(Debug, Serialize, Deserialize)]
struct Receipt {
    inode: u64,
}
pub(super) fn intent(target: Fence) -> EffectSpec {
    EffectSpec {
        target,
        kind: KIND.into(),
        payload: Value::Null,
        settlement: Settlement::Release(Value::Null),
        required: false,
    }
}
impl Run<'_> {
    pub(crate) fn control_socket(&mut self) -> Result<UnixListener, RunError> {
        let ticket = self.intend_inner(intent(self.owner.clone()))?;
        let result = (|| {
            crate::fault_point!(SocketBind)?;
            let dir = directory(self.root, &self.owner.resource)
                .map_err(StoreError::from)?
                .ok_or_else(|| RunError::unavailable("socket directory missing"))?;
            let name = FileName::new(&format!("{}.sock", self.id)).map_err(StoreError::from)?;
            if dir.entry(&name).map_err(StoreError::from)?.is_some() {
                return Err(RunError::unavailable("socket path already exists"));
            }
            let listener =
                crate::socket_at::in_directory(&dir, &name, |name| UnixListener::bind(name))??;
            let stat = dir
                .entry(&name)
                .map_err(StoreError::from)?
                .ok_or_else(|| RunError::unavailable("socket vanished"))?;
            let cname = std::ffi::CString::new(name.as_str())
                .map_err(|_| RunError::broken("socket name"))?;
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
                serde_json::to_value(Receipt { inode: stat.st_ino })?,
            )?;
            Ok(listener)
        })();
        result
    }
    pub(super) fn socket_disposition(&self, effect: &Effect) -> (EffectState, Option<String>) {
        let disposition = (|| -> Result<_, RunError> {
            let name = FileName::new(&format!("{}.sock", self.id)).map_err(StoreError::from)?;
            let Some(dir) = directory(self.root, &self.owner.resource).map_err(StoreError::from)?
            else {
                return Ok(EffectState::Released);
            };
            let Some(stat) = dir.entry(&name).map_err(StoreError::from)? else {
                return Ok(EffectState::Released);
            };
            if stat.st_uid != crate::fs::operator_uid()
                || stat.st_mode & libc::S_IFMT != libc::S_IFSOCK
            {
                return Err(RunError::broken("socket generation unverified"));
            }
            // An owner killed between bind and receipt leaves its socket
            // unrecorded. Only this run binds its run-unique name in the
            // private directory, so the name is the generation (§7.3).
            if let Some(receipt) = effect.receipt.clone() {
                let receipt: Receipt = serde_json::from_value(receipt)?;
                if stat.st_ino != receipt.inode {
                    return Err(RunError::broken("socket generation unverified"));
                }
            }
            dir.remove(&name, false).map_err(StoreError::from)?;
            Ok(EffectState::Released)
        })();
        match disposition {
            Ok(state) => (state, None),
            Err(error) => (EffectState::Residue, Some(error.to_string())),
        }
    }
    pub(super) fn settle_socket(
        &mut self,
        effect: &Effect,
        cx: &EffectContext,
    ) -> Result<(), RunError> {
        let (state, reason) = self.socket_disposition(effect);
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
/// Bounded connection used only for observation or waking a journal request.
/// The caller's read context and checkout binding remain authoritative.
pub fn connect_control_socket(
    cx: &crate::ctx::ObserveContext,
    root: &StateRoot,
    owner: &ResourceId,
    run: RunId,
) -> crate::observe::Observed<crate::probe::ProbeIo> {
    let target = (|| -> Result<_, crate::fs::FsError> {
        root.verify_binding()?;
        let directory = directory(root, owner)?.ok_or_else(|| crate::fs::FsError::Untrusted {
            path: root.path().into(),
            reason: "socket directory absent".into(),
        })?;
        let name = FileName::new(&format!("{run}.sock"))?;
        Ok(crate::probe::ObservationTarget::UnixSocketAt { directory, name })
    })();
    match target {
        Ok(target) => cx.connect(target),
        Err(error) => crate::observe::Observed::Unknown(crate::root::unknown(&error)),
    }
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
