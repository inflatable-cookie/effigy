//! One producer/consumer path: a real sealed apply issues the #510 journal
//! receipt; the input builder consumes it and the gated publisher binds it.
//! Only engine I/O, guest filesystem and guest probes are doubles.
use super::*;
use crate::runner::environment;
use crate::runner::workspace_client::guest_fake::FakeGuest;
use crate::runner::workspace_command::provisioning::reserved_guest::{
    prepare_reserved_guest, tests::creator_probe,
};
use effigy_containers::environment::{
    CreationPreflight, EnvContext, EnvRef, EnvSettlement, Retention, Up, WorkspaceActivation,
    WorkspaceBegin,
};
use effigy_kernel::ctx::{EffectClass, Placement};
use effigy_kernel::gate::Wait;
use effigy_kernel::id::{
    ExecutionTarget, InvocationSpec, ResolvedInvocation, Route, Surface, TaskId,
};
use effigy_kernel::run::BeginOpts;
use std::cell::RefCell;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug)]
enum Case {
    Fresh,
    #[cfg(feature = "fault-injection")]
    FinishFault,
    HandoffFault,
    Recreate,
    WrongId,
    WrongRun,
    Missing,
    Ambiguous,
    Expired,
}

fn begin(
    ctx: &EnvContext<'static>,
    env: &EnvRef,
    plan: &ReservedPublicationPlan,
    index: u8,
) -> Box<WorkspaceActivation<'static>> {
    let task = TaskId::new(env.checkout, "", Surface::Published, "creator-proof", None).unwrap();
    let invocation = ResolvedInvocation::new(InvocationSpec {
        owner: task.resource(),
        task,
        route: Route::Task,
        argv: vec![],
        cwd: String::new(),
        target: ExecutionTarget::Host,
        env: BTreeMap::new(),
        gates: vec![],
        attachments: vec![],
        class: EffectClass::Mutate,
        placement: Placement::Host,
        provenance: BTreeMap::new(),
    })
    .unwrap();
    let inputs = plan
        .publish_inputs(
            ArtifactEvidence::Unselected {
                image_reference: "example:one".into(),
                backend: plan.backend_label().into(),
            },
            TargetEvidence::Absent {
                project: plan.policy().project_name.clone(),
                backend: plan.backend_label().into(),
            },
        )
        .unwrap();
    let begun = WorkspaceActivation::begin(
        ctx,
        env,
        &inputs,
        &invocation,
        BeginOpts {
            id: RunId::new(1_760_000_000_000, [index; 10]).unwrap(),
            wait: Wait::FailFast,
            acknowledge: vec![],
            custody: None,
            next: effigy_kernel::outcome::Invocation::validated(
                vec!["effigy".into(), "container".into(), "status".into()],
                |_| true,
            )
            .unwrap(),
        },
        "creator-proof-supervisor",
    )
    .unwrap();
    match begun {
        WorkspaceBegin::Activate(run) => run,
        _ => panic!("an unknown successor must reserve an activation"),
    }
}

fn apply(
    ctx: &EnvContext<'static>,
    env: &EnvRef,
    plan: &ReservedPublicationPlan,
    observation: &mut FakeBackend,
    marker: &Path,
    index: u8,
) -> Box<WorkspaceActivation<'static>> {
    let policy = plan.policy();
    // Prepare/seal before admission; the only promotion is inside the fenced
    // apply. There is no private request constructor or fabricated receipt here.
    let sealed = crate::runner::container_runtime_prep::seal_render_for_preflight(
        policy,
        plan.deadline().at(),
    )
    .unwrap();
    let selected = plan.backend();
    let up = Up {
        render: policy.render.as_ref().map(|render| &render.closure),
        project: &policy.project_name,
        backend: selected,
        retention: Retention::Persistent,
        workdir: &policy.repo_root,
        primary_service: &policy.primary_service,
        compose_files: &policy.compose_files,
    };
    let mut creation_request = up.creation_request();
    creation_request.deadline = plan.deadline().at();
    let preflight =
        CreationPreflight::observe(selected.adapter().as_mut(), creation_request).unwrap();
    let mut activation = begin(ctx, env, plan, index);
    recheck_live_predecessor(plan, observation).unwrap();
    let outcome = activation
        .apply_sealed_with(up, sealed.receipt(), &preflight, || {
            std::fs::write(marker, "applied").unwrap();
            Ok(())
        })
        .unwrap();
    let receipt = outcome
        .creation
        .as_ref()
        .unwrap()
        .receipt()
        .expect("sealed apply issued a receipt");
    assert_eq!(receipt.run(), activation.environment_mut().id());
    assert_eq!(
        receipt.generation(),
        activation.reservation().generation().get()
    );
    assert_eq!(receipt.created_ids().collect::<Vec<_>>(), [CONTAINER_ID]);
    if index == 2 && marker.with_file_name("before.json").exists() {
        assert_eq!(receipt.workspace_predecessor(), Some(OTHER_CONTAINER_ID));
    }
    activation
}

fn run(case: Case) {
    let observation = observation_checkout("creator-integrated");
    let bin = observation._fixture.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(
        observation.root.join(".git/config"),
        "[effigy]\n runtimeScope = ephemeral\n",
    )
    .unwrap();
    // This is a changed desired configuration, while any predecessor below is
    // stopped and has no selected/live workspace session.
    std::fs::write(
        observation.root.join("compose.yml"),
        "services:\n  app:\n    image: example:one\n    environment:\n      REVISION: successor\n",
    )
    .unwrap();
    let marker = observation._fixture.path().join("applied");
    let before = observation._fixture.path().join("before.json");
    let after = observation._fixture.path().join("after.json");
    let docker = bin.join("docker");
    std::fs::write(
        &docker,
        format!(
            r#"#!/bin/sh
case " $* " in
 *' image inspect '*) echo '[{{"Id":"sha256:{}"}}]' ;;
 *' ps '*) if [ -f '{}' ]; then echo '{}'; elif [ -f '{}' ]; then echo '{}'; fi ;;
 *' inspect '*) case " $* " in *'{}'*) cat '{}' ;; *) cat '{}' ;; esac ;;
 *) echo "unexpected fixture engine call: $*" >&2; exit 19 ;;
esac
"#,
            "cc".repeat(32),
            marker.display(),
            CONTAINER_ID,
            before.display(),
            OTHER_CONTAINER_ID,
            OTHER_CONTAINER_ID,
            before.display(),
            after.display()
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&docker, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut paths = vec![bin];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let _env = effigy_test_support::env::ScopedEnv::set_many(&[
        ("PATH", std::env::join_paths(paths).unwrap()),
        ("EFFIGY_COMPOSE_BACKEND", "docker".into()),
        ("DOCKER_CONTEXT", "default".into()),
    ]);
    effigy_containers::with_test_effigy_home(&observation.home, || {
        let mut policy =
            effigy_containers::prepare_container_policy(&observation.root, Some("web")).unwrap();
        policy.workspace_user = Some("dev".into());
        policy.workspace_home = Some("/home/dev".into());
        let selected = environment::backend(&observation.root, &policy);
        let artifact = produced_artifact(&observation.root, host_family());
        // The real staging owner verifies ELF bytes, not just the producer's
        // digest sidecar. Supply the host-native family at the external-I/O seam.
        let interpreter = if host_arch() == "amd64" {
            "/lib64/ld-linux-x86-64.so.2"
        } else {
            "/lib/ld-linux-aarch64.so.1"
        };
        let mut bytes = vec![0; 512];
        bytes[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
        let machine: u16 = if host_arch() == "amd64" { 62 } else { 183 };
        bytes[18..20].copy_from_slice(&machine.to_le_bytes());
        bytes[32..40].copy_from_slice(&64u64.to_le_bytes());
        bytes[54..56].copy_from_slice(&56u16.to_le_bytes());
        bytes[56..58].copy_from_slice(&1u16.to_le_bytes());
        bytes[64..68].copy_from_slice(&3u32.to_le_bytes());
        bytes[72..80].copy_from_slice(&128u64.to_le_bytes());
        bytes[96..104].copy_from_slice(&((interpreter.len() + 1) as u64).to_le_bytes());
        bytes[128..128 + interpreter.len()].copy_from_slice(interpreter.as_bytes());
        std::fs::write(&artifact, &bytes).unwrap();
        std::fs::write(
            workspace_artifact_receipt_path(&artifact),
            format!(
                "release_triple={}\nsha256={}\n",
                host_family().release_triple(),
                Digest::of(&bytes)
            ),
        )
        .unwrap();
        let identity = host_identity(host_machine(), "gnu");
        let shared = Rc::new(RefCell::new(
            FakeGuest::new().with_container(identity.clone()),
        ));
        shared.borrow_mut().put_file(
            crate::runner::workspace_client::context::CONTEXT_CURRENT_PATH,
            b"3",
            0o644,
        );
        let backend = || {
            let mut value = FakeBackend::container_mode(
                selected.clone(),
                ReservedAddressing {
                    proxy_bind: Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
                    colima_gateway: None,
                },
            )
            .with_artifact(artifact.clone());
            value.guest = Some(shared.clone());
            value
        };
        let mut external = backend();
        let plan = plan(
            ReservedPlanRequest {
                policy: &policy,
                deadline: PublicationDeadline::new(
                    Instant::now()
                        + Duration::from_secs(if matches!(case, Case::Expired) {
                            20
                        } else {
                            120
                        }),
                ),
            },
            &mut external,
        )
        .unwrap()
        .unwrap();
        let rendered = plan.rendered_policy();
        // Mirror the exact ownership expectations from the frozen render at the
        // engine boundary, not a guessed/project-only label set.
        let mut labels = serde_json::Map::new();
        for file in &rendered.compose_files {
            let doc: serde_yaml::Value =
                serde_yaml::from_str(&rendered.read_compose(file).unwrap()).unwrap();
            if let Some(values) = doc
                .get("services")
                .and_then(|x| x.get("app"))
                .and_then(|x| x.get("labels"))
                .and_then(|x| x.as_mapping())
            {
                for (key, value) in values {
                    let value = match value {
                        serde_yaml::Value::String(s) => s.clone(),
                        serde_yaml::Value::Bool(b) => b.to_string(),
                        _ => continue,
                    };
                    labels.insert(key.as_str().unwrap().into(), value.into());
                }
            }
        }
        labels.insert(
            "com.docker.compose.project".into(),
            rendered.project_name.clone().into(),
        );
        labels.insert("com.docker.compose.service".into(), "app".into());
        labels.insert("com.docker.compose.oneoff".into(), "False".into());
        let row = |id: &str, running: bool, revision: &str| {
            serde_json::json!([{
                "Id":id, "Name":format!("/{}-app-1", rendered.project_name), "Image":format!("sha256:{}", "cc".repeat(32)),
                "State":{"Running":running}, "Config":{"Image":"example:one","Labels":labels,"Env":[format!("REVISION={revision}")]}
            }])
        };
        std::fs::write(
            &after,
            serde_json::to_vec(&row(CONTAINER_ID, true, "successor")).unwrap(),
        )
        .unwrap();
        if matches!(case, Case::Recreate) {
            std::fs::write(
                &before,
                serde_json::to_vec(&row(OTHER_CONTAINER_ID, false, "predecessor")).unwrap(),
            )
            .unwrap();
        }
        let ctx = environment::context(&observation.root).unwrap();
        let env = environment::env_ref(&rendered).unwrap();
        assert_eq!(
            plan.guest_inputs().environment_resource,
            env.resource.name().to_string(),
            "the frozen materialisation must use the kernel's canonical resource identity"
        );
        if matches!(case, Case::WrongRun | Case::Ambiguous) {
            let seed = apply(&ctx, &env, &plan, &mut external, &marker, 1);
            seed.abort(&ctx, Wait::FailFast).unwrap();
            std::fs::remove_file(&marker).unwrap();
        }
        let mut activation = if matches!(case, Case::Missing | Case::WrongRun) {
            begin(&ctx, &env, &plan, 2)
        } else {
            apply(&ctx, &env, &plan, &mut external, &marker, 2)
        };
        let facts = ReservedDecisionFacts::of(&mut activation, &ctx).unwrap();
        let original_guest = shared.borrow().tree_digest();
        assert!(matches!(facts.inputs.target, TargetEvidence::Absent { .. }));
        if matches!(case, Case::Expired) {
            std::thread::sleep(
                plan.deadline()
                    .at()
                    .saturating_duration_since(Instant::now())
                    + Duration::from_millis(1),
            );
        }
        if matches!(case, Case::WrongId) {
            external.binding = Observed::Verified(
                VerifiedBinding {
                    container_id: OTHER_CONTAINER_ID.into(),
                    selected: selected.clone(),
                },
                evidence(),
            );
            external.identity = Observed::Verified(
                WorkspaceContainerIdentity {
                    container_id: OTHER_CONTAINER_ID.into(),
                    ..identity.clone()
                },
                evidence(),
            );
        }
        let probe: ReservedProbeFactory<'_> = Box::new(|target| {
            assert_eq!(target.container_id, CONTAINER_ID);
            Ok(creator_probe(identity.clone(), shared.clone()))
        });
        let prepared = observe(
            ReservedObservationRequest {
                repo_root: &observation.root,
            },
            &plan,
            &rendered,
            &probe,
            &facts,
            &mut external,
        );
        let success = match case {
            Case::Fresh | Case::Recreate => true,
            Case::HandoffFault => true,
            #[cfg(feature = "fault-injection")]
            Case::FinishFault => true,
            _ => false,
        };
        if !success {
            let text = prepared
                .expect_err("unproved successor cannot reach publication")
                .to_string();
            match case {
                Case::WrongRun => {
                    assert!(text.contains("not created by this activation"), "{text}")
                }
                Case::Ambiguous => assert!(text.contains("two activations"), "{text}"),
                Case::Expired => assert!(text.contains("deadline"), "{text}"),
                _ => assert!(text.contains("creator"), "{text}"),
            }
            assert_eq!(
                shared
                    .borrow()
                    .text(crate::runner::workspace_client::context::CONTEXT_CURRENT_PATH)
                    .as_deref(),
                Some("3")
            );
            assert_eq!(
                shared.borrow().tree_digest(),
                original_guest,
                "refusal wrote no guest byte or selector"
            );
            activation.abort(&ctx, Wait::FailFast).unwrap();
            return;
        }
        let prepared = prepared.unwrap();
        let (targets, _) = prepared.decision_pair();
        assert_eq!(
            targets.verified().unwrap()[0].created_by,
            Some(activation.environment_mut().id())
        );
        let input = prepared.into_input(Box::new(|until| {
            assert_eq!(until, plan.deadline().at());
            // Independently call the consumer on fresh observations and a fresh
            // journal read under the held gate, never return the earlier pair.
            let mut independent = backend();
            let fresh_probe: ReservedProbeFactory<'_> =
                Box::new(|_| Ok(creator_probe(identity.clone(), shared.clone())));
            let reread = observe(
                ReservedObservationRequest {
                    repo_root: &observation.root,
                },
                &plan,
                &rendered,
                &fresh_probe,
                &facts,
                &mut independent,
            )?;
            Ok(reread.decision_pair())
        }));
        let mut publication = prepare_reserved_guest(&mut activation, input)
            .expect("real gated target binding and guest publication");
        let committed = activation
            .commit(&publication.receipt, &publication.channel.launch)
            .unwrap();
        let obligation = committed.selection_obligation().unwrap();
        publication
            .guest
            .switch(activation.environment_mut(), obligation)
            .unwrap();
        activation
            .selected(
                obligation,
                &publication.channel.launch,
                publication.guest.as_mut(),
            )
            .unwrap();
        assert_eq!(
            shared
                .borrow()
                .text(crate::runner::workspace_client::context::CONTEXT_CURRENT_PATH)
                .as_deref(),
            Some("4")
        );
        #[cfg(feature = "fault-injection")]
        if matches!(case, Case::FinishFault) {
            let environment_run = activation.environment_mut().id();
            let result = effigy_kernel::fault_injection::with_hook(
                |name| {
                    if name == "SettleBefore" {
                        Err(std::io::Error::other("finish fault"))
                    } else {
                        Ok(())
                    }
                },
                || activation.finish_selected(),
            );
            let Err((_, custody)) = result else {
                panic!("environment settlement must fail")
            };
            let effigy_containers::environment::WorkspaceFinishCustody::Session {
                supervisor,
                session,
                environment,
                ..
            } = *custody
            else {
                panic!("selected session custody must be returned")
            };
            assert_eq!(session.run(), obligation.session_run());
            assert_eq!(environment.id(), environment_run);
            let reader = match effigy_kernel::store::Store::read(&ctx.observe(), ctx.root) {
                Observed::Verified(reader, _) => reader,
                other => panic!("journal unavailable: {other:?}"),
            };
            let stored =
                effigy_kernel::workspace_session::committed_selection(&reader, &env.resource)
                    .verified()
                    .cloned()
                    .flatten()
                    .expect("durable selection");
            assert_eq!(
                stored.state(),
                effigy_kernel::workspace_session::SelectionState::Switched
            );
            (*environment)
                .settle(&ctx.cleanup().unwrap(), &mut EnvSettlement)
                .unwrap();
            publication.channel.launch.stop();
            (*supervisor)
                .settle(&ctx.cleanup().unwrap(), &mut EnvSettlement)
                .unwrap();
            return;
        }
        let (supervisor, session) = activation.finish_selected().unwrap();
        if matches!(case, Case::HandoffFault) {
            let expected = session.run();
            let endpoint = publication.channel.launch.endpoint().unwrap();
            let owner = crate::runner::environment::workspace::WorkspaceOwner {
                supervisor,
                session,
                channel: publication.channel,
                serving: publication.serving,
                creation: None,
                effect: std::sync::Arc::new(ctx.cx.clone()),
                observe: std::sync::Arc::new(ctx.observe()),
                root: ctx.root,
            };
            let Err((error, mut owner)) =
                crate::runner::workspace_client::supervisor::ServingHandoff::committed(
                    owner, obligation,
                )
            else {
                panic!("pending obligation must refuse")
            };
            assert!(matches!(
                error,
                crate::runner::workspace_client::supervisor::HandoffRefusal::Unswitched(_)
            ));
            assert_eq!(owner.session.run(), expected);
            assert!(std::net::TcpStream::connect(endpoint).is_ok());
            owner.channel.launch.stop();
            owner
                .supervisor
                .settle(&ctx.cleanup().unwrap(), &mut EnvSettlement)
                .unwrap();
            assert!(std::net::TcpStream::connect(endpoint).is_err());
            return;
        }
        assert_ne!(
            session.environment_generation().get(),
            4,
            "context generation is deliberately distinct from the environment fence"
        );
        assert_eq!(
            session.environment_generation().get(),
            facts.reservation_generation
        );
        publication.channel.launch.stop();
        supervisor
            .settle(&ctx.cleanup().unwrap(), &mut EnvSettlement)
            .unwrap();
    });
}

#[test]
fn creator_binding_fresh_successor_publishes_from_the_actual_receipt() {
    run(Case::Fresh);
}
#[cfg(feature = "fault-injection")]
#[test]
fn creator_binding_finish_failure_retains_selected_session_and_durable_obligation() {
    run(Case::FinishFault);
}
#[test]
fn creator_binding_pending_handoff_returns_exact_owner_for_settlement() {
    run(Case::HandoffFault);
}
#[test]
fn creator_binding_non_live_config_successor_publishes_its_new_id() {
    run(Case::Recreate);
}
#[test]
fn creator_binding_wrong_id_refuses_before_publication() {
    run(Case::WrongId);
}
#[test]
fn creator_binding_wrong_run_refuses_before_publication() {
    run(Case::WrongRun);
}
#[test]
fn creator_binding_missing_receipt_refuses_before_publication() {
    run(Case::Missing);
}
#[test]
fn creator_binding_ambiguous_receipts_refuse_before_publication() {
    run(Case::Ambiguous);
}
#[test]
fn creator_binding_expired_deadline_refuses_before_publication() {
    run(Case::Expired);
}
