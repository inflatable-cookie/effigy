use super::*;

fn test_volumes() -> Vec<ManagedVolume> {
    vec![
        ManagedVolume {
            name: "proj-db-data".to_string(),
            service: "db".to_string(),
            persist: true,
            size_bytes: None,
            mount_point: Some("/var/lib/mysql".to_string()),
        },
        ManagedVolume {
            name: "proj-cache-data".to_string(),
            service: "cache".to_string(),
            persist: false,
            size_bytes: None,
            mount_point: None,
        },
        ManagedVolume {
            name: "proj-search-data".to_string(),
            service: "search".to_string(),
            persist: true,
            size_bytes: None,
            mount_point: None,
        },
    ]
}

// ── Classification ───────────────────────────────────────────────

#[test]
fn reset_without_keep_data_removes_all() {
    let vols = test_volumes();
    let class = classify_for_reset(&vols, false);
    assert_eq!(class.remove.len(), 3);
    assert!(class.keep.is_empty());
}

#[test]
fn reset_with_keep_data_preserves_persistent() {
    let vols = test_volumes();
    let class = classify_for_reset(&vols, true);
    assert_eq!(class.remove.len(), 1);
    assert_eq!(class.remove[0], "proj-cache-data");
    assert_eq!(class.keep.len(), 2);
    assert!(class.keep.contains(&"proj-db-data".to_string()));
    assert!(class.keep.contains(&"proj-search-data".to_string()));
}

#[test]
fn empty_volumes_produce_empty_classification() {
    let class = classify_for_reset(&[], true);
    assert!(class.remove.is_empty());
    assert!(class.keep.is_empty());
}

// ── Command specs ────────────────────────────────────────────────

#[test]
fn list_volumes_command_format() {
    let cmd = list_volumes_command("my-project");
    assert_eq!(cmd.program, "docker");
    assert!(cmd.args.contains(&"volume".to_string()));
    assert!(cmd.args.contains(&"ls".to_string()));
    assert!(cmd.args.iter().any(|a| a.contains("my-project-")));
}

#[test]
fn inspect_volume_command_format() {
    let cmd = inspect_volume_command("my-project-db-data");
    assert_eq!(cmd.program, "docker");
    assert!(cmd.args.contains(&"inspect".to_string()));
    assert!(cmd.args.contains(&"my-project-db-data".to_string()));
}

#[test]
fn export_volume_command_format() {
    let cmd = export_volume_command("my-project-db-data", Path::new("/tmp/backup.tar.gz"));
    assert_eq!(cmd.program, "docker");
    assert!(cmd.args.contains(&"run".to_string()));
    assert!(cmd.args.contains(&"--rm".to_string()));
    // Volume mount for source.
    assert!(cmd
        .args
        .iter()
        .any(|a| a.contains("my-project-db-data:/source")));
    // Output mount.
    assert!(cmd.args.iter().any(|a| a.contains("/tmp:/output")));
    // Tar command.
    assert!(cmd.args.contains(&"czf".to_string()));
    assert!(cmd.args.iter().any(|a| a.contains("backup.tar.gz")));
}

#[test]
fn import_volume_command_format() {
    let cmd = import_volume_command("my-project-db-data", Path::new("/tmp/backup.tar.gz"));
    assert_eq!(cmd.program, "docker");
    assert!(cmd.args.contains(&"run".to_string()));
    // Volume mount for target.
    assert!(cmd
        .args
        .iter()
        .any(|a| a.contains("my-project-db-data:/target")));
    // Input mount (read-only).
    assert!(cmd.args.iter().any(|a| a.contains("/tmp:/input:ro")));
    // Tar extract.
    assert!(cmd.args.contains(&"xzf".to_string()));
}

#[test]
fn remove_volume_command_format() {
    let cmd = remove_volume_command("my-project-db-data");
    assert_eq!(cmd.program, "docker");
    assert!(cmd.args.contains(&"volume".to_string()));
    assert!(cmd.args.contains(&"rm".to_string()));
    assert!(cmd.args.contains(&"my-project-db-data".to_string()));
}

#[test]
fn reset_commands_from_classification() {
    let vols = test_volumes();
    let class = classify_for_reset(&vols, true);
    let cmds = reset_commands(&class);
    // Only the non-persistent volume should be removed.
    assert_eq!(cmds.len(), 1);
    assert!(cmds[0].args.contains(&"proj-cache-data".to_string()));
}

#[test]
fn reset_commands_all_volumes() {
    let vols = test_volumes();
    let class = classify_for_reset(&vols, false);
    let cmds = reset_commands(&class);
    assert_eq!(cmds.len(), 3);
}

// ── ManagedVolume construction ───────────────────────────────────

#[test]
fn from_volume_info() {
    let info = VolumeInfo {
        name: "proj-db-data".to_string(),
        persist: true,
        service: "db".to_string(),
    };
    let managed = ManagedVolume::from_volume_info(&info);
    assert_eq!(managed.name, "proj-db-data");
    assert!(managed.persist);
    assert_eq!(managed.service, "db");
    assert!(managed.size_bytes.is_none());
}
