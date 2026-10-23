//! Machine resource locks share one physical namespace. Checkout journals
//! remain independent; their routing snapshot is consulted only for a machine gate.
use super::*;
use crate::fs::DirectoryIdentity;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, OnceLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MachineGateAddress {
    home: PathBuf,
    owner: Option<DirectoryIdentity>,
    state: Option<DirectoryIdentity>,
    unavailable: Option<String>,
}

#[derive(Debug)]
pub(super) struct MachineGates {
    address: MachineGateAddress,
    state: OnceLock<Result<Box<StateRoot>, FsError>>,
}

impl StateRoot {
    /// Private derived-file locks have no journal or machine-store layout.
    /// Only Gate exposes this root, so callers cannot open a database here.
    pub(crate) fn file_gate_root(cx: &EffectContext, directory: &DirFd) -> Result<Self, FsError> {
        cx.require(Capability::HostAuthority)
            .map_err(|_| FsError::HostAuthorityRequired)?;
        crate::fs::check(
            &directory.stat()?,
            Trust::Private,
            crate::fs::Kind::Dir,
            directory.path(),
        )?;
        directory.require_local()?;
        Ok(Self {
            dir: directory.clone(),
            owner: directory.clone(),
            writable: true,
            database: super::Database::Checkout,
            adoption: false,
            checkout: None,
            machine_gates: None,
        })
    }

    pub(super) fn route_machine_gates(self, cx: &ObserveContext, home: &Path) -> Self {
        self.restore_machine_gates(
            cx,
            MachineGateAddress {
                home: home.into(),
                owner: None,
                state: None,
                unavailable: None,
            },
        )
    }

    /// Observation with an explicit machine lock domain, without reading a
    /// machine database or making an unavailable domain a local-read failure.
    pub fn checkout_observe_in(
        cx: &ObserveContext,
        home: &Path,
        location: &crate::migration::CheckoutLocation,
    ) -> Observed<Self> {
        Self::checkout_observe(cx, location).map(|root| root.route_machine_gates(cx, home))
    }

    pub(crate) fn restore_machine_gates(
        mut self,
        cx: &ObserveContext,
        address: MachineGateAddress,
    ) -> Self {
        let domain = MachineGates {
            address,
            state: OnceLock::new(),
        };
        // Ordinary checkout work consults no machine-state directory. A
        // helper restores a previously captured generation; first machine
        // gate use captures an initially unbound domain.
        if domain.address.state.is_some() || domain.address.unavailable.is_some() {
            match domain.observe(cx) {
                Observed::Verified(root, _) => {
                    let _ = domain.state.set(Ok(Box::new(root)));
                }
                Observed::Absent(_) => {}
                Observed::Unknown(reason) => {
                    let _ = domain.state.set(Err(FsError::untrusted(
                        &domain.address.home,
                        format!("machine gate domain: {reason:?}"),
                    )));
                }
            }
        }
        self.machine_gates = Some(Arc::new(domain));
        self
    }

    pub(crate) fn machine_gate_address(&self) -> Option<MachineGateAddress> {
        let domain = self.machine_gates.as_ref()?;
        let mut address = domain.address.clone();
        match domain.state.get() {
            Some(Ok(root)) => match root.dir.identity() {
                Ok(identity) => {
                    address.state = Some(identity);
                    match root.owner.identity() {
                        Ok(owner) => address.owner = Some(owner),
                        Err(error) => address.unavailable = Some(error.to_string()),
                    }
                }
                Err(error) => address.unavailable = Some(error.to_string()),
            },
            Some(Err(error)) => address.unavailable = Some(error.to_string()),
            None => {}
        }
        Some(address)
    }

    /// Internal store/object mutexes belong to their selected journal. Their
    /// former machine scope described the old central layout, not a host fact.
    pub(crate) fn store_gate(
        &self,
        cx: &EffectContext,
        name: &str,
    ) -> Result<crate::id::ResourceId, crate::store::StoreError> {
        use crate::id::{ResourceId, Scope};
        use crate::store::StoreError;
        cx.require(Capability::HostAuthority)
            .map_err(|_| StoreError::HostAuthorityRequired)?;
        cx.operation()
            .check("store mutex identity")
            .map_err(StoreError::Interrupted)?;
        let local = !self.is_machine() && (self.checkout.is_some() || self.machine_gates.is_some());
        let scope = if local {
            Scope::Checkout(self.directory_gate_identity()?)
        } else {
            Scope::Machine
        };
        Ok(ResourceId::store(scope, name)?)
    }

    /// Checkout lifetime leases are lock files, outside machine journal state.
    /// They must survive checkout deletion without bootstrapping machine.db or
    /// its layout marker. Primitive roots retain their explicit test namespace.
    pub(crate) fn checkout_lease_gates(&self, create: bool) -> Result<Option<DirFd>, FsError> {
        let home = if let Some(domain) = &self.machine_gates {
            domain.verify_home()?;
            &domain.address.home
        } else if self.is_machine() {
            self.owner.path()
        } else if self.checkout.is_none() {
            return if create {
                self.family(Family::Gates)
            } else {
                self.read_family(Family::Gates)
            };
        } else {
            return Err(FsError::untrusted(
                self.path(),
                "checkout lease requires an explicit machine domain",
            ));
        };
        let home = DirFd::walk(home, Trust::Operator)?;
        let Some(effigy) = home.child(".effigy", Trust::Operator, create)? else {
            return Ok(None);
        };
        let gates = effigy.child("gates", Trust::Private, create)?;
        if create {
            if let Some(gates) = &gates {
                gates.require_local()?;
            }
        }
        Ok(gates)
    }

    pub(crate) fn machine_gate_effects(&self, cx: &EffectContext) -> Result<Option<Self>, FsError> {
        let Some(domain) = &self.machine_gates else {
            return if self.is_machine() || self.checkout.is_none() {
                Ok(None)
            } else {
                Err(FsError::untrusted(
                    self.path(),
                    "machine resource requires an explicit machine gate domain",
                ))
            };
        };
        crate::inheritance::require_durable(cx.operation()).map_err(|error| {
            FsError::io("machine gate runtime class", &domain.address.home, &error)
        })?;
        cx.require(Capability::HostAuthority)
            .map_err(|_| FsError::HostAuthorityRequired)?;
        domain.verify_home()?;
        if let Some(held) = domain.state.get() {
            held.as_ref().map_err(Clone::clone)?.verify_binding()?;
        }
        // New machine gate namespaces carry the new-format seal before a
        // gate is held, so peers cannot mistake them for legacy DB writers.
        let current = Self::runtime_machine_effects(cx, &domain.address.home)
            .map_err(|error| FsError::untrusted(&domain.address.home, error.to_string()))?;
        let root = domain
            .state
            .get_or_init(|| Ok(Box::new(current)))
            .as_ref()
            .map_err(Clone::clone)?;
        root.verify_binding()?;
        root.dir.require_local()?;
        let mut root = root.as_ref().clone();
        root.writable = true;
        Ok(Some(root))
    }

    pub(crate) fn machine_gate_observe(&self, cx: &ObserveContext) -> Observed<Option<Self>> {
        if cx.placement() == Placement::Guest {
            return Observed::Unknown(UnknownReason::HostAuthorityRequired);
        }
        let Some(domain) = &self.machine_gates else {
            return if self.is_machine() || self.checkout.is_none() {
                Observed::Verified(
                    None,
                    Evidence::new("explicit primitive gate root".into(), None)
                        .expect("fixed source"),
                )
            } else {
                Observed::Unknown(UnknownReason::Untrusted(
                    "machine resource requires an explicit machine gate domain".into(),
                ))
            };
        };
        let root = if let Some(held) = domain.state.get() {
            match held {
                Ok(root) => match domain.verify_home().and_then(|()| root.verify_binding()) {
                    Ok(()) => Observed::Verified(
                        root.as_ref().clone(),
                        Evidence::new("retained machine gate root".into(), None)
                            .expect("fixed source"),
                    ),
                    Err(error) => Observed::Unknown(unknown(&error)),
                },
                Err(error) => Observed::Unknown(unknown(error)),
            }
        } else {
            domain.observe(cx)
        };
        root.map(|mut root| {
            let _ = domain.state.set(Ok(Box::new(root.clone())));
            root.writable = false;
            Some(root)
        })
    }
}

impl MachineGates {
    fn verify_home(&self) -> Result<(), FsError> {
        if let Some(reason) = &self.address.unavailable {
            return Err(FsError::untrusted(&self.address.home, reason));
        }
        let current = DirFd::walk(&self.address.home, Trust::Operator)?.identity()?;
        if self
            .address
            .owner
            .is_some_and(|expected| expected != current)
        {
            return Err(FsError::untrusted(
                &self.address.home,
                "machine home generation changed",
            ));
        }
        Ok(())
    }
    fn observe(&self, cx: &ObserveContext) -> Observed<StateRoot> {
        if cx.placement() == Placement::Guest {
            return Observed::Unknown(UnknownReason::HostAuthorityRequired);
        }
        if let Err(error) = self.verify_home() {
            return Observed::Unknown(unknown(&error));
        }
        match StateRoot::machine_observe(cx, &self.address.home) {
            Observed::Verified(root, evidence) => match root.dir.identity() {
                Ok(identity)
                    if self
                        .address
                        .state
                        .is_none_or(|expected| expected == identity) =>
                {
                    Observed::Verified(root, evidence)
                }
                _ => Observed::Unknown(UnknownReason::Untrusted(
                    "machine gate directory generation changed".into(),
                )),
            },
            Observed::Absent(_) if self.address.state.is_some() => Observed::Unknown(
                UnknownReason::Untrusted("retained machine gate directory disappeared".into()),
            ),
            other => other,
        }
    }
}
