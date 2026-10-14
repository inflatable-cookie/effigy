//! Private fixture proofs for listener-handoff status and removal planning.
//!
//! Every test uses a fresh temporary directory and owned children only. Nothing
//! here reads or mutates the fixed system definition, queries the fixed launchd
//! job, elevates, binds a privileged endpoint, or runs an administrator command.
//! The production inspection, classification, rendering and bounded-capture
//! boundaries are exercised through their real functions with injected,
//! non-authorizing observations.

use super::super::ListenerHandoffSetup;
use super::{
    capture_bounded, classify_launchd_capture, classify_observed_handoff_with, inspect_handoff,
    observe_definition_file, observe_stable_definition, removal_plan_steps, render_removal_human,
    render_status_human, BoundedCapture, DefinitionFileReading, DefinitionState, JobObservation,
    JobRunState, OperatorExpectation, OperatorMatch, ProgramPathState, RemovalDisposition,
    RemovalRefusal, SetupDefinitionError, LAUNCHD_PLIST_PATH, MAX_DEFINITION_BYTES,
};
use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn current_uid() -> u32 {
    nix::unistd::Uid::current().as_raw()
}

fn current_setup() -> ListenerHandoffSetup {
    ListenerHandoffSetup::for_current_operator().expect("resolve the current operator")
}

/// Expectation that accepts the fixture file's current owner (a private
/// fixture cannot be root-owned), then adds the real operator and executable.
fn fixture_expectation(setup: &ListenerHandoffSetup) -> OperatorExpectation {
    let mut expectation = OperatorExpectation::from_setup(setup);
    expectation.definition_owner_uid = current_uid();
    expectation
}

fn write_definition(directory: &Path, setup: &ListenerHandoffSetup) -> PathBuf {
    let path = directory.join("dev.effigy.gateway.user.plist");
    write_fixture(&path, &setup.render_launchd_plist());
    path
}

/// Write fixture bytes with a deterministic, safe mode regardless of umask.
fn write_fixture(path: &Path, contents: &str) {
    std::fs::write(path, contents).expect("write fixture definition");
    std::fs::set_permissions(path, Permissions::from_mode(0o644)).expect("set fixture mode");
}

fn present_reading(reading: DefinitionFileReading) -> (super::FileIdentity, Vec<u8>) {
    match reading {
        DefinitionFileReading::Present { identity, bytes } => (identity, bytes),
        other => panic!("expected a present reading, got {other:?}"),
    }
}

fn replace_once(haystack: &str, needle: &str, replacement: &str) -> String {
    let position = haystack
        .find(needle)
        .unwrap_or_else(|| panic!("fixture must contain {needle:?}"));
    let mut replaced = String::with_capacity(haystack.len());
    replaced.push_str(&haystack[..position]);
    replaced.push_str(replacement);
    replaced.push_str(&haystack[position + needle.len()..]);
    replaced
}

/// A matching on-disk definition, operator and executable verify and allow a
/// checked removal plan, whose literal steps are fixed and marked unexecuted.
#[test]
fn listener_handoff_maintenance_verifies_matching_definition_and_allows_plan() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let setup = current_setup();
    let path = write_definition(directory.path(), &setup);
    let expectation = fixture_expectation(&setup);

    let reading = observe_definition_file(&path);
    let (identity, bytes) = present_reading(reading.clone());
    assert_eq!(identity.uid, current_uid(), "fixture owner is observed");
    assert!(
        bytes.starts_with(b"<?xml"),
        "fixture bytes are the definition"
    );

    let report = inspect_handoff(
        &reading,
        JobObservation::Loaded {
            state: JobRunState::Running,
        },
        &expectation,
    );
    assert_eq!(report.definition, DefinitionState::Verified);
    assert_eq!(report.program, ProgramPathState::Current);
    assert_eq!(report.operator, OperatorMatch::Matching);
    assert_eq!(report.removal, RemovalDisposition::PlanAvailable);

    let human = render_removal_human(&report);
    assert!(
        human.contains("UNEXECUTED"),
        "plan states unexecuted: {human}"
    );
    assert!(human.contains("no gateway was stopped"));
    let [bootout, remove] = removal_plan_steps();
    assert_eq!(
        bootout,
        "sudo launchctl bootout system/dev.effigy.gateway.user"
    );
    assert_eq!(
        remove,
        "sudo rm /Library/LaunchDaemons/dev.effigy.gateway.user.plist"
    );
    assert!(human.contains(bootout));
    assert!(human.contains(remove));

    let status = render_status_human(&report);
    assert!(status.contains("definition: verified"));
    assert!(status.contains("program: current"));
    assert!(status.contains("operator: matching"));
    assert!(status.contains("removal: plan available"));
    assert_eq!(
        LAUNCHD_PLIST_PATH,
        "/Library/LaunchDaemons/dev.effigy.gateway.user.plist"
    );
}

/// A verified definition with a confirmed-absent job still allows the fixed
/// plan; both raw file and job being absent reports nothing installed.
#[test]
fn listener_handoff_maintenance_reports_nothing_installed_for_absent_file_and_job() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let setup = current_setup();
    let expectation = fixture_expectation(&setup);

    let missing = observe_definition_file(&directory.path().join("absent.plist"));
    assert_eq!(missing, DefinitionFileReading::Missing);
    let report = inspect_handoff(&missing, JobObservation::ConfirmedAbsent, &expectation);
    assert_eq!(report.definition, DefinitionState::Missing);
    assert_eq!(report.removal, RemovalDisposition::NothingInstalled);
    let human = render_removal_human(&report);
    assert!(human.contains("nothing installed"), "{human}");

    // A present, verified definition with a confirmed-absent job is determinate.
    let path = write_definition(directory.path(), &setup);
    let reading = observe_definition_file(&path);
    let report = inspect_handoff(&reading, JobObservation::ConfirmedAbsent, &expectation);
    assert_eq!(report.removal, RemovalDisposition::PlanAvailable);
}

/// An unresolvable operator reports an explicit unresolved state, never a
/// fabricated operator mismatch, and refuses removal.
#[test]
fn listener_handoff_maintenance_reports_unresolved_operator_honestly() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let setup = current_setup();
    let path = write_definition(directory.path(), &setup);
    let reading = observe_definition_file(&path);
    let report = classify_observed_handoff_with(
        reading,
        JobObservation::ConfirmedAbsent,
        Err(SetupDefinitionError::OperatorRecord),
    );
    assert_eq!(report.definition, DefinitionState::OperatorUnresolved);
    assert_eq!(report.definition.label(), "operator_unresolved");
    assert_eq!(report.operator, OperatorMatch::Unknown);
    assert_eq!(
        report.removal,
        RemovalDisposition::Refused(RemovalRefusal::OperatorUnavailable)
    );
    let status = render_status_human(&report);
    assert!(status.contains("definition: operator_unresolved"));
    assert!(!status.contains("definition: operator_mismatch"));

    // A missing definition with a confirmed-absent job is still determinate:
    // no operator is needed to report nothing installed.
    let missing = observe_definition_file(&directory.path().join("absent.plist"));
    let report = classify_observed_handoff_with(
        missing,
        JobObservation::ConfirmedAbsent,
        Err(SetupDefinitionError::OperatorRecord),
    );
    assert_eq!(report.definition, DefinitionState::Missing);
    assert_eq!(report.removal, RemovalDisposition::NothingInstalled);
}

/// The production launchd probe uses a fixed absolute path, so a
/// caller-controlled `PATH` cannot substitute the program.
#[cfg(target_os = "macos")]
#[test]
fn listener_handoff_maintenance_uses_absolute_launchctl_path() {
    assert_eq!(super::super::maintenance::LAUNCHCTL_PATH, "/bin/launchctl");
    assert!(std::path::Path::new(super::super::maintenance::LAUNCHCTL_PATH).is_absolute());
}

/// A job loaded without a verified definition is an uncertain association and
/// refuses removal.
#[test]
fn listener_handoff_maintenance_refuses_loaded_job_without_definition() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let setup = current_setup();
    let expectation = fixture_expectation(&setup);
    let missing = observe_definition_file(&directory.path().join("absent.plist"));
    let report = inspect_handoff(
        &missing,
        JobObservation::Loaded {
            state: JobRunState::Running,
        },
        &expectation,
    );
    assert_eq!(
        report.removal,
        RemovalDisposition::Refused(RemovalRefusal::JobWithoutDefinition)
    );
    assert_eq!(report.removal.refusal(), Some("job_without_definition"));
}

/// Ownership, mode and object-type refusals are distinct and never produce a
/// usable plan.
#[test]
fn listener_handoff_maintenance_refuses_foreign_owner_unsafe_mode_and_symlink() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let setup = current_setup();
    let path = write_definition(directory.path(), &setup);

    let mut foreign = fixture_expectation(&setup);
    foreign.definition_owner_uid = current_uid().wrapping_add(1);
    let reading = observe_definition_file(&path);
    let report = inspect_handoff(&reading, JobObservation::ConfirmedAbsent, &foreign);
    assert_eq!(report.definition, DefinitionState::ForeignOwner);
    assert_eq!(
        report.removal,
        RemovalDisposition::Refused(RemovalRefusal::DefinitionForeignOwner)
    );

    std::fs::set_permissions(&path, Permissions::from_mode(0o666)).expect("widen fixture mode");
    let expectation = fixture_expectation(&setup);
    let reading = observe_definition_file(&path);
    let report = inspect_handoff(&reading, JobObservation::ConfirmedAbsent, &expectation);
    assert_eq!(report.definition, DefinitionState::UnsafeMode);
    assert_eq!(
        report.removal,
        RemovalDisposition::Refused(RemovalRefusal::DefinitionUnsafeMode)
    );
    std::fs::set_permissions(&path, Permissions::from_mode(0o644)).expect("restore fixture mode");

    let link = directory.path().join("link.plist");
    std::os::unix::fs::symlink(&path, &link).expect("fixture symlink");
    assert_eq!(
        observe_definition_file(&link),
        DefinitionFileReading::Symlink
    );
    let report = inspect_handoff(
        &DefinitionFileReading::Symlink,
        JobObservation::ConfirmedAbsent,
        &expectation,
    );
    assert_eq!(
        report.removal,
        RemovalDisposition::Refused(RemovalRefusal::DefinitionSymlink)
    );

    let report = inspect_handoff(
        &DefinitionFileReading::NotRegular,
        JobObservation::ConfirmedAbsent,
        &expectation,
    );
    assert_eq!(report.definition, DefinitionState::NotRegular);
    assert_eq!(
        observe_definition_file(directory.path()),
        DefinitionFileReading::NotRegular,
        "a directory is not a regular definition"
    );

    let report = inspect_handoff(
        &DefinitionFileReading::Changed,
        JobObservation::ConfirmedAbsent,
        &expectation,
    );
    assert_eq!(
        report.removal,
        RemovalDisposition::Refused(RemovalRefusal::DefinitionChanged)
    );
}

/// Shape, operator and executable-path refusals are distinct, including a
/// moved executable and a missing one, and an extra root key never validates.
#[test]
fn listener_handoff_maintenance_refuses_shape_operator_and_program_mismatch() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let setup = current_setup();
    let expectation = fixture_expectation(&setup);
    let base = setup.render_launchd_plist();

    // Extra root key: unknown shape.
    let extra_root = replace_once(
        &base,
        "</dict>\n</plist>",
        "  <key>ExtraUnknownKey</key>\n  <string>x</string>\n</dict>\n</plist>",
    );
    let path = directory.path().join("extra.plist");
    write_fixture(&path, &extra_root);
    let report = inspect_handoff(
        &observe_definition_file(&path),
        JobObservation::ConfirmedAbsent,
        &expectation,
    );
    assert_eq!(report.definition, DefinitionState::ShapeMismatch);

    // A different, existing executable: stale.
    let mut stale_setup = setup.clone();
    stale_setup.program_path = PathBuf::from("/bin/sh");
    let path = directory.path().join("stale.plist");
    write_fixture(&path, &stale_setup.render_launchd_plist());
    let report = inspect_handoff(
        &observe_definition_file(&path),
        JobObservation::ConfirmedAbsent,
        &expectation,
    );
    assert_eq!(report.definition, DefinitionState::ProgramStale);
    assert_eq!(report.program, ProgramPathState::Stale);
    assert_eq!(
        report.removal,
        RemovalDisposition::Refused(RemovalRefusal::DefinitionProgramStale)
    );

    // A missing executable: missing path.
    let mut missing_setup = setup.clone();
    missing_setup.program_path = directory.path().join("does-not-exist");
    let path = directory.path().join("missing-program.plist");
    write_fixture(&path, &missing_setup.render_launchd_plist());
    let report = inspect_handoff(
        &observe_definition_file(&path),
        JobObservation::ConfirmedAbsent,
        &expectation,
    );
    assert_eq!(report.definition, DefinitionState::ProgramMissing);
    assert_eq!(report.program, ProgramPathState::Missing);

    // A different operator name: mismatch.
    let mut operator_setup = setup.clone();
    operator_setup.user_name = "someone-else".to_owned();
    let path = directory.path().join("operator.plist");
    write_fixture(&path, &operator_setup.render_launchd_plist());
    let report = inspect_handoff(
        &observe_definition_file(&path),
        JobObservation::ConfirmedAbsent,
        &expectation,
    );
    assert_eq!(report.definition, DefinitionState::OperatorMismatch);
    assert_eq!(report.operator, OperatorMatch::Mismatched);

    // Malformed bytes.
    let path = directory.path().join("malformed.plist");
    std::fs::write(&path, b"not a plist").expect("write malformed fixture");
    std::fs::set_permissions(&path, Permissions::from_mode(0o644)).expect("set fixture mode");
    let report = inspect_handoff(
        &observe_definition_file(&path),
        JobObservation::ConfirmedAbsent,
        &expectation,
    );
    assert_eq!(report.definition, DefinitionState::Malformed);
}

/// An oversized file and a permission refusal are distinct from verified
/// absence.
#[test]
fn listener_handoff_maintenance_refuses_oversized_and_permission_states() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let setup = current_setup();
    let expectation = fixture_expectation(&setup);

    let path = directory.path().join("oversized.plist");
    std::fs::write(&path, vec![b'x'; MAX_DEFINITION_BYTES + 1]).expect("write oversized fixture");
    assert_eq!(
        observe_definition_file(&path),
        DefinitionFileReading::Oversized
    );
    let report = inspect_handoff(
        &DefinitionFileReading::Oversized,
        JobObservation::ConfirmedAbsent,
        &expectation,
    );
    assert_eq!(report.definition, DefinitionState::Oversized);

    let report = inspect_handoff(
        &DefinitionFileReading::PermissionDenied,
        JobObservation::ConfirmedAbsent,
        &expectation,
    );
    assert_eq!(report.definition, DefinitionState::PermissionDenied);
    assert_eq!(
        report.removal,
        RemovalDisposition::Refused(RemovalRefusal::DefinitionPermissionDenied)
    );

    // A real, owner-readable-denied file is observed as a permission refusal.
    let denied = directory.path().join("denied.plist");
    std::fs::write(&denied, setup.render_launchd_plist()).expect("write denied fixture");
    std::fs::set_permissions(&denied, Permissions::from_mode(0o000)).expect("deny fixture read");
    assert_eq!(
        observe_definition_file(&denied),
        DefinitionFileReading::PermissionDenied,
        "an unreadable owned file is a permission refusal, not absence"
    );

    let report = inspect_handoff(
        &DefinitionFileReading::Unreadable,
        JobObservation::ConfirmedAbsent,
        &expectation,
    );
    assert_eq!(
        report.removal,
        RemovalDisposition::Refused(RemovalRefusal::DefinitionUnreadable)
    );
}

/// A FIFO at the fixed path is a bounded `NotRegular` refusal, never a
/// blocking open, even though no writer ever connects.
#[test]
fn listener_handoff_maintenance_refuses_fifo_without_blocking() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let fifo = directory.path().join("fifo.plist");
    nix::unistd::mkfifo(
        &fifo,
        nix::sys::stat::Mode::S_IRUSR | nix::sys::stat::Mode::S_IWUSR,
    )
    .expect("create fixture fifo");
    let started = std::time::Instant::now();
    assert_eq!(
        observe_definition_file(&fifo),
        DefinitionFileReading::NotRegular,
        "a FIFO is a non-regular object, not a definition"
    );
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "a FIFO must not block the definition observation"
    );
}

/// launchd capture outcomes map to distinct bounded job observations.
#[test]
fn listener_handoff_maintenance_classifies_launchd_capture_outcomes() {
    let label = super::super::LAUNCHD_JOB_LABEL;
    let path = LAUNCHD_PLIST_PATH;

    let loaded = format!(
        "system/{label} = {{\n\tactive count = 1\n\tpath = {path}\n\tstate = running\n}}\n"
    );
    assert_eq!(
        classify_launchd_capture(label, path, BoundedCapture::Success(loaded.into_bytes())),
        JobObservation::Loaded {
            state: JobRunState::Running
        }
    );

    let idle = format!("system/{label} = {{\n\tpath = {path}\n\tstate = not running\n}}\n");
    assert_eq!(
        classify_launchd_capture(label, path, BoundedCapture::Success(idle.into_bytes())),
        JobObservation::Loaded {
            state: JobRunState::NotRunning
        }
    );

    let foreign_path =
        format!("system/{label} = {{\n\tpath = /tmp/elsewhere.plist\n\tstate = running\n}}\n");
    assert_eq!(
        classify_launchd_capture(
            label,
            path,
            BoundedCapture::Success(foreign_path.into_bytes())
        ),
        JobObservation::AssociationMismatch
    );

    let absent = format!("Bad request.\nCould not find service \"{label}\" in domain for system\n");
    assert_eq!(
        classify_launchd_capture(
            label,
            path,
            BoundedCapture::NonZero {
                code: Some(113),
                stdout: Vec::new(),
                stderr: absent.into_bytes(),
            }
        ),
        JobObservation::ConfirmedAbsent
    );

    assert_eq!(
        classify_launchd_capture(
            label,
            path,
            BoundedCapture::NonZero {
                code: Some(1),
                stdout: Vec::new(),
                stderr: b"Operation not permitted".to_vec(),
            }
        ),
        JobObservation::PermissionDenied
    );
    assert_eq!(
        classify_launchd_capture(
            label,
            path,
            BoundedCapture::NonZero {
                code: Some(1),
                stdout: Vec::new(),
                stderr: b"some other failure".to_vec(),
            }
        ),
        JobObservation::NonZero
    );
    assert_eq!(
        classify_launchd_capture(label, path, BoundedCapture::SpawnFailed),
        JobObservation::Unavailable
    );
    assert_eq!(
        classify_launchd_capture(label, path, BoundedCapture::TimedOut),
        JobObservation::TimedOut
    );
    assert_eq!(
        classify_launchd_capture(label, path, BoundedCapture::OutputLimit),
        JobObservation::OutputLimit
    );
    assert_eq!(
        classify_launchd_capture(label, path, BoundedCapture::Success(b"garbage".to_vec())),
        JobObservation::Malformed
    );

    // Non-determinate job observations refuse with their own bounded reason.
    let directory = tempfile::tempdir().expect("fixture directory");
    let setup = current_setup();
    let expectation = fixture_expectation(&setup);
    let path_on_disk = write_definition(directory.path(), &setup);
    for (observation, reason) in [
        (JobObservation::TimedOut, RemovalRefusal::JobTimedOut),
        (JobObservation::OutputLimit, RemovalRefusal::JobOutputLimit),
        (JobObservation::NonZero, RemovalRefusal::JobNonZero),
        (JobObservation::Malformed, RemovalRefusal::JobMalformed),
        (
            JobObservation::AssociationMismatch,
            RemovalRefusal::JobAssociationMismatch,
        ),
        (JobObservation::Unavailable, RemovalRefusal::JobUnavailable),
        (JobObservation::Unsupported, RemovalRefusal::JobUnsupported),
        (
            JobObservation::PermissionDenied,
            RemovalRefusal::JobPermissionDenied,
        ),
    ] {
        let report = inspect_handoff(
            &observe_definition_file(&path_on_disk),
            observation,
            &expectation,
        );
        assert_eq!(
            report.removal,
            RemovalDisposition::Refused(reason),
            "observation {observation:?}"
        );
    }
}

/// The production bounded capture runs owned children with a deadline and a
/// byte limit, and its output flows through the production classifier.
#[test]
fn listener_handoff_maintenance_captures_owned_child_bounded() {
    let label = super::super::LAUNCHD_JOB_LABEL;
    let loaded =
        format!("system/{label} = {{\n\tpath = {LAUNCHD_PLIST_PATH}\n\tstate = running\n}}\n");
    let output = capture_bounded(
        "/bin/sh",
        &["-c", &format!("printf '%s' '{loaded}'")],
        Duration::from_secs(5),
        MAX_DEFINITION_BYTES,
    );
    let capture = match output {
        BoundedCapture::Success(bytes) => BoundedCapture::Success(bytes),
        other => panic!("expected success, got {}", capture_label(&other)),
    };
    assert_eq!(
        classify_launchd_capture(label, LAUNCHD_PLIST_PATH, capture),
        JobObservation::Loaded {
            state: JobRunState::Running
        }
    );

    let absent = capture_bounded(
        "/bin/sh",
        &[
            "-c",
            &format!("printf 'Bad request.\\nCould not find service \"{label}\" in domain for system\\n' 1>&2; exit 113"),
        ],
        Duration::from_secs(5),
        MAX_DEFINITION_BYTES,
    );
    assert_eq!(
        classify_launchd_capture(label, LAUNCHD_PLIST_PATH, absent),
        JobObservation::ConfirmedAbsent
    );

    // A child that never ends hits the deadline and is terminated. `exec`
    // replaces the shell so the owned child is the long-running process.
    let started = std::time::Instant::now();
    assert_eq!(
        capture_label(&capture_bounded(
            "/bin/sh",
            &["-c", "exec sleep 5"],
            Duration::from_millis(200),
            MAX_DEFINITION_BYTES,
        )),
        "timed_out"
    );
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "deadline must bound the capture"
    );

    // A child that overflows the limit is terminated and reported truncated.
    assert_eq!(
        capture_label(&capture_bounded(
            "/bin/sh",
            &["-c", "exec yes effigy"],
            Duration::from_secs(5),
            4096,
        )),
        "output_limit"
    );
}

fn capture_label(capture: &BoundedCapture) -> &'static str {
    match capture {
        BoundedCapture::Success(_) => "success",
        BoundedCapture::NonZero { .. } => "non_zero",
        BoundedCapture::SpawnFailed => "spawn_failed",
        BoundedCapture::TimedOut => "timed_out",
        BoundedCapture::OutputLimit => "output_limit",
    }
}

/// Two observations of an unchanged file are identical and carry a bound
/// identity; a rewritten file yields a different reading.
#[test]
fn listener_handoff_maintenance_binds_file_identity_and_detects_change() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let setup = current_setup();
    let path = write_definition(directory.path(), &setup);

    let stable = observe_stable_definition(&path);
    let (first, _) = present_reading(stable.clone());
    assert_eq!(stable, observe_definition_file(&path));
    assert!(first.size > 0);
    assert!(first.is_regular());
    assert!(!first.is_group_or_other_writable());

    // A second file with the same bytes still carries a distinct inode, so the
    // identity binding is what distinguishes the two observations.
    let copy = directory.path().join("copy.plist");
    std::fs::write(&copy, setup.render_launchd_plist()).expect("write copy fixture");
    let (copy_identity, _) = present_reading(observe_definition_file(&copy));
    assert_ne!(first.inode, copy_identity.inode);

    let mut rewritten = setup.clone();
    rewritten.user_name = format!("{}-changed", setup.user_name);
    std::fs::write(&path, rewritten.render_launchd_plist()).expect("rewrite fixture");
    let after = observe_definition_file(&path);
    assert_ne!(stable, after, "rewritten bytes change the observation");
    let (second, _) = present_reading(after);
    assert_ne!(first.size, second.size, "rewritten bytes change the size");
}

/// The removal rendering never claims an executed effect and the fixed steps
/// and reasons are stable.
#[test]
fn listener_handoff_maintenance_rendering_is_bounded_and_unexecuted() {
    let labels: Vec<&str> = [
        RemovalRefusal::DefinitionForeignOwner,
        RemovalRefusal::DefinitionUnsafeMode,
        RemovalRefusal::DefinitionChanged,
        RemovalRefusal::DefinitionSymlink,
        RemovalRefusal::DefinitionNotRegular,
        RemovalRefusal::DefinitionOversized,
        RemovalRefusal::DefinitionUnreadable,
        RemovalRefusal::DefinitionPermissionDenied,
        RemovalRefusal::DefinitionMalformed,
        RemovalRefusal::DefinitionShapeMismatch,
        RemovalRefusal::DefinitionOperatorMismatch,
        RemovalRefusal::DefinitionProgramMissing,
        RemovalRefusal::DefinitionProgramStale,
        RemovalRefusal::JobPermissionDenied,
        RemovalRefusal::JobTimedOut,
        RemovalRefusal::JobOutputLimit,
        RemovalRefusal::JobNonZero,
        RemovalRefusal::JobMalformed,
        RemovalRefusal::JobAssociationMismatch,
        RemovalRefusal::JobUnavailable,
        RemovalRefusal::JobUnsupported,
        RemovalRefusal::JobWithoutDefinition,
        RemovalRefusal::OperatorUnavailable,
    ]
    .into_iter()
    .map(RemovalRefusal::label)
    .collect();
    assert!(labels.iter().all(|label| !label.is_empty()));
    assert_eq!(labels.len(), 23);

    let directory = tempfile::tempdir().expect("fixture directory");
    let setup = current_setup();
    let expectation = fixture_expectation(&setup);
    let missing = observe_definition_file(&directory.path().join("absent.plist"));
    let report = inspect_handoff(&missing, JobObservation::ConfirmedAbsent, &expectation);
    let human = render_removal_human(&report);
    assert!(!human.contains("has been removed"));
    assert!(!human.contains("gateway is stopped"));
}
