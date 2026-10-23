//! Production routing: no implicit legacy adoption, fallback or global journal.
use super::*;
use crate::fs::{DirectoryIdentity, FileName};
use crate::gate::{Gate, Wait};
use crate::id::{CheckoutId, CheckoutKind, Digest, ResourceId, Scope};
use crate::migration::{authority, Authority, CheckoutLocation};
use crate::record::{atomic_replace, read_record, Record, ReplaceOpts};
use crate::store::StoreError;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const BINDING: &str = "checkout.json";

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    format: String,
    checkout: DirectoryIdentity,
    git: Option<(PathBuf, DirectoryIdentity)>,
}
impl Record for Binding {
    const MAX_BYTES: u64 = 16 << 10;
    fn decode(bytes: &[u8]) -> Result<Self, String> {
        let binding: Self = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        if binding.format != "effigy.checkout-state.v1"
            || binding
                .git
                .as_ref()
                .is_some_and(|(path, _)| !path.is_absolute())
        {
            return Err("unsupported checkout binding".into());
        }
        Ok(binding)
    }
}

impl StateRoot {
    /// Pure checkout routing with the current Git resolution supplied by the
    /// engine. A copied token or retargeted `.git` file cannot select an old
    /// journal whose original Git directory still happens to exist.
    pub fn checkout_observe(cx: &ObserveContext, location: &CheckoutLocation) -> Observed<Self> {
        let root = match Self::observe(cx, &location.root) {
            Observed::Verified(root, _) => root,
            other => return other,
        };
        let checked = (|| {
            let expected = binding(&root, location)?;
            match authority(cx, &root) {
                Observed::Verified(Authority::Committed { .. }, _) => {
                    crate::migration::adoption::check_git_location(cx, &root, &expected.git)?
                }
                Observed::Verified(Authority::Fresh, _) => {
                    match read_record::<Binding>(cx, root.dir(), &FileName::new(BINDING)?) {
                        Observed::Verified(existing, _) if existing == expected => {}
                        Observed::Absent(_)
                            if root
                                .dir()
                                .entry(&FileName::new(root.database_name())?)?
                                .is_none() => {}
                        other => {
                            return Err(unreadable(format!(
                                "checkout generation disagrees: {other:?}"
                            )))
                        }
                    }
                }
                other => return Err(unreadable(format!("checkout publication: {other:?}"))),
            }
            Ok::<_, StoreError>(())
        })();
        match checked {
            Ok(()) => Observed::Verified(
                root,
                Evidence::new("verified checkout journal location".into(), None)
                    .expect("fixed source"),
            ),
            Err(error) => Observed::Unknown(UnknownReason::StoreUnreadable(error.to_string())),
        }
    }

    /// Opens one checkout journal after the legacy cutover has been resolved.
    /// The caller supplies its verified per-checkout Git directory, including
    /// the worktree's own Git directory for linked worktrees. Fresh journals
    /// bind both filesystem generations before any database is created.
    pub fn checkout_effects(
        cx: &EffectContext,
        machine_home: &Path,
        location: &CheckoutLocation,
    ) -> Result<Self, StoreError> {
        cx.require(Capability::HostAuthority)
            .map_err(|_| StoreError::HostAuthorityRequired)?;
        let observe = observation(cx)?;
        require_cutover(&observe, machine_home, Some(&location.root))?;
        let root = Self::effects(cx, &location.root)?;
        let expected = binding(&root, location)?;
        match authority(&observe, &root) {
            Observed::Verified(Authority::Committed { .. }, _) => {
                crate::migration::adoption::check_git_location(&observe, &root, &expected.git)?;
                return Ok(root);
            }
            Observed::Verified(Authority::Fresh, _) => {}
            other => return Err(unreadable(format!("checkout publication: {other:?}"))),
        }
        let name = FileName::new(BINDING)?;
        match read_record::<Binding>(&observe, root.dir(), &name) {
            Observed::Verified(existing, _) if existing == expected => return Ok(root),
            Observed::Absent(_) => {}
            other => {
                return Err(unreadable(format!(
                    "checkout generation disagrees: {other:?}"
                )))
            }
        }
        // All token-init spellings meet this filesystem-generation gate.
        let digest = Digest::of(
            &serde_json::to_vec(&expected.checkout)
                .map_err(|error| unreadable(error.to_string()))?,
        );
        let mut token = [0; 16];
        token.copy_from_slice(&digest.0[..16]);
        let id = ResourceId::store(
            Scope::Checkout(CheckoutId::new(CheckoutKind::Primary, token)),
            "checkout-binding",
        )?;
        let guard = Gate::acquire(cx, &root, &[id], Wait::FailFast)?;
        match read_record::<Binding>(&observe, root.dir(), &name) {
            Observed::Verified(existing, _) if existing == expected => {}
            Observed::Absent(_) => {
                if root
                    .dir()
                    .entry(&FileName::new(root.database_name())?)?
                    .is_some()
                {
                    return Err(unreadable(
                        "unbound checkout database; explicit migration is required",
                    ));
                }
                let bytes =
                    serde_json::to_vec(&expected).map_err(|error| unreadable(error.to_string()))?;
                atomic_replace(
                    cx,
                    &guard,
                    root.dir(),
                    &name,
                    &bytes,
                    ReplaceOpts::default(),
                )?;
            }
            other => {
                return Err(unreadable(format!(
                    "checkout generation disagrees: {other:?}"
                )))
            }
        }
        verify(&observe, &root)?;
        Ok(root)
    }

    /// Opens only shared machine facts. An old `kernel.db` is never opened
    /// for writes or silently ignored before explicit migration commits.
    pub fn runtime_machine_effects(cx: &EffectContext, home: &Path) -> Result<Self, StoreError> {
        cx.require(Capability::HostAuthority)
            .map_err(|_| StoreError::HostAuthorityRequired)?;
        require_cutover(&observation(cx)?, home, None)?;
        Ok(Self::machine_effects(cx, home)?)
    }
}

fn binding(root: &StateRoot, location: &CheckoutLocation) -> Result<Binding, StoreError> {
    Ok(Binding {
        format: "effigy.checkout-state.v1".into(),
        checkout: root.owner.identity()?,
        git: location
            .git_directory
            .as_deref()
            .map(|path| {
                let dir = DirFd::open(path)?;
                Ok::<_, FsError>((dir.path().into(), dir.identity()?))
            })
            .transpose()?,
    })
}

fn observation(cx: &EffectContext) -> Result<ObserveContext, StoreError> {
    cx.child(Duration::from_secs(10))
        .operation()
        .observe()
        .map_err(|_| StoreError::Unbounded)
}

fn require_cutover(
    cx: &ObserveContext,
    machine_home: &Path,
    checkout: Option<&Path>,
) -> Result<(), StoreError> {
    // A sealed checkout remains independently usable even when the machine
    // store is unavailable. A partial import remains a refusal everywhere.
    let local = match checkout {
        Some(path) => StateRoot::observe(cx, path),
        None => StateRoot::machine_observe(cx, machine_home),
    };
    match local {
        Observed::Verified(root, _) => match authority(cx, &root) {
            Observed::Verified(Authority::Committed { .. }, _) => return Ok(()),
            Observed::Verified(Authority::Prepared { id }, _) => {
                return Err(StoreError::MigrationPending(id))
            }
            Observed::Verified(Authority::Fresh, _) => {}
            other => return Err(unreadable(format!("runtime publication: {other:?}"))),
        },
        Observed::Absent(_) => {}
        other => return Err(unreadable(format!("runtime root: {other:?}"))),
    }
    if crate::migration::committed(cx, machine_home)?.is_some() {
        return Ok(());
    }
    match StateRoot::legacy_observe(cx, machine_home) {
        Observed::Verified(legacy, _) => {
            let database = legacy
                .dir()
                .entry(&FileName::new(crate::store::DB_NAME)?)?
                .is_some();
            let pre_run_writer = !database
                && legacy.dir().entry(&FileName::new("machine.db")?)?.is_none()
                && !crate::migration::inspect_gates(cx, &legacy)?.is_empty();
            if database || pre_run_writer {
                return Err(StoreError::MigrationPending(
                    "legacy runtime exists; run `effigy run migrate --plan`, stop or drain its exact writers, then `effigy run migrate --legacy-launchers-stopped`".into(),
                ));
            }
        }
        Observed::Absent(_) => {}
        other => return Err(unreadable(format!("legacy runtime root: {other:?}"))),
    }
    Ok(())
}

pub(crate) fn verify(cx: &ObserveContext, root: &StateRoot) -> Result<(), StoreError> {
    if root.is_machine() || !root.requires_adoption() {
        return Ok(());
    }
    match read_record::<Binding>(cx, root.dir(), &FileName::new(BINDING)?) {
        Observed::Absent(_) => Ok(()), // primitive fixtures and import seals
        Observed::Verified(binding, _) => {
            if root.owner.identity()? != binding.checkout {
                return Err(unreadable("checkout directory generation changed"));
            }
            if let Some((path, identity)) = binding.git {
                if DirFd::open(&path)?.identity()? != identity {
                    return Err(unreadable("checkout Git directory generation changed"));
                }
            }
            Ok(())
        }
        other => Err(unreadable(format!("checkout binding: {other:?}"))),
    }
}

fn unreadable(reason: impl Into<String>) -> StoreError {
    StoreError::Corrupt(reason.into())
}
