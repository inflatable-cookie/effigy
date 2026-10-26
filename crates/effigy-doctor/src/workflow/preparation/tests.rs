use std::path::{Path, PathBuf};

use super::*;

fn empty_manifest_snapshot() -> ManifestSnapshot {
    ManifestSnapshot {
        manifest_paths: Vec::new(),
        parsed_catalogs: Vec::new(),
        preferred_js_pm: None,
        parse_ok_any: false,
    }
}

#[test]
fn prepare_manifest_snapshot_collects_once_when_fix_is_disabled() {
    let mut state = DoctorState::new();
    let resolved_root = Path::new("/tmp/doctor-workspace");
    let mut collect_count = 0usize;
    let mut apply_count = 0usize;

    let manifest = prepare_manifest_snapshot_with(
        resolved_root,
        false,
        &mut state,
        |_, _| {
            collect_count += 1;
            Ok(empty_manifest_snapshot())
        },
        |_, _, _| {
            apply_count += 1;
        },
    )
    .expect("fix disabled branch");

    assert_eq!(collect_count, 1);
    assert_eq!(apply_count, 0);
    assert!(!manifest.parse_ok_any);
}

#[test]
fn prepare_manifest_snapshot_recollects_once_when_fix_is_enabled() {
    let mut state = DoctorState::new();
    let resolved_root = Path::new("/tmp/doctor-workspace");
    let mut collect_count = 0usize;
    let mut apply_count = 0usize;

    let manifest = prepare_manifest_snapshot_with(
        resolved_root,
        true,
        &mut state,
        |_, _| {
            collect_count += 1;
            if collect_count == 1 {
                return Ok(empty_manifest_snapshot());
            }
            Ok(ManifestSnapshot {
                manifest_paths: vec![PathBuf::from("/tmp/doctor-workspace/effigy.toml")],
                parsed_catalogs: Vec::new(),
                preferred_js_pm: None,
                parse_ok_any: true,
            })
        },
        |_, _, _| {
            apply_count += 1;
        },
    )
    .expect("fix enabled branch");

    assert_eq!(collect_count, 2);
    assert_eq!(apply_count, 1);
    assert!(manifest.parse_ok_any);
}

#[test]
fn prepare_manifest_snapshot_propagates_recollect_error_when_fix_is_enabled() {
    let mut state = DoctorState::new();
    let resolved_root = Path::new("/tmp/doctor-workspace");
    let mut collect_count = 0usize;

    let result = prepare_manifest_snapshot_with(
        resolved_root,
        true,
        &mut state,
        |_, _| {
            collect_count += 1;
            if collect_count == 1 {
                return Ok(empty_manifest_snapshot());
            }
            Err(DoctorError::cwd_failure(std::io::Error::other(
                "simulated recollect failure",
            )))
        },
        |_, _, _| {},
    );

    let err = match result {
        Err(err) => err,
        Ok(_) => panic!("recollect error should bubble up"),
    };
    assert!(matches!(err, DoctorError::TaskInvocation(_)));
    assert_eq!(collect_count, 2);
}
