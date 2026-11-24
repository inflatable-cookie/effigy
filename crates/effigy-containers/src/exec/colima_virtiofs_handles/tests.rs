use super::*;
use std::time::Duration;

const ROOT: &str = "/Volumes/Stash/a worktree";
const DISK: &str = "/Users/test/.colima/_lima/colima-effigy/disk";
const VZ: &str = "/System/Library/Frameworks/Virtualization.framework/Versions/A/XPCServices/com.apple.Virtualization.VirtualMachine.xpc/Contents/MacOS/com.apple.Virtualization.VirtualMachine";

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(5)
}

#[test]
fn scopes_descriptor_inventory_to_the_profile_disk_owner() {
    let deadline = deadline();
    let descriptors = format!(
        "p42\0\nf3\0tREG\0n{DISK}\0\nf4\0tDIR\0n{ROOT}\0\np43\0\nf5\0tDIR\0n{ROOT}/foreign\0\n"
    );
    let mut expected = [
        (
            vec!["-nP", "-X", "-a", "-p", "42,43", "-F0p", "-f", "--", DISK],
            &b"p42\0\np44\0\n"[..],
        ),
        (
            vec!["-nP", "-X", "-p", "42", "-F0pftn"],
            descriptors.as_bytes(),
        ),
    ]
    .into_iter();
    let survivors = probe(
        Path::new(ROOT),
        Path::new(DISK),
        deadline,
        &[42, 43],
        |args, until| {
            let (expected_args, output) = expected.next().unwrap();
            assert_eq!((args, until), (expected_args.as_slice(), deadline));
            Some(output.to_vec())
        },
    );
    assert!(expected.next().is_none());
    assert_eq!(survivors.len(), 1);
    assert_eq!(survivors[&42].paths, [PathBuf::from(ROOT)]);
}

#[test]
fn reports_workspace_root_and_nested_handles_and_ignores_other_paths() {
    let runtime = format!("{ROOT}/.effigy/runtime");
    let checkout = format!("{ROOT}/.effigy/state/objects/hash/checkout");
    let descriptors = format!(
        "p42\0\n\
         f2\0tREG\0n{DISK}\0\n\
         f3\0tDIR\0n{ROOT}\0\n\
         f4\0tDIR\0n{runtime}\0\n\
         f5\0tDIR\0n{checkout}\0\n\
         f6\0tDIR\0n{runtime}\0\n\
         f7\0tDIR\0n{ROOT}-sibling/.effigy/runtime\0\n\
         f8\0tREG\0n/other/worktree/file\0\n\
         f9\0tREG\0n{ROOT}/loose-file\0\n\
         fcwd\0tDIR\0n{ROOT}/cwd\0\n"
    );
    let survivors = workspace_handles(
        descriptors.as_bytes(),
        &[42],
        Path::new(ROOT),
        Path::new(DISK),
    );
    let handles = &survivors[&42];
    assert_eq!(handles.count, 5);
    assert_eq!(
        handles.warning(42, Path::new(ROOT), "effigy"),
        format!(
            "[warn] Colima profile `effigy` still has 5 open VirtioFS handles under `{ROOT}` after teardown (Virtualization PID 42). Paths include `{ROOT}`, `{runtime}`, `{checkout}`, `{ROOT}/loose-file`. Run `colima restart effigy` to release them.",
        )
    );
}

#[test]
fn keeps_the_count_when_more_paths_survive_than_fit_in_the_warning() {
    let mut handles = Survivors::default();
    for index in 0..12 {
        handles.record(&Path::new(ROOT).join(index.to_string()));
    }
    assert_eq!(handles.count, 12);
    assert_eq!(handles.paths.len(), 5);
    let warning = handles.warning(42, Path::new(ROOT), "effigy-release");
    assert!(warning.contains("12 open VirtioFS handles"));
    assert!(warning.contains(&format!("`{ROOT}/4`")));
    assert!(!warning.contains(&format!("`{ROOT}/5`")));
    assert!(warning.ends_with("Run `colima restart effigy-release` to release them."));
}

#[test]
fn no_warning_when_no_handles_survive_or_the_disk_owner_is_unverified() {
    for descriptors in [
        format!("p42\0\nf3\0tREG\0n{DISK}\0\nf4\0tDIR\0n{ROOT}-sibling\0\n"),
        format!("p42\0\nf3\0tREG\0n/foreign/disk\0\nf4\0tDIR\0n{ROOT}\0\n"),
        format!("p43\0\nf3\0tREG\0n{DISK}\0\nf4\0tDIR\0n{ROOT}\0\n"),
        format!("p42\0\nf3\0tREG\0n{DISK}\0\nf4\0tDIR\0n{ROOT}/incomplete"),
    ] {
        assert!(workspace_handles(
            descriptors.as_bytes(),
            &[42],
            Path::new(ROOT),
            Path::new(DISK)
        )
        .is_empty());
    }
}

#[test]
fn absent_processes_never_fall_through_to_an_unscoped_lsof() {
    let survivors = probe(Path::new(ROOT), Path::new(DISK), deadline(), &[], |_, _| {
        panic!("no Virtualization PID was found");
    });
    assert!(survivors.is_empty());
}

#[test]
fn candidates_must_be_apple_virtualization_executables() {
    assert!(is_vz_executable(Path::new(VZ)));
    assert!(!is_vz_executable(Path::new("/opt/homebrew/bin/limactl")));
    assert!(!is_vz_executable(Path::new(&format!(
        "/somewhere/{VZ_EXECUTABLE}"
    ))));
}

#[test]
fn unavailable_or_timed_out_probes_are_best_effort() {
    for stop_at in 0..2 {
        let mut calls = 0;
        let survivors = probe(
            Path::new(ROOT),
            Path::new(DISK),
            deadline(),
            &[42],
            |_, _| {
                let call = calls;
                calls += 1;
                (call < stop_at).then(|| b"p42\0\n".to_vec())
            },
        );
        assert!(survivors.is_empty());
        assert_eq!(calls, stop_at + 1);
    }

    let expired = Instant::now();
    let survivors = probe(Path::new(ROOT), Path::new(DISK), expired, &[42], |_, _| {
        panic!("an expired probe must not start a command");
    });
    assert!(survivors.is_empty());
}
