use super::*;
use effigy_manifest::with_test_user_config_home;
use std::sync::{Mutex, MutexGuard, OnceLock};

fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

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
        "",
        r#"time="2026-04-20T23:15:01+01:00" level=fatal msg="colima is not running""#,
    ));
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
    assert!(cmd.args.contains(&"--dns".to_string()));
    assert!(cmd.args.contains(&"1.1.1.1".to_string()));
    assert!(cmd.args.contains(&"8.8.8.8".to_string()));
    assert!(!cmd.allow_failure);
}

#[test]
fn colima_start_command_defaults_to_containerd_when_docker_exists() {
    let _lock = env_lock();
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path().join(".effigy-home");
    let bin = temp.path().join("bin");
    std::fs::create_dir_all(&home).expect("mkdir home");
    std::fs::create_dir_all(&bin).expect("mkdir bin");
    std::fs::write(bin.join("docker"), "#!/bin/sh\n").expect("write docker");
    let previous_path = std::env::var_os("PATH");
    let previous_backend = std::env::var_os("EFFIGY_COMPOSE_BACKEND");
    unsafe {
        std::env::set_var("PATH", bin.display().to_string());
        std::env::remove_var("EFFIGY_COMPOSE_BACKEND");
    }

    let cmd = with_test_user_config_home(&home, || colima_start_command(&test_policy("effigy")));

    match previous_path {
        Some(value) => unsafe {
            std::env::set_var("PATH", value);
        },
        None => unsafe {
            std::env::remove_var("PATH");
        },
    }
    match previous_backend {
        Some(value) => unsafe {
            std::env::set_var("EFFIGY_COMPOSE_BACKEND", value);
        },
        None => unsafe {
            std::env::remove_var("EFFIGY_COMPOSE_BACKEND");
        },
    }

    assert!(cmd
        .args
        .windows(2)
        .any(|window| window == ["--runtime", "containerd"]));
}

#[test]
fn colima_start_command_honors_user_global_docker_preference() {
    let _lock = env_lock();
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path().join(".effigy-home");
    std::fs::create_dir_all(&home).expect("mkdir home");
    std::fs::write(
        home.join("config.toml"),
        "[containers]\nbackend = \"docker\"\n",
    )
    .expect("write config");
    let previous_backend = std::env::var_os("EFFIGY_COMPOSE_BACKEND");
    unsafe {
        std::env::remove_var("EFFIGY_COMPOSE_BACKEND");
    }

    let cmd = with_test_user_config_home(&home, || colima_start_command(&test_policy("effigy")));

    match previous_backend {
        Some(value) => unsafe {
            std::env::set_var("EFFIGY_COMPOSE_BACKEND", value);
        },
        None => unsafe {
            std::env::remove_var("EFFIGY_COMPOSE_BACKEND");
        },
    }

    assert!(cmd
        .args
        .windows(2)
        .any(|window| window == ["--runtime", "docker"]));
}

#[test]
fn colima_start_command_forwards_host_ssh_agent() {
    let cmd = colima_start_command(&test_policy("anyprofile"));
    assert!(
        cmd.args.contains(&"--ssh-agent".to_string()),
        "colima start must include --ssh-agent so /run/host-services/ssh-auth.sock is forwarded into the VM for workspace agent-socket bind mounts; args={:?}",
        cmd.args
    );
}

#[test]
fn colima_start_command_adds_arch_and_vm_type_overrides_on_macos() {
    let mut args = vec!["start".to_owned()];
    append_colima_platform_overrides(&mut args, "macos", Some("aarch64"), Some("vz"));

    assert!(args
        .windows(2)
        .any(|window| window == ["--arch", "aarch64"]));
    assert!(args.windows(2).any(|window| window == ["--vm-type", "vz"]));
}

#[test]
fn colima_start_command_skips_arch_and_vm_type_overrides_on_linux() {
    let mut args = vec!["start".to_owned()];
    append_colima_platform_overrides(&mut args, "linux", Some("aarch64"), Some("vz"));

    assert!(!args.iter().any(|value| value == "--arch"));
    assert!(!args.iter().any(|value| value == "--vm-type"));
}

#[test]
fn colima_start_command_applies_managed_resources_for_effigy_profile() {
    let _lock = env_lock();
    unsafe {
        std::env::set_var(
            "EFFIGY_INTERNAL_HOST_MEMORY_BYTES",
            (128u64 * 1024 * 1024 * 1024).to_string(),
        );
    }
    let cmd = colima_start_command(&test_policy("effigy"));
    let expected = managed_colima_profile_resources("effigy").expect("managed resources");
    unsafe {
        std::env::remove_var("EFFIGY_INTERNAL_HOST_MEMORY_BYTES");
    }

    assert!(cmd.args.contains(&"--memory".to_string()));
    assert!(cmd.args.contains(&expected.memory_gib.to_string()));
    assert!(!cmd.args.contains(&"--swap".to_string()));
}

#[test]
fn managed_colima_profile_resources_scale_with_host_memory() {
    let _lock = env_lock();
    unsafe {
        std::env::set_var(
            "EFFIGY_INTERNAL_HOST_MEMORY_BYTES",
            (16u64 * 1024 * 1024 * 1024).to_string(),
        );
    }
    let small = managed_colima_profile_resources("effigy").expect("managed resources");
    unsafe {
        std::env::set_var(
            "EFFIGY_INTERNAL_HOST_MEMORY_BYTES",
            (128u64 * 1024 * 1024 * 1024).to_string(),
        );
    }
    let large = managed_colima_profile_resources("effigy").expect("managed resources");
    unsafe {
        std::env::remove_var("EFFIGY_INTERNAL_HOST_MEMORY_BYTES");
    }

    assert_eq!(small.memory_gib, 4);
    assert_eq!(small.swap_gib, 4);
    assert_eq!(large.memory_gib, 32);
    assert_eq!(large.swap_gib, 16);
}

#[test]
fn prepare_managed_colima_profile_writes_memory_and_swap_provision() {
    let _lock = env_lock();
    let root = std::env::temp_dir().join(format!(
        "effigy-colima-config-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("mkdir home");
    let previous_home = std::env::var_os("HOME");
    unsafe {
        std::env::set_var("HOME", &root);
        std::env::set_var(
            "EFFIGY_INTERNAL_HOST_MEMORY_BYTES",
            (128u64 * 1024 * 1024 * 1024).to_string(),
        );
    }

    prepare_managed_colima_profile(&test_policy("effigy")).expect("prepare profile");

    unsafe {
        std::env::remove_var("EFFIGY_INTERNAL_HOST_MEMORY_BYTES");
        if let Some(home) = previous_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }
    }

    let rendered = std::fs::read_to_string(root.join(".colima/effigy/colima.yaml"))
        .expect("read colima config");
    let parsed = serde_yaml::from_str::<serde_yaml::Value>(&rendered).expect("parse yaml");
    let root = parsed.as_mapping().expect("yaml mapping");
    assert_eq!(
        root.get(serde_yaml::Value::String("memory".to_owned()))
            .and_then(serde_yaml::Value::as_u64),
        Some(32)
    );
    let provision = root
        .get(serde_yaml::Value::String("provision".to_owned()))
        .and_then(serde_yaml::Value::as_sequence)
        .expect("provision sequence");
    let entry = provision
        .iter()
        .find_map(serde_yaml::Value::as_mapping)
        .expect("provision entry");
    assert_eq!(
        entry
            .get(serde_yaml::Value::String("mode".to_owned()))
            .and_then(serde_yaml::Value::as_str),
        Some("after-boot")
    );
    let script = entry
        .get(serde_yaml::Value::String("script".to_owned()))
        .and_then(serde_yaml::Value::as_str)
        .expect("script");
    assert!(script.contains("# effigy-managed-swap"));
    assert!(script.contains("swap_gib=16"));
}

#[test]
fn prepare_managed_colima_profile_writes_macos_arch_and_vm_type_overrides() {
    let mut root = serde_yaml::Mapping::new();
    upsert_colima_platform_overrides(&mut root, "macos", Some("aarch64"), Some("vz"));

    assert_eq!(
        root.get(serde_yaml::Value::String("arch".to_owned()))
            .and_then(serde_yaml::Value::as_str),
        Some("aarch64")
    );
    assert_eq!(
        root.get(serde_yaml::Value::String("vmType".to_owned()))
            .and_then(serde_yaml::Value::as_str),
        Some("vz")
    );
}

#[test]
fn colima_stop_command_uses_profile() {
    let policy = test_policy("myprofile");
    let cmd = colima_stop_command(&policy);
    assert_eq!(cmd.program, "colima");
    assert!(cmd.args.contains(&"stop".to_string()));
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
        managed_volumes: vec![],
        shared_services: vec![],
        project_name: "test-project".to_string(),
        primary_service: "app".to_string(),
        dns_domain: None,
        dns_tls: false,
        dns_port: None,
        dns_routes: vec![],
        service_aliases: vec![],
        declared_ports: vec![],
        ports_declared_explicitly: false,
        declared_mounts: vec![],
        declared_media_mounts: vec![],
        pull_production_hook: None,
        health_check: None,
        health_timeout_secs: 60,
        workspace_user: None,
        workspace_home: None,
        on_task_exit: ManifestContainerOnTaskExit::Stop,
        shutdown: ManifestContainerShutdownMode::Graceful,
        detach_timeout_secs: 10,
        host_processes: vec![],
    }
}
