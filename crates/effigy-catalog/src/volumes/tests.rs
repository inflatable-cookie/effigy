use super::*;

fn test_volumes() -> Vec<ManagedVolume> {
    vec![
        ManagedVolume {
            name: "proj-db-data".to_string(),
            service: "db".to_string(),
            persist: true,
            size_bytes: None,
            mount_point: Some("/var/lib/mysql".to_string()),
            mount_target: Some("/var/lib/mysql".to_string()),
        },
        ManagedVolume {
            name: "proj-cache-data".to_string(),
            service: "cache".to_string(),
            persist: false,
            size_bytes: None,
            mount_point: None,
            mount_target: Some("/workspace-root/app/node_modules".to_string()),
        },
        ManagedVolume {
            name: "proj-search-data".to_string(),
            service: "search".to_string(),
            persist: true,
            size_bytes: None,
            mount_point: None,
            mount_target: Some("/workspace-root/api/target".to_string()),
        },
        ManagedVolume {
            name: "proj-pnpm-store".to_string(),
            service: "app".to_string(),
            persist: false,
            size_bytes: None,
            mount_point: None,
            mount_target: Some("/home/dev/.local/share/pnpm/store".to_string()),
        },
    ]
}

// ── Classification ───────────────────────────────────────────────

#[test]
fn reset_without_keep_data_removes_all() {
    let vols = test_volumes();
    let class = classify_for_reset(&vols, false);
    assert_eq!(class.remove.len(), 4);
    assert!(class.keep.is_empty());
}

#[test]
fn reset_with_keep_data_preserves_persistent() {
    let vols = test_volumes();
    let class = classify_for_reset(&vols, true);
    assert_eq!(class.remove.len(), 2);
    assert!(class.remove.contains(&"proj-cache-data".to_string()));
    assert!(class.remove.contains(&"proj-pnpm-store".to_string()));
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
fn list_all_volumes_command_format() {
    let cmd = list_all_volumes_command();
    assert_eq!(cmd.program, "docker");
    assert_eq!(
        cmd.args,
        vec![
            "volume",
            "ls",
            "--format",
            "{{.Name}}\t{{.Driver}}\t{{.Labels}}"
        ]
    );
}

#[test]
fn inspect_volume_command_format() {
    let cmd = inspect_volume_command("my-project-db-data");
    assert_eq!(cmd.program, "docker");
    assert!(cmd.args.contains(&"inspect".to_string()));
    assert!(cmd.args.contains(&"my-project-db-data".to_string()));
}

#[test]
fn volume_usage_command_format() {
    let cmd = volume_usage_command("/var/lib/docker/volumes/my-project/_data");
    assert_eq!(cmd.program, "__effigy_volume_usage");
    assert_eq!(cmd.args, vec!["/var/lib/docker/volumes/my-project/_data"]);
}

#[test]
fn volume_usage_batch_command_format() {
    let cmd = volume_usage_batch_command(&[
        "/var/lib/docker/volumes/one/_data".to_string(),
        "/var/lib/docker/volumes/two/_data".to_string(),
    ]);
    assert_eq!(cmd.program, "__effigy_volume_usage_batch");
    assert_eq!(
        cmd.args,
        vec![
            "/var/lib/docker/volumes/one/_data",
            "/var/lib/docker/volumes/two/_data"
        ]
    );
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
    assert_eq!(cmds.len(), 4);
}

// ── ManagedVolume construction ───────────────────────────────────

#[test]
fn from_volume_info() {
    let info = VolumeInfo {
        project_name: "proj".to_string(),
        name: "proj-db-data".to_string(),
        named: true,
        persist: true,
        service: "db".to_string(),
        mount: Some("/var/lib/mysql".to_string()),
    };
    let managed = ManagedVolume::from_volume_info(&info);
    assert_eq!(managed.name, "proj-db-data");
    assert!(managed.persist);
    assert_eq!(managed.service, "db");
    assert!(managed.size_bytes.is_none());
    assert_eq!(managed.mount_target.as_deref(), Some("/var/lib/mysql"));
}

#[test]
fn managed_volume_classifies_cache_kinds_from_mount_target() {
    let volumes = test_volumes();
    assert_eq!(volumes[0].cache_kind(), None);
    assert_eq!(volumes[1].cache_kind(), Some(CacheVolumeKind::NodeModules));
    assert_eq!(volumes[2].cache_kind(), Some(CacheVolumeKind::RustTarget));
    assert_eq!(volumes[3].cache_kind(), Some(CacheVolumeKind::PnpmStore));
}

#[test]
fn parse_listed_volume_names_reads_first_column() {
    let names =
        parse_listed_volume_names("proj-db-data\tlocal\tlabel=value\nproj-cache-data\tlocal\t\n");
    assert_eq!(names, vec!["proj-db-data", "proj-cache-data"]);
}

#[test]
fn parse_inspect_volume_metadata_reads_mount_and_size() {
    let metadata = parse_inspect_volume_metadata(
        r#"[{
            "Name": "proj-db-data",
            "Mountpoint": "/var/lib/docker/volumes/proj-db-data/_data",
            "UsageData": {
                "Size": 4096
            }
        }]"#,
    )
    .expect("metadata");

    assert_eq!(metadata.name, "proj-db-data");
    assert_eq!(
        metadata.mount_point.as_deref(),
        Some("/var/lib/docker/volumes/proj-db-data/_data")
    );
    assert_eq!(metadata.size_bytes, Some(4096));
}

#[test]
fn parse_volume_usage_bytes_reads_kibibytes_output() {
    let parsed = parse_volume_usage_bytes("2048\t/var/lib/docker/volumes/demo/_data\n");
    assert_eq!(parsed, Some(2_097_152));
}

#[test]
fn parse_volume_usage_bytes_map_reads_multiple_lines() {
    let parsed = parse_volume_usage_bytes_map(
        "2048\t/var/lib/docker/volumes/one/_data\n1024\t/var/lib/docker/volumes/two/_data\n",
    );
    assert_eq!(
        parsed.get("/var/lib/docker/volumes/one/_data"),
        Some(&2_097_152)
    );
    assert_eq!(
        parsed.get("/var/lib/docker/volumes/two/_data"),
        Some(&1_048_576)
    );
}

#[test]
fn merge_runtime_volume_metadata_updates_matching_entries_only() {
    let merged = merge_runtime_volume_metadata(
        &test_volumes(),
        &[RuntimeVolumeMetadata {
            name: "proj-db-data".to_owned(),
            mount_point: Some("/data/db".to_owned()),
            size_bytes: Some(2048),
        }],
    );

    assert_eq!(merged[0].size_bytes, Some(2048));
    assert_eq!(merged[0].mount_point.as_deref(), Some("/data/db"));
    assert!(merged[1].size_bytes.is_none());
    assert!(merged[2].size_bytes.is_none());
}
