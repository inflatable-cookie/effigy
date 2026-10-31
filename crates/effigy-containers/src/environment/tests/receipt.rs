//! effigy#508: what a creation receipt may be, and every way one must not be
//! minted.
//!
//! The real adapters keep no creator record at all, so they answer
//! `Unknown(NoBackendRecord)`; `adapters.rs` proves that for both of them. The
//! fake below keeps the one record a backend that *can* answer would keep:
//! which run's create returned which exact full container id, in which project.
//!
//! Every proof here is about that record alone. A creation timestamp, an
//! inventory that was empty before an apply and populated after it, Compose's
//! own project and service labels, and a run id the caller supplies are all
//! facts a competing Compose actor reproduces, so none of them is evidence and
//! none of them is modelled. What a replacement changes is the id, so a
//! replacement is a second create with a second id, never a record that moves.

use super::*;
use effigy_kernel::observe::CreatorRef;

/// The exact full id this run's apply created.
const CREATED: &str = "3f6a1d0c7b9e4f2a8c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f6a7b";
/// The id a competing actor's replacement carries: the same project, service,
/// name and labels, a different container.
const REPLACEMENT: &str = "9c8b7a6f5e4d3c2b1a0f9e8d7c6b5a4f3e2d1c0b9a8f7e6d5c4b3a2f1e0d9c8b";

fn evidence() -> Evidence {
    Evidence::new("backend creator record".to_owned(), None).expect("fixed evidence")
}

fn run_id(n: u8) -> RunId {
    RunId::new(1_760_000_000_000, [n; 10]).expect("run id")
}

/// What a fresh-target check may accept as a receipt. Anything else — absence
/// or an unrecorded creator — is a refusal, never an attribution.
fn receipt(observed: &Observed<RunId>) -> Option<RunId> {
    observed.verified().copied()
}

/// A backend that keeps the record the real engines do not: one creator row per
/// `(project, exact full container id)`, written by the create that returned
/// the id. It models the API the real adapters are missing, so the seam's
/// contract is proved before any engine supplies it.
#[derive(Clone)]
struct Recorded {
    selection: BackendSelection,
    state: Rc<RefCell<RecordedState>>,
}

#[derive(Default)]
struct RecordedState {
    rows: BTreeMap<(String, String), RunId>,
    /// Why the record cannot be read, when it cannot.
    unreadable: Option<String>,
}

impl Recorded {
    fn new() -> Self {
        Self {
            selection: selection(),
            state: Rc::new(RefCell::new(RecordedState::default())),
        }
    }

    /// One create: the engine returned `container_id` to the client of `run`.
    fn created(&self, project: &str, run: RunId, container_id: &str) {
        self.state
            .borrow_mut()
            .rows
            .insert((project.to_owned(), container_id.to_owned()), run);
    }

    fn unreadable(&self, detail: &str) {
        self.state.borrow_mut().unreadable = Some(detail.to_owned());
    }
}

impl EnvironmentBackend for Recorded {
    fn selection(&self) -> &BackendSelection {
        &self.selection
    }
    fn inventory(&mut self, _: &Path, project: &str) -> Observed<Vec<Workload>> {
        panic!("this fake models the creator record, not an inventory of `{project}`")
    }
    fn created_by(&mut self, project: &str, container_id: &str) -> Observed<RunId> {
        let state = self.state.borrow();
        if let Some(detail) = &state.unreadable {
            return Observed::Unknown(UnknownReason::NoBackendRecord(detail.clone()));
        }
        match state
            .rows
            .get(&(project.to_owned(), container_id.to_owned()))
        {
            Some(run) => Observed::Verified(*run, evidence()),
            None => Observed::Absent(AbsenceProof::NoCreator {
                creator: CreatorRef {
                    project: project.to_owned(),
                    container_id: container_id.to_owned(),
                },
            }),
        }
    }
}

/// A creation record bound to the exact full id is the only receipt: the run
/// whose apply created the container is named, and nothing else is.
#[test]
fn a_creation_receipt_names_the_run_whose_apply_created_the_container() {
    let ours = run_id(1);
    let mut backend = Recorded::new();
    backend.created(PROJECT, ours, CREATED);
    assert_eq!(receipt(&backend.created_by(PROJECT, CREATED)), Some(ours));
}

/// A competing Compose actor creates the same project, service, name and
/// labels. The engine's record names *that* actor's run, so the caller's
/// comparison refuses: the seam never promotes a matching container to the
/// asking run.
#[test]
fn a_competing_creators_container_is_never_this_runs_receipt() {
    let ours = run_id(2);
    let theirs = run_id(3);
    let mut backend = Recorded::new();
    backend.created(PROJECT, theirs, CREATED);
    let observed = backend.created_by(PROJECT, CREATED);
    assert_eq!(receipt(&observed), Some(theirs));
    assert_ne!(receipt(&observed), Some(ours));
}

/// A replacement keeps the name, service and labels and carries a new id, so
/// the replaced container's receipt cannot move to it: the new id has no
/// creator row, which is an absence, not an attribution.
#[test]
fn a_replacement_never_serves_the_replaced_containers_receipt() {
    let ours = run_id(4);
    let mut backend = Recorded::new();
    backend.created(PROJECT, ours, CREATED);
    let observed = backend.created_by(PROJECT, REPLACEMENT);
    assert_eq!(receipt(&observed), None);
    assert_eq!(
        observed,
        Observed::Absent(AbsenceProof::NoCreator {
            creator: CreatorRef {
                project: PROJECT.to_owned(),
                container_id: REPLACEMENT.to_owned(),
            },
        })
    );
    // The old id keeps the receipt it earned; the next caller that sees the
    // replacement asks about the replacement.
    assert_eq!(receipt(&backend.created_by(PROJECT, CREATED)), Some(ours));
}

/// A container another project created is not this project's container, even
/// when the caller asks with the right id.
#[test]
fn an_id_another_project_created_is_not_this_projects_receipt() {
    let theirs = run_id(5);
    let mut backend = Recorded::new();
    backend.created("another-project", theirs, CREATED);
    assert_eq!(receipt(&backend.created_by(PROJECT, CREATED)), None);
}

/// A record that cannot be read is unknown, never an absence and never a
/// receipt: a caller may not read a failed read as "no run created it".
#[test]
fn an_unreadable_creator_record_is_unknown() {
    let mut backend = Recorded::new();
    backend.created(PROJECT, run_id(6), CREATED);
    backend.unreadable("the creator record could not be read (permission denied)");
    match backend.created_by(PROJECT, CREATED) {
        Observed::Unknown(UnknownReason::NoBackendRecord(detail)) => {
            assert!(detail.contains("permission denied"), "{detail}");
        }
        other => panic!("an unreadable record must be unknown, not {other:?}"),
    }
}

/// A failed apply creates nothing, so it records nothing: the id an earlier
/// attempt never created has no receipt, and only the retry's own create
/// records one — for the retry, never retroactively for the attempt that
/// crashed.
#[test]
fn a_failed_apply_records_no_receipt_and_the_retry_records_its_own() {
    let crashed = run_id(7);
    let retry = run_id(8);
    let mut backend = Recorded::new();
    assert_eq!(receipt(&backend.created_by(PROJECT, CREATED)), None);
    backend.created(PROJECT, retry, CREATED);
    let observed = backend.created_by(PROJECT, CREATED);
    assert_eq!(receipt(&observed), Some(retry));
    assert_ne!(receipt(&observed), Some(crashed));
}

/// The trait's real implementations refuse for the reason the seam documents,
/// and each names the engine it targets. `adapters.rs` proves neither adapter
/// calls the backend to reach this answer.
#[test]
fn a_backend_without_a_creator_record_names_the_missing_api() {
    for kind in [
        BackendKind::Colima,
        BackendKind::DockerDesktop,
        BackendKind::DockerEngine,
    ] {
        let mut selection = selection();
        selection.kind = kind;
        match selection.adapter().created_by(PROJECT, CREATED) {
            Observed::Unknown(UnknownReason::NoBackendRecord(detail)) => {
                assert!(detail.contains(kind.label()), "{detail}");
                assert!(
                    detail.contains("keeps no record of which client created a container"),
                    "{detail}"
                );
            }
            other => panic!("{} must not answer: {other:?}", kind.label()),
        }
    }
}
