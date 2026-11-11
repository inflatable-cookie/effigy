//! Journal observations for private host fixtures. The path identifies the
//! declared process; no new supervisor writes a listener file there.
use super::*;
use effigy_kernel::ctx::*;
use effigy_kernel::observe::Observed;
use effigy_kernel::run::{self, Effect, RunSummary};

pub(super) fn runs(checkout: &Path) -> Result<Vec<(RunSummary, Vec<Effect>)>, String> {
    let observe = OperationContext::new(
        Deadline::At(Instant::now() + Duration::from_secs(2)),
        CancelToken::default(),
        Limits {
            capture_tail: 1024,
            log_rotate: 4096,
            log_keep: 2,
            line_max: 512,
            queue_bytes: 2048,
            probe_concurrency: 1,
        },
        EffectClass::Observe,
        Placement::Host,
    )
    .observe()
    .map_err(|error| error.to_string())?;
    let root = match effigy_containers::environment::observe_root(&observe, checkout) {
        Observed::Verified(root, _) => root,
        Observed::Absent(_) => return Ok(Vec::new()),
        Observed::Unknown(reason) => return Err(format!("host journal: {reason:?}")),
    };
    let reader = match effigy_kernel::store::Store::read(&observe, &root) {
        Observed::Verified(reader, _) => reader,
        other => return Err(format!("host journal reader: {other:?}")),
    };
    let rows = match run::recent(&reader, None, 1000) {
        Observed::Verified(rows, _) => rows,
        other => return Err(format!("host runs: {other:?}")),
    };
    rows.into_iter()
        .filter(|run| {
            run.invocation.as_ref().is_some_and(|invocation| {
                invocation.spec().owner.kind() == effigy_kernel::id::ResourceKind::HostProcess
            })
        })
        .map(|row| match run::effects(&reader, row.id) {
            Observed::Verified(effects, _) => Ok((row, effects)),
            other => Err(format!("host effects: {other:?}")),
        })
        .collect()
}

pub(super) fn listener(path: &Path) -> Result<Option<Value>, String> {
    let checkout = path
        .ancestors()
        .find(|part| part.file_name().is_some_and(|name| name == ".effigy"))
        .and_then(Path::parent)
        .ok_or("listener checkout missing")?;
    for (run, effects) in runs(checkout)? {
        let owner = &run
            .invocation
            .as_ref()
            .expect("host invocation")
            .spec()
            .owner;
        let key = owner.key();
        let environment = key[0].as_deref().ok_or("host environment missing")?;
        let profile = key[1].as_deref().ok_or("host profile missing")?;
        let process = key[2].as_deref().ok_or("host process missing")?;
        let directory = checkout
            .join(".effigy/runtime/host-processes")
            .join(environment);
        let filename = format!("{process}.listener.json");
        if path != directory.join(profile).join(&filename) && path != directory.join(&filename) {
            continue;
        }
        let Some(effect) = effects
            .iter()
            .rev()
            .find(|effect| effect.spec.kind == "host:lifecycle")
        else {
            return Ok(None);
        };
        let mut state = effect.spec.payload.clone();
        if let Some(nonce) = state["custody_nonce"].as_str() {
            let process = effects
                .iter()
                .rev()
                .filter(|effect| effect.spec.kind == "kernel:process")
                .filter_map(|effect| effect.receipt.as_ref())
                .find(|receipt| receipt["nonce"].as_str() == Some(nonce))
                .cloned();
            state["process"] = process.unwrap_or(Value::Null);
        }
        return Ok(Some(state));
    }
    Ok(None)
}
