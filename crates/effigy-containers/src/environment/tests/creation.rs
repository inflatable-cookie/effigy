//! The engine create effect (effigy#510): a receipt is minted only from the
//! full ids an engine create response named, and nothing else — no inventory,
//! label match, timestamp or Compose progress line — can stand in for it.
//!
//! These are fixture proofs for the attribution path and its refusals. No
//! container adapter issues such a create yet, so the production path records
//! an explicit absence of attribution; the last test here proves exactly that,
//! and the rest prove the semantics the day an adapter can supply the response.

use super::*;
use crate::environment::backend::{CreatedContainerId, WorkspaceCreation};
use crate::environment::creation::{
    created_by, CreationRefusal, CreationRequest, RecordedCreation, CREATE_EFFECT,
};
use crate::environment::{EnvironmentBackend, Workload};
use crate::runtime::scope::{PROJECT_LABEL, SCOPE_LABEL};
use effigy_kernel::observe::AbsenceProof;
use effigy_kernel::run::EffectState;

const PRIMARY: &str = "workspace";

fn our_id() -> String {
    "a".repeat(64)
}

/// A foreign actor's container: identical labels, another id.
fn foreign_id() -> String {
    "b".repeat(64)
}

fn our_name() -> String {
    format!("{PROJECT}-{PRIMARY}-1")
}

/// One engine row as `inspect` answers it for a created container.
fn workload(name: &str, id: &str) -> Workload {
    let labels = [
        ("com.docker.compose.project", PROJECT),
        ("com.docker.compose.service", PRIMARY),
        ("com.docker.compose.oneoff", "False"),
        (PROJECT_LABEL, PROJECT),
        // A shared-runtime-identity environment omits the scope label, so it is
        // not creation identity.
        (SCOPE_LABEL, "4677bfcb81da18054ef32e71aa3f3c29"),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_owned(), value.to_owned()))
    .collect();
    Workload {
        id: id.to_owned(),
        name: name.to_owned(),
        running: true,
        labels,
        binds: Vec::new(),
        image: format!("sha256:{}", "c".repeat(64)),
        image_reference: "fixture:latest".to_owned(),
    }
}

/// A strict fake engine: it answers exactly the ids it was given rows for, and
/// records every read and create.
struct CreationFake {
    selection: BackendSelection,
    rows: std::collections::BTreeMap<String, Workload>,
    /// The engine create response this backend can report, if any.
    response: Option<Observed<Vec<CreatedContainerId>>>,
    /// When set, `containers_by_id` answers this instead of the rows.
    by_id: Option<Observed<Vec<Workload>>>,
    /// When set, `image_id` answers this instead of the resolved id.
    image: Option<Observed<String>>,
    reads: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
    creates: std::rc::Rc<std::cell::Cell<usize>>,
}

impl CreationFake {
    fn attributed_to(row: Workload) -> Self {
        let id = row.id.clone();
        let name = row.name.clone();
        let mut fake = Self::with_rows(vec![row]);
        fake.response = Some(Observed::Verified(
            vec![CreatedContainerId::from_response(&id, &name)],
            evidence(),
        ));
        fake
    }

    fn with_rows(rows: Vec<Workload>) -> Self {
        Self {
            selection: selection(),
            rows: rows.into_iter().map(|row| (row.id.clone(), row)).collect(),
            response: None,
            by_id: None,
            image: None,
            reads: Default::default(),
            creates: Default::default(),
        }
    }

    /// A backend that issues no create of its own: the production shape.
    fn no_engine_create() -> Self {
        let mut fake = Self::with_rows(Vec::new());
        fake.response = None;
        fake
    }

    fn reads(&self) -> Vec<String> {
        self.reads.borrow().clone()
    }

    fn creates(&self) -> usize {
        self.creates.get()
    }
}

impl EnvironmentBackend for CreationFake {
    fn selection(&self) -> &BackendSelection {
        &self.selection
    }
    fn inventory(&mut self, _: &Path, _: &str) -> Observed<Vec<Workload>> {
        panic!("the create effect never takes an inventory")
    }
    fn create_workspace_container(
        &mut self,
        request: &WorkspaceCreation<'_>,
    ) -> Observed<Vec<CreatedContainerId>> {
        self.creates.set(self.creates.get() + 1);
        assert_eq!(request.project, PROJECT, "unmodelled project");
        assert_eq!(request.service, PRIMARY, "unmodelled service");
        match &self.response {
            Some(response) => response.clone(),
            None => Observed::Unknown(UnknownReason::NoBackendRecord(format!(
                "this fixture backend issues no create of its own for `{}`",
                request.service
            ))),
        }
    }
    fn containers_by_id(&mut self, _: &Path, ids: &[String]) -> Observed<Vec<Workload>> {
        self.reads.borrow_mut().extend(ids.iter().cloned());
        if let Some(answer) = &self.by_id {
            return answer.clone();
        }
        assert!(
            ids.iter().all(|id| self.rows.contains_key(id)),
            "the adapter contract answers every created id or refuses: {ids:?}"
        );
        let rows: Vec<Workload> = ids
            .iter()
            .filter_map(|id| self.rows.get(id).cloned())
            .collect();
        Observed::Verified(rows, evidence())
    }
    fn image_id(&mut self, _: &Path, reference: &str) -> Observed<String> {
        if let Some(answer) = &self.image {
            return answer.clone();
        }
        assert_eq!(reference, "fixture:latest", "unmodelled image");
        Observed::Verified(format!("sha256:{}", "c".repeat(64)), evidence())
    }
}

fn evidence() -> effigy_kernel::observe::Evidence {
    effigy_kernel::observe::Evidence::new("creation fixture".into(), None).expect("fixed evidence")
}

fn request(workdir: &std::path::Path) -> CreationRequest<'_> {
    CreationRequest {
        workdir,
        project: PROJECT,
        primary_service: PRIMARY,
        compose_files: &[],
    }
}

fn engine_response(id: &str, name: &str) -> CreatedContainerId {
    CreatedContainerId::from_response(id, name)
}

/// A fresh first-up: the id the engine create returned is read back by that
/// exact id and seals as the created container.
#[test]
fn a_fresh_first_up_seals_the_id_the_engine_create_returned() {
    let workdir = std::path::Path::new(".");
    let mut fake = CreationFake::attributed_to(workload(&our_name(), &our_id()));
    let created = crate::environment::creation::verify(
        &mut fake,
        request(workdir),
        &[engine_response(&our_id(), &our_name())],
    )
    .expect("the created id is exact");
    assert_eq!(created.len(), 1);
    assert_eq!(created[0].container_id(), our_id());
    assert_eq!(created[0].service(), PRIMARY);
    assert_eq!(created[0].name(), our_name());
    assert_eq!(created[0].image_id(), format!("sha256:{}", "c".repeat(64)));
    assert_eq!(fake.reads(), vec![our_id()]);
}

/// An unchanged re-up creates nothing: an apply whose create returns no id
/// seals no created container.
#[test]
fn an_unchanged_reup_seals_no_created_id() {
    let workdir = std::path::Path::new(".");
    let mut fake = CreationFake::with_rows(Vec::new());
    let created =
        crate::environment::creation::verify(&mut fake, request(workdir), &[]).expect("nothing");
    assert!(created.is_empty());
    assert!(
        fake.reads().is_empty(),
        "an unchanged re-up reads no container: {:?}",
        fake.reads()
    );
}

/// Partial recreation: only the container the engine create returned is
/// created, so a container the apply left running never carries this run's
/// receipt.
#[test]
fn a_partial_recreation_seals_only_the_created_id() {
    let workdir = std::path::Path::new(".");
    let mut fake = CreationFake::attributed_to(workload(&our_name(), &our_id()));
    let created = crate::environment::creation::verify(
        &mut fake,
        request(workdir),
        &[engine_response(&our_id(), &our_name())],
    )
    .expect("the created id is exact");
    assert_eq!(created.len(), 1);
    let receipt = sealed(&created);
    assert!(receipt.created_by(PROJECT, &our_id()).is_some());
    assert!(
        receipt.created_by(PROJECT, &foreign_id()).is_none(),
        "a container this create did not name is never this run's creation"
    );
}

/// A competing actor's container carries indistinguishable labels. Its id was
/// never in a create response, so it is never read and never attributed.
#[test]
fn a_competing_identically_labelled_creation_is_never_adopted() {
    let workdir = std::path::Path::new(".");
    let foreign = workload(&format!("{PROJECT}-{PRIMARY}-2"), &foreign_id());
    let mut fake = CreationFake::attributed_to(workload(&our_name(), &our_id()));
    fake.rows.insert(foreign.id.clone(), foreign.clone());
    let created = crate::environment::creation::verify(
        &mut fake,
        request(workdir),
        &[engine_response(&our_id(), &our_name())],
    )
    .expect("the created id is exact");
    assert_eq!(created.len(), 1, "exactly one container was created");
    assert!(
        !fake.reads().iter().any(|id| id == &foreign.id),
        "the foreign container is never read: {:?}",
        fake.reads()
    );
    let receipt = sealed(&created);
    assert!(receipt.created_by(PROJECT, &foreign_id()).is_none());
}

/// The refusals: an id the engine cannot answer, an unreadable answer, two
/// containers for one service, a created set without the workspace service, a
/// mutable-only image, an image that moved, a one-off container, another
/// project and a name that disagrees with the labels.
#[test]
fn a_creation_that_is_not_exact_refuses() {
    let workdir = std::path::Path::new(".");
    let row = workload(&our_name(), &our_id());

    // The engine proves the id gone: verified non-existence, so no receipt.
    let mut missing = CreationFake::with_rows(vec![row.clone()]);
    missing.by_id = Some(Observed::Absent(AbsenceProof::ContainerGone {
        id: our_id(),
    }));
    assert!(matches!(
        crate::environment::creation::verify(
            &mut missing,
            request(workdir),
            &[engine_response(&our_id(), &our_name())]
        ),
        Err(CreationRefusal::Missing(_))
    ));

    // An unreadable read-back never becomes a partial receipt.
    let mut unreadable = CreationFake::with_rows(vec![row.clone()]);
    unreadable.by_id = Some(Observed::Unknown(UnknownReason::Inconsistent(
        "the engine answered nothing".to_owned(),
    )));
    assert!(matches!(
        crate::environment::creation::verify(
            &mut unreadable,
            request(workdir),
            &[engine_response(&our_id(), &our_name())]
        ),
        Err(CreationRefusal::Unknown(_)) | Err(CreationRefusal::Missing(_))
    ));

    // Two created containers for the declared workspace service.
    let mut duplicate = CreationFake::with_rows(vec![
        row.clone(),
        workload(&format!("{PROJECT}-{PRIMARY}-2"), &foreign_id()),
    ]);
    assert!(matches!(
        crate::environment::creation::verify(
            &mut duplicate,
            request(workdir),
            &[
                engine_response(&our_id(), &our_name()),
                engine_response(&foreign_id(), &format!("{PROJECT}-{PRIMARY}-2")),
            ]
        ),
        Err(CreationRefusal::DuplicateService(service)) if service == PRIMARY
    ));

    // A created set that does not carry the declared workspace service.
    let mut other = workload(&format!("{PROJECT}-cache-1"), &our_id());
    other
        .labels
        .insert("com.docker.compose.service".to_owned(), "cache".to_owned());
    let mut without_workspace = CreationFake::with_rows(vec![other.clone()]);
    assert!(matches!(
        crate::environment::creation::verify(
            &mut without_workspace,
            request(workdir),
            &[engine_response(&our_id(), &other.name)]
        ),
        Err(CreationRefusal::Partial(_))
    ));

    // A container whose own image field is only the mutable reference.
    let mut reference_only = CreationFake::with_rows(vec![row.clone()]);
    reference_only
        .rows
        .get_mut(&our_id())
        .expect("fixture row")
        .image = "fixture:latest".to_owned();
    assert!(matches!(
        crate::environment::creation::verify(
            &mut reference_only,
            request(workdir),
            &[engine_response(&our_id(), &our_name())]
        ),
        Err(CreationRefusal::ReferenceOnlyImage(_))
    ));

    // The reference now names another image than the container runs.
    let mut moved = CreationFake::with_rows(vec![row.clone()]);
    moved.image = Some(Observed::Verified(
        format!("sha256:{}", "d".repeat(64)),
        evidence(),
    ));
    assert!(matches!(
        crate::environment::creation::verify(
            &mut moved,
            request(workdir),
            &[engine_response(&our_id(), &our_name())]
        ),
        Err(CreationRefusal::ImageMismatch(_))
    ));

    // A one-off `compose run` container is never an environment target.
    let mut oneoff = CreationFake::with_rows(vec![row.clone()]);
    oneoff
        .rows
        .get_mut(&our_id())
        .expect("fixture row")
        .labels
        .insert("com.docker.compose.oneoff".to_owned(), "True".to_owned());
    assert!(matches!(
        crate::environment::creation::verify(
            &mut oneoff,
            request(workdir),
            &[engine_response(&our_id(), &our_name())]
        ),
        Err(CreationRefusal::OneOff(_))
    ));

    // A container of another project, answered for this create's id.
    let mut foreign_project = CreationFake::with_rows(vec![row.clone()]);
    foreign_project
        .rows
        .get_mut(&our_id())
        .expect("fixture row")
        .labels
        .insert(
            "com.docker.compose.project".to_owned(),
            "elsewhere".to_owned(),
        );
    assert!(matches!(
        crate::environment::creation::verify(
            &mut foreign_project,
            request(workdir),
            &[engine_response(&our_id(), &our_name())]
        ),
        Err(CreationRefusal::Service(_))
    ));

    // A name that does not carry the project and the service the labels claim.
    let mut misnamed = CreationFake::with_rows(vec![row.clone()]);
    let mut renamed = misnamed.rows.remove(&our_id()).expect("fixture row");
    renamed.name = format!("{PROJECT}-other-1");
    misnamed.rows.insert(our_id(), renamed.clone());
    assert!(matches!(
        crate::environment::creation::verify(
            &mut misnamed,
            request(workdir),
            &[engine_response(&our_id(), &renamed.name)]
        ),
        Err(CreationRefusal::Service(_))
    ));
}

/// A receipt sealed to a fixture run, for the parts of the receipt that do not
/// need the journal.
fn sealed(created: &[crate::environment::CreatedContainer]) -> crate::environment::CreationReceipt {
    crate::environment::creation::seal(
        &env("stack").resource,
        RunId::new(1_760_000_000_000, [9; 10]).expect("run"),
        request(std::path::Path::new(".")),
        selection().engine.as_str(),
        &selection().context,
        created.to_vec(),
        Vec::new(),
    )
}

/// One activation's engine create journals its receipt, and `created_by`
/// answers from that journal alone: this run for the id the create returned, the
/// absence proof for a foreign container.
#[test]
fn one_create_effect_journals_its_receipt_and_answers_created_by() {
    let fixture_home = home();
    let state = root(fixture_home.path());
    let ctx = EnvContext::new(&state, CancelToken::default()).expect("context");
    let stack = env("stack");
    let workdir = std::path::Path::new(".");
    let mut fake = CreationFake::attributed_to(workload(&our_name(), &our_id()));
    let mut run = EnvRun::begin(&ctx, &stack, Operation::Up, Wait::FailFast).expect("run");
    let run_id = run.id();
    let mut applies = 0;
    let outcome = run
        .create_with(&mut fake, &selection(), request(workdir), || {
            applies += 1;
            Ok(())
        })
        .expect("the create effect");
    assert_eq!(applies, 1);
    let receipt = outcome.receipt().expect("the create is attributed");
    assert_eq!(receipt.run(), run_id);
    assert_eq!(receipt.created_ids().collect::<Vec<_>>(), vec![our_id()]);
    assert!(receipt.created_by(PROJECT, &our_id()).is_some());
    assert!(receipt.created_by(PROJECT, &foreign_id()).is_none());
    run.finish(true).expect("settle");

    match created_by(
        &ctx.observe(),
        ctx.root,
        &stack.resource,
        PROJECT,
        &our_id(),
    ) {
        Observed::Verified(run, _) => assert_eq!(run, run_id),
        other => panic!("expected this run, got {other:?}"),
    }
    assert!(matches!(
        created_by(
            &ctx.observe(),
            ctx.root,
            &stack.resource,
            PROJECT,
            &foreign_id()
        ),
        Observed::Absent(AbsenceProof::NoCreator { .. })
    ));
    assert!(matches!(
        created_by(
            &ctx.observe(),
            ctx.root,
            &stack.resource,
            "other",
            &our_id()
        ),
        Observed::Absent(AbsenceProof::NoCreator { .. })
    ));
}

/// A backend that issues no create of its own records an explicit absence of
/// attribution, and `created_by` says so instead of claiming nothing was
/// created. This is today's production shape.
#[test]
fn an_unattributed_apply_is_never_answered_as_absence() {
    let fixture_home = home();
    let state = root(fixture_home.path());
    let ctx = EnvContext::new(&state, CancelToken::default()).expect("context");
    let stack = env("stack");
    let workdir = std::path::Path::new(".");
    let mut fake = CreationFake::no_engine_create();
    let mut run = EnvRun::begin(&ctx, &stack, Operation::Up, Wait::FailFast).expect("run");
    let mut applies = 0;
    let outcome = run
        .create_with(&mut fake, &selection(), request(workdir), || {
            applies += 1;
            Ok(())
        })
        .expect("the apply still runs");
    assert_eq!(applies, 1);
    assert!(outcome.receipt().is_none());
    assert!(outcome
        .unattributed()
        .is_some_and(|reason| reason.contains("issues no create")));
    assert!(outcome.created_ids().is_empty());
    run.finish(true).expect("settle");
    match created_by(
        &ctx.observe(),
        ctx.root,
        &stack.resource,
        PROJECT,
        &our_id(),
    ) {
        Observed::Unknown(UnknownReason::NoBackendRecord(reason)) => {
            assert!(reason.contains("issues no create"), "{reason}")
        }
        other => panic!("expected an unattributed unknown, got {other:?}"),
    }
}

/// A retry of one activation returns the outcome it journalled and runs no
/// second apply or create.
#[test]
fn a_retry_returns_the_same_journalled_outcome_without_a_second_apply() {
    let fixture_home = home();
    let state = root(fixture_home.path());
    let ctx = EnvContext::new(&state, CancelToken::default()).expect("context");
    let stack = env("stack");
    let workdir = std::path::Path::new(".");
    let mut fake = CreationFake::attributed_to(workload(&our_name(), &our_id()));
    let mut run = EnvRun::begin(&ctx, &stack, Operation::Up, Wait::FailFast).expect("run");
    let mut applies = 0;
    let first = run
        .create_with(&mut fake, &selection(), request(workdir), || {
            applies += 1;
            Ok(())
        })
        .expect("create");
    let second = run
        .create_with(&mut fake, &selection(), request(workdir), || {
            applies += 10;
            Ok(())
        })
        .expect("the retry reconciles the same effect");
    assert_eq!(applies, 1, "a retry never runs a second apply");
    assert_eq!(first, second);
    assert_eq!(fake.creates(), 1, "a retry asks for no second create");
    run.finish(true).expect("settle");
}

/// A create whose response could not be verified stays unknown: a retry does
/// not run the apply again and never mints attribution afterwards.
#[test]
fn an_unverified_create_stays_unknown_and_never_mints_later() {
    let fixture_home = home();
    let state = root(fixture_home.path());
    let ctx = EnvContext::new(&state, CancelToken::default()).expect("context");
    let stack = env("stack");
    let workdir = std::path::Path::new(".");
    // The engine create returned an id, but the container it names runs a
    // mutable reference rather than an image id.
    let mut fake = CreationFake::attributed_to(workload(&our_name(), &our_id()));
    fake.rows.get_mut(&our_id()).expect("fixture row").image = "fixture:latest".to_owned();
    let mut run = EnvRun::begin(&ctx, &stack, Operation::Up, Wait::FailFast).expect("run");
    let refused = run.create_with(&mut fake, &selection(), request(workdir), || Ok(()));
    match refused {
        Err(EnvError::Backend(BackendError::Uncertain(message))) => {
            assert!(message.contains("cannot attribute"), "{message}")
        }
        other => panic!("expected an unattributable refusal, got {other:?}"),
    }
    // The engine now answers a well-formed container: the retry still refuses,
    // because this activation never journalled an authenticated result.
    fake.rows.get_mut(&our_id()).expect("fixture row").image = format!("sha256:{}", "c".repeat(64));
    let mut applies = 0;
    let retried = run.create_with(&mut fake, &selection(), request(workdir), || {
        applies += 1;
        Ok(())
    });
    assert_eq!(applies, 0, "the retry ran a second apply");
    match retried {
        Err(EnvError::Backend(BackendError::Uncertain(message))) => assert!(
            message.contains("no authenticated result"),
            "the refusal names the unrecorded effect: {message}"
        ),
        other => panic!("expected an unauthenticated create, got {other:?}"),
    }
    run.finish(false).expect("settle");
}

/// A failed apply journals no receipt and attributes nothing.
#[test]
fn a_failed_apply_creates_no_receipt() {
    let fixture_home = home();
    let state = root(fixture_home.path());
    let ctx = EnvContext::new(&state, CancelToken::default()).expect("context");
    let stack = env("stack");
    let workdir = std::path::Path::new(".");
    let mut fake = CreationFake::attributed_to(workload(&our_name(), &our_id()));
    let mut run = EnvRun::begin(&ctx, &stack, Operation::Up, Wait::FailFast).expect("run");
    let refused = run.create_with(&mut fake, &selection(), request(workdir), || {
        Err(BackendError::NotStarted("the runtime refused".to_owned()))
    });
    assert!(matches!(refused, Err(EnvError::Backend(_))), "{refused:?}");
    assert_eq!(fake.creates(), 0, "a failed apply asks for no create");
    run.finish(false).expect("settle");
    assert!(matches!(
        created_by(
            &ctx.observe(),
            ctx.root,
            &stack.resource,
            PROJECT,
            &our_id()
        ),
        Observed::Absent(AbsenceProof::NoCreator { .. })
    ));
}

/// One container named by two activations' create responses is ambiguous: no
/// run is chosen.
#[test]
fn one_container_named_by_two_activations_is_never_attributed() {
    let fixture_home = home();
    let state = root(fixture_home.path());
    let ctx = EnvContext::new(&state, CancelToken::default()).expect("context");
    let stack = env("stack");
    let workdir = std::path::Path::new(".");
    for _ in 0..2 {
        let mut fake = CreationFake::attributed_to(workload(&our_name(), &our_id()));
        let mut run = EnvRun::begin(&ctx, &stack, Operation::Up, Wait::FailFast).expect("run");
        run.create_with(&mut fake, &selection(), request(workdir), || Ok(()))
            .expect("create");
        run.finish(true).expect("settle");
    }
    assert!(matches!(
        created_by(
            &ctx.observe(),
            ctx.root,
            &stack.resource,
            PROJECT,
            &our_id()
        ),
        Observed::Unknown(UnknownReason::Inconsistent(_))
    ));
}

/// The journal effect is the create kind, retained after settle, and carries
/// the record as its result: that is what `created_by` reads.
#[test]
fn the_create_effect_is_applied_with_the_record_as_its_result() {
    let fixture_home = home();
    let state = root(fixture_home.path());
    let ctx = EnvContext::new(&state, CancelToken::default()).expect("context");
    let stack = env("stack");
    let workdir = std::path::Path::new(".");
    let mut fake = CreationFake::attributed_to(workload(&our_name(), &our_id()));
    let mut run = EnvRun::begin(&ctx, &stack, Operation::Up, Wait::FailFast).expect("run");
    let run_id = run.id();
    run.create_with(&mut fake, &selection(), request(workdir), || Ok(()))
        .expect("create");
    run.finish(true).expect("settle");
    let reader = match Store::read(&ctx.observe(), ctx.root) {
        Observed::Verified(reader, _) => reader,
        other => panic!("store unreadable: {other:?}"),
    };
    let effects = match effigy_kernel::run::effects(&reader, run_id) {
        Observed::Verified(effects, _) => effects,
        other => panic!("journal unreadable: {other:?}"),
    };
    let create = effects
        .iter()
        .find(|effect| effect.spec.kind == CREATE_EFFECT)
        .expect("the create effect is journalled");
    assert_eq!(
        create.state,
        EffectState::Retained,
        "the applied create settles as a retained effect, and its record stays"
    );
    let stored = crate::environment::creation::decode_stored(
        create.receipt.as_ref().expect("the result is recorded"),
    )
    .expect("the stored record reads");
    assert_eq!(stored.project(), PROJECT);
    assert_eq!(stored.created()[0].container_id(), our_id());
    assert_eq!(
        stored.backend().map(|(engine, _)| engine),
        Some(selection().engine.as_str())
    );
    assert!(matches!(
        stored.record(),
        RecordedCreation::Attributed { .. }
    ));
}
