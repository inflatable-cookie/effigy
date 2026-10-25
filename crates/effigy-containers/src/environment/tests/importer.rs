//! The marker importer against copies of real stack metadata: read-only
//! `nerdctl inspect` captures of an Underlay worktree stack and a
//! DecodeLabs site stack (environment variables stripped), and a real
//! v0.14.1 partial-activation marker. Live stacks are never touched.

use super::*;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/environment");
const UNDERLAY_CHECKOUT: &str =
    "/Users/tom/.paseo/worktrees/25ojb7u5/bovine-deck-preview-fd4b9a3b-runner";
const UNDERLAY_SCOPE: &str = "4677bfcb81da18054ef32e71aa3f3c29";
const SITE_CHECKOUT: &str = "/Users/tom/Dev/legacy/sites/acowtancy";

/// Loads captures with the recorded checkout path moved to `checkout`.
fn captured(files: &[&str], from: &str, checkout: &Path) -> Vec<Workload> {
    files
        .iter()
        .flat_map(|file| {
            let raw = std::fs::read_to_string(format!("{FIXTURES}/{file}")).expect("fixture");
            parse_inspect(
                raw.replace(from, &checkout.display().to_string())
                    .as_bytes(),
            )
            .expect("capture parses")
        })
        .collect()
}

struct Legacy {
    dir: PathBuf,
    before: BTreeMap<PathBuf, Vec<u8>>,
}

impl Legacy {
    fn write(home: &Path, files: &[(&str, Vec<u8>)]) -> Self {
        let dir = home.join("host-container-leases/effigy/acme-stack");
        std::fs::create_dir_all(&dir).unwrap();
        let mut before = BTreeMap::new();
        for (name, bytes) in files {
            std::fs::write(dir.join(name), bytes).unwrap();
            before.insert(dir.join(name), bytes.clone());
        }
        Self { dir, before }
    }

    /// M3: every legacy file is byte-identical and nothing was added.
    fn assert_untouched(&self) {
        let now = std::fs::read_dir(&self.dir)
            .unwrap()
            .map(|entry| {
                let path = entry.unwrap().path();
                let bytes = std::fs::read(&path).unwrap();
                (path, bytes)
            })
            .collect::<BTreeMap<_, _>>();
        assert_eq!(now, self.before);
    }
}

fn import_with(
    ctx: &EnvContext<'_>,
    legacy: &Legacy,
    workloads: Vec<Workload>,
    ownership: &Ownership,
    primary: &str,
) -> Result<Imported, EnvError> {
    let markers = LegacyMarkers::read(&legacy.dir, "stack");
    import(
        ctx,
        ImportRequest {
            env: &env("stack"),
            project: PROJECT,
            markers: &markers,
            workloads: verified(workloads),
            ownership,
            primary_service: primary,
            backend: &selection(),
        },
    )
}

#[test]
fn importer_adopts_a_running_underlay_stack_without_any_backend_call() {
    let home = home();
    let checkout = home.path().join("checkout");
    std::fs::create_dir_all(&checkout).unwrap();
    let root = root(home.path());
    let ctx = EnvContext::new(&root, CancelToken::default()).unwrap();
    let partial = std::fs::read(format!("{FIXTURES}/underlay-stack.partial.json")).unwrap();
    let legacy = Legacy::write(
        home.path(),
        &[
            ("stack.persistent", vec![]),
            ("stack.complete", vec![]),
            ("stack.partial.json", partial.clone()),
        ],
    );
    let workloads = captured(
        &[
            "underlay-workspace.inspect.json",
            "underlay-mysql.inspect.json",
        ],
        UNDERLAY_CHECKOUT,
        &checkout,
    );
    assert!(workloads.iter().all(|workload| workload.running));
    let ownership = Ownership {
        checkout: checkout.canonicalize().unwrap(),
        scope: Some(UNDERLAY_SCOPE.to_owned()),
    };
    let imported = import_with(&ctx, &legacy, workloads.clone(), &ownership, "workspace").unwrap();
    assert_eq!(
        imported,
        Imported::Recorded {
            retention: Retention::Persistent,
            active: true,
            labels: LabelEvidence::Verified { workloads: 2 },
        }
    );
    let adopted = record(&ctx, &env("stack"));
    assert!(adopted.active());
    let provenance = adopted.engine().unwrap().imported.clone().unwrap();
    assert_eq!(provenance.markers.len(), 3);
    assert!(!provenance.pending);
    assert!(provenance
        .markers
        .iter()
        .all(|marker| marker.sha256.len() == 64));
    // Idempotent: a second import changes nothing, not even the generation.
    let generation = adopted.generation;
    assert_eq!(
        import_with(&ctx, &legacy, workloads, &ownership, "workspace").unwrap(),
        Imported::AlreadyRecorded
    );
    assert_eq!(record(&ctx, &env("stack")).generation, generation);
    // The adopted stack is attachable and protected like any other.
    let mut task = holder(&ctx, "dev", 1);
    let held = hold(&mut task, &ctx, &env("stack"), Wait::FailFast, true).unwrap();
    assert!(matches!(held.mode, AttachMode::Borrow));
    task.finish(WorkloadOutcome::Succeeded).unwrap();
    task.settle(&ctx.cleanup, &mut EnvSettlement).unwrap();
    legacy.assert_untouched();
    assert_store_sound(&ctx);
}

#[test]
fn importer_refuses_to_adopt_workloads_of_another_checkout_or_scope() {
    let home = home();
    let checkout = home.path().join("checkout");
    let elsewhere = home.path().join("elsewhere");
    std::fs::create_dir_all(&checkout).unwrap();
    std::fs::create_dir_all(&elsewhere).unwrap();
    let root = root(home.path());
    let ctx = EnvContext::new(&root, CancelToken::default()).unwrap();
    let legacy = Legacy::write(home.path(), &[("stack.complete", vec![])]);
    let foreign = captured(
        &["underlay-workspace.inspect.json"],
        UNDERLAY_CHECKOUT,
        &elsewhere,
    );
    for ownership in [
        Ownership {
            checkout: checkout.canonicalize().unwrap(),
            scope: Some(UNDERLAY_SCOPE.to_owned()),
        },
        Ownership {
            checkout: elsewhere.canonicalize().unwrap(),
            scope: Some("0".repeat(32)),
        },
    ] {
        let home = self::home();
        let root = self::root(home.path());
        let ctx = EnvContext::new(&root, CancelToken::default()).unwrap();
        let imported =
            import_with(&ctx, &legacy, foreign.clone(), &ownership, "workspace").unwrap();
        assert!(matches!(
            imported,
            Imported::Recorded {
                active: false,
                labels: LabelEvidence::Unknown(_),
                ..
            }
        ));
        // M4: an unproven stack is never active, so no run can hold it and
        // no ensure may adopt it without the reconciler's own ownership proof.
        let mut task = holder(&ctx, "dev", 1);
        assert!(matches!(
            hold(&mut task, &ctx, &env("stack"), Wait::FailFast, false),
            Err(HoldError::NotActive)
        ));
        task.settle(&ctx.cleanup, &mut EnvSettlement).unwrap();
    }
    drop(ctx);
    legacy.assert_untouched();
}

#[test]
fn importer_adopts_a_pre_label_site_only_with_a_bind_mount_of_the_checkout() {
    let home = home();
    let checkout = home.path().join("site");
    std::fs::create_dir_all(&checkout).unwrap();
    let ownership = Ownership {
        checkout: checkout.canonicalize().unwrap(),
        scope: None,
    };
    let running = |mut workloads: Vec<Workload>| {
        for workload in &mut workloads {
            workload.running = true;
        }
        workloads
    };
    // The web server binds the checkout itself: positive evidence.
    let web = running(captured(
        &["decodelabs-site-web.inspect.json"],
        SITE_CHECKOUT,
        &checkout,
    ));
    let root = root(home.path());
    let ctx = EnvContext::new(&root, CancelToken::default()).unwrap();
    let legacy = Legacy::write(home.path(), &[("stack.persistent", vec![])]);
    assert!(matches!(
        import_with(&ctx, &legacy, web, &ownership, "web").unwrap(),
        Imported::Recorded {
            active: true,
            labels: LabelEvidence::Verified { workloads: 1 },
            ..
        }
    ));
    // Its database binds only a file below the checkout: not evidence, so
    // the whole stack stays unknown (M4) and nothing is adopted.
    let other = self::home();
    let root = self::root(other.path());
    let ctx = EnvContext::new(&root, CancelToken::default()).unwrap();
    let stack = running(captured(
        &[
            "decodelabs-site-web.inspect.json",
            "decodelabs-site-mysql.inspect.json",
        ],
        SITE_CHECKOUT,
        &checkout,
    ));
    assert!(matches!(
        import_with(&ctx, &legacy, stack, &ownership, "web").unwrap(),
        Imported::Recorded {
            active: false,
            labels: LabelEvidence::Unknown(_),
            ..
        }
    ));
    legacy.assert_untouched();
}

#[test]
fn importer_waits_for_an_unexpired_legacy_lease_and_imports_after_it() {
    let home = home();
    let root = root(home.path());
    let ctx = EnvContext::new(&root, CancelToken::default()).unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let lease = |expires: u128| {
        serde_json::to_vec(&serde_json::json!({
            "schema": "effigy.host-container-lease.v1",
            "schema_version": 1,
            "token": "t",
            "container_name": "stack",
            "profile": "effigy",
            "project_name": PROJECT,
            "expires_at_epoch_ms": expires,
        }))
        .unwrap()
    };
    let pending = Legacy::write(home.path(), &[("stack.json", lease(now + 60_000))]);
    let refused = import_with(
        &ctx,
        &pending,
        vec![],
        &Ownership {
            checkout: home.path().canonicalize().unwrap(),
            scope: None,
        },
        "app",
    );
    assert!(matches!(refused, Err(EnvError::Invalid(ref message)) if message.contains("lease")));
    assert!(matches!(
        EnvRecord::read(&ctx.observe(), &root, &env("stack").resource),
        Observed::Absent(_)
    ));
    let expired = Legacy::write(home.path(), &[("stack.json", lease(now - 1))]);
    let imported = import_with(
        &ctx,
        &expired,
        vec![],
        &Ownership {
            checkout: home.path().canonicalize().unwrap(),
            scope: None,
        },
        "app",
    )
    .unwrap();
    // A task-started stack is held; with no workloads nothing is active.
    assert_eq!(
        imported,
        Imported::Recorded {
            retention: Retention::Held,
            active: false,
            labels: LabelEvidence::Absent,
        }
    );
    expired.assert_untouched();
}

#[test]
fn importer_without_markers_or_workloads_records_nothing() {
    let home = home();
    let root = root(home.path());
    let ctx = EnvContext::new(&root, CancelToken::default()).unwrap();
    let legacy = Legacy::write(home.path(), &[]);
    assert_eq!(
        import_with(
            &ctx,
            &legacy,
            vec![],
            &Ownership {
                checkout: home.path().canonicalize().unwrap(),
                scope: None,
            },
            "app"
        )
        .unwrap(),
        Imported::Nothing
    );
    assert!(matches!(
        EnvRecord::read(&ctx.observe(), &root, &env("stack").resource),
        Observed::Absent(_)
    ));
}
