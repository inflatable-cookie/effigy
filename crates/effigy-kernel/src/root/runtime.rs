//! Production routing: no implicit legacy adoption, fallback or global journal.
use super::*;
use crate::fs::{DirectoryIdentity, FileName};
use crate::gate::{Gate, Wait};
use crate::id::{CheckoutId, ResourceId, Scope};
use crate::migration::{authority, Authority, CheckoutLocation};
use crate::record::{atomic_replace, read_record, Record, ReplaceOpts};
use crate::store::StoreError;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const BINDING: &str = "checkout.json";
const MACHINE_FORMAT: &str = "machine-format.json";

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MachineFormat {
    format: String,
    home: DirectoryIdentity,
}
impl Record for MachineFormat {
    const MAX_BYTES: u64 = 4096;
    fn decode(bytes: &[u8]) -> Result<Self, String> {
        let record: Self = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        if record.format != "effigy.machine-state.v1" {
            return Err("unsupported machine state format".into());
        }
        Ok(record)
    }
}

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
    /// Compatibility observation for a persisted owner before relocation.
    /// Published checkout authority never depends on the machine source again.
    pub(crate) fn foreground_legacy(
        cx: &ObserveContext,
        local: &Self,
        machine_home: Option<&Path>,
    ) -> Result<Option<Self>, StoreError> {
        local.verify_binding()?;
        match authority(cx, local) {
            Observed::Verified(Authority::Committed { .. }, _) => return Ok(None),
            Observed::Verified(Authority::Fresh, _) => {
                match read_record::<Binding>(cx, local.dir(), &FileName::new(BINDING)?) {
                    Observed::Verified(_, _) => return Ok(None),
                    Observed::Absent(_) => {}
                    other => {
                        return Err(unreadable(format!(
                            "foreground checkout authority: {other:?}"
                        )))
                    }
                }
            }
            other => {
                return Err(unreadable(format!(
                    "foreground checkout publication: {other:?}"
                )))
            }
        }
        let home = machine_home.ok_or_else(|| {
            unreadable("HOME is required to verify this persisted owner's legacy predecessor")
        })?;
        if crate::migration::committed(cx, home)?.is_some() {
            return Ok(None);
        }
        let legacy = match Self::legacy_observe(cx, home) {
            Observed::Verified(root, _) => root,
            Observed::Absent(_) => return Ok(None),
            other => {
                return Err(unreadable(format!(
                    "legacy foreground predecessor source: {other:?}"
                )))
            }
        };
        let database = legacy
            .dir()
            .entry(&FileName::new(crate::store::DB_NAME)?)?
            .is_some();
        if !database && machine_format(cx, &legacy)? {
            return Ok(None);
        }
        if legacy.owner.identity()? == local.owner.identity()? {
            return Err(unreadable(
                "legacy machine source overlaps the foreground checkout",
            ));
        }
        Ok(Some(legacy))
    }

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
            Ok(()) => match root.retain_checkout(location) {
                Ok(root) => Observed::Verified(
                    root,
                    Evidence::new("verified checkout journal location".into(), None)
                        .expect("fixed source"),
                ),
                Err(error) => Observed::Unknown(crate::root::unknown(&error)),
            },
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
        crate::inheritance::require_durable(cx.operation()).map_err(|error| {
            StoreError::from(crate::fs::FsError::io(
                "runtime class",
                &location.root,
                &error,
            ))
        })?;
        cx.require(Capability::HostAuthority)
            .map_err(|_| StoreError::HostAuthorityRequired)?;
        let observe = observation(cx)?;
        require_cutover(&observe, machine_home, Some(&location.root))?;
        let root = Self::effects(cx, &location.root)?
            .retain_checkout(location)?
            .route_machine_gates(&observe, machine_home);
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
        // All first durable initializers meet the same filesystem scope,
        // independently of whether a persisted token exists yet.
        let id = ResourceId::store(
            Scope::Checkout(
                root.local_gate_identity(&observe)?
                    .ok_or_else(|| unreadable("checkout filesystem identity unavailable"))?,
            ),
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

    /// Pure production observation. An unmigrated legacy journal is Unknown,
    /// rather than an empty machine store that hides retained routes or pins.
    pub fn runtime_machine_observe(cx: &ObserveContext, home: &Path) -> Observed<Self> {
        if cx.placement() == Placement::Guest {
            return Observed::Unknown(UnknownReason::HostAuthorityRequired);
        }
        match require_cutover(cx, home, None) {
            Ok(()) => Self::machine_observe(cx, home),
            Err(error) => Observed::Unknown(UnknownReason::StoreUnreadable(error.to_string())),
        }
    }

    /// Opens only shared machine facts. An old `kernel.db` is never opened
    /// for writes or silently ignored before explicit migration commits.
    pub fn runtime_machine_effects(cx: &EffectContext, home: &Path) -> Result<Self, StoreError> {
        crate::inheritance::require_durable(cx.operation()).map_err(|error| {
            StoreError::from(crate::fs::FsError::io("runtime class", home, &error))
        })?;
        cx.require(Capability::HostAuthority)
            .map_err(|_| StoreError::HostAuthorityRequired)?;
        let observe = observation(cx)?;
        require_cutover(&observe, home, None)?;
        let root = Self::machine_effects(cx, home)?;
        if matches!(
            authority(&observe, &root),
            Observed::Verified(Authority::Committed { .. }, _)
        ) || machine_format(&observe, &root)?
        {
            return Ok(root);
        }
        let guard = Gate::acquire(
            cx,
            &root,
            &[format_gate()?],
            Wait::Until(std::time::Instant::now() + Duration::from_secs(10)),
        )?;
        if !machine_format(&observe, &root)? {
            let bytes = serde_json::to_vec(&MachineFormat {
                format: "effigy.machine-state.v1".into(),
                home: root.owner.identity()?,
            })
            .map_err(|error| unreadable(error.to_string()))?;
            atomic_replace(
                cx,
                &guard,
                root.dir(),
                &FileName::new(MACHINE_FORMAT)?,
                &bytes,
                ReplaceOpts::default(),
            )?;
        }
        Ok(root)
    }
}

fn format_gate() -> Result<ResourceId, StoreError> {
    Ok(ResourceId::store(Scope::Machine, "runtime-layout")?)
}
fn machine_format(cx: &ObserveContext, root: &StateRoot) -> Result<bool, StoreError> {
    match read_record::<MachineFormat>(cx, root.dir(), &FileName::new(MACHINE_FORMAT)?) {
        Observed::Verified(record, _) if record.home == root.owner.identity()? => Ok(true),
        Observed::Absent(_) => Ok(false),
        other => Err(unreadable(format!("machine format binding: {other:?}"))),
    }
}

pub(super) fn binding_identity(
    cx: &ObserveContext,
    root: &StateRoot,
) -> Result<Option<CheckoutId>, StoreError> {
    match read_record::<Binding>(cx, root.dir(), &FileName::new(BINDING)?) {
        Observed::Verified(binding, _) => {
            verify(cx, root)?;
            super::checkout::filesystem_identity(
                binding.checkout,
                binding.git.map(|(_, identity)| identity),
            )
            .map(Some)
        }
        Observed::Absent(_) => Ok(None),
        other => Err(unreadable(format!("checkout binding: {other:?}"))),
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
            Observed::Verified(Authority::Fresh, _) => {
                // A published local binding is independent authority. Routine
                // checkout work never reconsults machine state after bootstrap.
                if checkout.is_some()
                    && matches!(
                        read_record::<Binding>(cx, root.dir(), &FileName::new(BINDING)?),
                        Observed::Verified(_, _)
                    )
                {
                    return Ok(());
                }
            }
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
            let initializer = format_gate()?.name().to_string();
            let source_hints = ResourceId::store(Scope::Machine, "source-hints")?
                .name()
                .to_string();
            let pre_run_writer = !database
                && legacy.dir().entry(&FileName::new("machine.db")?)?.is_none()
                && !machine_format(cx, &legacy)?
                && crate::migration::inspect_gates(cx, &legacy)?
                    .iter()
                    .any(|blocker| {
                        // This new-format initializer has no old-binary meaning
                        // and never opens a database. Concurrent bootstrap waits
                        // on its gate after the legacy-writer check.
                        blocker.scope != "machine"
                            || (blocker.resource != initializer && blocker.resource != source_hints)
                    });
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
