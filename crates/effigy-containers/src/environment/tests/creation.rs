//! effigy#508: the creation window on both sides of the one Compose apply
//! (052 §8.12).
//!
//! The window reads the project's exact-identity inventory immediately before
//! and immediately after the one gated apply, under the exclusive `Gate(Env)`
//! the apply already holds. These proofs drive it with a strict fake backend
//! that replays scripted inventories, so every case is deterministic: a fresh
//! create, an unchanged re-up, a partial recreate, a duplicate service, a
//! missing or mismatched label, a failed apply, a timed-out window and a retry.

use super::*;
use effigy_kernel::gate::Wait;
use effigy_kernel::id::Generation;

/// Ids the proofs use. They are exact full ids, never prefixes.
const FRESH: &str = "3f6a1d0c7b9e4f2a8c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f6a7b";
const DB: &str = "9c8b7a6f5e4d3c2b1a0f9e8d7c6b5a4f3e2d1c0b9a8f7e6d5c4b3a2f1e0d9c8b";
const OLD: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const RETRY: &str = "2222222222222222222222222222222222222222222222222222222222222222";
/// A stamp no apply has reached yet: the window requires `Created` no older
/// than its own start.
const AFTER_APPLY: &str = "2999-01-01T00:00:00.000000000Z";
/// A stamp from before any window this suite opens.
const BEFORE_APPLY: &str = "2000-01-01T00:00:00.000000000Z";

fn evidence() -> Evidence {
    Evidence::new("fake creation window".to_owned(), None).expect("fixed evidence")
}

/// One container the fake backend reports, with the labels a Compose-created
/// Effigy container carries.
fn row(project: &str, id: &str, service: &str) -> Workload {
    Workload {
        id: id.to_owned(),
        name: format!("{project}-{service}-1"),
        running: true,
        labels: BTreeMap::from([
            ("com.docker.compose.project".to_owned(), project.to_owned()),
            ("com.docker.compose.service".to_owned(), service.to_owned()),
            ("com.effigy.managed".to_owned(), "true".to_owned()),
            ("com.effigy.project".to_owned(), project.to_owned()),
            (
                "com.effigy.scope".to_owned(),
                "4677bfcb81da18054ef32e71aa3f3c29".to_owned(),
            ),
            (
                "com.effigy.checkout".to_owned(),
                "/fixtures/checkout".to_owned(),
            ),
        ]),
        binds: vec![],
        image: "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_owned(),
        created: AFTER_APPLY.to_owned(),
    }
}

/// A container with one label replaced, or added when `value` is `Some`.
fn labelled(mut workload: Workload, label: &str, value: Option<&str>) -> Workload {
    match value {
        Some(value) => {
            workload.labels.insert(label.to_owned(), value.to_owned());
        }
        None => {
            workload.labels.remove(label);
        }
    }
    workload
}

fn when(mut workload: Workload, created: &str) -> Workload {
    workload.created = created.to_owned();
    workload
}

/// A strict fake backend that replays the window's inventories in order.
struct Inventories {
    selection: BackendSelection,
    queued: VecDeque<Observed<Vec<Workload>>>,
    reads: Rc<RefCell<Vec<String>>>,
}

impl Inventories {
    fn new() -> Self {
        Self {
            selection: selection(),
            queued: VecDeque::new(),
            reads: Rc::new(RefCell::new(Vec::new())),
        }
    }

    /// One inventory read: the window takes the before read first, then the
    /// after read.
    fn then(&mut self, rows: Vec<Workload>) -> &mut Self {
        self.queued.push_back(Observed::Verified(rows, evidence()));
        self
    }

    fn unavailable(&mut self, reason: UnknownReason) -> &mut Self {
        self.queued.push_back(Observed::Unknown(reason));
        self
    }

    fn reads(&self) -> Vec<String> {
        self.reads.borrow().clone()
    }
}

impl EnvironmentBackend for Inventories {
    fn selection(&self) -> &BackendSelection {
        &self.selection
    }
    fn inventory(&mut self, _: &Path, project: &str) -> Observed<Vec<Workload>> {
        panic!("the window reads through inventory_until, not inventory ({project})")
    }
    fn inventory_until(
        &mut self,
        _: &Path,
        project: &str,
        _: std::time::Instant,
    ) -> Observed<Vec<Workload>> {
        assert_eq!(project, PROJECT, "the window asked about another project");
        self.reads.borrow_mut().push(project.to_owned());
        self.queued
            .pop_front()
            .unwrap_or_else(|| Observed::Verified(Vec::new(), evidence()))
    }
    fn created_by(&mut self, project: &str, container_id: &str) -> Observed<RunId> {
        panic!("a proof does not ask the raw backend about `{container_id}` in `{project}`")
    }
}

/// Run one gated apply with the scripted inventories and return what its
/// creation window proved.
fn window(
    backend: &mut Inventories,
    apply: impl FnOnce() -> Result<(), BackendError>,
) -> (Creation, RunId, EnvRef) {
    let (result, creation, run, environment) = window_with(backend, apply);
    assert!(result.is_ok(), "the apply itself: {result:?}");
    (creation, run, environment)
}

/// [`window`] for a proof that expects the apply to fail: the window's own
/// answer and the apply's error are both returned.
fn window_with(
    backend: &mut Inventories,
    apply: impl FnOnce() -> Result<(), BackendError>,
) -> (Result<(), EnvError>, Creation, RunId, EnvRef) {
    let home = home();
    let root = root(home.path());
    let ctx = EnvContext::new(&root, CancelToken::default()).expect("context");
    let environment = env("stack");
    let mut run =
        EnvRun::begin(&ctx, &environment, Operation::Up, Wait::FailFast).expect("admitted");
    let result = run.apply_with_backend(&selection(), PROJECT, up_workdir(), backend, apply);
    let creation = run.creation();
    let run_id = run.id();
    run.finish(result.is_ok()).expect("finished");
    (result, creation, run_id, environment)
}

/// A fresh create names the exact full ids this run's apply created, bound to
/// the run, the environment, the backend, the project and the evidence.
#[test]
fn a_fresh_create_names_the_exact_containers_this_apply_created() {
    let mut backend = Inventories::new();
    backend
        .then(Vec::new())
        .then(vec![row(PROJECT, FRESH, "app"), row(PROJECT, DB, "db")]);
    let (creation, run, environment) = window(&mut backend, || Ok(()));
    assert_eq!(backend.reads(), [PROJECT, PROJECT]);
    let receipt = creation.receipt().expect("a completed window is a receipt");
    assert_eq!(receipt.run, run);
    assert_eq!(receipt.environment, environment.resource);
    assert_eq!(receipt.project, PROJECT);
    assert_eq!(receipt.engine, selection().engine.as_str());
    assert_eq!(receipt.context, selection().context);
    assert_eq!(receipt.evidence.before, 0);
    assert_eq!(receipt.evidence.after, 2);
    assert_eq!(
        receipt
            .created
            .iter()
            .map(|created| (created.service.as_str(), created.container_id.as_str()))
            .collect::<Vec<_>>(),
        [("app", FRESH), ("db", DB)]
    );
    assert!(receipt.attached.is_empty());
    assert_eq!(
        receipt
            .for_service("app")
            .map(|row| row.container_id.as_str()),
        Some(FRESH)
    );
    // The receipt attributes exactly those ids, and only to this run.
    for id in [FRESH, DB] {
        assert_eq!(
            creation.created_by(PROJECT, id).verified().copied(),
            Some(run),
            "{id}"
        );
    }
}

/// An unchanged re-up creates nothing: the containers it found are adopted,
/// and none of them is `created_by` this run.
#[test]
fn an_unchanged_re_up_creates_nothing_and_adopts_what_it_found() {
    let mut backend = Inventories::new();
    backend
        .then(vec![row(PROJECT, OLD, "app")])
        .then(vec![row(PROJECT, OLD, "app")]);
    let (creation, _run, _environment) = window(&mut backend, || Ok(()));
    let receipt = creation.receipt().expect("a completed window is a receipt");
    assert!(receipt.created.is_empty(), "{:?}", receipt.created);
    assert_eq!(receipt.attached, [OLD]);
    assert_eq!(
        creation.created_by(PROJECT, OLD),
        Observed::Absent(AbsenceProof::NoCreator {
            creator: effigy_kernel::observe::CreatorRef {
                project: PROJECT.to_owned(),
                container_id: OLD.to_owned(),
            },
        })
    );
}

/// A partial recreate names the container the apply replaced and nothing else.
#[test]
fn a_partial_recreate_names_only_the_replaced_container() {
    let mut backend = Inventories::new();
    backend
        .then(vec![row(PROJECT, OLD, "app"), row(PROJECT, DB, "db")])
        .then(vec![row(PROJECT, FRESH, "app"), row(PROJECT, DB, "db")]);
    let (creation, run, _environment) = window(&mut backend, || Ok(()));
    let receipt = creation.receipt().expect("a completed window is a receipt");
    assert_eq!(
        receipt
            .created
            .iter()
            .map(|created| created.container_id.as_str())
            .collect::<Vec<_>>(),
        [FRESH]
    );
    assert_eq!(receipt.attached, [DB]);
    assert_eq!(
        creation.created_by(PROJECT, FRESH).verified().copied(),
        Some(run)
    );
    assert_eq!(creation.created_by(PROJECT, DB).verified().copied(), None);
}

/// Two containers for one service refuse: the window cannot say which one its
/// apply created.
#[test]
fn a_duplicate_service_refuses_the_window() {
    let mut backend = Inventories::new();
    backend
        .then(Vec::new())
        .then(vec![row(PROJECT, FRESH, "app"), row(PROJECT, DB, "app")]);
    let (creation, _run, _environment) = window(&mut backend, || Ok(()));
    assert!(
        matches!(
            creation,
            Creation::Unproven(UnknownReason::Inconsistent(ref detail)) if detail.contains("service `app`")
        ),
        "{creation:?}"
    );
    assert!(matches!(
        creation.created_by(PROJECT, FRESH),
        Observed::Unknown(UnknownReason::Inconsistent(_))
    ));
}

/// A created container must carry Effigy's managed, project, scope and
/// checkout labels and a single service identity; a missing or mismatched one
/// refuses the window rather than attributing it.
#[test]
fn a_missing_or_mismatched_label_refuses_the_window() {
    let cases: [(&str, Option<&str>, &str); 5] = [
        ("com.effigy.managed", None, "com.effigy.managed"),
        (
            "com.effigy.project",
            Some("another-project"),
            "com.effigy.project",
        ),
        ("com.effigy.scope", None, "com.effigy.scope"),
        ("com.effigy.checkout", Some("  "), "com.effigy.checkout"),
        (
            "com.docker.compose.service",
            None,
            "Compose service identity",
        ),
    ];
    for (label, value, named) in cases {
        let mut backend = Inventories::new();
        backend
            .then(Vec::new())
            .then(vec![labelled(row(PROJECT, FRESH, "app"), label, value)]);
        let (creation, _run, _environment) = window(&mut backend, || Ok(()));
        match &creation {
            Creation::Unproven(UnknownReason::Inconsistent(detail)) => {
                assert!(detail.contains(named), "{label}: {detail}")
            }
            other => panic!("{label} did not refuse the window: {other:?}"),
        }
    }
}

/// A container that reports another image in its Compose label, or was created
/// before the apply started, is not something this window created.
#[test]
fn an_image_or_creation_stamp_mismatch_refuses_the_window() {
    // The Compose image label disagrees with the image the container runs.
    let mut backend = Inventories::new();
    backend.then(Vec::new()).then(vec![labelled(
        row(PROJECT, FRESH, "app"),
        "com.docker.compose.image",
        Some("sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"),
    )]);
    let (creation, _run, _environment) = window(&mut backend, || Ok(()));
    assert!(
        matches!(
            creation,
            Creation::Unproven(UnknownReason::Inconsistent(ref detail)) if detail.contains("Compose image label")
        ),
        "{creation:?}"
    );

    // A stamp older than the apply start: the container predates the window.
    let mut backend = Inventories::new();
    backend
        .then(Vec::new())
        .then(vec![when(row(PROJECT, FRESH, "app"), BEFORE_APPLY)]);
    let (creation, _run, _environment) = window(&mut backend, || Ok(()));
    assert!(
        matches!(
            creation,
            Creation::Unproven(UnknownReason::Inconsistent(ref detail)) if detail.contains("before this window's apply started")
        ),
        "{creation:?}"
    );
}

/// An apply that fails after it started proves nothing: the window is
/// incomplete, so no receipt is minted. An apply the backend never received
/// created nothing at all.
#[test]
fn a_failed_apply_mints_no_receipt() {
    let mut backend = Inventories::new();
    backend.then(Vec::new());
    let (failed, creation, _run, _environment) = window_with(&mut backend, || {
        Err(BackendError::Uncertain("compose up exited with 1".into()))
    });
    assert!(failed.is_err(), "the apply failure is reported");
    assert!(
        matches!(
            creation,
            Creation::Unproven(UnknownReason::Inconsistent(ref detail)) if detail.contains("exited with 1")
        ),
        "{creation:?}"
    );
    // Only the before read happened: nothing is read after an unproven apply.
    assert_eq!(backend.reads(), [PROJECT]);

    let mut backend = Inventories::new();
    backend.then(Vec::new());
    let (failed, creation, _run, _environment) = window_with(&mut backend, || {
        Err(BackendError::NotStarted("runtime refused".into()))
    });
    assert!(failed.is_err(), "the apply failure is reported");
    assert_eq!(creation, Creation::NothingCreated);
    assert!(matches!(
        creation.created_by(PROJECT, FRESH),
        Observed::Absent(AbsenceProof::NoCreator { .. })
    ));
}

/// A window whose read timed out is unproven, and the retry's own complete
/// window is the only receipt: the first attempt's partial window can never
/// name a container.
#[test]
fn a_timed_out_window_cannot_mint_a_receipt_and_the_retry_records_its_own() {
    let mut backend = Inventories::new();
    backend
        .then(Vec::new())
        .unavailable(UnknownReason::TimedOut {
            phase: "list project".to_owned(),
        });
    let (first, first_run, _environment) = window(&mut backend, || Ok(()));
    assert!(
        matches!(
            first,
            Creation::Unproven(UnknownReason::TimedOut { ref phase }) if phase == "list project"
        ),
        "{first:?}"
    );
    assert!(matches!(
        first.created_by(PROJECT, RETRY),
        Observed::Unknown(UnknownReason::TimedOut { .. })
    ));

    // The retry is its own run with its own complete window.
    let mut retry_backend = Inventories::new();
    retry_backend
        .then(Vec::new())
        .then(vec![row(PROJECT, RETRY, "app")]);
    let (retry, retry_run, _environment) = window(&mut retry_backend, || Ok(()));
    assert_ne!(first_run, retry_run);
    assert_eq!(
        retry.created_by(PROJECT, RETRY).verified().copied(),
        Some(retry_run)
    );
    // The first attempt still refuses, even for the id the retry created.
    assert!(matches!(
        first.created_by(PROJECT, RETRY),
        Observed::Unknown(UnknownReason::TimedOut { .. })
    ));
}

/// Another project's container is not this window's, even with a matching id.
#[test]
fn another_projects_container_is_not_this_windows_receipt() {
    let mut backend = Inventories::new();
    backend
        .then(Vec::new())
        .then(vec![row(PROJECT, FRESH, "app")]);
    let (creation, _run, _environment) = window(&mut backend, || Ok(()));
    assert_eq!(
        creation
            .created_by("another-project", FRESH)
            .verified()
            .copied(),
        None
    );
}

/// A run that has not applied yet has no window: it never answers for anyone's
/// container.
#[test]
fn a_run_that_has_not_applied_has_no_window() {
    let home = home();
    let root = root(home.path());
    let ctx = EnvContext::new(&root, CancelToken::default()).expect("context");
    let environment = env("stack");
    let run = EnvRun::begin(&ctx, &environment, Operation::Up, Wait::FailFast).expect("admitted");
    let creation = run.creation();
    assert!(matches!(creation, Creation::Unproven(_)), "{creation:?}");
    assert!(matches!(
        creation.created_by(PROJECT, FRESH),
        Observed::Unknown(_)
    ));
}

/// The receipt's own absence proof is scoped to what the window covered: a
/// container it did not create is an absence for this window, never a receipt.
#[test]
fn a_receipt_scopes_absence_to_the_window_it_covers() {
    let mut backend = Inventories::new();
    backend
        .then(Vec::new())
        .then(vec![row(PROJECT, FRESH, "app")]);
    let (creation, run, _environment) = window(&mut backend, || Ok(()));
    let receipt = creation.receipt().expect("receipt");
    assert_eq!(
        receipt.created_by(PROJECT, FRESH).verified().copied(),
        Some(run)
    );
    assert_eq!(receipt.created_by(PROJECT, OLD).verified().copied(), None);
    let _unused = Generation::new(1);
}
