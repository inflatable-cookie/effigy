//! The Run owns staging journal transitions and reference fencing.
use super::*;
use crate::ctx::Capability;
use crate::fs::FileName;
use crate::objects::{self, Closure, Objects, SealedPreflight};
use crate::root::Family;
use rusqlite::OptionalExtension;

/// Bound for one engine record: a small policy document, never a payload.
const ENGINE_RECORD_MAX: usize = 16 << 10;

impl Objects {
    /// Journal, stage and publish a closure, then atomically publish its row and
    /// desired reference. A crash selects the previous or the complete new
    /// closure. Settle removes abandoned staging and unreferenced publications.
    pub fn publish_desired(run: &mut Run<'_>, closure: &Closure) -> Result<String, RunError> {
        Self::publish(run, closure, None)
    }
    /// Publish exactly the closure a preflight seal verified, taking its private
    /// verified tree as the source instead of staging the bytes again.
    ///
    /// This is the same fenced publisher — one journalled intent, one objects
    /// gate, one reference transaction — so canonical promotion still happens
    /// only here, under the caller's `Gate(Env)`, and never in the seal. The
    /// receipt is re-verified under the gate before it is moved: its digest,
    /// bytes and every entry's private mode must still be exactly the verified
    /// inventory it carries, so a private tree altered after sealing never
    /// becomes canonical. A receipt whose tree
    /// a completed publication already consumed, or whose tree is gone, costs a
    /// fresh stage rather than a failure.
    pub fn publish_sealed_desired(
        run: &mut Run<'_>,
        sealed: &SealedPreflight,
        closure: &Closure,
    ) -> Result<String, RunError> {
        // A receipt belongs to the environment activation that owns the render,
        // and it is consumed only with that environment's gate held: a run's
        // owner gate is acquired at begin and retained for the run's life, so a
        // run owned by anything else (or one whose environment gate is no longer
        // held) can never promote a preflight seal.
        if run.owner.resource.kind() != crate::id::ResourceKind::Env {
            return Err(RunError::refused(
                "a preflight receipt is published only under Gate(Env)",
            ));
        }
        if !run.guard.ids().any(|held| held == &run.owner.resource) {
            return Err(RunError::refused(
                "publishing a preflight receipt needs its environment gate",
            ));
        }
        if !sealed.matches(closure) {
            return Err(RunError::refused(
                "the preflight seal does not describe this closure",
            ));
        }
        Self::publish(run, closure, Some(sealed))
    }

    fn publish(
        run: &mut Run<'_>,
        closure: &Closure,
        sealed: Option<&SealedPreflight>,
    ) -> Result<String, RunError> {
        run.can_effect()?;
        if run.root.is_machine() {
            return Err(RunError::refused(
                "immutable objects belong in a checkout journal",
            ));
        }
        run.cx
            .require(Capability::SourceStore)
            .map_err(|_| RunError::refused("object publication needs Prepare or Mutate"))?;
        let ticket = run.intend_inner(EffectSpec {
            target: run.owner.clone(),
            kind: objects::KIND.into(),
            payload: Value::String(closure.digest().into()),
            settlement: Settlement::Release(Value::Null),
            required: false,
        })?;
        let result = (|| {
            let _guard = objects::lock(&run.cx, run.root)?;
            let name = objects::validate_digest(closure.digest())?;
            let objects = run
                .root
                .family(Family::Objects)
                .map_err(StoreError::from)?
                .expect("created family");
            let staging = run
                .root
                .family(Family::Staging)
                .map_err(StoreError::from)?
                .expect("created family");
            let observe = run
                .cx
                .operation()
                .child(std::time::Duration::from_secs(30))
                .observe()
                .map_err(|_| RunError::unavailable("object observation budget"))?;
            // A preflight receipt names a private, already written tree; it is
            // consumed here or nowhere.
            let sealed_tree = match sealed {
                Some(sealed) => sealed.stage(run.root)?.map(|tree| (sealed, tree)),
                None => None,
            };
            run.root.dir().sync().map_err(StoreError::from)?;
            if let Some((sealed, tree)) = &sealed_tree {
                // Verified again under this gate: a private tree altered
                // after sealing never becomes canonical.
                let verified = objects::open(tree, closure.digest(), &observe)?;
                if verified.closure != *closure {
                    return Err(RunError::broken("the sealed preflight is not this closure"));
                }
                // The receipt's inventory pins the bytes *and* the private
                // modes exactly as they were verified at seal time. A tree
                // whose bytes still hash the same but whose modes drifted
                // (a file left 0700, say, which is still private) is not the
                // sealed artifact and must never become canonical.
                if verified.inventory != sealed.inventory() {
                    return Err(RunError::broken("the sealed preflight inventory changed"));
                }
                if objects.entry(&name).map_err(StoreError::from)?.is_none() {
                    crate::fault_point!(ObjectRenameBefore)?;
                    #[cfg(test)]
                    crate::objects::preflight::test_hooks::promote_point();
                    if !tree
                        .move_to_exclusive(&name, &objects, &name)
                        .map_err(StoreError::from)?
                    {
                        // Another publication won the name; trust only the
                        // verifier's reading of it.
                        let winner = objects::open(&objects, closure.digest(), &observe)?;
                        if winner.closure != *closure {
                            return Err(RunError::broken("object collision"));
                        }
                    }
                    objects.sync().map_err(StoreError::from)?;
                    crate::fault_point!(ObjectRenameAfter)?;
                }
            } else {
                crate::fault_point!(ObjectStageBefore)?;
                let stage = staging
                    .child(&run.id.to_string(), crate::fs::Trust::Private, true)
                    .map_err(StoreError::from)?
                    .expect("created staging");
                crate::fault_point!(ObjectStageCreated)?;
                if objects.entry(&name).map_err(StoreError::from)?.is_none() {
                    let dir = stage
                        .child(name.as_str(), crate::fs::Trust::Private, true)
                        .map_err(StoreError::from)?
                        .expect("created closure");
                    crate::fault_point!(ObjectSealBefore)?;
                    objects::write(&dir, closure, &run.cx)?;
                    crate::fault_point!(ObjectSealAfter)?;
                    stage.sync().map_err(StoreError::from)?;
                    staging.sync().map_err(StoreError::from)?;
                    crate::fault_point!(ObjectRenameBefore)?;
                    stage
                        .move_to(&name, &objects, &name)
                        .map_err(StoreError::from)?;
                    objects.sync().map_err(StoreError::from)?;
                    stage.sync().map_err(StoreError::from)?;
                    crate::fault_point!(ObjectRenameAfter)?;
                }
            }
            let published = objects::open(&objects, closure.digest(), &observe)?;
            if published.closure != *closure {
                return Err(RunError::broken("object collision"));
            }
            if let Some(sealed) = sealed {
                // Canonical objects, including existing and concurrent winners,
                // must describe the exact bytes and modes the receipt verified.
                if published.inventory != sealed.inventory() {
                    return Err(RunError::broken(
                        "the canonical preflight inventory changed",
                    ));
                }
            }
            // The private seal tree is this publication's to remove, under the
            // gate it holds, whether it was the source or already canonical.
            if let Some(sealed) = sealed {
                #[cfg(test)]
                crate::objects::preflight::test_hooks::consume_point();
                crate::objects::preflight::consume(&staging, sealed.token())?;
            }
            crate::fault_point!(ObjectPublishBefore)?;
            run.store.transaction(&run.cx, |txn| {
                journal::check_fence(txn.conn(), &run.owner)?;
                txn.conn().execute("INSERT INTO objects(digest,kind,path,size,created_at) VALUES (?1,'closure-v1',?2,?3,?4) ON CONFLICT(digest) DO NOTHING",
                    params![closure.digest(), published.path.to_string_lossy(), closure.size() as i64, txn.now()])?;
                let matches: bool = txn.conn().query_row("SELECT kind='closure-v1' AND path=?2 AND size=?3 FROM objects WHERE digest=?1",
                    params![closure.digest(), published.path.to_string_lossy(), closure.size() as i64], |r| r.get(0))?;
                if !matches { return Err(RunError::broken("object row conflicts with closure")); }
                crate::fault_point!(ObjectRefBefore)?;
                set_desired(txn, &run.owner, closure.digest())?;
                journal::write_effect(txn, run.id, ticket.sequence, EffectState::Applied, Some(&Value::String(closure.digest().into())), None)?;
                Ok::<_, RunError>(())
            })?;
            crate::fault_point!(ObjectRefAfter)?;
            crate::fault_point!(ObjectPublishAfter)?;
            // Dead seal trees are reclaimed here, under this publication's
            // objects gate and after its fenced commit: one provably dead tree
            // per publication keeps the gate-held critical section bounded, and
            // recovery is infallible and post-commit so no residue state — an
            // over-full family, a trust refusal, an I/O error — can turn this
            // committed publication into a failure.
            crate::objects::preflight::recover(&staging);
            Ok(closure.digest().to_owned())
        })();
        if result.is_err() {
            run.poisoned = true;
        }
        result
    }
    /// Promote exactly the desired digest the backend applied. The owner gate
    /// and generation fence serialize promotions; an outdated digest refuses.
    pub fn promote(run: &mut Run<'_>, expected: &str) -> Result<(), RunError> {
        run.can_effect()?;
        if run.root.is_machine() {
            return Err(RunError::refused(
                "immutable objects belong in a checkout journal",
            ));
        }
        if run.cx.operation().class() != crate::ctx::EffectClass::Mutate {
            return Err(RunError::refused("object promotion needs Mutate"));
        }
        objects::validate_digest(expected)?;
        let _guard = objects::lock(&run.cx, run.root)?;
        let result = (|| {
            crate::fault_point!(ObjectRefBefore)?;
            run.store.transaction(&run.cx, |txn| {
                journal::check_fence(txn.conn(), &run.owner)?;
                let resource = run.owner.resource.name().to_string();
                let desired: Option<String> = txn.conn().query_row("SELECT digest FROM object_refs WHERE resource=?1 AND role='desired'", [&resource], |r| r.get(0)).optional()?;
                if desired.as_deref() != Some(expected) { return Err(RunError::broken("desired object changed before promotion")); }
                let seq = sequence(txn, &resource)?;
                txn.conn().execute("INSERT INTO object_refs(digest,resource,role,seq) SELECT digest,resource,'superseded',?2 FROM object_refs WHERE resource=?1 AND role='last_applied' AND digest<>?3 ON CONFLICT(digest,resource,role) DO UPDATE SET seq=excluded.seq", params![resource, seq, expected])?;
                txn.conn().execute("DELETE FROM object_refs WHERE resource=?1 AND (role='last_applied' OR (role='superseded' AND digest=?2))", params![resource, expected])?;
                txn.conn().execute("INSERT INTO object_refs(digest,resource,role,seq) VALUES (?1,?2,'last_applied',?3)", params![expected, resource, seq])?;
                txn.conn().execute("UPDATE resources SET last_applied=?2,updated_at=?3 WHERE name=?1", params![resource, expected, txn.now()])?;
                Ok::<_, RunError>(())
            })?;
            crate::fault_point!(ObjectRefAfter)?;
            Ok(())
        })();
        if result.is_err() {
            run.poisoned = true;
        }
        result
    }
    /// Per-resource reset drops only this owner's references. Physical object
    /// reclamation remains bounded retention; shared references stay protected.
    pub fn reset(run: &mut Run<'_>) -> Result<(), RunError> {
        run.can_effect()?;
        if run.root.is_machine() {
            return Err(RunError::refused(
                "immutable objects belong in a checkout journal",
            ));
        }
        if run.cx.operation().class() != crate::ctx::EffectClass::Mutate {
            return Err(RunError::refused("object reset needs Mutate"));
        }
        let _guard = objects::lock(&run.cx, run.root)?;
        crate::fault_point!(ObjectRefBefore)?;
        run.store.transaction(&run.cx, |txn| {
            journal::check_fence(txn.conn(), &run.owner)?;
            if run.owner.resource.kind() == crate::id::ResourceKind::Env {
                super::attachments::ensure_no_holders(txn.conn(), &run.owner.resource)?;
            }
            let resource = run.owner.resource.name().to_string();
            txn.conn()
                .execute("DELETE FROM object_refs WHERE resource=?1", [&resource])?;
            txn.conn().execute(
                "UPDATE resources SET desired=NULL,last_applied=NULL,updated_at=?2 WHERE name=?1",
                params![resource, txn.now()],
            )?;
            Ok::<_, RunError>(())
        })?;
        crate::fault_point!(ObjectRefAfter)?;
        Ok(())
    }
}
fn sequence(txn: &crate::store::Txn<'_>, resource: &str) -> Result<i64, RunError> {
    Ok(txn.conn().query_row(
        "SELECT coalesce(max(seq),0)+1 FROM object_refs WHERE resource=?1",
        [resource],
        |r| r.get(0),
    )?)
}
fn set_desired(
    txn: &mut crate::store::Txn<'_>,
    fence: &Fence,
    digest: &str,
) -> Result<(), RunError> {
    let resource = fence.resource.name().to_string();
    let seq = sequence(txn, &resource)?;
    txn.conn().execute(
        "DELETE FROM object_refs WHERE resource=?1 AND role='desired'",
        [&resource],
    )?;
    txn.conn().execute(
        "INSERT INTO object_refs(digest,resource,role,seq) VALUES (?1,?2,'desired',?3)",
        params![digest, resource, seq],
    )?;
    txn.conn().execute(
        "UPDATE resources SET desired=?2,updated_at=?3 WHERE name=?1",
        params![resource, digest, txn.now()],
    )?;
    Ok(())
}
impl Run<'_> {
    /// Update exact backend metadata under the owning generation's gate.
    pub fn set_backend(&mut self, backend: &crate::store::BackendMetadata) -> Result<(), RunError> {
        self.can_effect()?;
        self.cx
            .require(Capability::SourceStore)
            .map_err(|_| RunError::refused("backend metadata needs Prepare or Mutate"))?;
        if backend.engine.trim().is_empty()
            || backend.context.trim().is_empty()
            || backend.source.trim().is_empty()
        {
            return Err(RunError::refused(
                "backend metadata fields must be nonempty",
            ));
        }
        self.store.transaction(&self.cx, |txn| {
            journal::check_fence(txn.conn(), &self.owner)?;
            txn.conn().execute(
                "UPDATE resources SET backend=?2,updated_at=?3 WHERE name=?1",
                params![
                    self.owner.resource.name().to_string(),
                    serde_json::to_string(backend)?,
                    txn.now()
                ],
            )?;
            Ok(())
        })
    }

    /// Replace the authority engine's own record under the owning
    /// generation's gate. The kernel stores it opaquely and bounds its size;
    /// it never grants authority by itself.
    pub fn set_engine_record(&mut self, record: &Value) -> Result<(), RunError> {
        self.can_effect()?;
        self.cx
            .require(Capability::SourceStore)
            .map_err(|_| RunError::refused("engine record needs Prepare or Mutate"))?;
        let encoded = serde_json::to_string(record)?;
        if encoded.len() > ENGINE_RECORD_MAX {
            return Err(StoreError::TooLarge {
                limit: ENGINE_RECORD_MAX as u64,
            }
            .into());
        }
        let result = (|| {
            crate::fault_point!(EngineRecordBefore)?;
            self.store.transaction(&self.cx, |txn| {
                journal::check_fence(txn.conn(), &self.owner)?;
                txn.conn().execute(
                    "UPDATE resources SET engine=?2,updated_at=?3 WHERE name=?1",
                    params![self.owner.resource.name().to_string(), encoded, txn.now()],
                )?;
                Ok::<_, RunError>(())
            })?;
            crate::fault_point!(EngineRecordAfter)?;
            Ok(())
        })();
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }

    pub(super) fn settle_staging(
        &mut self,
        effect: &Effect,
        cx: &EffectContext,
    ) -> Result<(), RunError> {
        let result = (|| -> Result<(), RunError> {
            let digest = effect
                .spec
                .payload
                .as_str()
                .ok_or_else(|| RunError::broken("staging digest missing"))?;
            let name = objects::validate_digest(digest)?;
            let _guard = objects::lock(cx, self.root)?;
            crate::fault_point!(ObjectCleanupBefore)?;
            if let Some(staging) = self
                .root
                .read_family(Family::Staging)
                .map_err(StoreError::from)?
            {
                staging
                    .remove_tree(
                        &FileName::new(&self.id.to_string()).map_err(StoreError::from)?,
                        crate::fs::Trust::Private,
                        20,
                    )
                    .map_err(StoreError::from)?;
                staging.sync().map_err(StoreError::from)?;
            }
            let referenced: bool = self.store.conn().query_row(
                "SELECT EXISTS(SELECT 1 FROM object_refs WHERE digest=?1)",
                [digest],
                |r| r.get(0),
            )?;
            if !referenced {
                // No publisher or retention can intervene while this gate is held.
                if let Some(dir) = self
                    .root
                    .read_family(Family::Objects)
                    .map_err(StoreError::from)?
                {
                    dir.remove_tree(&name, crate::fs::Trust::Private, 20)
                        .map_err(StoreError::from)?;
                    dir.sync().map_err(StoreError::from)?;
                }
                self.store.txn(cx, |txn| {
                    txn.conn()
                        .execute("DELETE FROM objects WHERE digest=?1", [digest])?;
                    Ok(())
                })?;
            }
            crate::fault_point!(ObjectCleanupAfter)?;
            Ok(())
        })();
        let (state, reason) = match result {
            Ok(()) => (EffectState::Released, None),
            Err(e) => (EffectState::Residue, Some(e.to_string())),
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
