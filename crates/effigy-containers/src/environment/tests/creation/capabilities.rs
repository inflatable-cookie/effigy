use super::*;
use crate::environment::ImageCapability;

fn nerdctl_row(byte: char) -> Workload {
    let mut row = row(PRIMARY, byte);
    row.image = "docker.io/library/fixture:latest".into();
    row.image_reference = row.image.clone();
    row.labels.remove("com.docker.compose.oneoff");
    row.labels
        .insert("com.docker.compose.config-hash".into(), id('d'));
    row.labels.insert("nerdctl/name".into(), row.name.clone());
    row.labels
        .insert("nerdctl/networks".into(), "[\"none\"]".into());
    row
}

fn nerdctl_render(reference: &str) -> Closure {
    let mut document: serde_json::Value =
        serde_json::from_slice(&render(&[PRIMARY]).files()["compose.yml"]).unwrap();
    document["services"][PRIMARY]["network_mode"] = "none".into();
    document["services"][PRIMARY]["image"] = reference.into();
    Closure::new([("compose.yml".into(), serde_json::to_vec(&document).unwrap())].into()).unwrap()
}

#[test]
fn nerdctl_reference_resolution_is_explicit_and_binds_a_reserved_fresh_id() {
    use effigy_kernel::workspace_session::{ContainerIdentity, TargetCandidate};
    let fixture = home();
    let state = root(fixture.path());
    let ctx = EnvContext::new(&state, CancelToken::default()).unwrap();
    let stack = env("stack");
    let render = nerdctl_render("fixture:latest");
    let reservation = fresh_reservation(&ctx, &stack);
    let mut run =
        EnvRun::begin_claimed(&ctx, &stack, Operation::Up, Wait::FailFast, &reservation).unwrap();
    let row = nerdctl_row('a');
    let mut fake = Fake::new(vec![], vec![row.clone()]);
    let outcome = run
        .create_with(&mut fake, &selection(), request(&render), || Ok(()))
        .unwrap();
    let receipt = outcome.receipt().unwrap();
    let identity = &receipt.image_identities()[PRIMARY];
    assert_eq!(identity.capability(), ImageCapability::ReferenceResolvedId);
    assert_eq!(identity.rendered_reference(), "fixture:latest");
    assert_eq!(
        identity.observed_reference(),
        "docker.io/library/fixture:latest"
    );
    assert_eq!(identity.resolved_id(), Some(image().as_str()));
    let diagnostic = serde_json::to_value(identity).unwrap();
    assert_eq!(diagnostic["capability"], "reference_resolved_id");
    assert_eq!(diagnostic["resolved_id"], image());
    fake.inventories.push_back(verified(vec![row.clone()]));
    run.bind_target_observing(
        &reservation,
        verified(vec![TargetCandidate {
            identity: ContainerIdentity {
                container_id: row.id,
                vm_boot_id: "boot".into(),
                backend: "fixture".into(),
            },
            project: PROJECT.into(),
            created_by: Some(run.id()),
        }]),
        &mut fake,
        Instant::now() + Duration::from_secs(5),
    )
    .unwrap();
    fake.inventories.push_back(verified(vec![nerdctl_row('a')]));
    fake.images
        .push_back(verified(format!("sha256:{}", id('f'))));
    assert!(receipt
        .reobserve(&mut fake, Instant::now() + Duration::from_secs(5))
        .is_err());
    run.finish(false).unwrap();
}

#[test]
fn nerdctl_reserved_non_live_successors_bind_and_refuse_bare_active() {
    use effigy_kernel::workspace_session::{ContainerIdentity, TargetCandidate};
    let fixture = home();
    let state = root(fixture.path());
    let stack = env("stack");
    let mut previous: Option<Workload> = None;
    let mut previous_generation = 0;
    for byte in ['a', 'd'] {
        let ctx = EnvContext::new(&state, CancelToken::default()).unwrap();
        let mut document: serde_json::Value =
            serde_json::from_slice(&nerdctl_render("fixture:latest").files()["compose.yml"])
                .unwrap();
        document["services"][PRIMARY]["environment"] =
            serde_json::json!({"REVISION":byte.to_string()});
        let render =
            Closure::new([("compose.yml".into(), serde_json::to_vec(&document).unwrap())].into())
                .unwrap();
        let seal = effigy_kernel::objects::Objects::seal_for_preflight(
            &ctx.cx,
            &state,
            &render,
            Instant::now() + Duration::from_secs(10),
        )
        .unwrap();
        let reservation = fresh_reservation(&ctx, &stack);
        assert!(reservation.generation().get() > previous_generation);
        previous_generation = reservation.generation().get();
        let mut run =
            EnvRun::begin_claimed(&ctx, &stack, Operation::Up, Wait::FailFast, &reservation)
                .unwrap();
        run.publish_sealed_desired(&seal, &render).unwrap();
        let row = nerdctl_row(byte);
        let before = previous.iter().cloned().collect();
        let mut fake = Fake::new(before, vec![row.clone()]);
        let outcome = run
            .create_with(&mut fake, &selection(), request(&render), || Ok(()))
            .unwrap();
        assert_eq!(
            outcome.created_ids().as_slice(),
            std::slice::from_ref(&row.id)
        );
        fake.inventories.push_back(verified(vec![row.clone()]));
        run.bind_target_observing(
            &reservation,
            verified(vec![TargetCandidate {
                identity: ContainerIdentity {
                    container_id: row.id.clone(),
                    vm_boot_id: "boot".into(),
                    backend: "fixture".into(),
                },
                project: PROJECT.into(),
                created_by: None,
            }]),
            &mut fake,
            Instant::now() + Duration::from_secs(5),
        )
        .unwrap();
        let refusal = run.activate(&[]).unwrap_err();
        assert!(refusal
            .to_string()
            .contains("reserved by a workspace activation"));
        run.finish(false).unwrap();
        reservation.cancel(&ctx.cx, &state, Wait::FailFast).unwrap();
        previous = Some(row);
    }
}

#[test]
fn nerdctl_zero_change_attaches_and_config_recreation_replaces_only_its_predecessor() {
    let render = nerdctl_render("fixture:latest");
    let old = nerdctl_row('a');
    let (created, attached) = infer(
        &mut Fake::new(vec![old.clone()], vec![old.clone()]),
        &render,
    )
    .unwrap();
    assert!(created.is_empty());
    assert_eq!(attached.as_slice(), std::slice::from_ref(&old.id));
    let (created, attached) = infer(
        &mut Fake::new(vec![old.clone()], vec![nerdctl_row('d')]),
        &render,
    )
    .unwrap();
    assert_eq!(created[0].container_id(), id('d'));
    assert_eq!(
        created[0].image_capability(),
        ImageCapability::ReferenceResolvedId
    );
    assert!(attached.is_empty());
    assert!(infer(
        &mut Fake::new(vec![old.clone()], vec![old, nerdctl_row('d')]),
        &render
    )
    .is_err());
}

#[test]
fn nerdctl_digest_reference_is_named_separately_from_container_image_id() {
    let reference = format!("fixture@sha256:{}", id('e'));
    let render = nerdctl_render(&reference);
    let mut row = nerdctl_row('a');
    row.image = format!("docker.io/library/{reference}");
    row.image_reference = row.image.clone();
    let (created, _) = infer(&mut Fake::new(vec![], vec![row]), &render).unwrap();
    assert_eq!(
        created[0].image_capability(),
        ImageCapability::DigestReference
    );
    // The resolved image-inspect ID is not confused with a manifest digest.
    assert_eq!(created[0].image_id(), image());
}

#[test]
fn nerdctl_missing_marker_requires_exact_name_network_and_service_membership() {
    let render = nerdctl_render("fixture:latest");
    let good = nerdctl_row('a');
    for (label, value) in [
        ("nerdctl/name", "another-workspace"),
        ("nerdctl/networks", "[\"foreign_default\"]"),
        ("nerdctl/networks", "[\"none\",\"none\"]"),
        ("nerdctl/networks", "not-json"),
        ("com.docker.compose.service", "foreign"),
        ("com.docker.compose.project", "foreign"),
        ("com.docker.compose.oneoff", "True"),
    ] {
        let mut bad = good.clone();
        bad.labels.insert(label.into(), value.into());
        assert!(
            infer(&mut Fake::new(vec![], vec![bad]), &render).is_err(),
            "{label}: {value}"
        );
    }
    let mut missing = good.clone();
    missing.labels.remove("nerdctl/networks");
    assert!(infer(&mut Fake::new(vec![], vec![missing]), &render).is_err());
    assert!(infer(
        &mut Fake::new(vec![], vec![good, nerdctl_row('d')]),
        &render
    )
    .is_err());
}

#[test]
fn nerdctl_tag_moves_reference_mismatch_and_docker_reference_only_still_refuse() {
    let render = nerdctl_render("fixture:latest");
    let good = nerdctl_row('a');
    let mut fake = Fake::new(vec![], vec![good.clone()]);
    fake.images = [verified(image()), verified(format!("sha256:{}", id('f')))].into();
    assert!(infer(&mut fake, &render).is_err());
    let mut wrong = good.clone();
    wrong.image = "docker.io/library/other:latest".into();
    assert!(infer(&mut Fake::new(vec![], vec![wrong]), &render).is_err());
    let mut docker = Fake::new(vec![], vec![good]);
    docker.selection.kind = BackendKind::DockerEngine;
    docker.selection.engine = crate::BackendId::docker_compose();
    assert!(infer(&mut docker, &render).is_err());
}

#[test]
fn pre_window_attached_tag_move_never_attributes_the_current_lookup_to_old_container() {
    // The actual old image X is not observable in nerdctl's reference-only row.
    // A tag moved X -> Y before preflight; all current lookups stably answer Y.
    let actual_old_image = format!("sha256:{}", id('a'));
    let current_lookup = format!("sha256:{}", id('b'));
    assert_ne!(actual_old_image, current_lookup);
    let render = nerdctl_render("fixture:latest");
    let old = nerdctl_row('f');
    let mut fake = Fake::new(vec![old.clone()], vec![old.clone()]);
    fake.image_references
        .insert("fixture:latest".into(), current_lookup.clone());
    let req = request(&render);
    let difference = CreationWindow::open(&mut fake, req)
        .unwrap()
        .close(&mut fake, req)
        .unwrap();
    let identity = &difference.image_identities[PRIMARY];
    assert_eq!(
        identity.capability(),
        ImageCapability::AttachedReferenceConfigHash
    );
    assert_eq!(identity.resolved_id(), None);
    assert_eq!(identity.config_hash(), Some(id('d').as_str()));
    assert!(!serde_json::to_string(identity)
        .unwrap()
        .contains(&current_lookup));
    assert!(difference.created.is_empty());
    assert_eq!(
        difference.attached.as_slice(),
        std::slice::from_ref(&old.id)
    );
    let mut fresh = Fake::new(vec![], vec![nerdctl_row('e')]);
    fresh
        .image_references
        .insert("fixture:latest".into(), current_lookup.clone());
    let req = request(&render);
    let difference = CreationWindow::open(&mut fresh, req)
        .unwrap()
        .close(&mut fresh, req)
        .unwrap();
    assert_eq!(
        difference.image_identities[PRIMARY].resolved_id(),
        Some(current_lookup.as_str())
    );
}

#[test]
fn attached_reference_requires_render_matching_config_hash_and_original_deadline() {
    let render = nerdctl_render("fixture:latest");
    for expired in [false, true] {
        let old = nerdctl_row('a');
        let mut fake = Fake::new(vec![old.clone()], vec![old]);
        let mut request = request(&render);
        if expired {
            request.deadline = Instant::now() + Duration::from_millis(500);
            fake.config_hash_delay = Duration::from_millis(600);
        } else {
            fake.config_hashes.insert(PRIMARY.into(), id('e'));
        }
        let window = CreationWindow::open(&mut fake, request).unwrap();
        assert!(window.close(&mut fake, request).is_err());
    }
}

#[test]
fn custom_network_survives_extra_hosts_only_override_and_ordered_network_names_merge() {
    for override_name in [false, true] {
        let mut base: serde_json::Value =
            serde_json::from_slice(&render(&[PRIMARY]).files()["compose.yml"]).unwrap();
        base["services"][PRIMARY]["networks"] = serde_json::json!(["custom"]);
        base["networks"] = serde_json::json!({"custom":{"name":"base-network"}});
        let mut extra = serde_json::json!({"services":{"workspace":{"extra_hosts":["effigy-host:host-gateway"]}}});
        if override_name {
            extra["networks"] = serde_json::json!({"custom":{"name":"override-network"}});
        }
        let render = Closure::new(
            [
                ("z-base.yml".into(), serde_json::to_vec(&base).unwrap()),
                (
                    "a-extra-hosts.yml".into(),
                    serde_json::to_vec(&extra).unwrap(),
                ),
                (
                    "inputs/environment.json".into(),
                    serde_json::to_vec(&serde_json::json!({
                "compose_files":["z-base.yml","a-extra-hosts.yml"]}))
                    .unwrap(),
                ),
            ]
            .into(),
        )
        .unwrap();
        let mut row = nerdctl_row('a');
        row.labels.insert(
            "nerdctl/networks".into(),
            serde_json::to_string(&[if override_name {
                "override-network"
            } else {
                "base-network"
            }])
            .unwrap(),
        );
        assert!(infer(&mut Fake::new(vec![], vec![row.clone()]), &render).is_ok());
        row.labels
            .insert("nerdctl/networks".into(), "[\"acme-stack_default\"]".into());
        assert!(infer(&mut Fake::new(vec![], vec![row]), &render).is_err());
    }
}
