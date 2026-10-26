use super::*;
use effigy_manifest::with_test_user_config_home;
use std::sync::MutexGuard;

fn env_lock() -> MutexGuard<'static, ()> {
    crate::test_env_lock()
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

/// The Colima argv each operation runs, and the command sequence a shutdown
/// mode produces.
#[test]
fn colima_command_cases() {
    #[derive(Clone, Copy)]
    enum Run {
        Status,
        Start,
        Stop,
        Shutdown(ManifestContainerShutdownMode),
    }
    enum Expect {
        /// One Colima command with these required and forbidden arguments.
        Command {
            args: &'static [&'static str],
            absent: &'static [&'static str],
            allow_failure: bool,
        },
        /// The labels of the commands this shutdown mode produces.
        Shutdown(&'static [&'static str]),
    }
    let cases: [(&str, Run, Expect); 5] = [
        (
            "status",
            Run::Status,
            Expect::Command {
                args: &["myprofile"],
                absent: &[],
                allow_failure: true,
            },
        ),
        (
            "start",
            Run::Start,
            Expect::Command {
                args: &["myprofile", "--dns", "1.1.1.1", "8.8.8.8"],
                absent: &[],
                allow_failure: false,
            },
        ),
        (
            "stop",
            Run::Stop,
            Expect::Command {
                args: &["stop", "myprofile"],
                absent: &[],
                allow_failure: false,
            },
        ),
        (
            "a graceful shutdown",
            Run::Shutdown(ManifestContainerShutdownMode::Graceful),
            Expect::Shutdown(&["docker compose down"]),
        ),
        (
            "an immediate shutdown",
            Run::Shutdown(ManifestContainerShutdownMode::Immediate),
            Expect::Shutdown(&["docker compose kill", "docker compose down"]),
        ),
    ];
    for (name, run, expected) in cases {
        match (run, expected) {
            (Run::Shutdown(mode), Expect::Shutdown(labels)) => {
                let mut policy = test_policy("default");
                policy.shutdown = mode;
                let commands = shutdown_compose_commands(&policy);
                assert_eq!(
                    commands.iter().map(|(_, label)| *label).collect::<Vec<_>>(),
                    labels,
                    "{name}"
                );
            }
            (Run::Status, Expect::Command { args, absent, allow_failure })
            | (Run::Start, Expect::Command { args, absent, allow_failure })
            | (Run::Stop, Expect::Command { args, absent, allow_failure }) => {
                let _lock = env_lock();
                let policy = test_policy("myprofile");
                let command = match run {
                    Run::Status => colima_status_command(&policy),
                    Run::Start => colima_start_command(&policy),
                    Run::Stop => colima_stop_command(&policy),
                    Run::Shutdown(_) => unreachable!("shutdown is handled above"),
                };
                assert_eq!(command.program, "colima", "{name}");
                for arg in args {
                    assert!(
                        command.args.contains(&(*arg).to_string()),
                        "{name}: missing {arg} in {:?}",
                        command.args
                    );
                }
                for arg in absent {
                    assert!(
                        !command.args.contains(&(*arg).to_string()),
                        "{name}: unexpected {arg} in {:?}",
                        command.args
                    );
                }
                assert_eq!(command.allow_failure, allow_failure, "{name}");
            }
            _ => panic!("{name}: mismatched row"),
        }
    }
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
    let _lock = env_lock();
    let cmd = colima_start_command(&test_policy("anyprofile"));
    assert!(
        cmd.args.contains(&"--ssh-agent".to_string()),
        "colima start must include --ssh-agent so /run/host-services/ssh-auth.sock is forwarded into the VM for workspace agent-socket bind mounts; args={:?}",
        cmd.args
    );
}

/// The macOS-only Colima flags: a VM needs an explicit architecture and VM
/// type there, and Linux has neither flag.
#[test]
fn colima_platform_override_cases() {
    let cases: [(&str, &[&[&str]]); 2] = [
        ("macos", &[&["--arch", "aarch64"], &["--vm-type", "vz"]]),
        ("linux", &[]),
    ];
    for (platform, expected_flags) in cases {
        let mut args = vec!["start".to_owned()];
        append_colima_platform_overrides(&mut args, platform, Some("aarch64"), Some("vz"));
        for flag in expected_flags {
            assert!(
                args.windows(flag.len())
                    .any(|window| window == *flag),
                "{platform}: missing {flag:?} in {args:?}"
            );
        }
        for absent in ["--arch", "--vm-type"] {
            let should_be_absent = !expected_flags
                .iter()
                .any(|flag| flag.first() == Some(&absent));
            assert_eq!(
                !args.iter().any(|value| value == absent),
                should_be_absent,
                "{platform}: {absent} in {args:?}"
            );
        }
    }
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
    assert!(cmd.args.contains(&"--disk".to_string()));
    assert!(cmd.args.contains(&expected.disk_gib.to_string()));
    assert!(!cmd.args.contains(&"--swap".to_string()));
}

/// Managed profile resources: memory and swap scale from host memory, the
/// disk target is 300 GiB unless config or an explicit override sets one.
#[test]
fn managed_colima_profile_resource_cases() {
    struct Case {
        name: &'static str,
        host_gib: u64,
        user_config: Option<&'static str>,
        explicit_disk: Option<u64>,
        /// The memory and swap the host size derives, for the rows that pin it.
        memory_gib: Option<u64>,
        swap_gib: Option<u64>,
        disk_gib: u64,
    }
    let cases = [
        Case {
            name: "a 16 GiB host",
            host_gib: 16,
            user_config: None,
            explicit_disk: None,
            memory_gib: Some(4),
            swap_gib: Some(4),
            disk_gib: 300,
        },
        Case {
            name: "a 128 GiB host",
            host_gib: 128,
            user_config: None,
            explicit_disk: None,
            memory_gib: Some(32),
            swap_gib: Some(16),
            disk_gib: 300,
        },
        Case {
            name: "a configured disk target",
            host_gib: 64,
            user_config: Some("[containers]\nprofile_disk_gib = 180\n"),
            explicit_disk: None,
            memory_gib: None,
            swap_gib: None,
            disk_gib: 180,
        },
        Case {
            name: "an explicit disk override",
            host_gib: 64,
            user_config: None,
            explicit_disk: Some(220),
            memory_gib: None,
            swap_gib: None,
            disk_gib: 220,
        },
    ];
    for case in cases {
        let _lock = env_lock();
        unsafe {
            std::env::set_var(
                "EFFIGY_INTERNAL_HOST_MEMORY_BYTES",
                (case.host_gib * 1024 * 1024 * 1024).to_string(),
            );
        }
        let resolve = || match case.explicit_disk {
            Some(disk) => {
                managed_colima_profile_resources_with_disk("effigy", Some(disk)).expect("resources")
            }
            None => managed_colima_profile_resources("effigy").expect("managed resources"),
        };
        let resources = match case.user_config {
            Some(config) => {
                let temp = tempfile::tempdir().expect("tempdir");
                let home = temp.path().join(".effigy-home");
                std::fs::create_dir_all(&home).expect("mkdir home");
                std::fs::write(home.join("config.toml"), config).expect("write config");
                with_test_user_config_home(&home, resolve)
            }
            None => resolve(),
        };
        unsafe {
            std::env::remove_var("EFFIGY_INTERNAL_HOST_MEMORY_BYTES");
        }
        if let Some(memory_gib) = case.memory_gib {
            assert_eq!(resources.memory_gib, memory_gib, "{}", case.name);
        }
        if let Some(swap_gib) = case.swap_gib {
            assert_eq!(resources.swap_gib, swap_gib, "{}", case.name);
        }
        assert_eq!(resources.disk_gib, case.disk_gib, "{}", case.name);
    }
}

#[test]
fn prepare_managed_colima_profile_writes_memory_and_swap_provision() {
    let _lock = env_lock();
    let root = effigy_test_support::Fixture::process_lifetime(&format!(
        "effigy-colima-config-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ))
    .expect("fixture directory");
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
    assert_eq!(
        root.get(serde_yaml::Value::String("disk".to_owned()))
            .and_then(serde_yaml::Value::as_u64),
        Some(300)
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
fn managed_drift_reads_a_disabled_forward_agent() {
    let _lock = env_lock();
    assert_eq!(
        managed_colima_profile_drift_in_config(Some("forwardAgent: false\n")),
        Some(ManagedColimaProfileDrift::SshAgentForwardingDisabled)
    );
    assert_eq!(
        managed_colima_profile_drift_in_config(Some("sshAgent: false\n")),
        Some(ManagedColimaProfileDrift::SshAgentForwardingDisabled)
    );
}

#[test]
fn managed_drift_leaves_a_configured_or_unknown_profile_alone() {
    let _lock = env_lock();
    assert_eq!(
        managed_colima_profile_drift_in_config(Some("forwardAgent: true\nmemory: 32\n")),
        None
    );
    // A key Colima has not written yet proves nothing, and a restart no
    // observation asked for would stop every stack on the profile.
    assert_eq!(
        managed_colima_profile_drift_in_config(Some("memory: 32\n")),
        None
    );
    assert_eq!(managed_colima_profile_drift_in_config(None), None);
    assert_eq!(
        managed_colima_profile_drift_in_config(Some(": not yaml :")),
        None
    );
}

#[test]
fn managed_drift_reads_the_profiles_own_config_file() {
    let _lock = env_lock();
    let root = effigy_test_support::Fixture::process_lifetime(&format!(
        "effigy-colima-drift-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ))
    .expect("fixture directory");
    let profile_dir = root.join(".colima/effigy-release");
    std::fs::create_dir_all(&profile_dir).expect("mkdir profile");
    std::fs::write(profile_dir.join("colima.yaml"), "forwardAgent: false\n").expect("write config");
    let previous_home = std::env::var_os("HOME");
    unsafe {
        std::env::set_var("HOME", &root);
    }

    let drift = managed_colima_profile_drift("effigy-release");

    unsafe {
        if let Some(home) = previous_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }
    }
    assert_eq!(
        drift,
        Some(ManagedColimaProfileDrift::SshAgentForwardingDisabled)
    );
}

/// Each notice names the profile it is about, a recreated profile says what it
/// lost, and the drift warning names the command that repairs it.
#[test]
fn managed_profile_notices_name_the_profile_and_what_a_deletion_lost() {
    for notice in [
        colima_profile_creation_notice("effigy-release"),
        colima_profile_recreated_notice("effigy-release"),
    ] {
        assert!(notice.contains("`effigy-release`"), "{notice}");
    }
    let recreated = colima_profile_recreated_notice("effigy-release");
    for lost in ["containers", "images", "volumes"] {
        assert!(recreated.contains(lost), "{recreated}");
    }
    assert!(colima_profile_start_notice("effigy").contains("`effigy`"));

    let drift = ManagedColimaProfileDrift::SshAgentForwardingDisabled.warning("effigy");
    assert!(drift.contains("`effigy`"), "{drift}");
    assert!(drift.contains("`colima restart effigy`"), "{drift}");
    assert!(drift.contains("did not restart"), "{drift}");
}

#[test]
fn managed_profile_prepare_writes_both_agent_forwarding_keys() {
    let _lock = env_lock();
    let root = effigy_test_support::Fixture::process_lifetime(&format!(
        "effigy-colima-forward-agent-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ))
    .expect("fixture directory");
    std::fs::create_dir_all(&root).expect("mkdir home");
    let previous_home = std::env::var_os("HOME");
    unsafe {
        std::env::set_var("HOME", &root);
    }

    prepare_managed_colima_profile(&test_policy("effigy")).expect("prepare profile");

    unsafe {
        if let Some(home) = previous_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }
    }
    let parsed = serde_yaml::from_str::<serde_yaml::Value>(
        &std::fs::read_to_string(root.join(".colima/effigy/colima.yaml")).expect("read config"),
    )
    .expect("parse yaml");
    let root = parsed.as_mapping().expect("yaml mapping");
    for key in ["sshAgent", "forwardAgent"] {
        assert_eq!(
            root.get(serde_yaml::Value::String(key.to_owned()))
                .and_then(serde_yaml::Value::as_bool),
            Some(true),
            "{key} must be true so the managed config matches the managed start"
        );
    }
}

fn test_policy(profile: &str) -> EffectiveContainerPolicy {
    use effigy_manifest::{
        ManifestContainerDriver, ManifestContainerOnTaskExit, ManifestContainerStartup,
    };
    EffectiveContainerPolicy {
        render: None,
        repo_root: std::path::PathBuf::from("/tmp"),
        name: "test".to_string(),
        driver: ManifestContainerDriver::Colima,
        startup: ManifestContainerStartup::Attached,
        profile: profile.to_string(),
        compose_source: crate::EffectiveComposeSource::Direct,
        compose_files: vec![std::path::PathBuf::from("docker-compose.yml")],
        compose_file_display: "docker-compose.yml".to_string(),
        managed_volumes: vec![],
        borrowed_volumes: vec![],
        data_peers: Vec::new(),
        data_from: None,
        shared_services: vec![],
        project_name: "test-project".to_string(),
        primary_service: "app".to_string(),
        dns_domain: None,
        dns_base_domain: None,
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
        secret_delivery: effigy_manifest::ManifestContainerSecretDelivery::ComposeEnv,
        secret_runtime_dir: None,
        source_secret_runtime_for_deferrals: false,
        workspace_user: None,
        workspace_home: None,
        on_task_exit: ManifestContainerOnTaskExit::Stop,
        shutdown: ManifestContainerShutdownMode::Graceful,
        detach_timeout_secs: 10,
        host_processes: vec![],
    }
}
