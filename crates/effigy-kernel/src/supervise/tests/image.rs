use super::*;

fn replace(world: &World, plan: &LifecyclePlan) {
    std::fs::rename(&plan.executable.path, world.dir.path().join("old-helper")).unwrap();
    std::fs::write(&plan.executable.path, "replacement must never execute").unwrap();
}

#[test]
fn replaced_native_image_verifies_in_the_real_sentinel() {
    let world = World::new();
    let cx = cx();
    let root = StateRoot::effects(&cx, &world.home()).unwrap();
    let fixture = fixture();
    let path = world.dir.path().join("bin/native-helper");
    std::fs::copy(fixture, &path).unwrap();
    let plan = LifecyclePlan {
        executable: Executable::capture(&path).unwrap(),
        placement: crate::supervise::Placement::Inline,
    };
    replace(&world, &plan);
    let mut run = Run::begin(
        &cx,
        &root,
        &inv("native"),
        opts(1, Some(plan)),
        &cleanup(&cx),
        &mut Strict,
    )
    .unwrap();
    assert!(run.has_sentinel());
    world.assert_no_residue();
    run.interrupt(&cleanup(&cx), WorkloadOutcome::NotStarted)
        .unwrap();
    run.settle(&cleanup(&cx), &mut Strict).unwrap();
    world.assert_owner_free(&root, "native", 2);
}

#[test]
fn replaced_image_admits_the_held_image_and_cleans_after_ack() {
    let world = World::new();
    let cx = cx();
    let root = StateRoot::effects(&cx, &world.home()).unwrap();
    let plan = world.helper("printf '\\001'; exec cat >/dev/null");
    replace(&world, &plan);
    let mut run = Run::begin(
        &cx,
        &root,
        &inv("held"),
        opts(1, Some(plan)),
        &cleanup(&cx),
        &mut Strict,
    )
    .unwrap();
    assert!(run.has_sentinel());
    world.assert_no_residue();
    run.interrupt(&cleanup(&cx), WorkloadOutcome::NotStarted)
        .unwrap();
    run.settle(&cleanup(&cx), &mut Strict).unwrap();
    world.assert_helper_reaped();
    world.assert_owner_free(&root, "held", 2);
}

#[test]
fn pin_faults_are_typed_settle_and_remove_partial_images() {
    let mut points = vec!["ImageBeforePin", "ImageAfterPin", "ImageVerified"];
    #[cfg(target_os = "macos")]
    points.extend(["ImageClone", "ImageCloned"]);
    #[cfg(target_os = "linux")]
    points.extend(["ImageFd", "ImageExec"]);
    for point in points {
        let world = World::new();
        let cx = cx();
        let root = StateRoot::effects(&cx, &world.home()).unwrap();
        let plan = world.helper("printf '\\001'; exec cat >/dev/null");
        replace(&world, &plan);
        let fired = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let seen = fired.clone();
        let error = crate::fault_injection::with_hook(
            move |at| {
                if at == point {
                    seen.store(true, std::sync::atomic::Ordering::Relaxed);
                    return Err(io::Error::from_raw_os_error(libc::ENOTSUP));
                }
                Ok(())
            },
            || {
                Run::begin(
                    &cx,
                    &root,
                    &inv("unsupported"),
                    opts(1, Some(plan)),
                    &cleanup(&cx),
                    &mut Strict,
                )
            },
        )
        .expect_err("unsupported zero-copy admission must refuse");
        assert!(fired.load(std::sync::atomic::Ordering::Relaxed), "{point}");
        let (helper, exit, _) = helper_exited(error);
        assert_eq!((helper.as_str(), exit), (SENTINEL, HelperExit::NotStarted));
        assert!(!world.pid_file().exists(), "refusal must precede spawn");
        world.assert_no_residue();
        world.assert_closed_not_started(&root, "unsupported", RunId::new(1, [1; 10]).unwrap());
        world.assert_owner_free(&root, "unsupported", 2);
    }
}

#[test]
fn pinning_refuses_a_source_changed_during_the_pin() {
    let world = World::new();
    let cx = cx();
    let root = StateRoot::effects(&cx, &world.home()).unwrap();
    let plan = world.helper("printf '\\001'; exec cat >/dev/null");
    replace(&world, &plan);
    let old = world.dir.path().join("old-helper");
    let error = crate::fault_injection::with_hook(
        move |at| {
            if at == "ImageAfterPin" {
                std::fs::write(&old, "modified held image")?;
            }
            Ok(())
        },
        || {
            private_image(&root, &plan.executable)
                .err()
                .expect("source mutation refused")
        },
    );
    let (_, exit, _) = helper_exited(error);
    assert_eq!(exit, HelperExit::NotStarted);
    world.assert_no_residue();
}

/// Per-process physical write accounting avoids a volume-free-space oracle
/// contaminated by other workers on the same APFS volume.
#[cfg(target_os = "macos")]
fn written_bytes() -> u64 {
    let mut info = std::mem::MaybeUninit::<libc::rusage_info_v2>::zeroed();
    // SAFETY: the v2 flavor writes exactly this aligned v2 allocation. The
    // current PID is valid; the result is read only after a successful call.
    assert_eq!(
        unsafe {
            libc::proc_pid_rusage(
                libc::getpid(),
                libc::RUSAGE_INFO_V2,
                info.as_mut_ptr().cast(),
            )
        },
        0
    );
    // SAFETY: proc_pid_rusage succeeded and initialized the v2 structure.
    unsafe { info.assume_init() }.ri_diskio_byteswritten
}

#[cfg(target_os = "macos")]
#[test]
fn large_replaced_image_admission_shares_data_and_reports_before_after() {
    let world = World::new();
    let cx = cx();
    let root = StateRoot::effects(&cx, &world.home()).unwrap();
    let plan = world.helper("printf '\\001'; exec cat >/dev/null");
    // A real, allocated 196 MiB image, comparable to the measured debug build.
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&plan.executable.path)
        .unwrap();
    let block = vec![b'#'; 1 << 20];
    for _ in 0..196 {
        file.write_all(&block).unwrap();
    }
    file.sync_all().unwrap();
    drop(file);
    let plan = LifecyclePlan {
        executable: Executable::capture(&plan.executable.path).unwrap(),
        placement: plan.placement,
    };
    replace(&world, &plan);
    let image = plan.executable.replaced_image().unwrap();
    // Reproduce the former byte-copy algorithm for a measured baseline. This
    // exists only in the proof, never as a production fallback.
    let baseline = world.dir.path().join("baseline");
    let before = written_bytes();
    let start = Instant::now();
    let mut output = std::fs::File::create(&baseline).unwrap();
    let mut buffer = vec![0u8; 1 << 20];
    let mut offset = 0;
    loop {
        let n = std::os::unix::fs::FileExt::read_at(image, &mut buffer, offset).unwrap();
        if n == 0 {
            break;
        }
        output.write_all(&buffer[..n]).unwrap();
        offset += n as u64;
    }
    output.sync_all().unwrap();
    let copy_time = start.elapsed();
    let copy_bytes = written_bytes() - before;
    drop(output);
    std::fs::set_permissions(&baseline, std::fs::Permissions::from_mode(0o700)).unwrap();
    let baseline_plan = LifecyclePlan {
        executable: Executable::capture(&baseline).unwrap(),
        placement: plan.placement,
    };
    let start = Instant::now();
    let mut baseline_run = Run::begin(
        &cx,
        &root,
        &inv("measure"),
        opts(1, Some(baseline_plan)),
        &cleanup(&cx),
        &mut Strict,
    )
    .unwrap();
    let baseline_admission = start.elapsed() + copy_time;
    let baseline_bytes = written_bytes() - before;
    baseline_run
        .interrupt(&cleanup(&cx), WorkloadOutcome::NotStarted)
        .unwrap();
    baseline_run.settle(&cleanup(&cx), &mut Strict).unwrap();

    let before = written_bytes();
    let start = Instant::now();
    let pin = private_image(&root, &plan.executable).unwrap().unwrap();
    let clone_time = start.elapsed();
    let clone_bytes = written_bytes() - before;
    assert_eq!(pin.executable.size, offset);
    assert_ne!(pin.executable.inode, plan.executable.inode);
    assert!(
        clone_bytes < offset / 100,
        "clone wrote {clone_bytes} for {offset} bytes"
    );
    drop(pin);
    let before = written_bytes();
    let start = Instant::now();
    let mut run = Run::begin(
        &cx,
        &root,
        &inv("measure"),
        opts(2, Some(plan)),
        &cleanup(&cx),
        &mut Strict,
    )
    .unwrap();
    let admission_time = start.elapsed();
    let admission_bytes = written_bytes() - before;
    assert!(
        admission_bytes < offset / 100,
        "admission wrote {admission_bytes}"
    );
    eprintln!(
        "replaced-image measurement: image={offset} legacy_copy_bytes={copy_bytes} legacy_copy_wall={copy_time:?} legacy_admission_bytes={baseline_bytes} legacy_admission_wall={baseline_admission:?} clone_bytes={clone_bytes} clone_wall={clone_time:?} admission_bytes={admission_bytes} admission_wall={admission_time:?}"
    );
    run.interrupt(&cleanup(&cx), WorkloadOutcome::NotStarted)
        .unwrap();
    run.settle(&cleanup(&cx), &mut Strict).unwrap();
    world.assert_no_residue();
}

#[test]
#[cfg(target_os = "linux")]
fn retained_native_helper_runs_from_the_pinned_image_after_descriptor_preparation() {
    let world = World::new();
    let cx = cx();
    let root = StateRoot::effects(&cx, &world.home()).unwrap();
    let path = world.dir.path().join("bin/native-helper");
    std::fs::copy(fixture(), &path).unwrap();
    let plan = LifecyclePlan {
        executable: Executable::capture(&path).unwrap(),
        placement: crate::supervise::Placement::Inline,
    };
    replace(&world, &plan);
    let pin = private_image(&root, &plan.executable).unwrap().unwrap();
    let home = world.dir.path().join("capability-free-home");
    std::fs::create_dir(&home).unwrap();
    let mut command = helper(&pin.executable, "capability-free").unwrap();
    command.arg(&home);
    prepare_helper_exec(&mut command, &pin.executable, "capability-free").unwrap();
    assert!(command.status().unwrap().success());
}

/// A fresh temporary directory on a filesystem other than `device`.
#[cfg(target_os = "macos")]
fn other_device_tempdir(device: u64) -> Option<tempfile::TempDir> {
    let mut parents = vec![std::env::temp_dir()];
    parents.push(std::path::PathBuf::from("/Users/Shared"));
    parents.push(std::path::PathBuf::from("/var/tmp"));
    for parent in parents {
        if !parent.is_dir() {
            continue;
        }
        let Ok(dir) = tempfile::Builder::new().prefix("kx-").tempdir_in(&parent) else {
            continue;
        };
        let Ok(metadata) = std::fs::metadata(dir.path()) else {
            continue;
        };
        if metadata.dev() != device {
            return Some(dir);
        }
    }
    None
}

/// A state root on another volume from the running image is an ordinary
/// operator setup — a project on an external drive, or `[scratch]` on a build
/// disk — and `fclonefileat` cannot cross filesystems there. The pin must
/// still be exactly the running image: a copy read back against the held
/// image, flushed and made read-only, that the sentinel re-execs and removes
/// after it acknowledges.
#[cfg(target_os = "macos")]
#[test]
fn cross_volume_replaced_image_is_pinned_by_a_verified_copy() {
    let world = World::new();
    let cx = cx();
    let root = StateRoot::effects(&cx, &world.home()).unwrap();
    let state_device = std::fs::metadata(root.path()).unwrap().dev();
    let Some(other) = other_device_tempdir(state_device) else {
        eprintln!("skip: this host offers one filesystem only, so no cross-volume pin is provable");
        return;
    };
    let path = other.path().join("bin/helper");
    std::fs::create_dir(other.path().join("bin")).unwrap();
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\necho $$ > '{}'\nprintf '\\001'; exec cat >/dev/null\n",
            world.pid_file().display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    let plan = LifecyclePlan {
        executable: Executable::capture(&path).unwrap(),
        placement: crate::supervise::Placement::Inline,
    };
    assert_ne!(std::fs::metadata(&path).unwrap().dev(), state_device);
    // The rebuild replaces the path on the image's own volume: a rename across
    // devices is the very thing a pin cannot do.
    std::fs::rename(&path, other.path().join("old-helper")).unwrap();
    std::fs::write(&path, "replacement must never execute").unwrap();

    let held = plan.executable.replaced_image().expect("held image");
    let mut expected = Vec::new();
    std::io::Read::read_to_end(&mut &*held, &mut expected).unwrap();
    let pin = private_image(&root, &plan.executable).unwrap().unwrap();
    let pinned = std::fs::metadata(&pin.executable.path).unwrap();
    assert_eq!(
        pinned.dev(),
        state_device,
        "the pin belongs to the state root's filesystem"
    );
    assert_eq!(
        std::fs::read(&pin.executable.path).unwrap(),
        expected,
        "the pin is exactly the running image's bytes"
    );
    assert_eq!(
        pinned.permissions().mode() & 0o222,
        0,
        "a copied pin cannot be rewritten"
    );
    drop(pin);
    world.assert_no_residue();

    // The sentinel re-execs the copy and the admitted run behaves as it does
    // with a same-volume clone.
    let mut run = Run::begin(
        &cx,
        &root,
        &inv("cross-volume"),
        opts(1, Some(plan)),
        &cleanup(&cx),
        &mut Strict,
    )
    .unwrap();
    assert!(run.has_sentinel());
    world.assert_no_residue();
    run.interrupt(&cleanup(&cx), WorkloadOutcome::NotStarted)
        .unwrap();
    run.settle(&cleanup(&cx), &mut Strict).unwrap();
    world.assert_helper_reaped();
    world.assert_owner_free(&root, "cross-volume", 2);
    world.assert_no_residue();
}
