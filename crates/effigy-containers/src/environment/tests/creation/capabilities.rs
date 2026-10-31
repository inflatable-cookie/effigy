use super::*;
use crate::environment::ImageCapability;

fn nerdctl_row(byte: char) -> Workload {
    let mut row = row(PRIMARY, byte);
    row.image = "docker.io/library/fixture:latest".into();
    row.image_reference = row.image.clone();
    row.labels.remove("com.docker.compose.oneoff");
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
    assert_eq!(identity.resolved_id(), image());
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
fn nerdctl_reserved_non_live_successors_bind_and_publish_active() {
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
        run.activate(&[]).unwrap();
        run.finish(true).unwrap();
        assert!(record(&ctx, &stack).active());
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
