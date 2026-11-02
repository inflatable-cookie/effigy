//! Same-executable qualification control: no capability or helper is faked.
use super::*;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

pub(super) fn run(home: &Path, cancel: CancelToken, mode: &str) {
    let cx = op(cancel).effects().unwrap();
    let root = StateRoot::effects(&cx, home).unwrap();
    let mut request = invocation().spec().clone();
    request.owner = ResourceId::environment(request.task.checkout(), "qualification").unwrap();
    let invocation = ResolvedInvocation::new(request).unwrap();
    let owner = invocation.spec().owner.clone();
    let reservation = effigy_kernel::workspace_session::WorkspaceActivationReservation::reserve(
        &cx,
        &root,
        &owner,
        Wait::FailFast,
    )
    .unwrap();
    let executable = if mode == "private-qualification-foreign" {
        let copy = home.join("foreign-helper");
        std::fs::copy(std::env::current_exe().unwrap(), &copy).unwrap();
        Executable::capture(&copy).unwrap()
    } else {
        Executable::current().unwrap()
    };
    let mut run = Run::begin_claimed_recoverable(
        &cx,
        &root,
        &invocation,
        BeginOpts {
            id: RunId::new(1, [1; 10]).unwrap(),
            wait: Wait::FailFast,
            acknowledge: vec![],
            next: next(),
            custody: Some(LifecyclePlan {
                executable,
                placement: effigy_kernel::supervise::Placement::Detached,
            }),
        },
        || Ok(cx.operation().cleanup(Duration::from_secs(15)).unwrap()),
        &mut Strict,
        reservation.claim().unwrap(),
    )
    .unwrap_or_else(|(error, _)| panic!("reserved admission: {error}"));
    let mut process_spec = spec(home);
    process_spec.argv = vec![
        "/bin/sh".into(),
        "-c".into(),
        "/bin/cat \"$2\"; /bin/cat \"$2\" >&2; : > \"$1\"".into(),
        "fixture".into(),
        home.join("executed").into_os_string(),
        home.join("interpolation/compose.env").into_os_string(),
    ];
    process_spec.terminal = Terminal::Discard;
    if mode == "private-qualification-descendant" {
        process_spec.argv[2] = "/bin/sleep 30 & : > \"$1\"".into();
    }
    let ordinary = Custody::spawn(&mut run, process_spec.clone(), &cx);
    let ordinary_refused = matches!(ordinary, Err(SpawnError::Refused(_)));
    let before = run.effects().unwrap().len();
    let directory = home.join("interpolation");
    std::fs::create_dir(&directory).unwrap();
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(directory.join("compose.env"))
        .unwrap();
    file.write_all(b"private-qualification-secret").unwrap();
    file.sync_all().unwrap();
    file.lock_shared().unwrap();
    let retained = effigy_kernel::interpolation::RetainedInterpolation::new(
        file.into(),
        std::fs::File::open(&directory).unwrap().into(),
        "compose.env".into(),
        effigy_kernel::interpolation::Location::Original,
    )
    .unwrap();
    // The qualification entry retains the exact file; normal spawn above still refuses.
    let bounded = if mode == "private-qualification-expired" {
        cx.child_until(Instant::now())
    } else {
        cx.clone()
    };
    let result = Custody::spawn_workspace_discard_qualification(
        &mut run,
        process_spec,
        &bounded,
        Some(retained),
        None,
        None,
    );
    let refused = result.is_err();
    let effects = run.effects().unwrap().len();
    let mut result = result.map(|mut process| {
        let exit = process.wait(cx.operation());
        let output = process.take_output();
        let closed = process.qualification_group_closure(cx.operation());
        let closed = matches!(closed, Observed::Verified(..) | Observed::Absent(_));
        if mode == "private-qualification-descendant" {
            assert!(!closed, "leader exit is not group closure");
        } else {
            assert!(closed, "ordinary completed group");
        }
        let completed = match exit {
            Observed::Verified(exit, _) => process.complete(&mut run, exit),
            _ => Err(RunError::unavailable("fixture exit unknown")),
        };
        (completed, output)
    });
    let cleanup = cx.operation().cleanup(Duration::from_secs(15)).unwrap();
    let cleanup_ok = if matches!(
        mode,
        "private-qualification" | "private-qualification-descendant"
    ) {
        match run.park_workspace(&reservation, Instant::now() + Duration::from_secs(10)) {
            Ok(parked) => {
                assert_eq!(
                    mode, "private-qualification",
                    "a live descendant must retain Env"
                );
                assert!(
                    !directory.join("compose.env").exists(),
                    "exact file cleanup precedes Env release"
                );
                assert!(
                    matches!(
                        effigy_kernel::gate::Gate::probe(
                            &cx.operation().observe().unwrap(),
                            &root,
                            &owner
                        ),
                        Observed::Verified(effigy_kernel::gate::GateState::Free, _)
                    ),
                    "Env is actually free for integrations"
                );
                parked.settle(&cleanup, &mut Strict).is_ok()
            }
            Err((error, retained)) => {
                assert_eq!(
                    mode, "private-qualification-descendant",
                    "closed owner must park: {error}"
                );
                assert!(
                    directory.join("compose.env").exists(),
                    "Unknown keeps exact file"
                );
                assert!(
                    matches!(
                        effigy_kernel::gate::Gate::probe(
                            &cx.operation().observe().unwrap(),
                            &root,
                            &owner
                        ),
                        Observed::Verified(effigy_kernel::gate::GateState::Held(_), _)
                    ),
                    "Unknown keeps Env"
                );
                retained
                    .settle_workspace_recoverable(&cleanup, &mut Strict)
                    .is_ok()
            }
        }
    } else {
        run.settle_workspace_recoverable(&cleanup, &mut Strict)
            .is_ok()
    };
    assert!(cleanup_ok, "qualification cleanup remains unsettled");
    reservation.cancel(&cx, &root, Wait::FailFast).unwrap();
    assert!(
        ordinary_refused && before == 0,
        "ordinary Discard must refuse without an intent"
    );
    if !matches!(
        mode,
        "private-qualification" | "private-qualification-descendant"
    ) {
        assert!(
            refused && effects == before,
            "unqualified helper/deadline must refuse before an intent"
        );
        assert!(!home.join("executed").exists());
        return;
    }
    let (completed, output) = result.as_mut().expect("qualified private process");
    assert!(completed.is_ok());
    assert!(output.is_empty(), "private bytes must not be retained");
    assert!(home.join("executed").is_file(), "actual command executed");
    assert!(
        !directory.join("compose.env").exists(),
        "verified closure cleaned exact file"
    );
}
