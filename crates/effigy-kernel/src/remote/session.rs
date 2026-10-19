//! Shared evidence rules for backend stop helpers. A transport must take a
//! complete live inventory in the registered container and immediately recheck
//! every identity at delivery. A numeric session id alone grants nothing.
use super::*;
use crate::observe::AbsenceProof;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub pid: u32,
    pub start_ticks: u64,
    pub pgid: u32,
    pub sid: u32,
    pub nonce: Observed<String>,
    /// Verified ancestry rooted at the registered leader (fallback only).
    pub descendant: bool,
}
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub container: ContainerIdentity,
    pub pid_ns: u64,
    pub leader: Option<Member>,
    /// Complete live session inventory, or leader group plus descendants when
    /// setsid is unavailable. An incomplete inventory must be Unknown instead.
    pub members: Vec<Member>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignalTarget {
    Session {
        sid: u32,
        leader_pid: u32,
        start_ticks: u64,
    },
    Member {
        pid: u32,
        start_ticks: u64,
        nonce: Nonce,
    },
}
pub fn valid_container_absence(id: &ContainerIdentity, proof: &AbsenceProof) -> bool {
    match proof {
        AbsenceProof::ContainerGone { id: missing } => missing == &id.id,
        AbsenceProof::BootChanged { recorded, current } => {
            recorded == &id.vm_boot_id && !current.is_empty() && current != recorded
        }
        _ => false,
    }
}
pub(super) fn valid_absence(reg: &RemoteRegistration, proof: &AbsenceProof) -> bool {
    valid_container_absence(&reg.container, proof)
}
fn binding(reg: &RemoteRegistration, snapshot: &Snapshot) -> Result<(), UnknownReason> {
    if !stream::valid_shape(reg) || !valid_identity(&snapshot.container) {
        return Err(inconsistent("invalid remote identity"));
    }
    if snapshot.container.id != reg.container.id
        || snapshot.container.created != reg.container.created
    {
        return Err(inconsistent(
            "replacement container cannot inherit registration",
        ));
    }
    if snapshot.pid_ns != reg.pid_ns {
        return Err(inconsistent("pid namespace changed without boot proof"));
    }
    Ok(())
}
pub fn observe(reg: &RemoteRegistration, snapshot: Observed<Snapshot>) -> Observed<SessionState> {
    let snapshot = match snapshot {
        Observed::Verified(s, _) => s,
        Observed::Absent(p) if valid_absence(reg, &p) => return Observed::Absent(p),
        Observed::Unknown(r) => return Observed::Unknown(r),
        _ => return Observed::Unknown(inconsistent("unproven remote absence")),
    };
    if snapshot.container.id == reg.container.id
        && !snapshot.container.vm_boot_id.is_empty()
        && snapshot.container.vm_boot_id != reg.container.vm_boot_id
    {
        return Observed::Absent(AbsenceProof::BootChanged {
            recorded: reg.container.vm_boot_id.clone(),
            current: snapshot.container.vm_boot_id,
        });
    }
    if let Err(r) = binding(reg, &snapshot) {
        return Observed::Unknown(r);
    }
    let leader_live = snapshot
        .leader
        .as_ref()
        .is_some_and(|m| m.pid == reg.leader_pid && m.start_ticks == reg.leader_start_ticks);
    if snapshot.members.is_empty() && !leader_live {
        return Observed::Verified(SessionState::Closed, evidence());
    }
    // A foreign reused session is uncertainty, not permission to kill or a
    // claim of closure. The same plan proves whether all live members are ours.
    match plan_signal(reg, &snapshot) {
        Observed::Verified(_, e) => Observed::Verified(SessionState::Running, e),
        Observed::Unknown(r) => Observed::Unknown(r),
        Observed::Absent(p) => Observed::Absent(p),
    }
}
pub fn plan_signal(reg: &RemoteRegistration, snapshot: &Snapshot) -> Observed<Vec<SignalTarget>> {
    if let Err(r) = binding(reg, snapshot) {
        return Observed::Unknown(r);
    }
    if snapshot.container.vm_boot_id != reg.container.vm_boot_id {
        return Observed::Unknown(inconsistent("boot changed before signal"));
    }
    let leader = snapshot.leader.as_ref().filter(|m| {
        m.pid == reg.leader_pid && m.start_ticks == reg.leader_start_ticks && m.sid == reg.sid
    });
    if reg.session_isolated && leader.is_some() {
        return Observed::Verified(
            vec![SignalTarget::Session {
                sid: reg.sid,
                leader_pid: reg.leader_pid,
                start_ticks: reg.leader_start_ticks,
            }],
            evidence(),
        );
    }
    let mut targets = Vec::new();
    for member in &snapshot.members {
        let scope = if reg.session_isolated {
            member.sid == reg.sid
        } else {
            member.descendant
                || (member.sid == reg.sid && leader.is_some_and(|l| l.pgid == member.pgid))
        };
        if !scope
            || member.pid == 0
            || member.pid > i32::MAX as u32
            || member.start_ticks < reg.leader_start_ticks
            || member
                .nonce
                .verified()
                .is_none_or(|n| n != reg.nonce.as_str())
        {
            return Observed::Unknown(inconsistent("remote member lacks generation evidence"));
        }
        targets.push(SignalTarget::Member {
            pid: member.pid,
            start_ticks: member.start_ticks,
            nonce: reg.nonce.clone(),
        });
    }
    Observed::Verified(targets, evidence())
}
/// Adapters implement this inside the exact container. Delivery must recheck
/// boot, namespace, leader start (session) or member start+nonce (individual)
/// atomically where supported, otherwise immediately before signalling (I-P4).
pub trait Transport {
    fn snapshot(&mut self, reg: &RemoteRegistration, cx: &OperationContext) -> Observed<Snapshot>;
    fn deliver(
        &mut self,
        reg: &RemoteRegistration,
        target: &SignalTarget,
        sig: Signal,
        cx: &CleanupContext,
    ) -> Observed<()>;
}
pub fn signal(
    transport: &mut impl Transport,
    reg: &RemoteRegistration,
    sig: Signal,
    cx: &CleanupContext,
) -> Observed<SessionState> {
    if let Err(r) = check(cx.operation(), "remote signal") {
        return Observed::Unknown(r);
    }
    let observed = transport.snapshot(reg, cx.operation());
    if let Err(r) = check(cx.operation(), "remote signal snapshot") {
        return Observed::Unknown(r);
    }
    let snapshot = match observed {
        Observed::Verified(s, _) => s,
        Observed::Absent(p) if valid_absence(reg, &p) => return Observed::Absent(p),
        Observed::Unknown(r) => return Observed::Unknown(r),
        _ => return Observed::Unknown(inconsistent("unproven signal target")),
    };
    if let Observed::Absent(p) = observe(reg, Observed::Verified(snapshot.clone(), evidence())) {
        return Observed::Absent(p);
    }
    let targets = match plan_signal(reg, &snapshot) {
        Observed::Verified(targets, _) => targets,
        Observed::Unknown(r) => return Observed::Unknown(r),
        Observed::Absent(p) => return Observed::Absent(p),
    };
    for target in targets {
        if let Err(r) = check(cx.operation(), "remote signal delivery") {
            return Observed::Unknown(r);
        }
        match transport.deliver(reg, &target, sig, cx) {
            Observed::Verified((), _) => {}
            Observed::Unknown(r) => return Observed::Unknown(r),
            // Member disappearance is not a session closure proof.
            Observed::Absent(_) => {}
        }
    }
    if let Err(r) = check(cx.operation(), "remote signal observation") {
        return Observed::Unknown(r);
    }
    let observed = transport.snapshot(reg, cx.operation());
    if let Err(r) = check(cx.operation(), "remote signal observation") {
        return Observed::Unknown(r);
    }
    observe(reg, observed)
}
