//! Oracles O1–O14 (§12.1) over a captured `World`. Each returns how many
//! subjects it judged, so the driver can refuse a vacuous pass, and each has
//! a negative control below that proves it can fail.
use super::world::World;
use super::*;
use crate::fault::Event;
use crate::gate::GateState;
use crate::ident::{self, ProcessState};
use crate::run::RunStatus;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Oracle {
    O1a,
    O1b,
    O2,
    O3,
    O4,
    O5,
    O6,
    O7,
    O8,
    O9,
    O10,
    O11,
    O12,
    O13,
    O14,
}
impl Oracle {
    pub(super) const ALL: [Oracle; 15] = [
        Oracle::O1a,
        Oracle::O1b,
        Oracle::O2,
        Oracle::O3,
        Oracle::O4,
        Oracle::O5,
        Oracle::O6,
        Oracle::O7,
        Oracle::O8,
        Oracle::O9,
        Oracle::O10,
        Oracle::O11,
        Oracle::O12,
        Oracle::O13,
        Oracle::O14,
    ];
    pub(super) fn check(self, world: &World) -> Result<usize, String> {
        match self {
            Oracle::O1a => o1a(world),
            Oracle::O1b => o1b(world),
            Oracle::O2 => o2(world),
            Oracle::O3 => o3(world),
            Oracle::O4 => o4(world),
            Oracle::O5 => o5(world),
            Oracle::O6 => o6(world),
            Oracle::O7 => o7(world),
            Oracle::O8 => o8(world),
            Oracle::O9 => o9(world),
            Oracle::O10 => o10(world),
            Oracle::O11 => o11(world),
            Oracle::O12 => o12(world),
            Oracle::O13 => o13(world),
            Oracle::O14 => o14(world),
        }
    }
}
fn closed(world: &World, complete: bool) -> impl Iterator<Item = &super::world::RunRow> {
    world.runs.iter().filter(move |run| {
        run.phase == "closed" && (run.cleanup.as_deref() == Some("complete")) == complete
    })
}
/// Owned processes of `run` that are still alive: a recorded leader with its
/// start identity, a member of a recorded group whose leader is ours or gone,
/// or any announced process carrying the run's custody nonce.
fn survivors(world: &World, run: crate::id::RunId) -> Vec<(Option<i64>, String)> {
    let mut alive = vec![];
    for custody in world.custody.iter().filter(|c| c.run == run) {
        let Some(receipt) = &custody.receipt else {
            continue;
        };
        let leader = ident::process_identity(receipt.identity.pid);
        let ours = match &leader {
            Observed::Verified(identity, _) => *identity == receipt.identity,
            Observed::Absent(_) => false,
            Observed::Unknown(reason) => {
                alive.push((
                    Some(custody.sequence),
                    format!("leader {} unknown: {reason:?}", receipt.pgid),
                ));
                continue;
            }
        };
        if ours && super::world::identity_alive(&receipt.identity) {
            alive.push((
                Some(custody.sequence),
                format!("leader {} alive", receipt.pgid),
            ));
        }
        let reused =
            matches!(&leader, Observed::Verified(identity, _) if *identity != receipt.identity);
        if !reused {
            let members = crate::custody::platform::group_pids(receipt.pgid).unwrap_or_default();
            let members: Vec<u32> = members
                .into_iter()
                .filter(|pid| {
                    !matches!(
                        ident::process_state(*pid),
                        Observed::Absent(_) | Observed::Verified(ProcessState::Zombie, _)
                    )
                })
                .collect();
            if !members.is_empty() {
                alive.push((
                    Some(custody.sequence),
                    format!("group {} members {members:?}", receipt.pgid),
                ));
            }
        }
    }
    let prefix = format!("{run}:");
    for process in world.workloads.iter().filter(|p| p.alive) {
        if let Some(sequence) = process
            .nonce
            .as_deref()
            .and_then(|nonce| nonce.strip_prefix(&prefix))
        {
            alive.push((
                sequence.parse().ok(),
                format!("pid {} carries nonce {prefix}{sequence}", process.pid),
            ));
        }
    }
    alive
}
fn o1a(world: &World) -> Result<usize, String> {
    let mut judged = 0;
    for run in closed(world, true) {
        judged += 1;
        let alive = survivors(world, run.id);
        if !alive.is_empty() {
            return Err(format!(
                "O1a: complete run {} has live owned processes: {alive:?}",
                run.id
            ));
        }
    }
    Ok(judged)
}
fn o1b(world: &World) -> Result<usize, String> {
    let mut judged = 0;
    for run in closed(world, false) {
        judged += 1;
        match world.statuses.get(&run.id.to_string()) {
            Some(Observed::Verified(
                RunStatus::Closed {
                    complete: false, ..
                },
                _,
            )) => {}
            other => return Err(format!("O1b: incomplete run {} reads {other:?}", run.id)),
        }
        let effects = &world.effects[&run.id.to_string()];
        for (sequence, what) in survivors(world, run.id) {
            let listed = sequence.is_some_and(|sequence| {
                effects
                    .iter()
                    .any(|e| e.sequence == sequence && !e.state.terminal())
            });
            if !listed {
                return Err(format!(
                    "O1b: run {} survivor not listed as residue: {what}",
                    run.id
                ));
            }
        }
        if !run.acknowledged && world.successor_blocked.get(&run.id.to_string()) != Some(&true) {
            return Err(format!(
                "O1b/K5: unacknowledged residue of {} did not block a successor",
                run.id
            ));
        }
        for effect in effects
            .iter()
            .filter(|e| e.spec.kind == "kernel:attachment" && !e.state.terminal())
        {
            let env = effect.spec.target.resource.name().to_string();
            let run_id = run.id.to_string();
            if !world
                .attachments
                .iter()
                .any(|(e, r, state)| *e == env && *r == run_id && state != "released")
            {
                return Err(format!(
                    "O1b/R4: residue attachment of {} lost its hold",
                    run.id
                ));
            }
        }
    }
    Ok(judged)
}
fn o2(world: &World) -> Result<usize, String> {
    for run in &world.runs {
        if run.phase == "closed" {
            continue;
        }
        match world.statuses.get(&run.id.to_string()) {
            Some(Observed::Verified(RunStatus::Interrupted { next, .. }, _))
                if valid_next(next.argv()) => {}
            other => {
                return Err(format!(
                    "O2: run {} is {} and reads {other:?}",
                    run.id, run.phase
                ))
            }
        }
    }
    Ok(world.runs.len())
}
fn o3(world: &World) -> Result<usize, String> {
    let mut open = BTreeMap::<&str, usize>::new();
    for run in &world.runs {
        let count = open.entry(&run.owner).or_default();
        if run.phase != "closed" {
            *count += 1;
        }
    }
    match open.iter().find(|(_, count)| **count > 1) {
        Some((owner, count)) => Err(format!("O3: {count} non-closed runs for {owner}")),
        None => Ok(open.len()),
    }
}
fn o4(world: &World) -> Result<usize, String> {
    for gate in &world.gates {
        match &gate.observed {
            Observed::Verified(GateState::Free, _) | Observed::Absent(_) => {}
            Observed::Verified(GateState::Held(diag), _) => {
                // The body is diagnostics only (I-G2) and is stale between
                // flock and rewrite, so a hold is attributed to the declared
                // live holders; the post-exit capture proves the gate frees.
                let live = world.live_holders.iter().any(|pid| {
                    !matches!(
                        ident::process_state(*pid),
                        Observed::Absent(_) | Observed::Verified(ProcessState::Zombie, _)
                    )
                });
                if !live {
                    return Err(format!(
                        "O4: gate {} held with no live declared holder (live: {:?}): {diag:?}",
                        gate.file, world.live_holders
                    ));
                }
            }
            Observed::Unknown(reason) => {
                return Err(format!("O4: gate {} unreadable: {reason:?}", gate.file))
            }
        }
    }
    if !world.scopes_held.is_empty() && world.live_holders.is_empty() {
        return Err(format!("O4: scope gates held: {:?}", world.scopes_held));
    }
    Ok(world.gates.len())
}
fn o5(world: &World) -> Result<usize, String> {
    let mut seen = BTreeMap::<&str, usize>::new();
    for call in &world.calls {
        if call.starts_with("apply ") || call.starts_with("release ") || call.starts_with("env:") {
            *seen.entry(call.as_str()).or_default() += 1;
        }
    }
    if let Some((call, count)) = seen.iter().find(|(_, count)| **count > 1) {
        return Err(format!(
            "O5: non-idempotent call applied {count} times: {call}"
        ));
    }
    let mut releases = BTreeMap::<(&str, &str), usize>::new();
    for (env, run) in &world.releases {
        *releases.entry((env, run)).or_default() += 1;
    }
    for (env, run, state) in &world.attachments {
        let count = releases
            .get(&(env.as_str(), run.as_str()))
            .copied()
            .unwrap_or(0);
        let expected = usize::from(state == "released");
        if count != expected {
            return Err(format!(
                "O5: attachment {env}/{run} ({state}) released {count} times"
            ));
        }
    }
    if releases.len()
        != world
            .attachments
            .iter()
            .filter(|(_, _, state)| state == "released")
            .count()
    {
        return Err("O5: release audit names an unknown attachment".into());
    }
    let mut claims = BTreeMap::<(&str, Option<&str>), usize>::new();
    for (resource, token) in &world.claim_releases {
        *claims.entry((resource, token.as_deref())).or_default() += 1;
    }
    if let Some((claim, count)) = claims.iter().find(|(_, count)| **count > 1) {
        return Err(format!("O5: claim {claim:?} released {count} times"));
    }
    Ok(seen.len() + world.attachments.len() + claims.len())
}
fn o6(world: &World) -> Result<usize, String> {
    match &world.integrity {
        Some(Ok(())) => Ok(1),
        Some(Err(error)) => Err(format!("O6: kernel.db: {error}")),
        None => Ok(0),
    }
}
fn o7(world: &World) -> Result<usize, String> {
    for (name, allowed, actual) in &world.replaced {
        if !actual.as_ref().is_some_and(|body| allowed.contains(body)) {
            return Err(format!("O7: {name} is neither generation: {actual:?}"));
        }
    }
    Ok(world.replaced.len())
}
fn o8(world: &World) -> Result<usize, String> {
    let mut last = BTreeMap::<&str, bool>::new();
    let mut judged = 0;
    for event in &world.events {
        match event {
            Event::Observed { subject, unknown } => {
                last.insert(subject, *unknown);
            }
            Event::Destructive { subject } => {
                judged += 1;
                match last.get(subject.as_str()) {
                    Some(false) => {}
                    Some(true) => return Err(format!("O8: destructive call on Unknown {subject}")),
                    None => return Err(format!("O8: destructive call on unobserved {subject}")),
                }
            }
        }
    }
    Ok(judged)
}
fn o9(world: &World) -> Result<usize, String> {
    for (label, baseline, faulted) in &world.outcomes {
        if baseline != faulted {
            return Err(format!(
                "O9: observability fault at {label} changed exit {baseline} to {faulted}"
            ));
        }
    }
    Ok(world.outcomes.len())
}
fn o10(world: &World) -> Result<usize, String> {
    for (label, diff) in &world.diffs {
        if !diff.is_empty() {
            return Err(format!("O10: {label} changed the filesystem: {diff:?}"));
        }
    }
    Ok(world.diffs.len())
}
fn o11(world: &World) -> Result<usize, String> {
    let mut judged = 0;
    for call in world.calls.iter().filter(|c| c.starts_with("env:")) {
        let destructive = call.starts_with("env:down") || call.starts_with("env:handover");
        if !destructive {
            continue;
        }
        judged += 1;
        if !call.ends_with(" holders=0") {
            return Err(format!(
                "O11: destructive environment call with holders: {call}"
            ));
        }
    }
    Ok(judged)
}
fn o12(world: &World) -> Result<usize, String> {
    for (process, signals) in &world.foreign {
        if !process.alive || !signals.is_empty() {
            return Err(format!(
                "O12: foreign fixture {} alive={} signals={signals:?}",
                process.pid, process.alive
            ));
        }
    }
    Ok(world.foreign.len())
}
fn o13(world: &World) -> Result<usize, String> {
    for admission in &world.admissions {
        if admission.unresolved != 0 || admission.open != 0 {
            return Err(format!(
                "O13: {} admitted for {} beside {} unresolved ({} open) predecessors",
                admission.run, admission.owner, admission.unresolved, admission.open
            ));
        }
    }
    Ok(world.admissions.len())
}
fn o14(world: &World) -> Result<usize, String> {
    if world.probe_bound.is_none() {
        return Ok(0);
    }
    for probe in &world.probes {
        // A live announced probe whose nonce cannot be read is not judged
        // safe: its generation evidence is missing, so it fails (I-O6).
        if probe.alive {
            return Err(format!(
                "O14: probe {} alive past its deadline plus grace ({:?}), nonce {:?}",
                probe.pid, world.probe_bound, probe.nonce
            ));
        }
    }
    Ok(world.probes.len())
}
#[cfg(test)]
mod controls {
    //! Negative controls: every oracle fails on a world that breaks it.
    //! Synthetic worlds prove the predicate; the live controls below break a
    //! real HOME the way a kernel bug would and require the oracle to see it.
    use super::super::world::*;
    use super::*;
    use crate::gate::HolderDiag;
    use crate::id::*;
    use crate::observe::{AbsenceProof, Evidence, UnknownReason};
    use crate::run::*;

    fn evidence() -> Evidence {
        Evidence::new("control".into(), None).unwrap()
    }
    fn run_id(n: u8) -> RunId {
        RunId::new(1, [n; 10]).unwrap()
    }
    fn row(n: u8, phase: &str, cleanup: Option<&str>) -> RunRow {
        RunRow {
            id: run_id(n),
            owner: "owner".into(),
            phase: phase.into(),
            cleanup: cleanup.map(str::to_owned),
            acknowledged: false,
            sentinel: None,
        }
    }
    fn me() -> u32 {
        std::process::id()
    }
    fn fails(oracle: Oracle, world: &World) {
        assert!(
            oracle.check(world).is_err(),
            "{oracle:?} accepted a violating world"
        );
    }
    fn passes(oracle: Oracle, world: &World) {
        assert!(
            oracle.check(world).is_ok(),
            "{oracle:?} refused a valid world"
        );
    }
    fn effect(sequence: i64, kind: &str, state: EffectState) -> Effect {
        Effect {
            sequence,
            spec: EffectSpec {
                target: Fence {
                    resource: ResourceId::environment(
                        CheckoutId::new(CheckoutKind::EphemeralClone, [1; 16]),
                        "env",
                    )
                    .unwrap(),
                    generation: Generation::new(1),
                },
                kind: kind.into(),
                payload: serde_json::Value::Null,
                settlement: Settlement::Retain,
                required: true,
            },
            state,
            receipt: None,
            reason: None,
        }
    }

    // --- Live controls: a real HOME broken the way a kernel bug would. ---

    /// Merged proof: oracle_controls_o1a_o1b_reject_surviving_processes, oracle_controls_o2_o3_o13_reject_open_or_parallel_runs, oracle_controls_o4_rejects_a_gate_held_without_a_declared_holder, oracle_controls_o5_rejects_double_application_and_release, oracle_controls_o6_o7_o9_o10_reject_damage, oracle_controls_o8_rejects_undo_after_unknown_or_unobserved, oracle_controls_o11_o12_o14_reject_unsafe_destruction, oracle_live_controls_detect_real_violations.
    #[test]
    fn oracle_controls_reject_every_violating_world() {
        // oracle_controls_o1a_o1b_reject_surviving_processes

        let mut world = World::default();
        world.runs.push(row(1, "closed", Some("complete")));
        world.effects.insert(run_id(1).to_string(), vec![]);
        passes(Oracle::O1a, &world);
        world.workloads.push(Proc {
            pid: me(),
            alive: true,
            nonce: Some(format!("{}:1", run_id(1))),
        });
        fails(Oracle::O1a, &world);
        // Incomplete: the survivor must be a listed residue effect.
        let mut world = World::default();
        world.runs.push(row(2, "closed", Some("incomplete")));
        world.effects.insert(
            run_id(2).to_string(),
            vec![effect(1, "kernel:process", EffectState::Released)],
        );
        world.statuses.insert(
            run_id(2).to_string(),
            Observed::Verified(
                RunStatus::Closed {
                    workload: crate::outcome::WorkloadOutcome::Interrupted,
                    complete: false,
                    effects: vec![],
                },
                evidence(),
            ),
        );
        world.successor_blocked.insert(run_id(2).to_string(), true);
        passes(Oracle::O1b, &world);
        world.workloads.push(Proc {
            pid: me(),
            alive: true,
            nonce: Some(format!("{}:1", run_id(2))),
        });
        fails(Oracle::O1b, &world);
        world.workloads.clear();
        world.successor_blocked.insert(run_id(2).to_string(), false);
        fails(Oracle::O1b, &world);
        world.successor_blocked.insert(run_id(2).to_string(), true);
        world.effects.insert(
            run_id(2).to_string(),
            vec![effect(1, "kernel:attachment", EffectState::Residue)],
        );
        fails(Oracle::O1b, &world);

        // oracle_controls_o2_o3_o13_reject_open_or_parallel_runs

        let mut world = World::default();
        world.runs.push(row(1, "active", None));
        world.statuses.insert(
            run_id(1).to_string(),
            Observed::Verified(RunStatus::Live(Phase::Active), evidence()),
        );
        fails(Oracle::O2, &world);
        let bad = crate::outcome::Invocation::validated(vec!["other".into()], |_| true).unwrap();
        world.statuses.insert(
            run_id(1).to_string(),
            Observed::Verified(
                RunStatus::Interrupted {
                    phase: Phase::Active,
                    next: bad,
                },
                evidence(),
            ),
        );
        fails(Oracle::O2, &world);
        world.statuses.insert(
            run_id(1).to_string(),
            Observed::Verified(
                RunStatus::Interrupted {
                    phase: Phase::Active,
                    next: next(),
                },
                evidence(),
            ),
        );
        passes(Oracle::O2, &world);
        passes(Oracle::O3, &world);
        world.runs.push(row(2, "stopping", None));
        fails(Oracle::O3, &world);
        world.admissions.push(Admission {
            run: "r".into(),
            owner: "o".into(),
            unresolved: 0,
            open: 0,
        });
        passes(Oracle::O13, &world);
        world.admissions[0].unresolved = 1;
        fails(Oracle::O13, &world);

        // oracle_controls_o4_rejects_a_gate_held_without_a_declared_holder

        let mut world = World::default();
        let held = |pid| GateRead {
            file: "scope/x.gate".into(),
            observed: Observed::Verified(
                GateState::Held(Box::new(Observed::Verified(
                    HolderDiag {
                        foreground: None,
                        run: None,
                        pid,
                        start: None,
                        boot: None,
                        argv0: None,
                        state: Observed::Verified(ProcessState::Active, evidence()),
                    },
                    evidence(),
                ))),
                evidence(),
            ),
        };
        world.gates.push(GateRead {
            file: "scope/free.gate".into(),
            observed: Observed::Absent(AbsenceProof::NoGateInode),
        });
        passes(Oracle::O4, &world);
        world.gates.push(held(me()));
        fails(Oracle::O4, &world);
        world.live_holders.insert(me());
        passes(Oracle::O4, &world);
        world.gates.push(GateRead {
            file: "scope/unknown.gate".into(),
            observed: Observed::Unknown(UnknownReason::Injected),
        });
        fails(Oracle::O4, &world);
        let mut world = World::default();
        world.scopes_held.push("scope".into());
        fails(Oracle::O4, &world);

        // oracle_controls_o5_rejects_double_application_and_release

        let mut world = World {
            calls: vec!["apply a".into(), "release a".into()],
            ..World::default()
        };
        passes(Oracle::O5, &world);
        world.calls.push("release a".into());
        fails(Oracle::O5, &world);
        let mut world = World::default();
        world
            .attachments
            .push(("env".into(), "run".into(), "released".into()));
        fails(Oracle::O5, &world);
        world.releases.push(("env".into(), "run".into()));
        passes(Oracle::O5, &world);
        world.releases.push(("env".into(), "run".into()));
        fails(Oracle::O5, &world);
        let world = World {
            claim_releases: vec![("c".into(), Some("t".into())); 2],
            ..World::default()
        };
        fails(Oracle::O5, &world);

        // oracle_controls_o6_o7_o9_o10_reject_damage

        let mut world = World {
            integrity: Some(Err("1 foreign-key violations".into())),
            ..World::default()
        };
        fails(Oracle::O6, &world);
        world.replaced.push((
            "t".into(),
            vec![b"old".to_vec(), b"new".to_vec()],
            Some(b"ne".to_vec()),
        ));
        fails(Oracle::O7, &world);
        world.replaced[0].2 = None;
        fails(Oracle::O7, &world);
        world.outcomes.push(("CaptureWrite".into(), 0, 1));
        fails(Oracle::O9, &world);
        world
            .diffs
            .push(("status".into(), vec!["created kernel.db".into()]));
        fails(Oracle::O10, &world);

        // oracle_controls_o8_rejects_undo_after_unknown_or_unobserved

        let mut world = World {
            events: vec![
                Event::Observed {
                    subject: "a".into(),
                    unknown: false,
                },
                Event::Destructive {
                    subject: "a".into(),
                },
            ],
            ..World::default()
        };
        passes(Oracle::O8, &world);
        world.events.insert(
            1,
            Event::Observed {
                subject: "a".into(),
                unknown: true,
            },
        );
        fails(Oracle::O8, &world);
        world.events = vec![Event::Destructive {
            subject: "b".into(),
        }];
        fails(Oracle::O8, &world);

        // oracle_controls_o11_o12_o14_reject_unsafe_destruction

        let mut world = World {
            calls: vec!["env:down e@2 holders=0".into()],
            ..World::default()
        };
        passes(Oracle::O11, &world);
        world.calls.push("env:handover e@3 holders=1".into());
        fails(Oracle::O11, &world);
        world.foreign.push((
            Proc {
                pid: me(),
                alive: true,
                nonce: None,
            },
            vec!["TERM".into()],
        ));
        fails(Oracle::O12, &world);
        world.foreign[0].1.clear();
        world.foreign[0].0.alive = false;
        fails(Oracle::O12, &world);
        world.probe_bound = Some(Duration::from_millis(1));
        world.probes.push(Proc {
            pid: me(),
            alive: true,
            nonce: Some("probe".into()),
        });
        fails(Oracle::O14, &world);

        // oracle_live_controls_detect_real_violations

        use super::super::scenarios::Fake;
        use crate::gate::{Gate, Wait};
        use crate::root::StateRoot;
        use crate::store::Store;
        let dir = home();
        let home = dir.home();
        install_audits(&home);
        let cx = mutate();
        let root = StateRoot::effects(&cx, &home).unwrap();
        let owner = ResourceId::store(Scope::Machine, "control").unwrap();
        let inv = super::super::scenarios::invocation(owner.clone(), vec![]);
        let mut fake = Fake::new(&home);
        let run = Run::begin(
            &cx,
            &root,
            &inv,
            BeginOpts {
                id: run_id(1),
                wait: Wait::FailFast,
                acknowledge: vec![],
                next: next(),
                custody: None,
            },
            &cleanup(&cx),
            &mut fake,
        )
        .unwrap();
        run.settle(&cleanup(&cx), &mut fake).unwrap();
        let world = || {
            let mut world = World::capture(&home);
            world.integrity = Some(
                Store::open(&mutate(), &root)
                    .unwrap()
                    .check_integrity()
                    .map_err(|e| format!("{e:?}")),
            );
            world
        };
        for oracle in [
            Oracle::O1a,
            Oracle::O3,
            Oracle::O4,
            Oracle::O5,
            Oracle::O6,
            Oracle::O13,
        ] {
            passes(oracle, &world());
        }
        // O1a: a live process carrying the complete run's custody nonce.
        let mut stray = std::process::Command::new(fixture_path())
            .arg("nonce-workload")
            .env("EFFIGY_CUSTODY_NONCE", format!("{}:1", run_id(1)))
            .spawn()
            .unwrap();
        std::fs::write(home.join("pids"), stray.id().to_string()).unwrap();
        wait_until("stray exec", Duration::from_secs(5), || {
            crate::custody::platform::read_nonce_key(stray.id(), b"EFFIGY_CUSTODY_NONCE=")
                .verified()
                .is_some()
        });
        fails(Oracle::O1a, &world());
        stray.kill().unwrap();
        stray.wait().unwrap();
        passes(Oracle::O1a, &world());
        // O14: a probe process with readable generation evidence outliving
        // its bound. The probe evidence is the fixture's readable nonce.
        let mut probe = std::process::Command::new(fixture_path())
            .arg("nonce-workload")
            .env("EFFIGY_PROBE_NONCE", "control-probe")
            .spawn()
            .unwrap();
        wait_until("probe exec", Duration::from_secs(5), || {
            crate::custody::platform::read_nonce_key(probe.id(), b"EFFIGY_PROBE_NONCE=")
                .verified()
                .is_some()
        });
        let probes = World {
            probes: vec![proc(probe.id(), b"EFFIGY_PROBE_NONCE=")],
            probe_bound: Some(Duration::from_millis(1)),
            ..World::default()
        };
        assert_eq!(probes.probes[0].nonce.as_deref(), Some("control-probe"));
        fails(Oracle::O14, &probes);
        probe.kill().unwrap();
        probe.wait().unwrap();
        let probes = World {
            probes: vec![proc(probe.id(), b"EFFIGY_PROBE_NONCE=")],
            probe_bound: Some(Duration::from_millis(1)),
            ..World::default()
        };
        passes(Oracle::O14, &probes);
        // O4: a gate held by a process the harness never declared live.
        let guard =
            Gate::acquire(&cx, &root, std::slice::from_ref(&owner), Wait::FailFast).unwrap();
        fails(Oracle::O4, &world());
        drop(guard);
        passes(Oracle::O4, &world());
        // O13 and O3: a run row inserted beside an open predecessor.
        let store = Store::open(&cx, &root).unwrap();
        for (n, phase) in [(7u8, "active"), (8, "active")] {
            store
                    .conn()
                    .execute(
                        "INSERT INTO runs(run_id,owner,generation,kind,placement,phase,created_at,updated_at) \
                         VALUES (?1,?2,9,'operation','host',?3,0,0)",
                        rusqlite::params![run_id(n).to_string(), owner.name().to_string(), phase],
                    )
                    .unwrap();
        }
        fails(Oracle::O13, &world());
        fails(Oracle::O3, &world());
        // O5: an attachment released twice.
        store
                .conn()
                .execute_batch(
                    "INSERT INTO attachments(environment,run_id,env_generation,mode,state) \
                     SELECT name,(SELECT run_id FROM runs LIMIT 1),1,'\"Borrow\"','attached' FROM resources LIMIT 1;
                     UPDATE attachments SET state='released';
                     UPDATE attachments SET state='attached';
                     UPDATE attachments SET state='released';",
                )
                .unwrap();
        fails(Oracle::O5, &world());
        // O6: a dangling foreign key.
        store
                .conn()
                .execute_batch(
                    "PRAGMA foreign_keys=OFF; \
                     INSERT INTO events(run_id,seq,kind,state,wall_time) VALUES ('missing',1,'x','intended',0);",
                )
                .unwrap();
        fails(Oracle::O6, &world());
    }
}
