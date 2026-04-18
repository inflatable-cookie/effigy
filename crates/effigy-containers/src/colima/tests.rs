use super::*;

#[test]
fn parse_colima_running_detects_running() {
    assert!(parse_colima_running("INFO[0000] colima is running\n", ""));
    assert!(parse_colima_running("", "colima is running"));
    assert!(parse_colima_running("RUNNING", ""));
}

#[test]
fn parse_colima_running_detects_not_running() {
    assert!(!parse_colima_running("", ""));
    assert!(!parse_colima_running("colima is stopped", ""));
    assert!(!parse_colima_running(
        "error: not found",
        "profile does not exist"
    ));
}

#[test]
fn colima_status_command_uses_profile() {
    let policy = test_policy("myprofile");
    let cmd = colima_status_command(&policy);
    assert_eq!(cmd.program, "colima");
    assert!(cmd.args.contains(&"myprofile".to_string()));
    assert!(cmd.allow_failure);
}

#[test]
fn colima_start_command_uses_profile() {
    let policy = test_policy("myprofile");
    let cmd = colima_start_command(&policy);
    assert_eq!(cmd.program, "colima");
    assert!(cmd.args.contains(&"myprofile".to_string()));
    assert!(!cmd.allow_failure);
}

#[test]
fn shutdown_graceful_produces_one_command() {
    let mut policy = test_policy("default");
    policy.shutdown = ManifestContainerShutdownMode::Graceful;
    let cmds = shutdown_compose_commands(&policy);
    assert_eq!(cmds.len(), 1);
    assert_eq!(cmds[0].1, "docker compose down");
}

#[test]
fn shutdown_immediate_produces_two_commands() {
    let mut policy = test_policy("default");
    policy.shutdown = ManifestContainerShutdownMode::Immediate;
    let cmds = shutdown_compose_commands(&policy);
    assert_eq!(cmds.len(), 2);
    assert_eq!(cmds[0].1, "docker compose kill");
    assert_eq!(cmds[1].1, "docker compose down");
}

fn test_policy(profile: &str) -> EffectiveContainerPolicy {
    use effigy_manifest::{
        ManifestContainerDriver, ManifestContainerOnTaskExit, ManifestContainerStartup,
    };
    EffectiveContainerPolicy {
        name: "test".to_string(),
        driver: ManifestContainerDriver::Colima,
        startup: ManifestContainerStartup::Attached,
        profile: profile.to_string(),
        compose_source: crate::EffectiveComposeSource::Direct,
        compose_files: vec![std::path::PathBuf::from("docker-compose.yml")],
        compose_file_display: "docker-compose.yml".to_string(),
        shared_services: vec![],
        project_name: "test-project".to_string(),
        primary_service: "app".to_string(),
        dns_domain: None,
        dns_tls: false,
        dns_port: None,
        declared_ports: vec![],
        ports_declared_explicitly: false,
        declared_mounts: vec![],
        health_check: None,
        health_timeout_secs: 60,
        ui_tabs: vec![],
        on_task_exit: ManifestContainerOnTaskExit::Stop,
        shutdown: ManifestContainerShutdownMode::Graceful,
        detach_timeout_secs: 10,
    }
}
