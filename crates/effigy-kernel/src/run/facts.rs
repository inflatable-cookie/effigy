//! Atomic machine facts: no external effects, run admission or completed history.
use super::*;
use crate::gate::GateGuard;
use crate::ident;
use crate::observe::Observed;
use crate::store::BootDeadline;
use rusqlite::OptionalExtension;
use std::time::Duration;

#[derive(Debug)]
pub struct FactOptions {
    pub owner: ResourceId,
    pub wait: Wait,
    pub next: Invocation,
}

/// Exclusive authority for one shared model. Engine callbacks prepare their
/// complete value outside SQLite; publication changes all conflicts atomically.
#[derive(Debug)]
pub struct MachineFacts {
    store: Store,
    _guard: GateGuard,
    cx: EffectContext,
    owner: ResourceId,
}
impl MachineFacts {
    pub fn begin(
        cx: &EffectContext,
        root: &StateRoot,
        options: FactOptions,
        cleanup: &CleanupContext,
        adapter: &mut impl SettlementAdapter,
    ) -> Result<Self, RunError> {
        cx.require(crate::ctx::Capability::AdapterEffect)
            .map_err(|_| RunError::refused("machine facts require Mutate"))?;
        cx.require(crate::ctx::Capability::HostAuthority)
            .map_err(|_| {
                RunError::refused(
                    "a managed run needs host authority, and this placement is a guest",
                )
            })?;
        if !root.is_machine()
            || !root.writable()
            || !["gateway-endpoints", "ports"].iter().any(|name| {
                ResourceId::store(crate::id::Scope::Machine, name)
                    .ok()
                    .as_ref()
                    == Some(&options.owner)
            })
        {
            return Err(RunError::refused(
                "machine facts require an explicit shared-fact root and owner",
            ));
        }
        let mut guard =
            Gate::acquire(cx, root, std::slice::from_ref(&options.owner), options.wait)?;
        let mut store = Store::open(cx, root)?;
        settle::compact_machine_closed(&mut store, &options.owner, cleanup)?;
        // Upgrade recovery uses the same settlement rules as Run::begin,
        // without manufacturing a successor run merely to edit a fact.
        for predecessor in journal::predecessors(store.conn(), &options.owner)? {
            let mut previous = Run {
                root,
                store,
                guard,
                cx: cx.clone(),
                id: predecessor.id,
                owner: Fence {
                    resource: options.owner.clone(),
                    generation: predecessor.generation,
                },
                next: options.next.clone(),
                poisoned: false,
                completed: Default::default(),
                sentinel: None,
                closed_effects: None,
                machine_startup: None,
                checkout_custody: None,
                ancestor_closure: None,
                processes: Default::default(),
                session_launch: None,
            };
            previous.settle_inner(cleanup, adapter)?;
            let effects = previous
                .effects()?
                .into_iter()
                .filter(|effect| !effect.state.terminal())
                .collect::<Vec<_>>();
            if !effects.is_empty() && !predecessor.acknowledged {
                return Err(RunError::PredecessorUnsettled {
                    run: predecessor.id,
                    effects,
                });
            }
            store = previous.store;
            guard = previous.guard;
        }
        Ok(Self {
            store,
            _guard: guard,
            cx: cx.clone(),
            owner: options.owner,
        })
    }
    pub fn state(&self) -> Result<Option<Value>, RunError> {
        let state: Option<Option<String>> = self
            .store
            .conn()
            .query_row(
                "SELECT state FROM resources WHERE name=?1",
                [self.owner.name().to_string()],
                |row| row.get(0),
            )
            .optional()?;
        Ok(state
            .flatten()
            .map(|value| serde_json::from_str(&value))
            .transpose()?)
    }
    pub fn next_generation(&self) -> Result<Generation, RunError> {
        let current = self
            .store
            .conn()
            .query_row(
                "SELECT generation FROM resources WHERE name=?1",
                [self.owner.name().to_string()],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .unwrap_or(0);
        Generation::new(current as u64)
            .next()
            .ok_or_else(|| RunError::broken("machine fact generation overflow"))
    }
    /// The complete snapshot, retained endpoint claims and omitted-claim
    /// quarantine commit together. A refusal leaves every old value intact.
    pub fn publish_retained(
        &mut self,
        generation: Generation,
        desired: &[(ResourceId, Value)],
        payload: &Value,
        quarantine: Duration,
    ) -> Result<(), RunError> {
        let wanted = desired
            .iter()
            .map(|(id, _)| id.name().to_string())
            .collect::<std::collections::BTreeSet<_>>();
        if wanted.len() != desired.len()
            || desired.iter().any(|(id, _)| {
                id.kind() != crate::id::ResourceKind::Endpoint
                    || id.scope() != crate::id::Scope::Machine
            })
        {
            return Err(RunError::refused(
                "machine fact conflicts require unique machine endpoints",
            ));
        }
        let clock = ident::boot_clock();
        let owner = self.owner.clone();
        crate::fault_point!(ClaimPublishBefore)?;
        self.store.transaction(&self.cx.child(Duration::from_secs(30)), |txn| {
            let current = txn.resource(&owner, "machine-facts")?.generation;
            let expected = generation.get().checked_sub(1).map(Generation::new)
                .ok_or_else(|| RunError::broken("machine fact generation must advance"))?;
            if current != expected { return Err(RunError::StaleGeneration {
                expected: Fence { resource: owner.clone(), generation: expected }, current: Some(current) }); }
            txn.next_generation(&owner)?;
            let revision = i64::try_from(generation.get()).map_err(|_| RunError::broken("machine fact generation overflow"))?;
            crate::fault_point!(ClaimReserveBefore)?;
            for (resource, details) in desired {
                let row = txn.conn().query_row("SELECT holder,run_id,state,quarantine_boot,quarantine_nanos FROM claims WHERE resource=?1",
                    [resource.name().to_string()], |row| Ok((row.get::<_,Option<String>>(0)?,row.get::<_,Option<String>>(1)?,row.get::<_,String>(2)?,row.get::<_,Option<String>>(3)?,row.get::<_,Option<i64>>(4)?))).optional()?;
                if let Some((holder, run, state, boot, nanos)) = row {
                    let retained = holder.as_deref() == Some(&owner.name().to_string()) && run.is_none() && state == "live";
                    let expired = match (boot,nanos) {
                        (Some(boot),Some(nanos)) => matches!((BootDeadline { boot,nanos:nanos as u64 }).passed(&clock),Observed::Verified(true,_)),
                        _ => false,
                    };
                    if !retained && state != "released" && !(state == "quarantined" && expired) { return Err(RunError::refused(format!("resource `{}` is already claimed by another run", resource.label()))); }
                }
                txn.resource(resource, "claims")?;
                txn.conn().execute("INSERT INTO claims(resource,holder,run_id,reservation_token,state,generation,revision,details) VALUES (?1,?2,NULL,NULL,'live',?3,?3,?4) ON CONFLICT(resource) DO UPDATE SET holder=excluded.holder,run_id=NULL,reservation_token=NULL,state='live',generation=excluded.generation,revision=excluded.revision,details=excluded.details,quarantine_boot=NULL,quarantine_nanos=NULL",
                    params![resource.name().to_string(),owner.name().to_string(),revision,serde_json::to_string(details)?])?;
            }
            crate::fault_point!(ClaimReserveAfter)?;
            crate::fault_point!(ClaimRetainBefore)?;
            let mut query = txn.conn().prepare("SELECT resource FROM claims WHERE holder=?1 AND run_id IS NULL AND state='live'")?;
            let retained = query.query_map([owner.name().to_string()], |row| row.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
            drop(query);
            for resource in retained.into_iter().filter(|name| !wanted.contains(name)) {
                let Observed::Verified(now, _) = &clock else { return Err(RunError::unavailable("unknown quarantine clock")); };
                let deadline = BootDeadline::after(now, quarantine);
                let nanos = i64::try_from(deadline.nanos).map_err(|_| RunError::broken("quarantine overflow"))?;
                txn.conn().execute("UPDATE claims SET state='quarantined',revision=?2,quarantine_boot=?3,quarantine_nanos=?4 WHERE resource=?1",
                    params![resource,revision,deadline.boot,nanos])?;
            }
            txn.conn().execute("UPDATE resources SET state=?2,updated_at=?3 WHERE name=?1", params![owner.name().to_string(),serde_json::to_string(payload)?,txn.now()])?;
            crate::fault_point!(ClaimRetainAfter)?;
            Ok::<_,RunError>(())
        })?;
        crate::fault_point!(ClaimPublishAfter)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ctx::{EffectClass, OperationContext, Placement};
    use crate::id::{EndpointKind, Scope};
    use crate::run::tests::{cleanup, context, next, Fake};
    use serde_json::json;

    fn options(owner: &str) -> FactOptions {
        FactOptions {
            owner: ResourceId::store(Scope::Machine, owner).unwrap(),
            wait: Wait::FailFast,
            next: next(),
        }
    }
    #[test]
    fn guest_read_only_and_non_fact_roots_refuse_before_database_creation() {
        let home = effigy_test_support::Fixture::new("kernel-machine-facts-").unwrap();
        let cx = context();
        let root = StateRoot::runtime_machine_effects(&cx, home.path()).unwrap();
        let guest = OperationContext::new(
            cx.operation().deadline(),
            Default::default(),
            cx.operation().limits().clone(),
            EffectClass::Mutate,
            Placement::Guest,
        )
        .effects()
        .unwrap();
        let mut adapter = Fake::new(home.path());
        assert!(matches!(
            MachineFacts::begin(&guest, &root, options("ports"), &cleanup(&cx), &mut adapter),
            Err(RunError::Refused(rule)) if rule.contains("host authority")
        ));
        let read = StateRoot::machine_observe(&cx.operation().observe().unwrap(), home.path())
            .verified()
            .unwrap()
            .clone();
        assert!(
            MachineFacts::begin(&cx, &read, options("ports"), &cleanup(&cx), &mut adapter).is_err()
        );
        assert!(
            MachineFacts::begin(&cx, &root, options("tasks"), &cleanup(&cx), &mut adapter).is_err()
        );
        let checkout = StateRoot::effects(&cx, home.path()).unwrap();
        assert!(MachineFacts::begin(
            &cx,
            &checkout,
            options("ports"),
            &cleanup(&cx),
            &mut adapter
        )
        .is_err());
        assert!(!root.path().join("machine.db").exists());
        assert!(!root.path().join("kernel.db").exists());
    }
    #[test]
    fn complete_conflict_set_rolls_back_foreign_claims_and_generation() {
        let home = effigy_test_support::Fixture::new("kernel-machine-facts-").unwrap();
        let cx = context();
        let root = StateRoot::runtime_machine_effects(&cx, home.path()).unwrap();
        let mut adapter = Fake::new(home.path());
        let held = ResourceId::endpoint(EndpointKind::Domain, "held.test").unwrap();
        let free = ResourceId::endpoint(EndpointKind::Domain, "free.test").unwrap();
        let mut first = MachineFacts::begin(
            &cx,
            &root,
            options("gateway-endpoints"),
            &cleanup(&cx),
            &mut adapter,
        )
        .unwrap();
        first
            .publish_retained(
                first.next_generation().unwrap(),
                &[(held.clone(), json!({"owner":"first"}))],
                &json!({"state":"first"}),
                Duration::from_secs(70),
            )
            .unwrap();
        let mut second =
            MachineFacts::begin(&cx, &root, options("ports"), &cleanup(&cx), &mut adapter).unwrap();
        let wanted = [(free.clone(), json!({})), (held, json!({}))];
        assert!(matches!(
            second.publish_retained(
                second.next_generation().unwrap(),
                &wanted,
                &json!({"state":"second"}),
                Duration::from_secs(70)
            ),
            Err(RunError::Refused(_))
        ));
        assert_eq!(second.state().unwrap(), None);
        assert_eq!(second.next_generation().unwrap(), Generation::new(1));
        let free_rows: i64 = second
            .store
            .conn()
            .query_row(
                "SELECT count(*) FROM resources WHERE name=?1",
                [free.name().to_string()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(free_rows, 0, "partial conflict set escaped rollback");
        assert_eq!(first.state().unwrap(), Some(json!({"state":"first"})));
        let runs: i64 = first
            .store
            .conn()
            .query_row("SELECT count(*) FROM runs", [], |row| row.get(0))
            .unwrap();
        assert_eq!(runs, 0);
    }
    #[test]
    fn stale_publication_cannot_advance_the_model_or_release_live_claims() {
        let home = effigy_test_support::Fixture::new("kernel-machine-facts-").unwrap();
        let cx = context();
        let root = StateRoot::runtime_machine_effects(&cx, home.path()).unwrap();
        let mut facts = MachineFacts::begin(
            &cx,
            &root,
            options("ports"),
            &cleanup(&cx),
            &mut Fake::new(home.path()),
        )
        .unwrap();
        let endpoint = ResourceId::endpoint(EndpointKind::Domain, "fenced.test").unwrap();
        let desired = [(endpoint, json!({"owner":"same"}))];
        let generation = facts.next_generation().unwrap();
        facts
            .publish_retained(
                generation,
                &desired,
                &json!({"revision":1}),
                Duration::from_secs(70),
            )
            .unwrap();
        assert!(matches!(
            facts.publish_retained(
                generation,
                &[],
                &json!({"revision":0}),
                Duration::from_secs(70)
            ),
            Err(RunError::StaleGeneration { .. })
        ));
        assert_eq!(facts.state().unwrap(), Some(json!({"revision":1})));
        let live: i64 = facts
            .store
            .conn()
            .query_row(
                "SELECT count(*) FROM claims WHERE state='live'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(live, 1);
    }
}
