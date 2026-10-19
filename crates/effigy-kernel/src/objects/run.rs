//! The Run owns staging journal transitions and reference fencing.
use super::*;
use crate::ctx::Capability;
use crate::fs::FileName;
use crate::objects::{self, Closure, Objects};
use crate::root::Family;
use rusqlite::OptionalExtension;

impl Objects {
    /// Journal, stage and seal a closure, then atomically publish its row and
    /// desired reference. A crash selects the previous or the complete new
    /// closure. Settle removes abandoned staging and unreferenced publications.
    pub fn publish_desired(run: &mut Run<'_>, closure: &Closure) -> Result<String, RunError> {
        run.can_effect()?;
        run.cx
            .require(Capability::SourceStore)
            .map_err(|_| RunError::Invalid("object publication needs Prepare or Mutate".into()))?;
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
            run.root.dir().sync().map_err(StoreError::from)?;
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
            let observe = run
                .cx
                .operation()
                .child(std::time::Duration::from_secs(30))
                .observe()
                .map_err(|_| RunError::Invalid("object observation budget".into()))?;
            let published = objects::open(&objects, closure.digest(), &observe)?;
            if published.closure != *closure {
                return Err(RunError::Invalid("object collision".into()));
            }
            // macOS requires write permission on a moved directory while it
            // updates `..`. Seal the root after rename, before the row/ref
            // commit makes it published; descendants are already read-only.
            // A republisher completes this step after a pre-commit crash.
            let dir = objects
                .child(name.as_str(), crate::fs::Trust::Private, false)
                .map_err(StoreError::from)?
                .ok_or_else(|| RunError::Invalid("object vanished before sealing".into()))?;
            crate::fault_point!(ObjectRootSealBefore)?;
            let mode = dir.stat().map_err(StoreError::from)?.st_mode & 0o777;
            if mode == 0o700 {
                dir.chmod(0o500).map_err(StoreError::from)?;
                dir.sync().map_err(StoreError::from)?;
            } else if mode != 0o500 {
                return Err(RunError::Invalid("object root mode is untrusted".into()));
            }
            crate::fault_point!(ObjectRootSealAfter)?;
            crate::fault_point!(ObjectPublishBefore)?;
            run.store.transaction(&run.cx, |txn| {
                journal::check_fence(txn.conn(), &run.owner)?;
                txn.conn().execute("INSERT INTO objects(digest,kind,path,size,created_at) VALUES (?1,'closure-v1',?2,?3,?4) ON CONFLICT(digest) DO NOTHING",
                    params![closure.digest(), published.path.to_string_lossy(), closure.size() as i64, txn.now()])?;
                let matches: bool = txn.conn().query_row("SELECT kind='closure-v1' AND path=?2 AND size=?3 FROM objects WHERE digest=?1",
                    params![closure.digest(), published.path.to_string_lossy(), closure.size() as i64], |r| r.get(0))?;
                if !matches { return Err(RunError::Invalid("object row conflicts with closure".into())); }
                crate::fault_point!(ObjectRefBefore)?;
                set_desired(txn, &run.owner, closure.digest())?;
                journal::write_effect(txn, run.id, ticket.sequence, EffectState::Applied, Some(&Value::String(closure.digest().into())), None)?;
                Ok::<_, RunError>(())
            })?;
            crate::fault_point!(ObjectRefAfter)?;
            crate::fault_point!(ObjectPublishAfter)?;
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
        if run.cx.operation().class() != crate::ctx::EffectClass::Mutate {
            return Err(RunError::Invalid("object promotion needs Mutate".into()));
        }
        objects::validate_digest(expected)?;
        let _guard = objects::lock(&run.cx, run.root)?;
        let result = (|| {
            crate::fault_point!(ObjectRefBefore)?;
            run.store.transaction(&run.cx, |txn| {
                journal::check_fence(txn.conn(), &run.owner)?;
                let resource = run.owner.resource.name().to_string();
                let desired: Option<String> = txn.conn().query_row("SELECT digest FROM object_refs WHERE resource=?1 AND role='desired'", [&resource], |r| r.get(0)).optional()?;
                if desired.as_deref() != Some(expected) { return Err(RunError::Invalid("desired object changed before promotion".into())); }
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
        if run.cx.operation().class() != crate::ctx::EffectClass::Mutate {
            return Err(RunError::Invalid("object reset needs Mutate".into()));
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
            .map_err(|_| RunError::Invalid("backend metadata needs Prepare or Mutate".into()))?;
        if backend.engine.trim().is_empty()
            || backend.context.trim().is_empty()
            || backend.source.trim().is_empty()
        {
            return Err(RunError::Invalid(
                "backend metadata fields must be nonempty".into(),
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
                .ok_or_else(|| RunError::Invalid("staging digest missing".into()))?;
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
                    dir.remove_tree(&name, 20).map_err(StoreError::from)?;
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
