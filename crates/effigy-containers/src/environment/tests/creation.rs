//! The engine create effect (effigy#510): exactly the containers one apply's
//! create report named are journalled as created, a retry reconciles that
//! effect or stays unknown, and no inventory, label match or later engine
//! answer can mint attribution.

use super::*;
use crate::environment::creation::{
    created_by, CreateReport, CreationReceipt, CreationRefusal, CreationRequest, CREATE_EFFECT,
};
use crate::environment::{EnvironmentBackend, Workload};
use crate::runtime::scope::{CHECKOUT_LABEL, MANAGED_LABEL, PROJECT_LABEL, SCOPE_LABEL};
use effigy_kernel::observe::AbsenceProof;
use effigy_kernel::run::EffectState;

const PRIMARY: &str = "app";

fn our_id() -> String {
    "a".repeat(64)
}

/// A foreign actor's container: identical labels, another name and id.
fn foreign_id() -> String {
    "b".repeat(64)
}

fn created_name() -> String {
    format!("{PROJECT}-{PRIMARY}-1")
}

/// One engine row as `inspect` answers it: managed labels, the Compose project,
/// the service, and an immutable image id.
fn workload(name: &str, id: &str) -> Workload {
    let labels = [
        ("com.docker.compose.project", PROJECT),
        ("com.docker.compose.service", PRIMARY),
        ("com.docker.compose.oneoff", "False"),
        (MANAGED_LABEL, "true"),
        (PROJECT_LABEL, PROJECT),
        (SCOPE_LABEL, "4677bfcb81da18054ef32e71aa3f3c29"),
        (CHECKOUT_LABEL, "/checkout"),
        ("com.effigy.service", PRIMARY),
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

/// A strict fake engine: it answers exactly the names it was given rows for,
/// and records every read.
#[derive(Default)]
struct CreationFake {
    selection: Option<BackendSelection>,
    rows: std::collections::BTreeMap<String, Workload>,
    /// When set, `containers_named` answers this instead of the rows.
    answer: Option<Observed<Vec<Workload>>>,
    /// When set, `image_id` answers this instead of the resolved id.
    image: Option<Observed<String>>,
    reads: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
}

impl CreationFake {
    fn with(row: Workload) -> Self {
        let mut fake = Self {
            selection: Some(selection()),
            ..Self::default()
        };
        fake.rows.insert(row.name.clone(), row);
        fake
    }

    fn unreadable() -> Self {
        Self {
            selection: Some(selection()),
            answer: Some(Observed::Unknown(UnknownReason::Inconsistent(
                "the engine answered nothing".to_owned(),
            ))),
            ..Self::default()
        }
    }

    fn absent() -> Self {
        Self {
            selection: Some(selection()),
            answer: Some(Observed::Absent(AbsenceProof::ContainerGone {
                id: created_name(),
            })),
            ..Self::default()
        }
    }

    fn reads(&self) -> Vec<String> {
        self.reads.borrow().clone()
    }
}

impl EnvironmentBackend for CreationFake {
    fn selection(&self) -> &BackendSelection {
        self.selection.as_ref().expect("fixture selection")
    }
    fn inventory(&mut self, _: &Path, _: &str) -> Observed<Vec<Workload>> {
        panic!("the create effect never takes an inventory")
    }
    fn containers_named(
        &mut self,
        _: &Path,
        project: &str,
        names: &[String],
    ) -> Observed<Vec<Workload>> {
        self.reads.borrow_mut().extend(names.iter().cloned());
        assert_eq!(project, PROJECT, "unmodelled project");
        if let Some(answer) = &self.answer {
            return answer.clone();
        }
        assert!(
            names.iter().all(|name| self.rows.contains_key(name)),
            "the adapter contract answers every name or refuses: {names:?}"
        );
        let rows: Vec<Workload> = names
            .iter()
            .filter_map(|name| self.rows.get(name).cloned())
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

fn request<'a>(workdir: &'a Path) -> CreationRequest<'a> {
    CreationRequest {
        workdir,
        project: PROJECT,
        primary_service: PRIMARY,
    }
}

fn report(name: &str) -> CreateReport {
    let mut report = CreateReport::default();
    report.created(name);
    report
}

/// A fresh first-up: the one container the apply reported creating is read back
/// by that exact name and sealed to this run.
#[test]
fn a_fresh_first_up_seals_exactly_the_created_container() {
    let workdir = std::path::Path::new(".");
    let mut fake = CreationFake::with(workload(&created_name(), &our_id()));
    let created = crate::environment::creation::resolve(
        &mut fake,
        request(workdir),
        &report(&created_name()),
    )
    .expect("the created set is exact");
    assert_eq!(created.len(), 1);
    assert_eq!(created[0].container_id(), our_id());
    assert_eq!(created[0].service(), PRIMARY);
    assert_eq!(created[0].name(), created_name());
    assert_eq!(created[0].image_id(), format!("sha256:{}", "c".repeat(64)));
    assert_eq!(fake.reads(), vec![created_name()]);
}

/// An unchanged re-up creates nothing: the receipt is empty, not a list of the
/// containers the apply attached.
#[test]
fn an_unchanged_reup_seals_no_created_id() {
    let workdir = std::path::Path::new(".");
    let mut fake = CreationFake::with(workload(&created_name(), &our_id()));
    let mut observed = CreateReport::default();
    observed.attached(&created_name());
    let created = crate::environment::creation::resolve(&mut fake, request(workdir), &observed)
        .expect("an attach-only apply is not a refusal");
    assert!(created.is_empty());
    assert!(
        fake.reads().is_empty(),
        "an unchanged re-up reads no container: {:?}",
        fake.reads()
    );
}

/// Partial recreation: only the container this apply actually recreated is
/// created, so the other, attached one never carries this run's receipt.
#[test]
fn a_partial_recreation_seals_only_the_created_containers() {
    let workdir = std::path::Path::new(".");
    let mut fake = CreationFake::with(workload(&created_name(), &our_id()));
    // The recreated primary and the attached cache are both reported; only the
    // recreated one is a creation.
    let mut observed = CreateReport::default();
    observed.created(&created_name());
    observed.attached(&format!("{PROJECT}-cache-1"));
    let created = crate::environment::creation::resolve(&mut fake, request(workdir), &observed)
        .expect("the created set is exact");
    assert_eq!(created.len(), 1);
    assert_eq!(created[0].container_id(), our_id());
    assert!(
        !fake
            .reads()
            .iter()
            .any(|name| name == &format!("{PROJECT}-cache-1")),
        "an attached container is never read as created"
    );
}

/// A competing actor's container carries indistinguishable labels and a
/// different name. It is never named by this apply's report, so it is never
/// read, never created and never attributed to this run.
#[test]
fn a_competing_identically_labelled_creation_is_never_adopted() {
    let workdir = std::path::Path::new(".");
    let mut foreign = workload(&format!("{PROJECT}-{PRIMARY}-2"), &foreign_id());
    // Indistinguishable labels: the only difference is the name the create
    // report names, which a competing actor cannot occupy.
    foreign
        .labels
        .insert("com.docker.compose.service".to_owned(), PRIMARY.to_owned());
    let mut fake = CreationFake::with(workload(&created_name(), &our_id()));
    fake.rows.insert(foreign.name.clone(), foreign.clone());
    let created = crate::environment::creation::resolve(
        &mut fake,
        request(workdir),
        &report(&created_name()),
    )
    .expect("the created set is exact");
    assert_eq!(created.len(), 1, "exactly one container was created");
    assert_eq!(created[0].container_id(), our_id());
    assert!(
        !fake.reads().iter().any(|name| name == &foreign.name),
        "the foreign container is never read: {:?}",
        fake.reads()
    );
    let receipt = sealed(&created);
    assert!(
        receipt.created_by(PROJECT, &foreign_id()).is_none(),
        "a foreign container is not this run's creation"
    );
    assert!(receipt.created_by(PROJECT, &our_id()).is_some());
}

/// The refusals: a container the engine cannot answer, two containers for one
/// service, a created set without the primary service, a mutable-only image,
/// an image that moved, a one-off container and a foreign service name.
#[test]
fn a_creation_that_is_not_exact_refuses() {
    let workdir = std::path::Path::new(".");
    let row = workload(&created_name(), &our_id());

    // An engine that proves the container gone never creates it.
    let mut missing = CreationFake::absent();
    assert!(matches!(
        crate::environment::creation::resolve(
            &mut missing,
            request(workdir),
            &report(&created_name())
        ),
        Err(CreationRefusal::Missing(_))
    ));

    let mut unreadable = CreationFake::unreadable();
    assert!(matches!(
        crate::environment::creation::resolve(
            &mut unreadable,
            request(workdir),
            &report(&created_name())
        ),
        Err(CreationRefusal::Unknown(_))
    ));

    // Two containers for the declared primary service.
    let mut duplicate = CreationFake::with(row.clone());
    duplicate.rows.insert(
        format!("{PROJECT}-{PRIMARY}-2"),
        workload(&format!("{PROJECT}-{PRIMARY}-2"), &foreign_id()),
    );
    let mut both = CreateReport::default();
    both.created(&created_name());
    both.created(&format!("{PROJECT}-{PRIMARY}-2"));
    assert!(matches!(
        crate::environment::creation::resolve(&mut duplicate, request(workdir), &both),
        Err(CreationRefusal::DuplicateService(service)) if service == PRIMARY
    ));

    // A created set that does not carry the declared primary service.
    let mut other = workload(&format!("{PROJECT}-cache-1"), &our_id());
    other
        .labels
        .insert("com.docker.compose.service".to_owned(), "cache".to_owned());
    let mut without_primary = CreationFake::with(other);
    assert!(matches!(
        crate::environment::creation::resolve(
            &mut without_primary,
            request(workdir),
            &report(&format!("{PROJECT}-cache-1"))
        ),
        Err(CreationRefusal::Partial(_))
    ));

    // A container whose own image field is only the mutable reference.
    let mut reference_only = CreationFake::with(row.clone());
    reference_only
        .rows
        .get_mut(&created_name())
        .expect("fixture row")
        .image = "fixture:latest".to_owned();
    assert!(matches!(
        crate::environment::creation::resolve(
            &mut reference_only,
            request(workdir),
            &report(&created_name())
        ),
        Err(CreationRefusal::ReferenceOnlyImage(_))
    ));

    // The reference now names another image than the container runs.
    let mut moved = CreationFake::with(row.clone());
    moved.image = Some(Observed::Verified(
        format!("sha256:{}", "d".repeat(64)),
        evidence(),
    ));
    assert!(matches!(
        crate::environment::creation::resolve(
            &mut moved,
            request(workdir),
            &report(&created_name())
        ),
        Err(CreationRefusal::ImageMismatch(_))
    ));

    // A one-off `compose run` container is never an environment target.
    let mut oneoff = CreationFake::with(row.clone());
    oneoff
        .rows
        .get_mut(&created_name())
        .expect("fixture row")
        .labels
        .insert("com.docker.compose.oneoff".to_owned(), "True".to_owned());
    assert!(matches!(
        crate::environment::creation::resolve(
            &mut oneoff,
            request(workdir),
            &report(&created_name())
        ),
        Err(CreationRefusal::OneOff(_))
    ));

    // A name that does not carry the project and the service the labels claim.
    let mut misnamed = CreationFake::with(row);
    let misnamed_row = misnamed.rows.remove(&created_name()).expect("fixture row");
    let other_name = format!("{PROJECT}-other-1");
    let mut renamed = misnamed_row;
    renamed.name = other_name.clone();
    misnamed.rows.insert(other_name.clone(), renamed);
    assert!(matches!(
        crate::environment::creation::resolve(
            &mut misnamed,
            request(workdir),
            &report(&other_name)
        ),
        Err(CreationRefusal::Service(_))
    ));
}

/// A receipt sealed to a fixture run, for the parts of the receipt that do not
/// need the journal.
fn sealed(created: &[crate::environment::CreatedContainer]) -> CreationReceipt {
    crate::environment::creation::seal(
        &env("stack").resource,
        RunId::new(1_760_000_000_000, [9; 10]).expect("run"),
        PROJECT,
        selection().engine.as_str(),
        &selection().context,
        created.to_vec(),
        Vec::new(),
    )
}

/// One activation's create effect journals its receipt, and `created_by`
/// answers from that journal alone: this run for the container it created, the
/// absence proof for a foreign container with the same labels.
#[test]
fn one_create_effect_journals_its_receipt_and_answers_created_by() {
    let fixture = home();
    let state = root(fixture.path());
    let ctx = EnvContext::new(&state, CancelToken::default()).expect("context");
    let stack = env("stack");
    let workdir = std::path::Path::new(".");
    let mut fake = CreationFake::with(workload(&created_name(), &our_id()));
    let mut run = EnvRun::begin(&ctx, &stack, Operation::Up, Wait::FailFast).expect("run");
    let run_id = run.id();
    let receipt = run
        .create_with(&mut fake, &selection(), request(workdir), |report| {
            report.created(&created_name());
            Ok(())
        })
        .expect("the create is exact");
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
    assert!(
        matches!(
            created_by(
                &ctx.observe(),
                ctx.root,
                &stack.resource,
                PROJECT,
                &foreign_id()
            ),
            Observed::Absent(AbsenceProof::NoCreator { .. })
        ),
        "a foreign container is not attributed"
    );
    // Another project never adopts this container.
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

/// A retry of one activation returns the receipt it journalled and runs no
/// second create.
#[test]
fn a_retry_returns_the_same_journalled_receipt_without_a_second_create() {
    let fixture = home();
    let state = root(fixture.path());
    let ctx = EnvContext::new(&state, CancelToken::default()).expect("context");
    let stack = env("stack");
    let workdir = std::path::Path::new(".");
    let mut fake = CreationFake::with(workload(&created_name(), &our_id()));
    let mut run = EnvRun::begin(&ctx, &stack, Operation::Up, Wait::FailFast).expect("run");
    let mut applies = 0;
    let first = run
        .create_with(&mut fake, &selection(), request(workdir), |report| {
            applies += 1;
            report.created(&created_name());
            Ok(())
        })
        .expect("create");
    let second = run
        .create_with(&mut fake, &selection(), request(workdir), |report| {
            applies += 10;
            report.created(&created_name());
            Ok(())
        })
        .expect("the retry reconciles the same effect");
    assert_eq!(applies, 1, "a retry never runs a second apply");
    assert_eq!(first, second);
    run.finish(true).expect("settle");
}

/// A create whose result was never authenticated stays unknown: a retry does
/// not run the apply again and never mints attribution from the engine answer
/// that now exists.
#[test]
fn an_unrecorded_create_stays_unknown_and_never_mints_from_a_later_answer() {
    let fixture = home();
    let state = root(fixture.path());
    let ctx = EnvContext::new(&state, CancelToken::default()).expect("context");
    let stack = env("stack");
    let workdir = std::path::Path::new(".");
    let mut fake = CreationFake::unreadable();
    let mut run = EnvRun::begin(&ctx, &stack, Operation::Up, Wait::FailFast).expect("run");
    let refused = run.create_with(&mut fake, &selection(), request(workdir), |report| {
        report.created(&created_name());
        Ok(())
    });
    match refused {
        Err(EnvError::Backend(BackendError::Uncertain(message))) => {
            assert!(message.contains("cannot attribute"), "{message}")
        }
        other => panic!("expected an unattributable refusal, got {other:?}"),
    }
    // The engine now answers a well-formed container: the retry still refuses,
    // because this activation never journalled a result for its create.
    fake.answer = None;
    fake.rows
        .insert(created_name(), workload(&created_name(), &our_id()));
    let mut applies = 0;
    let retried = run.create_with(&mut fake, &selection(), request(workdir), |report| {
        applies += 1;
        report.created(&created_name());
        Ok(())
    });
    assert!(applies == 0, "the retry ran a second apply");
    match retried {
        Err(EnvError::Backend(BackendError::Uncertain(message))) => assert!(
            message.contains("no authenticated result"),
            "the refusal names the unrecorded effect: {message}"
        ),
        other => panic!("expected an unauthenticated create, got {other:?}"),
    }
    run.finish(false).expect("settle");
    assert!(
        matches!(
            created_by(
                &ctx.observe(),
                ctx.root,
                &stack.resource,
                PROJECT,
                &our_id()
            ),
            Observed::Absent(AbsenceProof::NoCreator { .. })
        ),
        "an unrecorded create attributes nothing"
    );
}

/// A failed apply journals no receipt and attributes nothing.
#[test]
fn a_failed_apply_creates_no_receipt() {
    let fixture = home();
    let state = root(fixture.path());
    let ctx = EnvContext::new(&state, CancelToken::default()).expect("context");
    let stack = env("stack");
    let workdir = std::path::Path::new(".");
    let mut fake = CreationFake::with(workload(&created_name(), &our_id()));
    let mut run = EnvRun::begin(&ctx, &stack, Operation::Up, Wait::FailFast).expect("run");
    let refused = run.create_with(&mut fake, &selection(), request(workdir), |_| {
        Err(BackendError::NotStarted("the runtime refused".to_owned()))
    });
    assert!(matches!(refused, Err(EnvError::Backend(_))), "{refused:?}");
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

/// Two activations naming one created id is ambiguous: no run is chosen.
#[test]
fn one_container_named_by_two_activations_is_never_attributed() {
    let fixture = home();
    let state = root(fixture.path());
    let ctx = EnvContext::new(&state, CancelToken::default()).expect("context");
    let stack = env("stack");
    let workdir = std::path::Path::new(".");
    for _ in 0..2 {
        let mut fake = CreationFake::with(workload(&created_name(), &our_id()));
        let mut run = EnvRun::begin(&ctx, &stack, Operation::Up, Wait::FailFast).expect("run");
        run.create_with(&mut fake, &selection(), request(workdir), |report| {
            report.created(&created_name());
            Ok(())
        })
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

/// The journal effect is the create kind, applied, and carries the receipt as
/// its result: that is what `created_by` reads.
#[test]
fn the_create_effect_is_applied_with_the_receipt_as_its_result() {
    let fixture = home();
    let state = root(fixture.path());
    let ctx = EnvContext::new(&state, CancelToken::default()).expect("context");
    let stack = env("stack");
    let workdir = std::path::Path::new(".");
    let mut fake = CreationFake::with(workload(&created_name(), &our_id()));
    let mut run = EnvRun::begin(&ctx, &stack, Operation::Up, Wait::FailFast).expect("run");
    let run_id = run.id();
    run.create_with(&mut fake, &selection(), request(workdir), |report| {
        report.created(&created_name());
        Ok(())
    })
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
        "the applied create settles as a retained effect, and its receipt stays"
    );
    let stored = crate::environment::creation::decode_stored(
        create.receipt.as_ref().expect("the result is recorded"),
    )
    .expect("the stored receipt reads");
    assert_eq!(stored.project(), PROJECT);
    assert_eq!(stored.created()[0].container_id(), our_id());
    assert_eq!(stored.backend().0, selection().engine.as_str());
}
