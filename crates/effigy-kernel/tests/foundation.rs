use effigy_kernel::ctx::*;
use effigy_kernel::id::*;
use effigy_kernel::observe::*;
use effigy_kernel::outcome::*;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::time::{Duration, Instant};

fn checkout() -> CheckoutId {
    CheckoutId::new(CheckoutKind::Primary, [0; 16])
}
fn task(catalog: &str, name: &str, profile: Option<&str>) -> TaskId {
    TaskId::new(checkout(), catalog, Surface::Published, name, profile).unwrap()
}
fn limits() -> Limits {
    Limits {
        capture_tail: 1024,
        log_rotate: 4096,
        log_keep: 2,
        line_max: 512,
        queue_bytes: 2048,
        probe_concurrency: 2,
    }
}
fn context(class: EffectClass, placement: Placement) -> OperationContext {
    OperationContext::new(
        Deadline::At(Instant::now() + Duration::from_secs(30)),
        CancelToken::default(),
        limits(),
        class,
        placement,
    )
}

#[test]
fn identity_equivalent_coordinates_and_lossless_fields() {
    let expected = task("apps/api", "build", None).resource();
    for spelling in ["apps/api", "./apps/api/", "apps//api", "apps/web/../api"] {
        let id = task(spelling, "build", None).resource();
        assert_eq!(id, expected);
        assert_eq!(id.name(), expected.name());
    }
    assert_ne!(task("", "a:b", None), task("", "a-b", None));
    assert_ne!(task("a", "b:c", None), task("a:b", "c", None));
    assert_ne!(task("", "build", None), task("", "build", Some("default")));
    assert_eq!(
        task("café", "é", Some("ö")),
        task("cafe\u{301}", "e\u{301}", Some("o\u{308}"))
    );
    assert_eq!(
        task("각", "é", None),
        task("\u{1100}\u{1161}\u{11a8}", "e\u{301}", None)
    );
    assert!(TaskId::new(checkout(), "../escape", Surface::Published, "build", None).is_err());
    assert!(TaskId::new(checkout(), "/absolute", Surface::Published, "build", None).is_err());
    assert_ne!(
        ResourceId::data("docker", "default", "data").unwrap(),
        ResourceId::data("containerd", "default", "data").unwrap()
    );
    assert_eq!(
        ResourceId::endpoint(EndpointKind::Domain, "EXAMPLE.TEST.").unwrap(),
        ResourceId::endpoint(EndpointKind::Domain, "example.test").unwrap()
    );
    assert_eq!(
        ResourceId::endpoint(EndpointKind::Address, "0:0:0:0:0:0:0:1").unwrap(),
        ResourceId::endpoint(EndpointKind::Address, "::1").unwrap()
    );
}

#[test]
fn identity_property_field_boundaries_and_collision_refusal() {
    // Exhaustive product over empty catalog, separators, punctuation and
    // canonically equivalent Unicode. Compare canonical coordinates, so the
    // oracle never permits distinct normalized tuples to share bytes/names.
    let fields = [
        "a", "b", ":", "-", "a:b", "a-b", "é", "e\u{301}", "ö", "o\u{308}", "|",
    ];
    let mut encodings = BTreeMap::new();
    let mut names = BTreeMap::new();
    for catalog in ["", "a", "a/b", "é", "e\u{301}"] {
        for name in fields {
            for profile in std::iter::once(None).chain(fields.into_iter().map(Some)) {
                let id = task(catalog, name, profile).resource();
                if let Some(previous) = encodings.insert(id.canonical(), id.clone()) {
                    assert_eq!(previous, id);
                }
                if let Some(previous) = names.insert(id.name().to_string(), id.clone()) {
                    assert_eq!(previous, id);
                }
                assert!(id.check_existing(&id.canonical()).is_ok());
                let error = id
                    .check_existing(b"different canonical resource")
                    .unwrap_err();
                assert!(
                    matches!(error, IdentityError::Collision { existing, incoming } if existing == b"different canonical resource" && incoming == id.canonical())
                );
            }
        }
    }
    // Deterministic property stream for longer and arbitrary field boundaries.
    let mut seed = 0x12345678u64;
    let mut seen = BTreeSet::new();
    for _ in 0..4096 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let id = ResourceId::data(
            "engine",
            &format!("c:{seed}"),
            &format!("v-{}", seed.rotate_left(17)),
        )
        .unwrap();
        assert!(seen.insert(id.canonical()));
    }
}

#[test]
fn identity_v1_golden_table_all_kinds() {
    let run = RunId::new(0, [0; 10]).unwrap();
    let ids = [
        checkout().resource(),
        task("", "build", None).resource(),
        run.resource(),
        ResourceId::environment(checkout(), "dev").unwrap(),
        ResourceId::data("docker", "default", "db").unwrap(),
        ResourceId::endpoint(EndpointKind::Port, "080").unwrap(),
        ResourceId::store(Scope::Machine, "vault").unwrap(),
        ResourceId::gateway("default").unwrap(),
        ResourceId::host_process(checkout(), "dev", "effigy", "app").unwrap(),
    ];
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/identity-v1.json")).unwrap();
    for (id, row) in ids.iter().zip(expected.as_array().unwrap()) {
        let canonical: String = id.canonical().iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(canonical, row["canonical"].as_str().unwrap());
        assert_eq!(id.name().to_string(), row["name"].as_str().unwrap());
        assert_eq!(id.name().to_string().len(), 32);
    }
    assert_eq!(ids.len(), expected.as_array().unwrap().len());
    assert_eq!(run.to_string(), "aaaaaaaaaaaaaaaaaaaaaaaaaa");
    assert!(RunId::new(1 << 48, [0; 10]).is_err());
    assert_eq!(Generation::new(u64::MAX).next(), None);
    assert_eq!(Generation::new(10).next().unwrap().get(), 11);
}
fn invocation() -> InvocationSpec {
    let task = task("apps/api", "build", None);
    InvocationSpec {
        owner: task.resource(),
        gates: vec![task.resource()],
        task,
        route: Route::Task,
        argv: vec![OsString::from_vec(vec![b'a', 0xff])],
        cwd: "./apps/api/".into(),
        target: ExecutionTarget::Host,
        env: BTreeMap::new(),
        attachments: vec![],
        class: EffectClass::Mutate,
        placement: Placement::Host,
        provenance: BTreeMap::new(),
    }
}
#[test]
fn identity_invocation_digest_is_shared_and_deterministic() {
    let resolved = ResolvedInvocation::new(invocation()).unwrap();
    let mut equivalent = invocation();
    equivalent.cwd = "apps/api".into();
    equivalent.gates.extend(equivalent.gates.clone());
    assert_eq!(
        resolved.digest(),
        ResolvedInvocation::new(equivalent).unwrap().digest()
    );
    let plan = resolved.clone();
    let run = resolved.clone();
    let status = resolved.clone();
    assert_eq!(plan.digest(), run.digest());
    assert_eq!(run.digest(), status.digest());
    let mut changed = invocation();
    changed.argv.push("different".into());
    assert_ne!(
        resolved.digest(),
        ResolvedInvocation::new(changed).unwrap().digest()
    );
    assert_eq!(resolved.spec().cwd, "apps/api");
}

#[test]
fn context_deadline_and_remaining_budget_never_extend() {
    let cx = context(EffectClass::Observe, Placement::Host);
    let child = cx.child(Duration::from_millis(10));
    assert!(child.remaining().unwrap() <= Duration::from_millis(10));
    if let (Deadline::At(parent), Deadline::At(child)) =
        (cx.deadline(), cx.child(Duration::from_secs(60)).deadline())
    {
        assert_eq!(parent, child);
    } else {
        panic!("finite child");
    }
    let expired = OperationContext::new(
        Deadline::At(Instant::now() - Duration::from_secs(1)),
        CancelToken::default(),
        limits(),
        EffectClass::Mutate,
        Placement::Host,
    );
    assert_eq!(
        expired.child(Duration::from_secs(1)).remaining(),
        Some(Duration::ZERO)
    );
    assert!(
        matches!(expired.check("starting"),Err(Interrupt::TimedOut { phase,.. }) if phase=="starting")
    );
    let open = OperationContext::new(
        Deadline::Open(OpenReason::DevServer),
        CancelToken::default(),
        limits(),
        EffectClass::Mutate,
        Placement::Host,
    );
    assert_eq!(open.remaining(), None);
    assert!(open.observe().is_err());
    assert!(open.child(Duration::from_secs(1)).observe().is_ok());
    assert!(matches!(
        open.child(Duration::MAX).deadline(),
        Deadline::At(_)
    ));
}
#[test]
fn context_cancellation_propagates_but_cleanup_has_fresh_budget() {
    let cx = context(EffectClass::Mutate, Placement::Host);
    let child = cx.child(Duration::from_secs(1));
    cx.cancel_token().cancel(StopCause::Signal(15));
    cx.cancel_token().cancel(StopCause::Signal(2));
    assert!(matches!(
        child.check("work"),
        Err(Interrupt::Cancelled {
            cause: StopCause::Signal(15)
        })
    ));
    let cleanup = cx.cleanup(Duration::from_secs(2)).unwrap();
    assert!(cleanup.operation().check("settle").is_ok());
    cleanup
        .operation()
        .cancel_token()
        .cancel(StopCause::Signal(2));
    assert!(matches!(
        cleanup.operation().check("escalate"),
        Err(Interrupt::Cancelled {
            cause: StopCause::Signal(2)
        })
    ));
    assert!(cx.cleanup(Duration::MAX).is_err());
}
#[test]
fn context_effect_class_capability_matrix() {
    let classes = [
        EffectClass::Static,
        EffectClass::Observe,
        EffectClass::RefreshDerived,
        EffectClass::Prepare,
        EffectClass::Mutate,
    ];
    let capabilities = [
        Capability::DerivedStore,
        Capability::SourceStore,
        Capability::UserFile,
        Capability::AcquireSource,
        Capability::AdapterEffect,
        Capability::MintToken,
    ];
    let expected = [
        [false; 6],
        [false; 6],
        [true, false, false, false, false, false],
        [false, true, true, true, false, true],
        [true, true, true, false, true, true],
    ];
    for (class, row) in classes.into_iter().zip(expected) {
        let cx = context(class, Placement::Host);
        assert_eq!(cx.observe().is_ok(), class != EffectClass::Static);
        for (capability, allowed) in capabilities.into_iter().zip(row) {
            assert_eq!(
                cx.effects()
                    .is_ok_and(|effects| effects.require(capability).is_ok()),
                allowed,
                "{class:?} {capability:?}"
            );
        }
    }
    assert!(context(EffectClass::Mutate, Placement::Guest)
        .effects()
        .unwrap()
        .require(Capability::HostAuthority)
        .is_err());
}

#[test]
fn observation_map_preserves_absence_and_unknown_and_bounds_evidence() {
    let unknown = Observed::<u32>::Unknown(UnknownReason::Injected);
    assert!(unknown.verified().is_none());
    assert_eq!(
        unknown.map(|_| panic!("unknown grants no value")),
        Observed::<String>::Unknown(UnknownReason::Injected)
    );
    let absent = Observed::<u32>::Absent(AbsenceProof::NoSuchProcess { pid: 42 });
    assert_eq!(
        absent.map(|_| panic!("absent grants no value")),
        Observed::<String>::Absent(AbsenceProof::NoSuchProcess { pid: 42 })
    );
    let evidence = Evidence::new(
        "strict fake".into(),
        Some((task("", "build", None).resource(), Generation::new(3))),
    )
    .unwrap();
    assert_eq!(
        Observed::Verified(1, evidence.clone()).map(|v| v + 1),
        Observed::Verified(2, evidence)
    );
    assert!(Evidence::new("".into(), None).is_err());
    assert!(Evidence::new("a".repeat(Evidence::MAX_SOURCE_BYTES + 1), None).is_err());
}

#[test]
fn outcome_requires_natural_completion_and_preserves_non_success() {
    assert!(Outcome::new(
        Ok(()),
        WorkloadOutcome::Succeeded,
        CleanupOutcome::Complete,
        &[RequiredObligation::NotCompleted]
    )
    .is_err());
    assert!(Outcome::new(
        Ok(()),
        WorkloadOutcome::Succeeded,
        CleanupOutcome::Complete,
        &[RequiredObligation::ProcessExit(1)]
    )
    .is_err());
    for (workload, cleanup, code) in [
        (WorkloadOutcome::Succeeded, CleanupOutcome::Complete, 0),
        (WorkloadOutcome::Succeeded, CleanupOutcome::Incomplete, 1),
        (
            WorkloadOutcome::Failed { code: 7 },
            CleanupOutcome::Incomplete,
            7,
        ),
        (
            WorkloadOutcome::Cancelled {
                cause: StopCause::Signal(15),
                signal: Some(15),
            },
            CleanupOutcome::Complete,
            143,
        ),
        (
            WorkloadOutcome::TimedOut {
                phase: "test".into(),
            },
            CleanupOutcome::Complete,
            124,
        ),
        (
            WorkloadOutcome::Unknown(UnknownReason::Injected),
            CleanupOutcome::Complete,
            1,
        ),
    ] {
        assert_eq!(
            Outcome::new(
                Ok(()),
                workload,
                cleanup,
                &[
                    RequiredObligation::ProcessExit(0),
                    RequiredObligation::Applied
                ]
            )
            .unwrap()
            .exit_code(),
            code
        );
    }
    assert_eq!(
        Outcome::<()>::new(
            Err(ErrorInfo {
                code: ErrorCode::Busy,
                message: "busy".into(),
                details: serde_json::json!({"owner":"build"})
            }),
            WorkloadOutcome::NotStarted,
            CleanupOutcome::NotNeeded,
            &[]
        )
        .unwrap()
        .exit_code(),
        75
    );
}
#[test]
fn outcome_parser_checked_next_and_structured_rendering_survive_report_enospc() {
    let parser = |argv: &[OsString]| argv == [OsString::from("status"), OsString::from("build")];
    assert!(Invocation::validated(vec!["not-a-command".into()], parser).is_err());
    let next = Invocation::validated(vec!["status".into(), "build".into()], parser).unwrap();
    assert!(parser(next.argv()));
    let mut outcome = Outcome::new(
        Ok("done"),
        WorkloadOutcome::Succeeded,
        CleanupOutcome::Complete,
        &[RequiredObligation::Applied],
    )
    .unwrap();
    outcome.add_next(next);
    let json: serde_json::Value = serde_json::from_str(&outcome.json().unwrap()).unwrap();
    let text: serde_json::Value = serde_json::from_str(&outcome.text().unwrap()).unwrap();
    assert_eq!(json, text);
    struct Enospc;
    impl std::io::Write for Enospc {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::from_raw_os_error(libc::ENOSPC))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    for report in [outcome.json().unwrap(), outcome.text().unwrap()] {
        use std::io::Write;
        assert_eq!(
            Enospc
                .write_all(report.as_bytes())
                .unwrap_err()
                .raw_os_error(),
            Some(libc::ENOSPC)
        );
        outcome.add_warning(Warning {
            code: "report_io".into(),
            message: "ENOSPC".into(),
        });
        assert_eq!(outcome.exit_code(), 0);
    }
}

#[test]
fn outcome_error_details_stay_structured_and_invalid_signal_cannot_overflow() {
    let outcome = Outcome::<()>::new(
        Err(ErrorInfo {
            code: ErrorCode::Io,
            message: "report failed".into(),
            details: serde_json::json!({"phase":"stop","residue":["child"]}),
        }),
        WorkloadOutcome::NotStarted,
        CleanupOutcome::Incomplete,
        &[],
    )
    .unwrap();
    let report = outcome.report().unwrap();
    assert!(report["result"]["Err"]["details"].is_object());
    assert_eq!(report["result"]["Err"]["message"], "report failed");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&outcome.text().unwrap()).unwrap(),
        serde_json::from_str::<serde_json::Value>(&outcome.json().unwrap()).unwrap()
    );
    assert!(Outcome::new(
        Ok(()),
        WorkloadOutcome::Cancelled {
            cause: StopCause::Requested,
            signal: Some(i32::MAX)
        },
        CleanupOutcome::Complete,
        &[]
    )
    .is_err());
}

#[test]
fn context_guest_checkout_token_minting_requires_host_authority() {
    for class in [EffectClass::Prepare, EffectClass::Mutate] {
        assert!(context(class, Placement::Guest)
            .effects()
            .unwrap()
            .require(Capability::MintToken)
            .is_err());
        assert!(context(class, Placement::Host)
            .effects()
            .unwrap()
            .require(Capability::MintToken)
            .is_ok());
    }
}

#[test]
fn identity_wire_roundtrip_preserves_non_utf8_and_rejects_invalid_frames() {
    let inv = ResolvedInvocation::new(invocation()).unwrap();
    let encoded = serde_json::to_vec(&inv).unwrap();
    let decoded = ResolvedInvocation::decode(&encoded).unwrap();
    assert_eq!(decoded.digest(), inv.digest());
    assert_eq!(decoded.spec().argv, inv.spec().argv);
    assert_eq!(serde_json::to_vec(&decoded).unwrap(), encoded);
    for bytes in [
        vec![],
        b"{}".to_vec(),
        vec![b' '; ResolvedInvocation::MAX_WIRE_BYTES + 1],
    ] {
        assert!(ResolvedInvocation::decode(&bytes).is_err());
    }
    // Tamper the canonical bytes in place. Reordering through Value would
    // independently invalidate the frame and could mask a missing digest check.
    let canonical = String::from_utf8(encoded.clone()).unwrap();
    for (old, new) in [
        ("\"schema\":1", "\"schema\":2"),
        ("\"cwd\":\"apps/api\"", "\"cwd\":\"./apps/api/\""),
        ("\"catalog\":\"apps/api\"", "\"catalog\":\"./apps/api\""),
        ("\"argv\":[[97,255]]", "\"argv\":[[98,255]]"),
        ("\"argv\":[[97,255]]", "\"argv\":[[256,255]]"),
        ("\"class\":\"Mutate\"", "\"class\":\"Unknown\""),
        ("\"placement\":\"Host\"", "\"placement\":\"Detached\""),
    ] {
        assert!(canonical.contains(old), "missing fixture token {old}");
        let tampered = canonical.replacen(old, new, 1);
        assert!(
            ResolvedInvocation::decode(tampered.as_bytes()).is_err(),
            "{old}"
        );
    }
    let digest_at = canonical.rfind("\"digest\":[").unwrap() + "\"digest\":[".len();
    let digest_end = digest_at + canonical[digest_at..].find(',').unwrap();
    let first = canonical[digest_at..digest_end].parse::<u8>().unwrap();
    let mut changed_digest = canonical.clone();
    changed_digest.replace_range(digest_at..digest_end, &first.wrapping_add(1).to_string());
    assert!(ResolvedInvocation::decode(changed_digest.as_bytes()).is_err());
    let mut padded = encoded.clone();
    padded.push(b' ');
    assert!(ResolvedInvocation::decode(&padded).is_err());
    let unknown = format!("{},\"extra\":true}}", canonical.strip_suffix('}').unwrap());
    assert!(ResolvedInvocation::decode(unknown.as_bytes()).is_err());
    let duplicate = canonical.replacen("\"schema\":1", "\"schema\":1,\"schema\":1", 1);
    assert!(ResolvedInvocation::decode(duplicate.as_bytes()).is_err());
}

#[test]
fn identity_wire_checks_nested_resources_task_fields_and_constituents() {
    let mut spec = invocation();
    spec.target = ExecutionTarget::Container {
        env: ResourceId::environment(checkout(), "dev").unwrap(),
        service: "app".into(),
        user: "fixture".into(),
        workdir: "/workspace".into(),
    };
    spec.attachments.push(AttachmentRequest {
        environment: ResourceId::environment(checkout(), "dev").unwrap(),
        mode: AttachmentMode::Exclusive {
            stop_on_release: false,
        },
    });
    spec.env.insert(
        "TOKEN".into(),
        EnvBinding {
            provenance: "fixture".into(),
            secret_reference: Some("vault/key".into()),
        },
    );
    spec.provenance
        .insert("catalog".into(), Digest::of(b"fixture"));
    let inv = ResolvedInvocation::new(spec).unwrap();
    let encoded = serde_json::to_vec(&inv).unwrap();
    assert_eq!(
        ResolvedInvocation::decode(&encoded).unwrap().digest(),
        inv.digest()
    );
    let mut task = serde_json::to_value(&inv.spec().task).unwrap();
    task["catalog"] = "./apps/api".into();
    assert!(serde_json::from_value::<TaskId>(task).is_err());
    let mut resource = serde_json::to_value(&inv.spec().owner).unwrap();
    resource["key"][2] = "".into();
    assert!(serde_json::from_value::<ResourceId>(resource).is_err());
}

#[test]
fn context_effect_child_bounds_store_phases_without_extending_parent() {
    let cx = context(EffectClass::Mutate, Placement::Host)
        .effects()
        .unwrap();
    let child = cx.child(Duration::from_secs(30));
    assert!(
        child.operation().remaining().unwrap()
            <= cx.operation().remaining().unwrap() + Duration::from_millis(1)
    );
    assert_eq!(child.operation().class(), cx.operation().class());
    cx.operation().cancel_token().cancel(StopCause::Requested);
    assert!(child.operation().check("phase").is_err());
}

#[test]
fn identity_canonical_decode_roundtrips_all_stored_kinds_and_scopes() {
    let mut ids = vec![
        checkout().resource(),
        task("apps/api", "a:b", Some("fast")).resource(),
        task("", "build", None).resource(),
        RunId::new(1, [9; 10]).unwrap().resource(),
        ResourceId::environment(checkout(), "dev").unwrap(),
        ResourceId::data("docker", "fixture", "a-b").unwrap(),
        ResourceId::gateway("user").unwrap(),
        ResourceId::store(Scope::Machine, "gateway-endpoints").unwrap(),
        ResourceId::store(Scope::Checkout(checkout()), "task-lock:qa").unwrap(),
    ];
    for (kind, value) in [
        (EndpointKind::Domain, "example.test"),
        (EndpointKind::Address, "127.0.0.1"),
        (EndpointKind::Port, "8000"),
    ] {
        ids.push(ResourceId::endpoint(kind, value).unwrap());
    }
    for kind in [
        CheckoutKind::Primary,
        CheckoutKind::LinkedWorktree,
        CheckoutKind::EphemeralClone,
    ] {
        ids.push(
            TaskId::new(
                CheckoutId::new(kind, [5; 16]),
                "",
                Surface::Draft,
                "é",
                None,
            )
            .unwrap()
            .resource(),
        );
    }
    for id in ids {
        let bytes = id.canonical();
        assert_eq!(ResourceId::decode_canonical(&bytes).unwrap(), id);
        for end in 0..bytes.len() {
            assert!(ResourceId::decode_canonical(&bytes[..end]).is_err());
        }
        let mut extra = bytes.clone();
        extra.push(0);
        assert!(ResourceId::decode_canonical(&extra).is_err());
    }
}

#[test]
fn identity_canonical_decode_rejects_unknown_tags_noncanonical_and_hostile_lengths() {
    let original = ResourceId::store(Scope::Machine, "x").unwrap().canonical();
    for offset in [0, 11, 12, 13, 14] {
        let mut bad = original.clone();
        bad[offset] = 255;
        assert!(ResourceId::decode_canonical(&bad).is_err());
    }
    let mut huge = original.clone();
    huge[15..19].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(ResourceId::decode_canonical(&huge).is_err());
    let mut absent = original.clone();
    absent[14] = 0;
    assert!(ResourceId::decode_canonical(&absent).is_err());
    let mut utf8 = original;
    utf8[19] = 255;
    assert!(ResourceId::decode_canonical(&utf8).is_err());
    let id = ResourceId::endpoint(EndpointKind::Domain, "example.test").unwrap();
    let mut uppercase = id.canonical();
    let last = uppercase.last_mut().unwrap();
    *last = b'T';
    assert!(ResourceId::decode_canonical(&uppercase).is_err());
}
