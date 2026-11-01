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
pub(in crate::environment) fn row(service: &str, byte: char) -> Workload {
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
pub(in crate::environment) fn render(services: &[&str]) -> Closure {
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
pub(in crate::environment) fn request(render: &Closure) -> CreationRequest<'_> {
    CreationRequest {
        workdir: Path::new("."),
        project: PROJECT,
        primary_service: PRIMARY,
        compose_files: &[],
        render: Some(render),
        preflight: None,
        required: true,
        deadline: Instant::now() + Duration::from_secs(30),
    }
}
#[derive(Debug)]
pub(in crate::environment) struct Fake {
    selection: BackendSelection,
    inventories: VecDeque<Observed<Vec<Workload>>>,
    images: VecDeque<Observed<String>>,
    image_references: BTreeMap<String, String>,
    image_delays: VecDeque<Duration>,
    reads: usize,
    deadlines: Vec<Instant>,
    cancel_on_inventory: Option<CancelToken>,
    inventory_delay: Duration,
    config_hashes: BTreeMap<String, String>,
    config_hash_delay: Duration,
}
impl Fake {
    pub(in crate::environment) fn new(before: Vec<Workload>, after: Vec<Workload>) -> Self {
        let config_hashes = before
            .iter()
            .chain(&after)
            .filter_map(|row| row.labels.get("com.docker.compose.service"))
            .map(|service| (service.clone(), id('d')))
            .collect();
        Self {
            selection: selection(),
            inventories: [verified(before), verified(after)].into(),
            images: VecDeque::new(),
            image_references: BTreeMap::new(),
            image_delays: VecDeque::new(),
            reads: 0,
            deadlines: Vec::new(),
            cancel_on_inventory: None,
            inventory_delay: Duration::ZERO,
            config_hashes,
            config_hash_delay: Duration::ZERO,
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
        self.deadlines.push(deadline);
        assert!(Instant::now() < deadline);
        if let Some(cancel) = self.cancel_on_inventory.take() {
            cancel.cancel(effigy_kernel::ctx::StopCause::Signal(2));
        }
        std::thread::sleep(self.inventory_delay);
        self.inventories
            .pop_front()
            .expect("unexpected inventory read")
    }
    fn service_config_hashes_until(
        &mut self,
        _: &Path,
        _: &str,
        _: &[PathBuf],
        deadline: Instant,
    ) -> Observed<BTreeMap<String, String>> {
        self.deadlines.push(deadline);
        assert!(Instant::now() < deadline);
        std::thread::sleep(self.config_hash_delay);
        verified(self.config_hashes.clone())
    }
    fn image_id_until(&mut self, _: &Path, reference: &str, deadline: Instant) -> Observed<String> {
        self.deadlines.push(deadline);
        assert!(Instant::now() < deadline);
        std::thread::sleep(self.image_delays.pop_front().unwrap_or(Duration::ZERO));
        self.images.pop_front().unwrap_or_else(|| {
            verified(
                self.image_references
                    .get(reference)
                    .cloned()
                    .unwrap_or_else(image),
            )
        })
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
    let mut old_workspace = workspace.clone();
    old_workspace.image_reference = "fixture:previous".into();
    old_workspace.image = format!("sha256:{}", id('e'));
    let mut fake = Fake::new(
        vec![old_workspace.clone(), db.clone()],
        vec![replacement.clone(), db],
    );
    fake.image_references.insert(
        old_workspace.image_reference.clone(),
        old_workspace.image.clone(),
    );
    let (created, attached) = infer(&mut fake, &render).unwrap();
    assert_eq!(created[0].container_id(), replacement.id);
    assert_eq!(attached, [id('b')]);
}

#[test]
fn new_ids_must_equal_the_allowed_workspace_and_expected_dependency_set() {
    let render = render(&[PRIMARY, "database"]);
    let workspace = row(PRIMARY, 'a');
    let database = row("database", 'b');
    // The workspace may be recreated for any Compose configuration reason;
    // the complete declared inventory and exact dependency ID still constrain it.
    let (created, attached) = infer(
        &mut Fake::new(
            vec![workspace.clone(), database.clone()],
            vec![row(PRIMARY, 'd'), database.clone()],
        ),
        &render,
    )
    .unwrap();
    assert_eq!(created[0].container_id(), id('d'));
    assert_eq!(attached.as_slice(), std::slice::from_ref(&database.id));
    // Both predecessor and replacement, or a foreign extra, must still refuse.
    for extra in [workspace.clone(), row("foreign", 'f')] {
        assert!(infer(
            &mut Fake::new(
                vec![workspace.clone(), database.clone()],
                vec![row(PRIMARY, 'd'), database.clone(), extra],
            ),
            &render,
        )
        .is_err());
    }
    // The workspace is expected to be fresh, but the attached dependency isn't.
    assert!(infer(
        &mut Fake::new(
            vec![database.clone()],
            vec![workspace.clone(), row("database", 'd')]
        ),
        &render
    )
    .is_err());
    let mut old = workspace.clone();
    old.image_reference = "fixture:previous".into();
    old.image = format!("sha256:{}", id('e'));
    let mut fake = Fake::new(
        vec![old.clone(), database.clone()],
        vec![row(PRIMARY, 'd'), database.clone()],
    );
    fake.image_references
        .insert(old.image_reference.clone(), old.image.clone());
    let window = CreationWindow::open(&mut fake, request(&render)).unwrap();
    let difference = window.close(&mut fake, request(&render)).unwrap();
    assert!(difference.expected_created.is_empty());
    assert_eq!(
        difference.workspace_predecessor.as_deref(),
        Some(old.id.as_str())
    );
    // A requested reference change must result in a new full ID, not reuse.
    let mut fake = Fake::new(
        vec![old.clone(), database.clone()],
        vec![workspace, database.clone()],
    );
    fake.image_references
        .insert(old.image_reference.clone(), old.image.clone());
    assert!(infer(&mut fake, &render).is_err());
    // A moved old tag is not a verified recreation expectation.
    assert!(infer(
        &mut Fake::new(
            vec![old, database.clone()],
            vec![row(PRIMARY, 'd'), database]
        ),
        &render
    )
    .is_err());
}

#[test]
fn recreation_expectation_refuses_old_reference_retag_expiry_and_empty_reference() {
    let render = render(&[PRIMARY]);
    let mut old = row(PRIMARY, 'a');
    old.image_reference = "fixture:previous".into();
    old.image = format!("sha256:{}", id('e'));
    for expired in [false, true] {
        let mut fake = Fake::new(vec![old.clone()], vec![row(PRIMARY, 'd')]);
        fake.images = [
            verified(image()),
            verified(old.image.clone()),
            verified(image()),
            verified(if expired {
                old.image.clone()
            } else {
                format!("sha256:{}", id('f'))
            }),
        ]
        .into();
        let mut request = request(&render);
        if expired {
            request.deadline = Instant::now() + Duration::from_millis(500);
            fake.image_delays = [
                Duration::ZERO,
                Duration::ZERO,
                Duration::ZERO,
                Duration::from_millis(600),
            ]
            .into();
        }
        let window = CreationWindow::open(&mut fake, request).unwrap();
        assert!(window.close(&mut fake, request).is_err());
    }
    old.image_reference.clear();
    let mut fake = Fake::new(vec![old.clone()], vec![row(PRIMARY, 'd')]);
    fake.image_references.insert(String::new(), old.image);
    assert!(CreationWindow::open(&mut fake, request(&render)).is_err());
}

#[test]
fn incomplete_extra_duplicate_foreign_or_unproved_image_rows_refuse() {
    let render = render(&[PRIMARY]);
    let good = row(PRIMARY, 'a');
    let mut wrong_label = good.clone();
    wrong_label
        .labels
        .insert("com.effigy.checkout".into(), "/elsewhere".into());
    let mut reference_only = good.clone();
    reference_only.image = "other:latest".into();
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
        let first = run.create_with(&mut fake, &selection(), request(&render), || {
            applies += 1;
            Ok(())
        });
        let reads = fake.reads;
        assert_eq!(first.is_ok(), readable);
        let second = run.create_with(&mut fake, &selection(), request(&render), || {
            applies += 1;
            Ok(())
        });
        if readable {
            assert_eq!(first.unwrap(), second.unwrap());
        } else {
            assert!(second.is_err());
        }
        assert_eq!(applies, 1);
        assert_eq!(fake.reads, reads);
        run.finish(readable).unwrap();
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
fn primary_reference_belongs_to_exact_frozen_preflight() {
    let frozen = render(&[PRIMARY, "worker"]);
    let mut fake = Fake::new(vec![], vec![]);
    let preflight =
        crate::environment::CreationPreflight::observe(&mut fake, request(&frozen)).unwrap();
    assert_eq!(
        preflight
            .primary_image_reference(request(&frozen), &mut fake)
            .unwrap(),
        "fixture:latest"
    );
    let mut wrong_service = request(&frozen);
    wrong_service.primary_service = "worker";
    assert!(preflight
        .primary_image_reference(wrong_service, &mut fake)
        .is_err());
    let mut wrong_checkout = request(&frozen);
    wrong_checkout.workdir = Path::new("other");
    assert!(preflight
        .primary_image_reference(wrong_checkout, &mut fake)
        .is_err());
    fake.selection.context = "another-context".into();
    assert!(preflight
        .primary_image_reference(request(&frozen), &mut fake)
        .is_err());
    fake.selection = selection();
    fake.images
        .push_back(verified(format!("sha256:{}", id('d'))));
    assert!(
        preflight
            .primary_image_reference(request(&frozen), &mut fake)
            .is_err(),
        "retagged image refuses"
    );
    let mut expired = request(&frozen);
    expired.deadline = Instant::now() - Duration::from_millis(1);
    assert!(preflight
        .primary_image_reference(expired, &mut fake)
        .is_err());
    let other = render(&[PRIMARY]);
    assert!(preflight
        .primary_image_reference(request(&other), &mut fake)
        .is_err());
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

fn fresh_reservation(
    ctx: &EnvContext<'_>,
    stack: &EnvRef,
) -> effigy_kernel::workspace_session::WorkspaceActivationReservation {
    use effigy_kernel::workspace_session::*;
    let inputs = PublishInputs {
        artifact: ArtifactEvidence::Unselected {
            image_reference: "fixture:latest".into(),
            backend: "fixture".into(),
        },
        selection: effigy_kernel::id::Digest::of(b"selection"),
        inputs_digest: effigy_kernel::id::Digest::of(b"inputs"),
        target: TargetEvidence::Absent {
            project: PROJECT.into(),
            backend: "fixture".into(),
        },
        platform: None,
        guest: Some(GuestContextInputs {
            version: "0.15.0".into(),
            protocol: 1,
            host_user: "1000".into(),
            checkout_id: stack.checkout.text(),
            environment_resource: stack.resource.name().to_string(),
            profile: "fixture".into(),
            runtime: None,
            vm_type: None,
            routes: vec![],
            sources: vec![],
            paths: GuestPaths {
                host_checkout: "/checkout".into(),
                guest_checkout: "/workspace".into(),
                workdir: "/workspace".into(),
                user: "1000".into(),
                home: "/home/worker".into(),
            },
        }),
    };
    match WorkspaceActivationReservation::decide(
        &ctx.cx,
        ctx.root,
        &stack.resource,
        &inputs,
        Wait::FailFast,
    )
    .unwrap()
    {
        ActivationDecision::Activate(reservation) => *reservation,
        _ => panic!("fresh reservation"),
    }
}

#[test]
fn fresh_binding_accepts_only_the_created_workspace_id_never_attached_or_dependency_ids() {
    use effigy_kernel::workspace_session::{ContainerIdentity, TargetCandidate};
    for attached in [false, true] {
        let fixture = home();
        let state = root(fixture.path());
        let ctx = EnvContext::new(&state, CancelToken::default()).unwrap();
        let stack = env("stack");
        let render = render(&[PRIMARY, "database"]);
        let reservation = fresh_reservation(&ctx, &stack);
        let mut run =
            EnvRun::begin_claimed(&ctx, &stack, Operation::Up, Wait::FailFast, &reservation)
                .unwrap();
        let after = vec![row(PRIMARY, 'a'), row("database", 'b')];
        let before = if attached { after.clone() } else { vec![] };
        let mut fake = Fake::new(before, after);
        run.create_with(&mut fake, &selection(), request(&render), || Ok(()))
            .unwrap();
        let candidate = |byte| {
            verified(vec![TargetCandidate {
                identity: ContainerIdentity {
                    container_id: id(byte),
                    vm_boot_id: "boot".into(),
                    backend: "fixture".into(),
                },
                project: PROJECT.into(),
                created_by: Some(run.id()),
            }])
        };
        let dependency = candidate('b');
        let workspace = candidate('a');
        fake.inventories
            .push_back(verified(vec![row(PRIMARY, 'a'), row("database", 'b')]));
        let deadline = Instant::now() + Duration::from_secs(5);
        assert!(run
            .bind_target_observing(&reservation, dependency, &mut fake, deadline)
            .is_err());
        assert_eq!(
            run.bind_target_observing(&reservation, workspace, &mut fake, deadline)
                .is_ok(),
            !attached
        );
        let _ = run.finish(false);
    }
}

#[test]
fn same_image_workspace_env_mount_and_entrypoint_recreation_bind_the_new_id() {
    use effigy_kernel::workspace_session::{ContainerIdentity, TargetCandidate};
    for (key, value) in [
        ("environment", serde_json::json!({"CONFIG_REVISION": "two"})),
        ("volumes", serde_json::json!(["/checkout/cache:/cache"])),
        (
            "entrypoint",
            serde_json::json!(["/bin/sh", "-c", "sleep 120"]),
        ),
    ] {
        let fixture = home();
        let state = root(fixture.path());
        let ctx = EnvContext::new(&state, CancelToken::default()).unwrap();
        let stack = env("stack");
        let original = render(&[PRIMARY, "database"]);
        let mut compose: serde_json::Value =
            serde_json::from_slice(&original.files()["compose.yml"]).unwrap();
        compose["services"][PRIMARY][key] = value;
        let render = Closure::new(
            [("compose.yml".into(), serde_json::to_vec(&compose).unwrap())]
                .into_iter()
                .collect(),
        )
        .unwrap();
        assert_ne!(original.digest(), render.digest());
        let reservation = fresh_reservation(&ctx, &stack);
        let mut run =
            EnvRun::begin_claimed(&ctx, &stack, Operation::Up, Wait::FailFast, &reservation)
                .unwrap();
        let old = row(PRIMARY, 'a');
        let replacement = row(PRIMARY, 'd');
        let database = row("database", 'b');
        let after = vec![replacement.clone(), database.clone()];
        let mut fake = Fake::new(vec![old.clone(), database], after.clone());
        let outcome = run
            .create_with(&mut fake, &selection(), request(&render), || Ok(()))
            .unwrap();
        let receipt = outcome.receipt().expect("same-image recreation receipt");
        assert_eq!(
            receipt.workspace_predecessor(),
            Some(old.id.as_str()),
            "{key}"
        );
        assert_eq!(
            receipt.created_ids().collect::<Vec<_>>(),
            [replacement.id.as_str()],
            "{key}"
        );
        assert_eq!(receipt.attached(), [id('b')], "{key}");
        let retry = run
            .create_with(&mut fake, &selection(), request(&render), || {
                panic!("second apply")
            })
            .unwrap();
        assert_eq!(retry, outcome);
        fake.inventories.push_back(verified(after));
        let candidate = verified(vec![TargetCandidate {
            identity: ContainerIdentity {
                container_id: replacement.id,
                vm_boot_id: "boot".into(),
                backend: "fixture".into(),
            },
            project: PROJECT.into(),
            created_by: Some(run.id()),
        }]);
        assert!(
            run.bind_target_observing(
                &reservation,
                candidate,
                &mut fake,
                Instant::now() + Duration::from_secs(5),
            )
            .is_ok(),
            "{key}"
        );
        run.finish(false).unwrap();
    }
}

#[test]
fn invalid_pre_inventory_refuses_before_the_one_apply() {
    let fixture = home();
    let state = root(fixture.path());
    let ctx = EnvContext::new(&state, CancelToken::default()).unwrap();
    let stack = env("stack");
    let render = render(&[PRIMARY]);
    let mut fake = Fake::new(vec![], vec![row(PRIMARY, 'a')]);
    fake.inventories[0] = unknown();
    let mut run = EnvRun::begin(&ctx, &stack, Operation::Up, Wait::FailFast).unwrap();
    assert!(run
        .create_with(&mut fake, &selection(), request(&render), || panic!(
            "invalid precondition launched Compose"
        ))
        .is_err());
    assert!(run
        .create_with(&mut fake, &selection(), request(&render), || panic!(
            "retry launched Compose"
        ))
        .is_err());
    let _ = run.finish(false);
}

/// Process death releases the actual flock, with no generation rewrite. A
/// journalled receipt remains history, never authority for a successor's target.
#[cfg(feature = "fault-injection")]
#[test]
fn attributed_apply_crash_releases_gate_without_fabricating_a_successor_receipt() {
    use effigy_kernel::fault_injection::with_hook;
    use effigy_kernel::gate::{Gate, GateState};
    use effigy_test_support::process::FixtureCommandExt;
    use std::os::unix::process::ExitStatusExt;
    const CHILD: &str = "EFFIGY_510_CREATION_CRASH";
    const DIRECTORY: &str = "EFFIGY_510_CREATION_DIRECTORY";
    if let Ok(boundary) = std::env::var(CHILD) {
        let directory = PathBuf::from(std::env::var_os(DIRECTORY).unwrap());
        let state = root(&directory);
        let ctx = EnvContext::new(&state, CancelToken::default()).unwrap();
        let stack = env("stack");
        let render = render(&[PRIMARY]);
        let mut run = EnvRun::begin(&ctx, &stack, Operation::Up, Wait::FailFast).unwrap();
        std::fs::write(directory.join("identity.json"), serde_json::to_vec(&serde_json::json!({"run": run.id().to_string(), "generation":run.fence().generation.get()})).unwrap()).unwrap();
        let pause_directory = directory.clone();
        let pause = move || {
            std::fs::write(pause_directory.join("paused"), b"ready").unwrap();
            loop {
                std::thread::sleep(Duration::from_millis(10));
            }
        };
        let mut fake = Fake::new(vec![], vec![row(PRIMARY, 'a')]);
        let hook_boundary = boundary.clone();
        with_hook(
            move |point| {
                if point == hook_boundary {
                    pause();
                }
                Ok(())
            },
            || {
                let mut requested = request(&render);
                requested.deadline = Instant::now() + Duration::from_secs(90);
                run.create_with(&mut fake, &selection(), requested, || {
                    use std::io::Write;
                    std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(directory.join("apply-count"))
                        .unwrap()
                        .write_all(b"apply\n")
                        .unwrap();
                    std::fs::write(
                        directory.join("engine-row.json"),
                        serde_json::to_vec(&row(PRIMARY, 'a')).unwrap(),
                    )
                    .unwrap();
                    if boundary == "during" {
                        std::fs::write(directory.join("paused"), b"ready").unwrap();
                        loop {
                            std::thread::sleep(Duration::from_millis(10));
                        }
                    }
                    Ok(())
                })
            },
        )
        .unwrap();
        panic!("crash boundary was not reached");
    }
    let proof_until = Instant::now() + Duration::from_secs(150);
    for boundary in ["during", "EnvBackendAfter", "AppliedBefore", "AppliedAfter"] {
        let fixture = home();
        let stdout = std::fs::File::create(fixture.path().join("child.stdout")).unwrap();
        let stderr = std::fs::File::create(fixture.path().join("child.stderr")).unwrap();
        let diagnostic = || {
            let bytes = std::fs::read(fixture.path().join("child.stderr")).unwrap();
            String::from_utf8_lossy(&bytes[bytes.len().saturating_sub(4096)..]).into_owned()
        };
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "environment::tests::creation::attributed_apply_crash_releases_gate_without_fabricating_a_successor_receipt", "--nocapture"])
            .env(CHILD, boundary).env(DIRECTORY, fixture.path())
            .current_dir(fixture.path()).stdout(stdout).stderr(stderr)
            .spawn_owned().unwrap();
        std::fs::write(fixture.path().join("child.pid"), child.id().to_string()).unwrap();
        let until = (Instant::now() + Duration::from_secs(60)).min(proof_until);
        while !fixture.path().join("paused").exists() {
            assert!(
                Instant::now() < until,
                "owned crash child {} did not reach {boundary}: {}",
                child.id(),
                diagnostic()
            );
            assert!(
                child.try_wait().unwrap().is_none(),
                "owned crash child {} exited before {boundary}: {}",
                child.id(),
                diagnostic()
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        let state = root(fixture.path());
        let ctx = EnvContext::new(&state, CancelToken::default()).unwrap();
        let stack = env("stack");
        assert!(matches!(
            Gate::probe(&ctx.observe(), &state, &stack.resource),
            Observed::Verified(GateState::Held(_), _)
        ));
        child.kill().unwrap();
        let exit = child.wait().unwrap();
        std::fs::write(fixture.path().join("child.exit"), exit.to_string()).unwrap();
        assert_eq!(exit.signal(), Some(9));
        assert!(matches!(
            Gate::probe(&ctx.observe(), &state, &stack.resource),
            Observed::Verified(GateState::Free, _)
        ));
        let identity: serde_json::Value =
            serde_json::from_slice(&std::fs::read(fixture.path().join("identity.json")).unwrap())
                .unwrap();
        let run_id = identity["run"].as_str().unwrap().parse::<RunId>().unwrap();
        let reader = match effigy_kernel::store::Store::read(&ctx.observe(), &state) {
            Observed::Verified(reader, _) => reader,
            other => panic!("journal unavailable: {other:?}"),
        };
        assert_eq!(
            reader
                .resource(&stack.resource)
                .verified()
                .unwrap()
                .generation
                .get(),
            identity["generation"].as_u64().unwrap(),
            "gate release is not simulated by advancing generation"
        );
        let effects = effigy_kernel::run::effects(&reader, run_id)
            .verified()
            .unwrap()
            .clone();
        let effect = effects
            .iter()
            .find(|effect| effect.spec.kind == crate::environment::CREATE_EFFECT)
            .unwrap();
        if boundary == "AppliedAfter" {
            assert!(
                effect.receipt.is_some(),
                "the completed receipt remains historical evidence"
            );
        } else {
            assert!(
                effect.receipt.is_none(),
                "crash fabricated an attributed receipt"
            );
        }
        drop(reader);
        if let Ok(mut successor) = EnvRun::begin(&ctx, &stack, Operation::Up, Wait::FailFast) {
            // A retry of the dead activation's journal cannot be transplanted
            // into this new run. Its inventory begins with the existing ID.
            let existing: Workload = serde_json::from_slice(
                &std::fs::read(fixture.path().join("engine-row.json")).unwrap(),
            )
            .unwrap();
            let mut fake = Fake::new(vec![existing.clone()], vec![existing]);
            let render = render(&[PRIMARY]);
            let outcome = successor
                .create_with(&mut fake, &selection(), request(&render), || Ok(()))
                .unwrap();
            assert!(outcome.created_ids().is_empty());
            assert_eq!(outcome.receipt().unwrap().attached(), [id('a')]);
            let _ = successor.finish(false);
        }
        assert_eq!(
            std::fs::read_to_string(fixture.path().join("apply-count")).unwrap(),
            "apply\n",
            "crashed activation executed its apply more than once"
        );
    }
}

#[test]
fn cancellation_or_expiry_during_pre_observation_never_launches_apply() {
    for cancel_during_read in [true, false] {
        let fixture = home();
        let state = root(fixture.path());
        let cancel = CancelToken::default();
        let ctx = EnvContext::new(&state, cancel.clone()).unwrap();
        let stack = env("stack");
        let render = render(&[PRIMARY]);
        let mut fake = Fake::new(vec![], vec![row(PRIMARY, 'a')]);
        let mut requested = request(&render);
        if cancel_during_read {
            fake.cancel_on_inventory = Some(cancel);
        } else {
            fake.inventory_delay = Duration::from_millis(40);
            requested.deadline = Instant::now() + Duration::from_millis(20);
        }
        let mut run = EnvRun::begin(&ctx, &stack, Operation::Up, Wait::FailFast).unwrap();
        if !cancel_during_read {
            requested.deadline = Instant::now() + Duration::from_millis(20);
        }
        assert!(run
            .create_with(&mut fake, &selection(), requested, || panic!(
                "stale pre-observation launched Compose"
            ))
            .is_err());
        let _ = run.finish(false);
    }
}

#[test]
fn target_recheck_never_renews_the_callers_original_deadline() {
    let fixture = home();
    let state = root(fixture.path());
    let ctx = EnvContext::new(&state, CancelToken::default()).unwrap();
    let stack = env("stack");
    let render = render(&[PRIMARY]);
    let reservation = fresh_reservation(&ctx, &stack);
    let mut run =
        EnvRun::begin_claimed(&ctx, &stack, Operation::Up, Wait::FailFast, &reservation).unwrap();
    let mut fake = Fake::new(vec![], vec![row(PRIMARY, 'a')]);
    run.create_with(&mut fake, &selection(), request(&render), || Ok(()))
        .unwrap();
    fake.deadlines.clear();
    fake.inventories
        .push_back(verified(vec![row(PRIMARY, 'a')]));
    let effigy_kernel::ctx::Deadline::At(original) = ctx.cx.operation().deadline() else {
        panic!("bounded caller")
    };
    run.recheck_creation(&reservation, &mut fake, original + Duration::from_secs(60))
        .unwrap();
    assert!(!fake.deadlines.is_empty());
    assert!(fake.deadlines.iter().all(|deadline| *deadline <= original));
    let _ = run.finish(false);
}

#[test]
fn ordinary_unsealed_apply_never_mints_a_weaker_workspace_receipt() {
    let fixture = home();
    let state = root(fixture.path());
    let ctx = EnvContext::new(&state, CancelToken::default()).unwrap();
    let stack = env("stack");
    let render = render(&[PRIMARY]);
    let mut run = EnvRun::begin(&ctx, &stack, Operation::Up, Wait::FailFast).unwrap();
    let mut fake = Fake::new(vec![], vec![row(PRIMARY, 'a')]);
    let mut requested = request(&render);
    requested.required = false;
    let mut applies = 0;
    let outcome = run
        .create_with(&mut fake, &selection(), requested, || {
            applies += 1;
            Ok(())
        })
        .unwrap();
    assert!(outcome.receipt().is_none());
    assert!(outcome.unproven().is_some());
    assert_eq!(applies, 1);
    assert_eq!(fake.reads, 0);
    run.finish(true).unwrap();
}

mod capabilities;
