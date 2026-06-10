use std::fs;
use std::path::PathBuf;

use super::{inspect_route_table_trust, load_trusted, RouteTableTrust};
use crate::routes::RouteTable;

fn temp_dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("tempdir")
}

#[test]
fn absent_file_is_trusted_as_empty() {
    let dir = temp_dir();
    let path = dir.path().join("routes.json");
    assert_eq!(inspect_route_table_trust(&path), RouteTableTrust::Absent);
    let loaded = load_trusted(&path).expect("load");
    assert!(loaded.expect("absent yields empty table").is_empty());
}

#[test]
fn effigy_written_table_is_trusted() {
    let dir = temp_dir();
    let path = dir.path().join("routes.json");
    RouteTable::new().save(&path).expect("save");

    assert_eq!(inspect_route_table_trust(&path), RouteTableTrust::Trusted);
    assert!(load_trusted(&path).expect("load").is_some());
}

#[test]
fn missing_marker_is_untrusted() {
    let dir = temp_dir();
    let path = dir.path().join("routes.json");
    // A hand-written table with valid routes but no managed marker.
    fs::write(&path, r#"{"routes":{}}"#).expect("write");
    set_owner_only(&path);

    let trust = inspect_route_table_trust(&path);
    assert!(matches!(trust, RouteTableTrust::Untrusted { .. }));
    assert!(trust.reason().unwrap().contains("no Effigy managed marker"));
    // Fail-closed: the daemon keeps its last-known-good table.
    assert!(load_trusted(&path).expect("load").is_none());
}

#[test]
fn foreign_marker_is_untrusted() {
    let dir = temp_dir();
    let path = dir.path().join("routes.json");
    fs::write(&path, r#"{"_managed_by":"someone-else","routes":{}}"#).expect("write");
    set_owner_only(&path);

    let trust = inspect_route_table_trust(&path);
    assert!(trust
        .reason()
        .unwrap()
        .contains("unrecognized managed marker"));
    assert!(load_trusted(&path).expect("load").is_none());
}

#[test]
fn invalid_json_is_untrusted() {
    let dir = temp_dir();
    let path = dir.path().join("routes.json");
    fs::write(&path, "not json").expect("write");
    set_owner_only(&path);

    assert!(matches!(
        inspect_route_table_trust(&path),
        RouteTableTrust::Untrusted { .. }
    ));
}

#[cfg(unix)]
#[test]
fn group_or_other_writable_is_untrusted() {
    use std::os::unix::fs::PermissionsExt;

    let dir = temp_dir();
    let path = dir.path().join("routes.json");
    RouteTable::new().save(&path).expect("save"); // trusted, 0o600
    assert_eq!(inspect_route_table_trust(&path), RouteTableTrust::Trusted);

    // Loosen to group+other writable: now a non-owner could tamper.
    fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).expect("chmod");
    let trust = inspect_route_table_trust(&path);
    assert!(trust.reason().unwrap().contains("group/other-writable"));
    assert!(load_trusted(&path).expect("load").is_none());
}

#[cfg(unix)]
#[test]
fn save_sets_owner_only_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let dir = temp_dir();
    let path = dir.path().join("routes.json");
    RouteTable::new().save(&path).expect("save");
    let mode = fs::metadata(&path).expect("metadata").permissions().mode() & 0o777;
    assert_eq!(
        mode, 0o600,
        "saved route table must be owner-only, got {mode:#o}"
    );
}

#[cfg(unix)]
fn set_owner_only(path: &PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).expect("chmod");
}

#[cfg(not(unix))]
fn set_owner_only(_path: &PathBuf) {}
