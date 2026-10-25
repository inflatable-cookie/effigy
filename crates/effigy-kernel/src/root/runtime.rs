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
    /// Pure checkout routing with the current Git resolution supplied by the
    /// engine. The journal opens by its location; a binding or import seal that
    /// no longer matches is stale provenance the next effect rewrites, never a
    /// reason to hide the journal (effigy#337).
    pub fn checkout_observe(cx: &ObserveContext, location: &CheckoutLocation) -> Observed<Self> {
        let root = match Self::observe(cx, &location.root) {
            Observed::Verified(root, _) => root,
            other => return other,
        };
        let checked = (|| {
            // A journal opens by its location. A binding or import seal which
            // no longer names this checkout's current generations or path is
            // stale evidence, not a refusal: the next effect rewrites the
            // binding and the journal keeps its runs (effigy#337).
            match authority(cx, &root) {
                Observed::Verified(Authority::Committed { .. } | Authority::Fresh, _) => {}
                other => {
                    return Err(unreadable(format!(
                        "{}: the checkout publication could not be proved ({})",
                        location.root.display(),
                        other.unproven()
                    )))
                }
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
        cx.require(Capability::HostAuthority)
            .map_err(|_| StoreError::HostAuthorityRequired)?;
        let observe = observation(cx)?;
        let local = cutover_root(cx, &observe, machine_home, Some(&location.root))?;
        require_cutover(&observe, local, machine_home, Some(&location.root))?;
        let root = Self::effects(cx, &location.root)?
            .retain_checkout(location)?
            .route_machine_gates(&observe, machine_home);
        let expected = binding(&root, location)?;
        match authority(&observe, &root) {
            // A sealed import opens by its location too; its recorded
            // destination generations are provenance (effigy#337).
            Observed::Verified(Authority::Committed { .. }, _) => return Ok(root),
            Observed::Verified(Authority::Fresh, _) => {}
            other => {
                return Err(unreadable(format!(
                    "{}: the checkout publication could not be proved ({})",
                    location.root.display(),
                    other.unproven()
                )))
            }
        }
        let name = FileName::new(BINDING)?;
        // What the state root records about itself. A stale or unreadable
        // record is provenance that went bad, never a refusal: it is rewritten
        // below (effigy#337). Only absence is different, because a database this
        // binary never bound is still not adopted silently.
        #[derive(PartialEq)]
        enum Recorded {
            Current,
            Stale,
            Absent,
        }
        let recorded = |cx: &ObserveContext| match read_record::<Binding>(cx, root.dir(), &name) {
            Observed::Verified(existing, _) if existing == expected => Recorded::Current,
            Observed::Verified(..) | Observed::Unknown(_) => Recorded::Stale,
            Observed::Absent(_) => Recorded::Absent,
        };
        if recorded(&observe) == Recorded::Current {
            return Ok(root);
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
        let current = recorded(&observe);
        if current != Recorded::Current {
            if current == Recorded::Absent
                && root
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
        Ok(root)
    }

    /// Pure production observation. An unmigrated legacy journal is Unknown,
    /// rather than an empty machine store that hides retained routes or pins.
    pub fn runtime_machine_observe(cx: &ObserveContext, home: &Path) -> Observed<Self> {
        if cx.placement() == Placement::Guest {
            return Observed::Unknown(UnknownReason::HostAuthorityRequired);
        }
        match require_cutover(cx, StateRoot::machine_observe(cx, home), home, None) {
            Ok(()) => Self::machine_observe(cx, home),
            Err(error) => Observed::Unknown(UnknownReason::StoreUnreadable(error.to_string())),
        }
    }

    /// Opens only shared machine facts. An old `kernel.db` is never opened
    /// for writes or silently ignored before explicit migration commits.
    pub fn runtime_machine_effects(cx: &EffectContext, home: &Path) -> Result<Self, StoreError> {
        cx.require(Capability::HostAuthority)
            .map_err(|_| StoreError::HostAuthorityRequired)?;
        let observe = observation(cx)?;
        let local = cutover_root(cx, &observe, home, None)?;
        require_cutover(&observe, local, home, None)?;
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
        // The marker records the home directory's generation for diagnostics.
        // A home whose generation moved since the record was written (a reboot
        // renumbers its volume) is rebound by the caller, never refused
        // (effigy#337).
        Observed::Verified(record, _) => Ok(record.home == root.owner.identity()?),
        Observed::Absent(_) => Ok(false),
        other => Err(unreadable(format!(
            "{}: the machine format binding could not be read ({})",
            root.path().display(),
            other.unproven()
        ))),
    }
}

pub(super) fn binding_identity(
    cx: &ObserveContext,
    root: &StateRoot,
) -> Result<Option<CheckoutId>, StoreError> {
    match read_record::<Binding>(cx, root.dir(), &FileName::new(BINDING)?) {
        Observed::Verified(binding, _) => {
            // The binding names which Git directory this checkout journals. The
            // gate identity is the location, so a reboot that renumbers a volume
            // cannot split a checkout's gates (effigy#337).
            let git = binding.git.map(|(path, _)| path);
            super::checkout::filesystem_identity(root.owner.path(), git.as_deref()).map(Some)
        }
        // An unreadable binding proves nothing about the location either: the
        // caller falls through to the seal, then to the checkout path alone.
        Observed::Absent(_) | Observed::Unknown(_) => Ok(None),
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
    local: Observed<StateRoot>,
    machine_home: &Path,
    checkout: Option<&Path>,
) -> Result<(), StoreError> {
    // A sealed checkout remains independently usable even when the machine
    // store is unavailable. A partial import remains a refusal everywhere.
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
            other => {
                return Err(unreadable(format!(
                    "{}: the checkout journal is not published yet ({})",
                    checkout.map_or_else(
                        || "this checkout".to_owned(),
                        |path| path.display().to_string()
                    ),
                    other.unproven()
                )))
            }
        },
        Observed::Absent(_) => {}
        other => {
            return Err(unreadable(format!(
                "{}: the state root could not be verified ({})",
                checkout.map_or_else(
                    || "this checkout".to_owned(),
                    |path| path.display().to_string()
                ),
                other.unproven()
            )))
        }
    }
    if crate::migration::committed(cx, machine_home)?.is_some() {
        return Ok(());
    }
    match StateRoot::legacy_observe(cx, machine_home) {
        Observed::Verified(legacy, _) => {
            if legacy
                .dir()
                .entry(&FileName::new(crate::store::DB_NAME)?)?
                .is_some()
            {
                return Err(StoreError::UnreleasedMainRuntime);
            }
            // An unreadable or foreign machine layout is `Unknown`, never a
            // fresh empty machine. A new-format layout has no database yet.
            machine_format(cx, &legacy)?;
        }
        Observed::Absent(_) => {}
        other => {
            return Err(unreadable(format!(
                "{}: legacy runtime state is unknown ({})",
                machine_home.join(".effigy").display(),
                other.unproven()
            )))
        }
    }
    Ok(())
}

/// The local root an effect decides the cutover from.
///
/// A leftover group/other-readable mode on `.effigy/state/` (v0.14
/// `effigy state`, papercut 9e5134a4) is not a fact about the journal. The same
/// adoption an effect performs, which only tightens a directory that already
/// exists, resolves it, so the cutover is decided on the real root instead of
/// refusing every task in the checkout. No new destination is created before
/// the cutover decision, and pure observation keeps such a root `Unknown`.
fn cutover_root(
    cx: &EffectContext,
    observe: &ObserveContext,
    machine_home: &Path,
    checkout: Option<&Path>,
) -> Result<Observed<StateRoot>, StoreError> {
    let local = match checkout {
        Some(path) => StateRoot::observe(observe, path),
        None => StateRoot::machine_observe(observe, machine_home),
    };
    match local {
        Observed::Unknown(reason) if crate::root::is_open_mode_only(&reason) => {
            let adopted = match checkout {
                Some(path) => StateRoot::effects(cx, path),
                None => StateRoot::machine_effects(cx, machine_home),
            }?;
            Ok(Observed::Verified(
                adopted,
                Evidence::new("adopted state root mode".into(), None).expect("fixed source"),
            ))
        }
        other => Ok(other),
    }
}

fn unreadable(reason: impl Into<String>) -> StoreError {
    StoreError::Corrupt(reason.into())
}
