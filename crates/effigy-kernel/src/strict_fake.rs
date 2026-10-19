//! Test-support adapters (§12.2), available only with `fault-injection`.
//! An expectation is consumed exactly once. Calls are recorded before matching,
//! including forbidden calls. Partial effects happen even on Unknown replies.
use crate::ctx::{CleanupContext, EffectContext, ObserveContext, OperationContext};
use crate::ident::{BootIdentity, ProcessIdentity, ProcessState};
use crate::observe::Observed;
use crate::probe::{Probe, Readiness};
use crate::remote::*;
use crate::run::Fence;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Mutex;

#[derive(Debug)]
pub struct Step<C, R> {
    pub call: C,
    pub reply: R,
    pub effects: Vec<(String, Value)>,
}
#[derive(Debug)]
pub struct StrictFake<C, R> {
    expected: VecDeque<Step<C, R>>,
    calls: Vec<C>,
    resources: BTreeMap<String, Value>,
    foreign: BTreeSet<String>,
}
impl<C: std::fmt::Debug + PartialEq + Clone, R> StrictFake<C, R> {
    pub fn new(steps: Vec<Step<C, R>>, foreign: BTreeMap<String, Value>) -> Self {
        Self {
            expected: steps.into(),
            calls: vec![],
            foreign: foreign.keys().cloned().collect(),
            resources: foreign,
        }
    }
    pub fn invoke(&mut self, call: C) -> R {
        self.calls.push(call.clone());
        let step = self.expected.pop_front().expect("unmodelled adapter call");
        assert_eq!(step.call, call, "unmodelled adapter invocation");
        for (resource, value) in step.effects {
            assert!(
                !self.foreign.contains(&resource),
                "foreign sentinel touched: {resource}"
            );
            self.resources.insert(resource, value);
        }
        step.reply
    }
    pub fn expect(&mut self, step: Step<C, R>) {
        self.expected.push_back(step);
    }
    pub fn calls(&self) -> &[C] {
        &self.calls
    }
    pub fn resources(&self) -> &BTreeMap<String, Value> {
        &self.resources
    }
    pub fn assert_finished(&self) {
        assert!(
            self.expected.is_empty(),
            "unconsumed adapter calls: {}",
            self.expected.len()
        );
    }
    pub fn assert_unselected(&self) {
        assert!(self.calls.is_empty(), "unselected backend received calls");
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendCall {
    pub operation: String,
    pub target: Fence,
}
/// Engines choose their own operation names; no backend grammar in the kernel.
pub type StrictBackend = StrictFake<BackendCall, Observed<Value>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlatformCall {
    Boot,
    Process(u32),
    State(u32),
    Clock,
}
#[derive(Debug)]
pub enum PlatformReply {
    Boot(Observed<BootIdentity>),
    Process(Observed<ProcessIdentity>),
    State(Observed<ProcessState>),
    Clock(Observed<crate::ident::BootInstant>),
}
pub type StrictPlatform = StrictFake<PlatformCall, PlatformReply>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeCall {
    Observe,
    Transport { name: String, target: String },
}
#[derive(Debug)]
pub struct StrictProbe(pub Mutex<StrictFake<ProbeCall, Observed<Readiness>>>);
impl Probe for StrictProbe {
    fn observe(&self, cx: &ObserveContext) -> Observed<Readiness> {
        assert!(!cx.remaining().is_zero(), "probe deadline expired");
        self.0.lock().expect("fake lock").invoke(ProbeCall::Observe)
    }
}
impl StrictProbe {
    pub fn transport(
        &self,
        name: String,
        target: String,
        cx: &ObserveContext,
    ) -> Observed<Readiness> {
        assert!(!cx.remaining().is_zero(), "probe deadline expired");
        self.0
            .lock()
            .expect("fake lock")
            .invoke(ProbeCall::Transport { name, target })
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteCall {
    Identity(ContainerTarget),
    Launch(ContainerIdentity),
    Signal(RemoteRegistration, Signal),
    Observe(RemoteRegistration),
    Find(ContainerIdentity, Nonce),
}
pub enum RemoteReply {
    Identity(Observed<ContainerIdentity>),
    Launch(
        Box<dyn FnOnce(RemoteCommand) -> Result<crate::custody::ProcessSpec, AdapterError> + Send>,
    ),
    Session(Observed<SessionState>),
    Find(Observed<Option<RemoteRegistration>>),
}
impl std::fmt::Debug for RemoteReply {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Identity(v) => f.debug_tuple("Identity").field(v).finish(),
            Self::Launch(_) => f.write_str("Launch([modelled callback])"),
            Self::Session(v) => f.debug_tuple("Session").field(v).finish(),
            Self::Find(v) => f.debug_tuple("Find").field(v).finish(),
        }
    }
}
#[derive(Debug)]
pub struct StrictRemote {
    pub tape: StrictFake<RemoteCall, RemoteReply>,
    /// Full spawn commands retained privately; Debug redacts all values.
    pub launches: Vec<RemoteCommand>,
}
fn bounded(cx: &OperationContext) {
    assert!(cx.remaining().is_some(), "unbounded adapter call");
    assert!(cx.check("strict fake").is_ok(), "expired adapter call");
}
impl RemoteExecAdapter for StrictRemote {
    fn identity(
        &mut self,
        target: &ContainerTarget,
        cx: &OperationContext,
    ) -> Observed<ContainerIdentity> {
        bounded(cx);
        match self.tape.invoke(RemoteCall::Identity(target.clone())) {
            RemoteReply::Identity(v) => v,
            _ => panic!("wrong model reply"),
        }
    }
    fn launch(
        &mut self,
        id: &ContainerIdentity,
        cmd: RemoteCommand,
        cx: &EffectContext,
    ) -> Result<crate::custody::ProcessSpec, AdapterError> {
        bounded(cx.operation());
        self.launches.push(cmd.clone());
        match self.tape.invoke(RemoteCall::Launch(id.clone())) {
            RemoteReply::Launch(f) => f(cmd),
            _ => panic!("wrong model reply"),
        }
    }
    fn signal_session(
        &mut self,
        reg: &RemoteRegistration,
        sig: Signal,
        cx: &CleanupContext,
    ) -> Observed<SessionState> {
        bounded(cx.operation());
        match self.tape.invoke(RemoteCall::Signal(reg.clone(), sig)) {
            RemoteReply::Session(v) => v,
            _ => panic!("wrong model reply"),
        }
    }
    fn observe_session(
        &mut self,
        reg: &RemoteRegistration,
        cx: &OperationContext,
    ) -> Observed<SessionState> {
        bounded(cx);
        match self.tape.invoke(RemoteCall::Observe(reg.clone())) {
            RemoteReply::Session(v) => v,
            _ => panic!("wrong model reply"),
        }
    }
    fn find_by_nonce(
        &mut self,
        id: &ContainerIdentity,
        nonce: &Nonce,
        cx: &OperationContext,
    ) -> Observed<Option<RemoteRegistration>> {
        bounded(cx);
        match self
            .tape
            .invoke(RemoteCall::Find(id.clone(), nonce.clone()))
        {
            RemoteReply::Find(v) => v,
            _ => panic!("wrong model reply"),
        }
    }
}

impl StrictPlatform {
    pub fn boot(&mut self) -> Observed<BootIdentity> {
        match self.invoke(PlatformCall::Boot) {
            PlatformReply::Boot(v) => v,
            _ => panic!("wrong platform model reply"),
        }
    }
    pub fn process(&mut self, pid: u32) -> Observed<ProcessIdentity> {
        match self.invoke(PlatformCall::Process(pid)) {
            PlatformReply::Process(v) => v,
            _ => panic!("wrong platform model reply"),
        }
    }
    pub fn state(&mut self, pid: u32) -> Observed<ProcessState> {
        match self.invoke(PlatformCall::State(pid)) {
            PlatformReply::State(v) => v,
            _ => panic!("wrong platform model reply"),
        }
    }
    pub fn clock(&mut self) -> Observed<crate::ident::BootInstant> {
        match self.invoke(PlatformCall::Clock) {
            PlatformReply::Clock(v) => v,
            _ => panic!("wrong platform model reply"),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ctx::*;
    use crate::observe::UnknownReason;
    use serde_json::json;
    use std::time::{Duration, Instant};
    fn step<C, R>(call: C, reply: R) -> Step<C, R> {
        Step {
            call,
            reply,
            effects: vec![],
        }
    }
    #[test]
    fn all_adapters_record_calls_inject_unknown_timeout_and_partial_effects() {
        let target = Fence {
            resource: crate::id::ResourceId::gateway("fake").unwrap(),
            generation: crate::id::Generation::new(1),
        };
        let call = BackendCall {
            operation: "prepare".into(),
            target,
        };
        let mut selected = StrictBackend::new(
            vec![Step {
                call: call.clone(),
                reply: Observed::Unknown(UnknownReason::Injected),
                effects: vec![("owned".into(), json!("partial"))],
            }],
            BTreeMap::from([("foreign".into(), json!("alive"))]),
        );
        let unselected = StrictBackend::new(vec![], BTreeMap::new());
        assert!(matches!(
            selected.invoke(call.clone()),
            Observed::Unknown(UnknownReason::Injected)
        ));
        assert_eq!(selected.calls(), &[call]);
        assert_eq!(selected.resources()["owned"], json!("partial"));
        assert_eq!(selected.resources()["foreign"], json!("alive"));
        selected.assert_finished();
        unselected.assert_unselected();
        let mut platform = StrictPlatform::new(
            vec![
                step(
                    PlatformCall::Boot,
                    PlatformReply::Boot(Observed::Unknown(UnknownReason::Injected)),
                ),
                step(
                    PlatformCall::Process(4),
                    PlatformReply::Process(Observed::Unknown(UnknownReason::PermissionDenied)),
                ),
                step(
                    PlatformCall::State(4),
                    PlatformReply::State(Observed::Unknown(UnknownReason::TimedOut {
                        phase: "platform".into(),
                    })),
                ),
                step(
                    PlatformCall::Clock,
                    PlatformReply::Clock(Observed::Unknown(UnknownReason::Injected)),
                ),
            ],
            BTreeMap::new(),
        );
        assert!(matches!(platform.boot(), Observed::Unknown(_)));
        assert!(matches!(platform.process(4), Observed::Unknown(_)));
        assert!(matches!(platform.state(4), Observed::Unknown(_)));
        assert!(matches!(platform.clock(), Observed::Unknown(_)));
        platform.assert_finished();
        assert_eq!(platform.calls().len(), 4);
        let probe = StrictProbe(Mutex::new(StrictFake::new(
            vec![
                step(
                    ProbeCall::Observe,
                    Observed::Unknown(UnknownReason::Injected),
                ),
                step(
                    ProbeCall::Transport {
                        name: "http".into(),
                        target: "disposable".into(),
                    },
                    Observed::Unknown(UnknownReason::TimedOut {
                        phase: "transport".into(),
                    }),
                ),
            ],
            BTreeMap::new(),
        )));
        let cx = OperationContext::new(
            Deadline::At(Instant::now() + Duration::from_secs(1)),
            CancelToken::default(),
            Limits {
                capture_tail: 1,
                log_rotate: 1,
                log_keep: 1,
                line_max: 1,
                queue_bytes: 1,
                probe_concurrency: 1,
            },
            EffectClass::Observe,
            Placement::Host,
        )
        .observe()
        .unwrap();
        assert!(matches!(probe.observe(&cx), Observed::Unknown(_)));
        assert!(matches!(
            probe.transport("http".into(), "disposable".into(), &cx),
            Observed::Unknown(_)
        ));
        probe.0.lock().unwrap().assert_finished();
    }
    #[test]
    fn unmodelled_mismatched_and_foreign_calls_fail_and_are_recorded() {
        let mut empty: StrictFake<u8, ()> = StrictFake::new(vec![], BTreeMap::new());
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| empty.invoke(1))).is_err()
        );
        assert_eq!(empty.calls(), &[1]);
        let mut wrong = StrictFake::new(vec![step(1, ())], BTreeMap::new());
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| wrong.invoke(2))).is_err()
        );
        assert_eq!(wrong.calls(), &[2]);
        let mut foreign = StrictFake::new(
            vec![Step {
                call: 1,
                reply: (),
                effects: vec![("foreign".into(), json!("dead"))],
            }],
            BTreeMap::from([("foreign".into(), json!("alive"))]),
        );
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| foreign.invoke(1))).is_err()
        );
        assert_eq!(foreign.resources()["foreign"], json!("alive"));
    }
}
