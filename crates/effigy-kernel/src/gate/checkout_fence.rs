//! A checkout's external resources cannot be retired while a custodian holds
//! their shared lease. The backend carries this inode's identity after deletion.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckoutFenceIdentity {
    /// The mount-time device number, recorded for diagnostics only. macOS
    /// assigns it at every mount, so it never identifies a lease across a
    /// reboot (effigy#337).
    pub device: u64,
    pub inode: u64,
}
impl CheckoutFenceIdentity {
    /// Whether a recorded identity still names the lease file `current`
    /// describes. The inode is the lease; the device number is not evidence,
    /// because a reboot that renumbers the volume would otherwise look like a
    /// replaced lease and refuse a custody handover (effigy#337).
    pub fn is_same_lease(&self, current: &Self) -> bool {
        self.inode == current.inode
    }
}
#[derive(Debug)]
pub struct CheckoutFenceGuard {
    pub(crate) guard: GateGuard,
    identity: CheckoutFenceIdentity,
}
impl CheckoutFenceGuard {
    pub fn identity(&self) -> CheckoutFenceIdentity {
        self.identity
    }
}
fn name(token: &str) -> Result<String, GateError> {
    if token.len() != 32
        || !token
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(GateError::Untrusted {
            path: PathBuf::new(),
            reason: "invalid checkout fence token".into(),
        });
    }
    Ok(format!("checkout-{token}.retirement"))
}
// libc device/inode widths differ across supported Unix targets, so `st_dev`'s
// `as u64` is required on macOS and redundant on Linux (effigy#520 Linux
// strict-Clippy window; same narrow allow as `custody::merged::atomic`).
#[allow(clippy::unnecessary_cast)]
fn identity(file: &File) -> Result<CheckoutFenceIdentity, GateError> {
    use std::os::fd::AsRawFd;
    let stat = fs::fstat(file.as_raw_fd()).map_err(|e| {
        FsError::io(
            "checkout fence identity",
            std::path::Path::new("checkout fence"),
            &e,
        )
    })?;
    Ok(CheckoutFenceIdentity {
        device: stat.st_dev as u64,
        inode: stat.st_ino as u64,
    })
}
impl Gate {
    /// Sync the lease inode and directory before a render publishes its identity.
    pub fn prepare_checkout_fence(
        cx: &EffectContext,
        root: &StateRoot,
        token: &str,
        wait: Wait,
    ) -> Result<CheckoutFenceGuard, GateError> {
        checkout_fence(cx, root, token, None, false, wait, true)
    }
    /// Prepare or hold a shared lease. No database is opened. Callers which
    /// own processes transfer the same descriptions to their memory backup.
    pub fn checkout_custody(
        cx: &EffectContext,
        root: &StateRoot,
        token: &str,
        wait: Wait,
    ) -> Result<CheckoutFenceGuard, GateError> {
        checkout_fence(cx, root, token, None, false, wait, false)
    }
    /// Retirement requires the exact existing inode recorded by the backend.
    /// Missing state never creates a replacement lock or grants absence.
    pub fn checkout_retirement(
        cx: &EffectContext,
        root: &StateRoot,
        token: &str,
        expected: CheckoutFenceIdentity,
        wait: Wait,
    ) -> Result<CheckoutFenceGuard, GateError> {
        checkout_fence(cx, root, token, Some(expected), true, wait, false)
    }
    /// A pure label input for an already prepared checkout fence.
    pub fn checkout_fence_identity(
        cx: &ObserveContext,
        root: &StateRoot,
        token: &str,
    ) -> Observed<CheckoutFenceIdentity> {
        if cx.placement() == crate::ctx::Placement::Guest {
            return Observed::Unknown(UnknownReason::HostAuthorityRequired);
        }
        let read = || -> Result<Option<CheckoutFenceIdentity>, GateError> {
            let name = name(token)?;
            cx.check("checkout fence observation")
                .map_err(GateError::Interrupted)?;
            let Some(gates) = root.checkout_lease_gates(false)? else {
                return Ok(None);
            };
            let Some(file) = open_gate(&gates, &name, false)? else {
                return Ok(None);
            };
            identity(&file).map(Some)
        };
        match read() {
            Ok(Some(value)) => Observed::Verified(
                value,
                Evidence::new("checkout retirement fence inode".into(), None)
                    .expect("fixed evidence"),
            ),
            Ok(None) => Observed::Absent(AbsenceProof::NoGateInode),
            Err(error) => Observed::Unknown(UnknownReason::Inconsistent(format!("{error:?}"))),
        }
    }
}
fn checkout_fence(
    cx: &EffectContext,
    root: &StateRoot,
    token: &str,
    expected: Option<CheckoutFenceIdentity>,
    exclusive: bool,
    wait: Wait,
    sync: bool,
) -> Result<CheckoutFenceGuard, GateError> {
    if crate::store::in_transaction() {
        return Err(GateError::InTransaction);
    }
    cx.require(Capability::HostAuthority)
        .map_err(|_| GateError::HostAuthorityRequired)?;
    let id = ResourceId::store(Scope::Machine, &format!("checkout-retirement:{token}")).map_err(
        |e| GateError::Untrusted {
            path: root.path().into(),
            reason: e.to_string(),
        },
    )?;
    if !matches!(
        cx.operation().class(),
        EffectClass::Mutate | EffectClass::Prepare
    ) {
        return Err(GateError::ClassRefused {
            id,
            class: cx.operation().class(),
        });
    }
    if !root.writable() {
        return Err(GateError::Untrusted {
            path: root.path().into(),
            reason: "state root opened read-only".into(),
        });
    }
    if exclusive && cx.operation().class() != EffectClass::Mutate {
        return Err(GateError::ClassRefused {
            id,
            class: cx.operation().class(),
        });
    }
    let name = name(token)?;
    let gates = root
        .checkout_lease_gates(!exclusive)?
        .ok_or_else(|| GateError::Untrusted {
            path: root.path().into(),
            reason: "checkout fence namespace missing".into(),
        })?;
    let file = open_gate(&gates, &name, !exclusive)?.ok_or_else(|| GateError::Untrusted {
        path: gates.path().into(),
        reason: "checkout fence missing; ownership remains unknown".into(),
    })?;
    let actual = identity(&file)?;
    if expected.is_some_and(|expected| !expected.is_same_lease(&actual)) {
        return Err(GateError::Untrusted {
            path: gates.path().into(),
            reason: "checkout fence inode replaced".into(),
        });
    }
    let started = Instant::now();
    let mut deadline = match wait {
        Wait::FailFast => started + FAIL_FAST,
        Wait::Until(at) => at,
    };
    if let Some(left) = cx.operation().remaining() {
        deadline = deadline.min(started + left);
    }
    let namespace =
        open_gate(&gates, "machine.scope", !exclusive)?.ok_or_else(|| GateError::Untrusted {
            path: gates.path().into(),
            reason: "machine fence namespace missing".into(),
        })?;
    let namespace = lock(cx, namespace, libc::LOCK_SH, deadline, started, wait, &id)?;
    let file = lock(
        cx,
        file,
        if exclusive {
            libc::LOCK_EX
        } else {
            libc::LOCK_SH
        },
        deadline,
        started,
        wait,
        &id,
    )?;
    let reopened = open_gate(&gates, &name, false)?.ok_or_else(|| GateError::Untrusted {
        path: gates.path().into(),
        reason: "checkout fence vanished while acquiring".into(),
    })?;
    if identity(&reopened)? != actual {
        return Err(GateError::Untrusted {
            path: gates.path().into(),
            reason: "checkout fence changed while acquiring".into(),
        });
    }
    if sync {
        fs::fsync(&file.file).map_err(|e| FsError::io("sync checkout fence", gates.path(), &e))?;
        gates.sync()?;
    }
    Ok(CheckoutFenceGuard {
        guard: GateGuard {
            release_actions: Default::default(),
            gates: Vec::new(),
            scopes: vec![("machine".into(), namespace), (name, file)],
        },
        identity: actual,
    })
}
