//! Backend qualification runner. Engine lanes supply disposable real-engine
//! fixtures for Colima docker/containerd, Docker Desktop and Linux Docker.
//! Kernel tests supply the strict fake; this is not a claim of real-engine QA.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Case {
    BeforeIdentity,
    Intended,
    ClientSpawned,
    Acknowledged,
    Applied,
    ContainerReplaced,
    VmRestarted,
    NoSetsid,
    ExecClientKilled,
    ForeignSession,
    BackendHung,
    GuestPermissionDenied,
}
pub const CASES: &[Case] = &[
    Case::BeforeIdentity,
    Case::Intended,
    Case::ClientSpawned,
    Case::Acknowledged,
    Case::Applied,
    Case::ContainerReplaced,
    Case::VmRestarted,
    Case::NoSetsid,
    Case::ExecClientKilled,
    Case::ForeignSession,
    Case::BackendHung,
    Case::GuestPermissionDenied,
];
/// The fixture kills the actual owner at the named boundary, or perturbs the
/// disposable guest. `effect` returns the durable journal entry, not a live
/// owner's in-memory receipt. Oracles independently inventory survivors and
/// compare foreign sentinels and the unselected backend's empty call log.
pub trait Fixture {
    fn arrange(&mut self, case: Case, cx: &OperationContext) -> Result<Option<Effect>, String>;
    fn adapter(&mut self) -> &mut dyn RemoteExecAdapter;
    fn verify_oracles(
        &mut self,
        case: Case,
        released: bool,
        cx: &OperationContext,
    ) -> Result<(), String>;
}
#[derive(Debug)]
pub struct CaseResult {
    pub case: Case,
    pub released: bool,
}
pub fn run(fixture: &mut impl Fixture, cx: &CleanupContext) -> Result<Vec<CaseResult>, String> {
    let mut results = Vec::new();
    for &case in CASES {
        check(cx.operation(), "remote conformance").map_err(|r| format!("{r:?}"))?;
        let effect = fixture.arrange(case, cx.operation())?;
        let released = match effect {
            Some(effect) => {
                let (state, _) = settle_effect(&effect, cx, Some(fixture.adapter()));
                state == EffectState::Released
            }
            None if case == Case::BeforeIdentity => true,
            None => return Err(format!("{case:?}: missing durable intent")),
        };
        let expected = !matches!(
            case,
            Case::ForeignSession | Case::BackendHung | Case::GuestPermissionDenied
        );
        if released != expected {
            return Err(format!(
                "{case:?}: released={released}, expected={expected}"
            ));
        }
        fixture.verify_oracles(case, released, cx.operation())?;
        results.push(CaseResult { case, released });
    }
    Ok(results)
}
