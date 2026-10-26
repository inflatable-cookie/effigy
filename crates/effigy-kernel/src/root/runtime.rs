//! Production routing: one checkout journal per location, no global journal.
use super::*;
use crate::fs::{DirectoryIdentity, FileName};
use crate::gate::{Gate, Wait};
use crate::id::{CheckoutId, ResourceId, Scope};
use crate::migration::CheckoutLocation;
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
    /// engine. The journal opens by its location; a binding that no longer
    /// matches is stale provenance the next effect rewrites, never a reason to
    /// hide the journal (effigy#337).
    pub fn checkout_observe(cx: &ObserveContext, location: &CheckoutLocation) -> Observed<Self> {
        let root = match Self::observe(cx, &location.root) {
            Observed::Verified(root, _) => root,
            other => return other,
        };
        match root.retain_checkout(location) {
            Ok(root) => Observed::Verified(
                root,
                Evidence::new("verified checkout journal location".into(), None)
                    .expect("fixed source"),
            ),
            Err(error) => Observed::Unknown(crate::root::unknown(&error)),
        }
    }

    /// Opens one checkout journal and binds it in place. The caller supplies
    /// its verified per-checkout Git directory, including the worktree's own
    /// Git directory for linked worktrees. A journal with no binding, or one
    /// whose binding is stale, is bound here.
    pub fn checkout_effects(
        cx: &EffectContext,
        machine_home: &Path,
        location: &CheckoutLocation,
    ) -> Result<Self, StoreError> {
        cx.require(Capability::HostAuthority)
            .map_err(|_| StoreError::HostAuthorityRequired)?;
        let observe = observation(cx)?;
        let root = Self::effects(cx, &location.root)?
            .retain_checkout(location)?
            .route_machine_gates(&observe, machine_home);
        let expected = binding(&root, location)?;
        let name = FileName::new(BINDING)?;
        if bound(&observe, &root, &name, &expected) {
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
        let guard = match Gate::acquire(cx, &root, &[id], Wait::FailFast) {
            Ok(guard) => guard,
            // A concurrent first open is binding this checkout right now. The
            // binding is provenance, not a gate: the journal opens by its
            // location and the next effect writes the binding.
            Err(crate::gate::GateError::Busy { .. } | crate::gate::GateError::TimedOut { .. }) => {
                return Ok(root)
            }
            Err(error) => return Err(error.into()),
        };
        if !bound(&observe, &root, &name, &expected) {
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

    /// Pure production observation of the shared machine facts.
    pub fn runtime_machine_observe(cx: &ObserveContext, home: &Path) -> Observed<Self> {
        Self::machine_observe(cx, home)
    }

    /// Opens only shared machine facts. A legacy `kernel.db` beside them is
    /// ignored, never opened, imported or deleted.
    pub fn runtime_machine_effects(cx: &EffectContext, home: &Path) -> Result<Self, StoreError> {
        cx.require(Capability::HostAuthority)
            .map_err(|_| StoreError::HostAuthorityRequired)?;
        let observe = observation(cx)?;
        let root = Self::machine_effects(cx, home)?;
        if machine_format(&observe, &root)? {
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

fn bound(cx: &ObserveContext, root: &StateRoot, name: &FileName, expected: &Binding) -> bool {
    matches!(
        read_record::<Binding>(cx, root.dir(), name),
        Observed::Verified(binding, _) if &binding == expected
    )
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
        // An unreadable binding proves nothing about the location: the caller
        // falls through to the checkout path alone.
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

/// A record Effigy wrote that no longer matches what it records: a broken
/// binding, never a corrupt `kernel.db` (effigy#337).
fn unreadable(reason: impl Into<String>) -> StoreError {
    StoreError::Broken(format!("the checkout's state binding: {}", reason.into()))
}
