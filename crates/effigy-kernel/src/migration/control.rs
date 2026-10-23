//! Explicit legacy control is available only before cutover. Inspection never
//! calls this path; requesting a stop does not import or activate anything.
use super::*;
use crate::ctx::{Capability, EffectClass, EffectContext};
use crate::gate::Gate;
use crate::id::RunId;
use crate::run::{self, Fence, Phase, Request, RequestAction};

#[derive(Debug, Serialize)]
pub struct LegacyStop {
    pub run: RunId,
    pub queued: bool,
    pub owner_signalled: Option<Observed<u32>>,
}

pub fn request_legacy_stop(
    cx: &EffectContext,
    source: &StateRoot,
    run: RunId,
    request: RunId,
) -> Result<LegacyStop, StoreError> {
    let observe = legacy_control_context(cx, source)?;
    let source = source.legacy_control();
    // A checkpointed source needs no WAL index even for a no-op stop.
    // An explicit stop may read a live WAL; inspection/import never do so.
    let read = match Store::read_sealed(&observe, &source) {
        Observed::Unknown(UnknownReason::StoreNeedsRecovery) => Store::read(&observe, &source),
        other => other,
    };
    let reader = match read {
        Observed::Verified(reader, _) => reader,
        Observed::Absent(_) => return Err(StoreError::Corrupt("legacy journal is absent".into())),
        Observed::Unknown(reason) => return Err(inconsistent(reason)),
    };
    let summary = match run::summary(&reader, run) {
        Observed::Verified(summary, _) => summary,
        Observed::Absent(_) => return Err(StoreError::Corrupt("legacy run is absent".into())),
        Observed::Unknown(reason) => return Err(inconsistent(reason)),
    };
    if summary.phase == Phase::Closed {
        return Ok(LegacyStop {
            run,
            queued: false,
            owner_signalled: None,
        });
    }
    let owner = summary
        .invocation
        .ok_or_else(|| StoreError::Corrupt("legacy invocation is unreadable".into()))?
        .spec()
        .owner
        .clone();
    drop(reader);
    run::request(
        cx,
        &source,
        &Request {
            id: request,
            run,
            expected: Fence {
                resource: owner.clone(),
                generation: summary.generation,
            },
            action: RequestAction::Stop,
        },
    )
    .map_err(|error| StoreError::Corrupt(format!("legacy stop request: {error}")))?;
    // Rechecks the gate's run, boot and start identity immediately before
    // signalling. Neither a copied receipt nor an unverified PID grants it.
    let owner_signalled = Some(Gate::signal_holder(
        cx,
        &source,
        &owner,
        &run,
        libc::SIGTERM,
    ));
    Ok(LegacyStop {
        run,
        queued: true,
        owner_signalled,
    })
}

/// An exact writer invocation, including legacy admission/retention writers
/// that have not admitted a RunId. This is a diagnostic receipt, reverified
/// against the held gate and the OS immediately before signalling.
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct WriterTarget {
    pub scope: String,
    pub resource: String,
    pub run: Option<String>,
    pub process: ProcessIdentity,
}
impl WriterTarget {
    pub fn from_blocker(blocker: &Blocker) -> Option<Self> {
        if blocker.role != "gate holder" {
            return None;
        }
        Some(Self {
            scope: blocker.scope.clone(),
            resource: blocker.resource.clone(),
            run: blocker.run.clone(),
            process: blocker.process.verified()?.clone(),
        })
    }
}
pub fn stop_legacy_writer(
    cx: &EffectContext,
    source: &StateRoot,
    target: &WriterTarget,
) -> Result<Observed<u32>, StoreError> {
    let _observe = legacy_control_context(cx, source)?;
    if !(target.scope == "machine" || gate_name(&target.scope)) || !gate_name(&target.resource) {
        return Err(StoreError::Corrupt(
            "invalid legacy writer coordinates".into(),
        ));
    }
    let state = ident::process_state(target.process.pid);
    let signal = |signal| {
        gate::signal_named_holder(
            cx,
            source,
            &target.scope,
            &target.resource,
            gate::ExpectedHolder::Writer(&target.process, target.run.as_deref()),
            signal,
        )
    };
    let stopped = signal(libc::SIGTERM);
    if matches!(stopped, Observed::Verified(..))
        && matches!(state, Observed::Verified(ProcessState::Stopped, _))
    {
        // A pending TERM cannot run in a stopped process. Resume only this
        // same freshly matched gate holder so it can handle the requested stop.
        let _ = signal(libc::SIGCONT);
    }
    Ok(stopped)
}

fn legacy_control_context(
    cx: &EffectContext,
    source: &StateRoot,
) -> Result<ObserveContext, StoreError> {
    crate::inheritance::require_durable(cx.operation()).map_err(|error| {
        StoreError::from(crate::fs::FsError::io(
            "runtime class",
            source.path(),
            &error,
        ))
    })?;
    cx.require(Capability::HostAuthority)
        .map_err(|_| StoreError::HostAuthorityRequired)?;
    cx.require(Capability::AdapterEffect)
        .map_err(|_| StoreError::ClassRefused(cx.operation().class()))?;
    if cx.operation().class() != EffectClass::Mutate {
        return Err(StoreError::ClassRefused(cx.operation().class()));
    }
    let observe = cx
        .operation()
        .observe()
        .map_err(|_| StoreError::Unbounded)?;
    source.verify_binding()?;
    if committed(&observe, source.owning_directory().path())?.is_some() {
        return Err(StoreError::Corrupt(
            "legacy cutover is committed; archived gates grant no control authority".into(),
        ));
    }
    let machine = match StateRoot::machine_observe(&observe, source.owning_directory().path()) {
        Observed::Verified(root, _) => root,
        Observed::Absent(_) => {
            return Err(StoreError::Corrupt("legacy state root is absent".into()))
        }
        Observed::Unknown(reason) => return Err(inconsistent(reason)),
    };
    match authority(&observe,&machine) {
        Observed::Verified(Authority::Fresh,_)=>{},
        _=>return Err(StoreError::Corrupt("runtime publication is not fresh legacy authority; resume migration instead of controlling archived writers".into())),
    }
    if machine
        .dir()
        .entry(&FileName::new("machine.db")?)?
        .is_some()
        || !legacy_present(&observe, source)?
    {
        return Err(StoreError::Corrupt(
            "legacy writer authority is absent or ambiguous with an unpublished machine database"
                .into(),
        ));
    }
    Ok(observe)
}
