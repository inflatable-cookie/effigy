//! effigy#508: the creation window on both sides of the one Compose apply
//! (052 §8.12).
//!
//! The window reads the project's exact-identity inventory immediately before
//! and immediately after the one gated apply, under the exclusive `Gate(Env)`
//! the apply already holds, and binds every candidate to the **admitted
//! render's own declared values**. These proofs drive it with a strict fake
//! backend that replays scripted inventories, so every case is deterministic:
//! a fresh create, an unchanged re-up, a partial recreate, a duplicate service,
//! a label, service or image that disagrees with the render, a failed apply, a
//! timed-out window and a retry.

use super::*;
use effigy_kernel::gate::Wait;
use effigy_kernel::objects::Closure;

/// Ids the proofs use. They are exact full ids, never prefixes.
const FRESH: &str = "3f6a1d0c7b9e4f2a8c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f6a7b";
const DB: &str = "9c8b7a6f5e4d3c2b1a0f9e8d7c6b5a4f3e2d1c0b9a8f7e6d5c4b3a2f1e0d9c8b";
const OLD: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const RETRY: &str = "2222222222222222222222222222222222222222222222222222222222222222";
/// The render's own declared values: the window binds to these, never to what
/// a container reports about itself.
const SCOPE: &str = "4677bfcb81da18054ef32e71aa3f3c29";
const CHECKOUT: &str = "/fixtures/checkout";
const IMAGE: &str = "postgres:16";
/// The exact image id the fake backend resolves `IMAGE` to, and the spelling it
/// reports as a fully-qualified reference.
const IMAGE_ID: &str = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const QUALIFIED_IMAGE: &str = "docker.io/library/postgres:16";
/// A different image the backend knows: a container created from it is never
/// the render's target, even when the reference it reports agrees.
const OTHER_IMAGE_ID: &str =
    "sha256:fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
/// A stamp no apply has reached yet: the window requires `Created` no older
/// than its own start.
const AFTER_APPLY: &str = "2999-01-01T00:00:00.000000000Z";
/// A stamp from before any window this suite opens.
const BEFORE_APPLY: &str = "2000-01-01T00:00:00.000000000Z";

fn evidence() -> Evidence {
    Evidence::new("fake creation window".to_owned(), None).expect("fixed evidence")
}

/// The admitted render, declaring one service per entry with the ownership
/// labels and image reference the window binds.
fn render(services: &[&str]) -> Closure {
    let mut compose = String::from("services:\n");
    for service in services {
        compose.push_str(&format!(
            "  {service}:\n    image: {IMAGE}\n    labels:\n      com.effigy.managed: \"true\"\n      com.effigy.project: {PROJECT}\n      com.effigy.scope: {SCOPE}\n      com.effigy.checkout: {CHECKOUT}\n",
        ));
    }
    Closure::new(BTreeMap::from([(
        "compose.yml".to_owned(),
        compose.into_bytes(),
    )]))
    .expect("render closure")
}

/// One container the fake backend reports, carrying the values the render
/// declares.
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
            ("com.effigy.scope".to_owned(), SCOPE.to_owned()),
            ("com.effigy.checkout".to_owned(), CHECKOUT.to_owned()),
        ]),
        binds: vec![],
        image: IMAGE_ID.to_owned(),
        image_reference: IMAGE.to_owned(),
        created: AFTER_APPLY.to_owned(),
    }
}

/// A container with one label replaced, or removed when `value` is `None`.
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

/// A strict fake backend that replays the window's inventories in order, and
/// answers its image table from the references the proofs declare.
struct Inventories {
    selection: BackendSelection,
    queued: VecDeque<Observed<Vec<Workload>>>,
    reads: Rc<RefCell<Vec<String>>>,
    /// The image table: reference spellings the backend resolves, and their
    /// exact image ids.
    images: BTreeMap<String, Observed<String>>,
    image_reads: Rc<RefCell<Vec<String>>>,
}

impl Inventories {
    fn new() -> Self {
        Self {
            selection: selection(),
            queued: VecDeque::new(),
            reads: Rc::new(RefCell::new(Vec::new())),
            images: BTreeMap::from([
                (
                    IMAGE.to_owned(),
                    Observed::Verified(IMAGE_ID.to_owned(), evidence()),
                ),
                (
                    QUALIFIED_IMAGE.to_owned(),
                    Observed::Verified(IMAGE_ID.to_owned(), evidence()),
                ),
            ]),
            image_reads: Rc::new(RefCell::new(Vec::new())),
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

    /// Replace what this backend's image table answers for one reference.
    fn image(&mut self, reference: &str, answer: Observed<String>) -> &mut Self {
        self.images.insert(reference.to_owned(), answer);
        self
    }

    fn reads(&self) -> Vec<String> {
        self.reads.borrow().clone()
    }

    fn image_reads(&self) -> Vec<String> {
        self.image_reads.borrow().clone()
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
    /// The window's image-table read, answered from the scripted table. An
    /// unscripted reference is a mistake in the proof, not a fact: this fake
    /// refuses it loudly, exactly as it refuses an unmodelled inventory read.
    fn image_id_until(
        &mut self,
        _: &Path,
        reference: &str,
        _: std::time::Instant,
    ) -> Observed<String> {
        self.image_reads.borrow_mut().push(reference.to_owned());
        self.images
            .get(reference)
            .cloned()
            .unwrap_or_else(|| panic!("the proof did not script the image table for `{reference}`"))
    }
}

/// Run one gated apply with the scripted inventories and the admitted render,
/// and return what its creation window proved.
fn window(
    backend: &mut Inventories,
    apply: impl FnOnce() -> Result<(), BackendError>,
) -> (Creation, RunId, EnvRef) {
    let render = render(&["app", "db"]);
    window_with(backend, Some(&render), apply)
}

/// [`window`] with the render the proof chooses, or none at all.
fn window_with(
    backend: &mut Inventories,
    render: Option<&Closure>,
    apply: impl FnOnce() -> Result<(), BackendError>,
) -> (Creation, RunId, EnvRef) {
    let (result, creation, run, environment) = window_allowing_failure(backend, render, apply);
    assert!(result.is_ok(), "the apply itself: {result:?}");
    (creation, run, environment)
}

/// [`window_with`] for a proof that expects the apply to fail: the window's own
/// answer and the apply's error are both returned.
fn window_allowing_failure(
    backend: &mut Inventories,
    render: Option<&Closure>,
    apply: impl FnOnce() -> Result<(), BackendError>,
) -> (Result<(), EnvError>, Creation, RunId, EnvRef) {
    let home = home();
    let root = root(home.path());
    let ctx = EnvContext::new(&root, CancelToken::default()).expect("context");
    let environment = env("stack");
    let mut run =
        EnvRun::begin(&ctx, &environment, Operation::Up, Wait::FailFast).expect("admitted");
    let result =
        run.apply_with_backend(&selection(), PROJECT, up_workdir(), render, backend, apply);
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
    assert_eq!(receipt.run(), run);
    assert_eq!(receipt.environment(), &environment.resource);
    assert_eq!(receipt.project(), PROJECT);
    assert_eq!(receipt.engine(), selection().engine.as_str());
    assert_eq!(receipt.context(), selection().context);
    assert_eq!(receipt.evidence().before(), 0);
    assert_eq!(receipt.evidence().after(), 2);
    assert!(!receipt.evidence().source().is_empty());
    assert!(receipt.apply_started() < receipt.apply_completed());
    assert_eq!(
        receipt
            .created()
            .iter()
            .map(|created| (created.service(), created.container_id()))
            .collect::<Vec<_>>(),
        [("app", FRESH), ("db", DB)]
    );
    assert!(receipt.attached().is_empty());
    let app = receipt.for_service("app").expect("app was created");
    assert_eq!(app.image_reference(), IMAGE);
    assert_eq!(
        app.image_id(),
        IMAGE_ID,
        "the receipt binds the exact image id the render's reference resolves to"
    );
    assert_eq!(app.created(), AFTER_APPLY);
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
    assert!(receipt.created().is_empty(), "{:?}", receipt.created());
    assert_eq!(receipt.attached(), [OLD]);
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
            .created()
            .iter()
            .map(|created| created.container_id())
            .collect::<Vec<_>>(),
        [FRESH]
    );
    assert_eq!(receipt.attached(), [DB]);
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

/// A container for a service the admitted render does not declare is not this
/// window's target, however well it labels itself.
#[test]
fn a_service_the_render_does_not_declare_refuses_the_window() {
    let mut backend = Inventories::new();
    backend
        .then(Vec::new())
        .then(vec![row(PROJECT, FRESH, "cache")]);
    let (creation, _run, _environment) = window(&mut backend, || Ok(()));
    assert!(
        matches!(
            creation,
            Creation::Unproven(UnknownReason::Inconsistent(ref detail)) if detail.contains("not a service the admitted render declares")
        ),
        "{creation:?}"
    );
}

/// Every ownership label is bound to the render's declared value: a container
/// whose own label differs, or that omits one, is never attributed — the
/// container's self-report is not evidence.
#[test]
fn a_label_that_differs_from_the_admitted_render_refuses_the_window() {
    for (label, value) in [
        ("com.effigy.managed", Some("false")),
        ("com.effigy.project", Some("another-project")),
        ("com.effigy.scope", Some("another-scope")),
        ("com.effigy.checkout", Some("/elsewhere")),
        ("com.effigy.scope", None),
        ("com.effigy.checkout", None),
    ] {
        let mut backend = Inventories::new();
        backend
            .then(Vec::new())
            .then(vec![labelled(row(PROJECT, FRESH, "app"), label, value)]);
        let (creation, _run, _environment) = window(&mut backend, || Ok(()));
        match &creation {
            Creation::Unproven(UnknownReason::Inconsistent(detail)) => {
                assert!(detail.contains(label), "{label}: {detail}")
            }
            other => panic!("{label} did not refuse the window: {other:?}"),
        }
    }
}

/// Two render files that declare different values for one bound label refuse:
/// the window cannot know which one the apply used.
#[test]
fn a_render_that_declares_one_label_twice_refuses_the_window() {
    let files = BTreeMap::from([
        (
            "compose.yml".to_owned(),
            format!(
                "services:\n  app:\n    image: {IMAGE}\n    labels:\n      com.effigy.scope: {SCOPE}\n"
            )
            .into_bytes(),
        ),
        (
            "override.yml".to_owned(),
            b"services:\n  app:\n    labels:\n      com.effigy.scope: another-scope\n".to_vec(),
        ),
    ]);
    let render = Closure::new(files).expect("render closure");
    let mut backend = Inventories::new();
    backend
        .then(Vec::new())
        .then(vec![row(PROJECT, FRESH, "app")]);
    let (creation, _run, _environment) = window_with(&mut backend, Some(&render), || Ok(()));
    assert!(
        matches!(
            creation,
            Creation::Unproven(UnknownReason::Inconsistent(ref detail)) if detail.contains("com.effigy.scope")
        ),
        "{creation:?}"
    );
}

/// The image reference comes from the render too: a container created from
/// another image is not this window's target, and a render that declares no
/// image cannot bind one at all.
#[test]
fn an_image_that_disagrees_with_the_admitted_render_refuses_the_window() {
    let mut backend = Inventories::new();
    let mut other_image = row(PROJECT, FRESH, "app");
    other_image.image_reference = "postgres:17".to_owned();
    backend.then(Vec::new()).then(vec![other_image]);
    let (creation, _run, _environment) = window(&mut backend, || Ok(()));
    assert!(
        matches!(
            creation,
            Creation::Unproven(UnknownReason::Inconsistent(ref detail)) if detail.contains("postgres:17")
        ),
        "{creation:?}"
    );

    // A service the render declares without an image reference.
    let no_image = Closure::new(BTreeMap::from([(
        "compose.yml".to_owned(),
        format!(
            "services:\n  app:\n    build: .\n    labels:\n      com.effigy.managed: \"true\"\n      com.effigy.project: {PROJECT}\n      com.effigy.scope: {SCOPE}\n      com.effigy.checkout: {CHECKOUT}\n"
        )
        .into_bytes(),
    )]))
    .expect("render closure");
    let mut backend = Inventories::new();
    backend
        .then(Vec::new())
        .then(vec![row(PROJECT, FRESH, "app")]);
    let (creation, _run, _environment) = window_with(&mut backend, Some(&no_image), || Ok(()));
    assert!(
        matches!(
            creation,
            Creation::Unproven(UnknownReason::Inconsistent(ref detail)) if detail.contains("declares no image")
        ),
        "{creation:?}"
    );
}

/// Compose resolves a short reference before it creates the container, so the
/// render's `postgres:16` and the engine's fully-qualified spelling are one
/// image.
#[test]
fn a_fully_qualified_spelling_of_the_declared_image_is_accepted() {
    let mut backend = Inventories::new();
    let mut qualified = row(PROJECT, FRESH, "app");
    qualified.image_reference = QUALIFIED_IMAGE.to_owned();
    backend.then(Vec::new()).then(vec![qualified]);
    let (creation, run, _environment) = window(&mut backend, || Ok(()));
    assert_eq!(
        creation.created_by(PROJECT, FRESH).verified().copied(),
        Some(run),
        "{creation:?}"
    );
}

/// The reference is not the image, and it is mutable. A tag moved after the
/// container was created leaves the container carrying the render's own
/// reference while running a different image — a retag, a rebuilt tag, or an
/// actor's `docker run` of another image under the same name. The container's
/// engine-reported id is the immutable fact, so it must equal the id the
/// render's reference resolves to *now*: here the reference matches and the ids
/// disagree, and the window refuses. This is the same-reference retag case.
#[test]
fn a_same_reference_retag_with_a_different_image_id_refuses_the_window() {
    let mut backend = Inventories::new();
    let mut mismatched = row(PROJECT, FRESH, "app");
    // The reference the render declares, exactly as Compose records it...
    mismatched.image_reference = QUALIFIED_IMAGE.to_owned();
    // ...and an image the render does not declare.
    mismatched.image = OTHER_IMAGE_ID.to_owned();
    backend.then(Vec::new()).then(vec![mismatched]);
    let (creation, _run, _environment) = window(&mut backend, || Ok(()));
    let report = format!("was created from image id `{OTHER_IMAGE_ID}`");
    let expected = format!("resolves to image id `{IMAGE_ID}`");
    assert!(
        matches!(
            &creation,
            Creation::Unproven(UnknownReason::Inconsistent(detail))
                if detail.contains(&report) && detail.contains(&expected)
        ),
        "{creation:?}"
    );
    assert!(matches!(
        creation.created_by(PROJECT, FRESH),
        Observed::Unknown(UnknownReason::Inconsistent(_))
    ));
}

/// A backend that reports the container's image only as a reference (nerdctl)
/// cannot bind an image id at all: the reference is mutable, so resolving it
/// after the apply says where the tag points now, not which image this container
/// was created from — a retag inside the window would be invisible. The window
/// refuses and names the missing engine API instead of accepting the reference
/// as a stand-in for the id, and it does not even spend an image-table read on
/// the container's own reference.
#[test]
fn a_container_that_reports_only_a_reference_refuses_the_window() {
    let mut backend = Inventories::new();
    let mut reference_only = row(PROJECT, FRESH, "app");
    reference_only.image_reference = QUALIFIED_IMAGE.to_owned();
    reference_only.image = QUALIFIED_IMAGE.to_owned();
    backend.then(Vec::new()).then(vec![reference_only]);
    let (creation, _run, _environment) = window(&mut backend, || Ok(()));
    assert_eq!(
        backend.image_reads(),
        [IMAGE],
        "only the render's declared reference is resolved; the container's own reference is never a binding"
    );
    let reference = format!("reports image `{QUALIFIED_IMAGE}`");
    assert!(
        matches!(
            &creation,
            Creation::Unproven(UnknownReason::Inconsistent(detail))
                if detail.contains(&reference)
                    && detail.contains("no immutable image id")
                    && detail.contains("retag")
        ),
        "{creation:?}"
    );
    assert!(matches!(
        creation.created_by(PROJECT, FRESH),
        Observed::Unknown(UnknownReason::Inconsistent(_))
    ));
}

/// An image table that cannot answer for the render's own reference refuses the
/// window: without the exact image id there is nothing to bind, and the
/// reference alone is not the image a container runs.
#[test]
fn an_image_the_backend_cannot_resolve_refuses_the_window() {
    let mut backend = Inventories::new();
    backend.image(
        IMAGE,
        Observed::Unknown(UnknownReason::Inconsistent(
            "the image inspect of `postgres:16` answered nothing".to_owned(),
        )),
    );
    backend
        .then(Vec::new())
        .then(vec![row(PROJECT, FRESH, "app")]);
    let (creation, _run, _environment) = window(&mut backend, || Ok(()));
    assert!(
        matches!(
            &creation,
            Creation::Unproven(UnknownReason::Inconsistent(detail))
                if detail.contains("answered nothing")
        ),
        "{creation:?}"
    );
}

/// An `image inspect` answer that is not an exact image id (a short id, a
/// reference, an empty value) is not a binding: the window refuses rather than
/// compare two spellings that might name different images.
#[test]
fn an_image_inspect_answer_that_is_not_an_exact_image_id_refuses_the_window() {
    for answer in ["postgres:16", "0123456789abcdef", "", "sha256:xyz"] {
        let mut backend = Inventories::new();
        backend.image(IMAGE, Observed::Verified(answer.to_owned(), evidence()));
        backend
            .then(Vec::new())
            .then(vec![row(PROJECT, FRESH, "app")]);
        let (creation, _run, _environment) = window(&mut backend, || Ok(()));
        assert!(
            matches!(
                &creation,
                Creation::Unproven(UnknownReason::Inconsistent(detail))
                    if detail.contains("not an exact image id")
            ),
            "{answer}: {creation:?}"
        );
    }
}

/// A container that reports no image identity at all is unproven: the window
/// has nothing to bind to the render's image id.
#[test]
fn a_container_reporting_no_image_identity_refuses_the_window() {
    let mut backend = Inventories::new();
    let mut anonymous = row(PROJECT, FRESH, "app");
    anonymous.image = String::new();
    backend.then(Vec::new()).then(vec![anonymous]);
    let (creation, _run, _environment) = window(&mut backend, || Ok(()));
    assert!(
        matches!(
            &creation,
            Creation::Unproven(UnknownReason::Inconsistent(detail))
                if detail.contains("reports no image id")
        ),
        "{creation:?}"
    );
}

/// A container that appeared in this window but was created before the apply
/// started is not something the apply created.
#[test]
fn a_creation_stamp_before_the_apply_refuses_the_window() {
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

/// A one-off container is never a service target.
#[test]
fn a_one_off_container_refuses_the_window() {
    let mut backend = Inventories::new();
    backend.then(Vec::new()).then(vec![labelled(
        row(PROJECT, FRESH, "app"),
        "com.docker.compose.oneoff",
        Some("True"),
    )]);
    let (creation, _run, _environment) = window(&mut backend, || Ok(()));
    assert!(
        matches!(
            creation,
            Creation::Unproven(UnknownReason::Inconsistent(ref detail)) if detail.contains("one-off")
        ),
        "{creation:?}"
    );
}

/// An apply with no admitted render has no exact expected values to bind, so it
/// publishes no receipt even when its inventory is exact.
#[test]
fn an_apply_without_an_admitted_render_refuses_the_window() {
    let mut backend = Inventories::new();
    backend
        .then(Vec::new())
        .then(vec![row(PROJECT, FRESH, "app")]);
    let (creation, _run, _environment) = window_with(&mut backend, None, || Ok(()));
    assert!(
        matches!(
            creation,
            Creation::Unproven(UnknownReason::Inconsistent(ref detail)) if detail.contains("no admitted render")
        ),
        "{creation:?}"
    );
}

/// An apply that fails after it started proves nothing: the window is
/// incomplete, so no receipt is minted. An apply the backend never received
/// created nothing at all.
#[test]
fn a_failed_apply_mints_no_receipt() {
    let render = render(&["app", "db"]);
    let mut backend = Inventories::new();
    backend.then(Vec::new());
    let (failed, creation, _run, _environment) =
        window_allowing_failure(&mut backend, Some(&render), || {
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
    let (failed, creation, _run, _environment) =
        window_allowing_failure(&mut backend, Some(&render), || {
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
}
