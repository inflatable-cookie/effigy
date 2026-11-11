// Read current host observations and their matching custody identities for fixtures.
use effigy_kernel::{ctx::EffectClass, observe::Observed, run, store::Store};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

pub(crate) fn read(path: &Path) -> Option<Value> {
    at_path(path).or_else(|| serde_json::from_slice(&fs::read(path).ok()?).ok())
}

pub(crate) fn at_path(path: &Path) -> Option<Value> {
    let repo = path
        .ancestors()
        .find(|p| p.file_name().is_some_and(|n| n == ".effigy"))?
        .parent()?;
    snapshots(repo)?.into_iter().find_map(|(managed, state)| {
        let plain = managed.parent()?.parent()?.join(managed.file_name()?);
        (path == managed || path == plain).then_some(state)
    })
}

pub(crate) fn runs(repo: &Path) -> Result<Vec<(run::RunSummary, Vec<run::Effect>)>, String> {
    let cx = effigy_gateway::operator::context(Duration::from_secs(10), EffectClass::Observe)
        .observe()
        .map_err(|error| error.to_string())?;
    let root = match effigy_containers::environment::observe_root(&cx, repo) {
        Observed::Verified(root, _) => root,
        Observed::Absent(_) => return Ok(Vec::new()),
        Observed::Unknown(reason) => return Err(format!("host journal: {reason:?}")),
    };
    let reader = match Store::read(&cx, &root) {
        Observed::Verified(reader, _) => reader,
        other => return Err(format!("host journal reader: {other:?}")),
    };
    let rows = match run::recent(&reader, None, usize::MAX / 2) {
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

pub(crate) fn snapshots(repo: &Path) -> Option<Vec<(PathBuf, Value)>> {
    let mut found = Vec::new();
    let mut seen = BTreeSet::new();
    for (run, effects) in runs(repo).ok()? {
        let Some(invocation) = run.invocation else {
            continue;
        };
        let owner = &invocation.spec().owner;
        if owner.kind() != effigy_kernel::id::ResourceKind::HostProcess
            || !seen.insert(owner.key().to_vec())
        {
            continue;
        }
        let [Some(environment), Some(profile), Some(process)] = owner.key() else {
            continue;
        };
        let Some(effect) = effects
            .iter()
            .rev()
            .find(|e| e.spec.kind == "host:lifecycle")
        else {
            continue;
        };
        let mut state = effect.spec.payload.clone();
        // Child assertions and cleanup use the exact generation's custody receipt.
        let terminal = matches!(
            state["status"].as_str(),
            Some("failed" | "stopped" | "owner_gone")
        );
        if state["custody_nonce"].is_string() && !terminal {
            let effect = effects
                .iter()
                .rev()
                .find(|effect| {
                    effect.spec.kind == "kernel:process"
                        && effect.receipt.as_ref().is_some_and(|receipt| {
                            receipt["nonce"] == state["custody_nonce"]
                        })
                })?;
            let process = effect.receipt.clone()?;
            let receipt: effigy_kernel::custody::ProcessReceipt =
                serde_json::from_value(process.clone()).ok()?;
            if state["run"] != run.id.to_string() || state["custody_nonce"] != receipt.nonce {
                continue;
            }
            if effect.state != run::EffectState::Released {
                state["process"] = process;
                state["child_pid"] = receipt.identity.pid.into();
                state["child_boot_identity"] = receipt.identity.boot.as_str().into();
                state["child_start_identity"] = match receipt.identity.start {
                    effigy_kernel::ident::ProcessStartIdentity::LinuxTicks(ticks) => {
                        serde_json::json!({"platform": "linux", "start_ticks": ticks})
                    }
                    effigy_kernel::ident::ProcessStartIdentity::MacOs {
                        seconds,
                        microseconds,
                    } => {
                        serde_json::json!({"platform": "macos", "start_seconds": seconds, "start_microseconds": microseconds})
                    }
                };
            } else if state["status"] == "ready" {
                continue;
            }
        }
        let path = path(repo, environment, profile, process);
        found.push((path, state));
    }
    Some(found)
}

pub(crate) fn path(repo: &Path, environment: &str, profile: &str, process: &str) -> PathBuf {
    let safe = |value: &str| {
        value.replace(
            |c: char| !c.is_ascii_alphanumeric() && !matches!(c, '-' | '_' | '.'),
            "-",
        )
    };
    repo.join(".effigy/runtime/host-processes")
        .join(safe(environment))
        .join(safe(profile))
        .join(format!("{}.listener.json", safe(process)))
}
