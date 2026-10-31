//! Gate-scoped ID difference fixtures. These do not authenticate a foreign
//! same-user actor forging identical labels inside the exclusive window.
use super::*;
use crate::environment::creation::{created_by, CreationRequest, CreationWindow};
use crate::environment::{EnvironmentBackend, Workload};
use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

const PRIMARY: &str = "workspace";
fn id(byte: char) -> String {
    byte.to_string().repeat(64)
}
fn image() -> String {
    format!("sha256:{}", id('c'))
}
fn evidence() -> effigy_kernel::observe::Evidence {
    effigy_kernel::observe::Evidence::new("inventory fixture".into(), None).unwrap()
}
fn verified<T>(value: T) -> Observed<T> {
    Observed::Verified(value, evidence())
}
fn unknown<T>() -> Observed<T> {
    Observed::Unknown(UnknownReason::Inconsistent("unreadable fixture".into()))
}
fn labels() -> BTreeMap<String, String> {
    [
        ("com.effigy.managed", "true"),
        ("com.effigy.project", PROJECT),
        ("com.effigy.scope", "scope"),
        ("com.effigy.checkout", "/checkout"),
    ]
    .into_iter()
    .map(|(k, v)| (k.into(), v.into()))
    .collect()
}
fn row(service: &str, byte: char) -> Workload {
    let mut labels = labels();
    labels.insert("com.docker.compose.project".into(), PROJECT.into());
    labels.insert("com.docker.compose.service".into(), service.into());
    labels.insert("com.docker.compose.oneoff".into(), "False".into());
    Workload {
        id: id(byte),
        name: format!("{PROJECT}-{service}-1"),
        running: true,
        labels,
        binds: Vec::new(),
        image: image(),
        image_reference: "fixture:latest".into(),
    }
}
fn render(services: &[&str]) -> Closure {
    let services: BTreeMap<_, _> = services
        .iter()
        .map(|name| {
            (
                *name,
                serde_json::json!({"image":"fixture:latest", "labels": labels()}),
            )
        })
        .collect();
    Closure::new(
        [(
            "compose.yml".into(),
            serde_json::to_vec(&serde_json::json!({"services":services})).unwrap(),
        )]
        .into_iter()
        .collect(),
    )
    .unwrap()
}
fn request(render: &Closure) -> CreationRequest<'_> {
    CreationRequest {
        workdir: Path::new("."),
        project: PROJECT,
        primary_service: PRIMARY,
        compose_files: &[],
        render: Some(render),
        preflight: None,
        deadline: Instant::now() + Duration::from_secs(30),
    }
}
struct Fake {
    selection: BackendSelection,
    inventories: VecDeque<Observed<Vec<Workload>>>,
    images: VecDeque<Observed<String>>,
    reads: usize,
}
impl Fake {
    fn new(before: Vec<Workload>, after: Vec<Workload>) -> Self {
        Self {
            selection: selection(),
            inventories: [verified(before), verified(after)].into(),
            images: VecDeque::new(),
            reads: 0,
        }
    }
}
impl EnvironmentBackend for Fake {
    fn selection(&self) -> &BackendSelection {
        &self.selection
    }
    fn inventory(&mut self, _: &Path, _: &str) -> Observed<Vec<Workload>> {
        panic!("unbounded observation")
    }
    fn inventory_until(&mut self, _: &Path, _: &str, deadline: Instant) -> Observed<Vec<Workload>> {
        self.reads += 1;
        assert!(Instant::now() < deadline);
        self.inventories
            .pop_front()
            .expect("unexpected inventory read")
    }
    fn image_id_until(&mut self, _: &Path, _: &str, deadline: Instant) -> Observed<String> {
        assert!(Instant::now() < deadline);
        self.images.pop_front().unwrap_or_else(|| verified(image()))
    }
}
fn infer(
    fake: &mut Fake,
    render: &Closure,
) -> Result<
    (Vec<crate::environment::CreatedContainer>, Vec<String>),
    crate::environment::CreationRefusal,
> {
    let request = request(render);
    CreationWindow::open(fake, request)?
        .close(fake, request)
        .map(|difference| (difference.created, difference.attached))
}

#[test]
fn fresh_reup_and_partial_recreation_partition_exact_ids() {
    let render = render(&[PRIMARY, "database"]);
    let workspace = row(PRIMARY, 'a');
    let db = row("database", 'b');
    let (created, attached) = infer(
        &mut Fake::new(vec![], vec![workspace.clone(), db.clone()]),
        &render,
    )
    .unwrap();
    assert_eq!(created.len(), 2);
    assert!(attached.is_empty());
    let (created, attached) = infer(
        &mut Fake::new(
            vec![workspace.clone(), db.clone()],
            vec![workspace.clone(), db.clone()],
        ),
        &render,
    )
    .unwrap();
    assert!(created.is_empty());
    assert_eq!(attached.len(), 2);
    let replacement = row(PRIMARY, 'd');
    let (created, attached) = infer(
        &mut Fake::new(vec![workspace, db.clone()], vec![replacement.clone(), db]),
        &render,
    )
    .unwrap();
    assert_eq!(created[0].container_id(), replacement.id);
    assert_eq!(attached, [id('b')]);
}

#[test]
fn incomplete_extra_duplicate_foreign_or_reference_only_rows_refuse() {
    let render = render(&[PRIMARY]);
    let good = row(PRIMARY, 'a');
    let mut wrong_label = good.clone();
    wrong_label
        .labels
        .insert("com.effigy.checkout".into(), "/elsewhere".into());
    let mut reference_only = good.clone();
    reference_only.image = "fixture:latest".into();
    let mut wrong_reference = good.clone();
    wrong_reference.image_reference = "other:latest".into();
    let mut short = good.clone();
    short.id = "abc".into();
    let mut foreign_project = good.clone();
    foreign_project
        .labels
        .insert("com.docker.compose.project".into(), "foreign".into());
    let mut oneoff = good.clone();
    oneoff
        .labels
        .insert("com.docker.compose.oneoff".into(), "True".into());
    for rows in [
        vec![],
        vec![good.clone(), row("extra", 'b')],
        vec![good.clone(), row(PRIMARY, 'b')],
        vec![wrong_label],
        vec![reference_only],
        vec![wrong_reference],
        vec![short],
        vec![foreign_project],
        vec![oneoff],
    ] {
        assert!(
            infer(&mut Fake::new(vec![], rows.clone()), &render).is_err(),
            "accepted {rows:?}"
        );
    }
    // A foreign wrong-label row already present before apply also refuses.
    let mut wrong = good.clone();
    wrong.labels.remove("com.effigy.managed");
    assert!(infer(&mut Fake::new(vec![wrong], vec![good]), &render).is_err());
}

#[test]
fn unreadable_inventory_or_changed_attached_image_refuses() {
    let render = render(&[PRIMARY]);
    for endpoint in [0, 1] {
        let mut fake = Fake::new(vec![], vec![row(PRIMARY, 'a')]);
        fake.inventories[endpoint] = unknown();
        assert!(infer(&mut fake, &render).is_err());
    }
    let mut fake = Fake::new(vec![row(PRIMARY, 'a')], vec![row(PRIMARY, 'a')]);
    fake.images = [verified(image()), verified(format!("sha256:{}", id('d')))].into();
    assert!(infer(&mut fake, &render).is_err());
}

#[test]
fn same_user_identically_labelled_actor_in_window_is_outside_the_guarantee() {
    // Observation cannot distinguish that actor from Compose. This cell
    // documents the narrowed guarantee explicitly, without actor authentication.
    let render = render(&[PRIMARY]);
    let (created, _) = infer(&mut Fake::new(vec![], vec![row(PRIMARY, 'f')]), &render).unwrap();
    assert_eq!(created[0].container_id(), id('f'));
}

#[test]
fn journal_retry_never_reapplies_or_fabricates_a_later_receipt() {
    let fixture = home();
    let state = root(fixture.path());
    let ctx = EnvContext::new(&state, CancelToken::default()).unwrap();
    let stack = env("stack");
    let render = render(&[PRIMARY]);
    for readable in [true, false] {
        let mut fake = Fake::new(vec![], vec![row(PRIMARY, 'a')]);
        if !readable {
            fake.inventories[1] = unknown();
        }
        let mut run = EnvRun::begin(&ctx, &stack, Operation::Up, Wait::FailFast).unwrap();
        let mut applies = 0;
        let first = run
            .create_with(&mut fake, &selection(), request(&render), || {
                applies += 1;
                Ok(())
            })
            .unwrap();
        let reads = fake.reads;
        assert_eq!(first.receipt().is_some(), readable);
        let second = run
            .create_with(&mut fake, &selection(), request(&render), || {
                applies += 1;
                Ok(())
            })
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(applies, 1);
        assert_eq!(fake.reads, reads);
        run.finish(true).unwrap();
    }
    assert!(matches!(
        created_by(&ctx.observe(), ctx.root, &stack.resource, PROJECT, &id('f')),
        Observed::Unknown(_)
    ));
}

#[test]
fn failed_apply_never_records_attribution() {
    let fixture = home();
    let state = root(fixture.path());
    let ctx = EnvContext::new(&state, CancelToken::default()).unwrap();
    let stack = env("stack");
    let render = render(&[PRIMARY]);
    let mut fake = Fake::new(vec![], vec![row(PRIMARY, 'a')]);
    let mut run = EnvRun::begin(&ctx, &stack, Operation::Up, Wait::FailFast).unwrap();
    assert!(run
        .create_with(&mut fake, &selection(), request(&render), || Err(
            BackendError::Uncertain("partial apply".into())
        ))
        .is_err());
    assert!(run
        .create_with(&mut fake, &selection(), request(&render), || panic!(
            "retry applied"
        ))
        .is_err());
    run.finish(false).unwrap();
    assert!(!matches!(
        created_by(&ctx.observe(), ctx.root, &stack.resource, PROJECT, &id('a')),
        Observed::Verified(_, _)
    ));
}

#[test]
fn stale_preflight_images_refuse_under_the_gate() {
    let render = render(&[PRIMARY]);
    let mut fake = Fake::new(vec![], vec![row(PRIMARY, 'a')]);
    let preflight =
        crate::environment::CreationPreflight::observe(&mut fake, request(&render)).unwrap();
    fake.images
        .push_back(verified(format!("sha256:{}", id('d'))));
    let mut gated = request(&render);
    gated.preflight = Some(&preflight);
    assert!(CreationWindow::open(&mut fake, gated).is_err());
    assert_eq!(
        fake.reads, 0,
        "stale image expectations never become an inventory window"
    );
}

#[test]
fn journalled_generation_and_recheck_refuse_replaced_or_retagged_rows() {
    let fixture = home();
    let state = root(fixture.path());
    let ctx = EnvContext::new(&state, CancelToken::default()).unwrap();
    let stack = env("stack");
    let render = render(&[PRIMARY]);
    let mut fake = Fake::new(vec![], vec![row(PRIMARY, 'a')]);
    let mut run = EnvRun::begin(&ctx, &stack, Operation::Up, Wait::FailFast).unwrap();
    let outcome = run
        .create_with(&mut fake, &selection(), request(&render), || Ok(()))
        .unwrap();
    let receipt = outcome.receipt().unwrap();
    assert_eq!(receipt.generation(), run.fence().generation.get());
    fake.inventories
        .push_back(verified(vec![row(PRIMARY, 'a')]));
    assert!(receipt
        .reobserve(&mut fake, Instant::now() + Duration::from_secs(1))
        .is_ok());
    fake.inventories
        .push_back(verified(vec![row(PRIMARY, 'b')]));
    assert!(receipt
        .reobserve(&mut fake, Instant::now() + Duration::from_secs(1))
        .is_err());
    fake.inventories
        .push_back(verified(vec![row(PRIMARY, 'a')]));
    fake.images
        .push_back(verified(format!("sha256:{}", id('d'))));
    assert!(receipt
        .reobserve(&mut fake, Instant::now() + Duration::from_secs(1))
        .is_err());
    run.finish(true).unwrap();
}

#[test]
fn stale_generation_during_or_after_apply_never_reaches_target_binding() {
    for during_apply in [true, false] {
        let fixture = home();
        let state = root(fixture.path());
        let ctx = EnvContext::new(&state, CancelToken::default()).unwrap();
        let stack = env("stack");
        let render = render(&[PRIMARY]);
        let reservation =
            effigy_kernel::workspace_session::WorkspaceActivationReservation::reserve(
                &ctx.cx,
                &state,
                &stack.resource,
                Wait::FailFast,
            )
            .unwrap();
        let mut run =
            EnvRun::begin_claimed(&ctx, &stack, Operation::Up, Wait::FailFast, &reservation)
                .unwrap();
        let mut fake = Fake::new(vec![], vec![row(PRIMARY, 'a')]);
        let advance = || {
            let mut store = effigy_kernel::store::Store::open(&ctx.cx, &state).unwrap();
            store
                .txn(&ctx.cx, |txn| txn.next_generation(&stack.resource))
                .unwrap();
        };
        let result = run.create_with(&mut fake, &selection(), request(&render), || {
            if during_apply {
                advance();
            }
            Ok(())
        });
        if during_apply {
            assert!(result.is_err());
        } else {
            assert!(result.unwrap().receipt().is_some());
            advance();
            let reads = fake.reads;
            assert!(run
                .recheck_creation(
                    &reservation,
                    &mut fake,
                    Instant::now() + Duration::from_secs(1)
                )
                .is_err());
            assert_eq!(
                reads, fake.reads,
                "stale fence refuses before engine observation"
            );
        }
        let _ = run.finish(false); // A deliberately stale run may retain uncertainty.
    }
}

#[test]
fn cancellation_during_apply_never_returns_a_receipt() {
    let fixture = home();
    let state = root(fixture.path());
    let cancel = CancelToken::default();
    let ctx = EnvContext::new(&state, cancel.clone()).unwrap();
    let stack = env("stack");
    let render = render(&[PRIMARY]);
    let mut run = EnvRun::begin(&ctx, &stack, Operation::Up, Wait::FailFast).unwrap();
    let mut fake = Fake::new(vec![], vec![row(PRIMARY, 'a')]);
    assert!(run
        .create_with(&mut fake, &selection(), request(&render), || {
            cancel.cancel(effigy_kernel::ctx::StopCause::Signal(2));
            Ok(())
        })
        .is_err());
    let _ = run.finish(false);
}
