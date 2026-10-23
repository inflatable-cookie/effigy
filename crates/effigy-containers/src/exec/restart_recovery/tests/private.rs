//! Private command-adapter evidence, not a real containerd/systemd proof.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

use super::recover_exited_owned_compose_services_for_project;

const FULL_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

#[test]
fn recurring_health_collision_refuses_recreation_through_production_adapters() {
    let fixture = tempfile::tempdir().expect("fresh private health fixture");
    let bin = fixture.path().join("bin");
    fs::create_dir(&bin).expect("private transport directory");
    fs::write(
        fixture.path().join("effigy.toml"),
        "[containers]\ndefault = \"web\"\n[containers.web]\ncompose_file = \"compose.yml\"\nprimary_service = \"db\"\nprofile = \"private-health\"\n",
    )
    .expect("private manifest");
    fs::write(
        fixture.path().join("compose.yml"),
        "services:\n  db:\n    image: fixture.invalid/db\n    healthcheck:\n      test: [CMD, fixture-health]\n    volumes:\n      - data:/data\n      - ./bind:/bind\nvolumes:\n  data: {}\n",
    )
    .expect("private declaration");
    for name in [
        "data",
        "bind",
        "sibling-data",
        "exec-sentinel",
        "routes",
        "lease",
    ] {
        fs::write(fixture.path().join(name), format!("retained-{name}"))
            .expect("private preservation marker");
    }
    let program = bin.join("colima");
    fs::write(&program, FAKE_COLIMA).expect("private command adapter");
    fs::set_permissions(&program, fs::Permissions::from_mode(0o700))
        .expect("executable private adapter");

    // Environment overrides belong only to the helper process. Nothing changes
    // this test runner's PATH, HOME, backend selection, or runtime state.
    let output = Command::new(std::env::current_exe().expect("test executable"))
        .args([
            "--ignored",
            "--exact",
            "exec::restart_recovery::production_refusal_tests::private_health_recovery_child",
            "--nocapture",
        ])
        .current_dir(fixture.path())
        .env("PATH", &bin)
        .env("PRIVATE_HEALTH_FIXTURE", fixture.path())
        .env("EFFIGY_COMPOSE_BACKEND", "containerd")
        .output()
        .expect("run and reap private proof process");
    assert!(
        output.status.success(),
        "private proof failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt =
        fs::read_to_string(fixture.path().join("refusal.txt")).expect("production refusal receipt");
    assert!(receipt.contains("service-only recreation is unavailable"));
    assert!(receipt.contains("immutable declaration-to-mount association"));
    assert!(receipt.contains("exclusive volume/data ownership proof"));
    assert!(receipt.contains("supported backend preservation plan"));
    assert!(receipt.contains("No service was recreated"));
    assert!(receipt.contains("removes owned mutable volumes"));
    assert!(receipt.contains(FULL_ID));

    let commands = fs::read_to_string(fixture.path().join("commands"))
        .expect("actual backend command receipt");
    assert_eq!(
        commands
            .lines()
            .filter(|line| line.ends_with("start fixture-db"))
            .count(),
        2
    );
    assert_eq!(
        commands
            .lines()
            .filter(|line| line.contains("systemctl stop "))
            .count(),
        1
    );
    assert_eq!(
        commands
            .lines()
            .filter(|line| line.contains("systemctl reset-failed "))
            .count(),
        1
    );
    assert!(commands.contains(&format!("systemctl stop {FULL_ID}.timer")));
    assert!(commands.contains(&format!(
        "systemctl reset-failed {FULL_ID}.service {FULL_ID}.timer"
    )));
    assert!(!commands.split_whitespace().any(|arg| arg == "compose"));
    assert!(!commands.contains(" rm "));
    assert!(!commands.contains("prune"));
    for name in [
        "data",
        "bind",
        "sibling-data",
        "exec-sentinel",
        "routes",
        "lease",
    ] {
        assert_eq!(
            fs::read_to_string(fixture.path().join(name)).expect("retained private marker"),
            format!("retained-{name}")
        );
    }
    println!("private health commands:\n{commands}\nproduction refusal:\n{receipt}\nprivate health refusal: two exact starts, one exact transient unload, recurring collision, no recreation/deletion; data/bind/sibling/routes/lease file markers retained. This does not prove a live exec session or real engine preservation.");
}

#[test]
fn reboot_timer_collision_with_a_running_container_hands_off_to_readiness() {
    let fixture = tempfile::tempdir().expect("fresh reboot fixture");
    let root = fixture.path();
    let bin = root.join("bin");
    fs::create_dir(&bin).expect("private bin");
    fs::write(root.join("effigy.toml"), "[containers.web]\ncompose_file = \"compose.yml\"\nprimary_service = \"db\"\nprofile = \"private-health\"\n")
        .expect("private manifest");
    fs::write(
        root.join("compose.yml"),
        "services:\n  db:\n    image: fixture.invalid/db\n",
    )
    .expect("private compose");
    let program = bin.join("colima");
    fs::write(&program, FAKE_COLIMA).expect("strict fake backend");
    fs::set_permissions(&program, fs::Permissions::from_mode(0o700)).expect("executable fake");
    let started = std::time::Instant::now();
    let output = Command::new(std::env::current_exe().expect("test executable"))
        .args([
            "--ignored",
            "--exact",
            "exec::restart_recovery::production_refusal_tests::private_health_recovery_child",
            "--nocapture",
        ])
        .current_dir(root)
        .env("PATH", &bin)
        .env("PRIVATE_HEALTH_FIXTURE", root)
        .env("PRIVATE_HEALTH_RUNNING", "1")
        .env("PRIVATE_HEALTH_EXPECT", "recovered")
        .env("EFFIGY_COMPOSE_BACKEND", "containerd")
        .output()
        .expect("run private reboot proof");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(10),
        "recovery itself never waits for readiness"
    );
    let receipt = fs::read_to_string(root.join("warnings.txt")).expect("recovery receipt");
    assert!(
        receipt.contains("health-check timer collision"),
        "{receipt}"
    );
    assert!(receipt.contains("the container is running"), "{receipt}");
    assert!(
        receipt.contains("readiness and its health check decide"),
        "{receipt}"
    );
    assert!(!receipt.contains("stale backend state"), "{receipt}");
    assert!(
        !receipt.contains("did not prove health-check readiness"),
        "{receipt}"
    );
    assert!(
        receipt.contains(&format!("Unit {FULL_ID}.timer")),
        "{receipt}"
    );
    assert!(
        receipt.contains("colima nerdctl --profile private-health -- inspect fixture-db"),
        "{receipt}"
    );
    let commands = fs::read_to_string(root.join("commands")).expect("strict call log");
    let commands: Vec<_> = commands.lines().collect();
    assert_eq!(
        commands,
        vec![
            "nerdctl --profile private-health -- ps --all --filter label=com.docker.compose.project=private-health-project --format {{.Names}}\t{{.Status}}\t{{.Ports}}\t{{.Label \"com.docker.compose.project\"}}\t{{.Label \"com.docker.compose.project.working_dir\"}}\t{{.Label \"com.docker.compose.service\"}}\t{{.Label \"com.docker.compose.oneoff\"}}",
            "nerdctl --profile private-health -- inspect fixture-db",
            "nerdctl --profile private-health -- start fixture-db",
            "nerdctl --profile private-health -- ps --all --filter label=com.docker.compose.project=private-health-project --format {{.Names}}\t{{.Status}}\t{{.Ports}}\t{{.Label \"com.docker.compose.project\"}}\t{{.Label \"com.docker.compose.project.working_dir\"}}\t{{.Label \"com.docker.compose.service\"}}\t{{.Label \"com.docker.compose.oneoff\"}}",
        ],
        "one container inspect before the start, no retry, no unit mutation: {commands:?}"
    );
}

#[test]
#[ignore = "invoked only by the private production-adapter fixture"]
fn private_health_recovery_child() {
    let root = PathBuf::from(std::env::var_os("PRIVATE_HEALTH_FIXTURE").expect("private root"));
    crate::with_test_effigy_home(&root.join("home"), || {
        effigy_manifest::with_test_user_config_home(&root.join("user"), || {
            let mut policy = crate::load_container_policy(&root, Some("web"))
                .expect("load private declared policy");
            policy.project_name = "private-health-project".to_owned();
            if std::env::var("PRIVATE_HEALTH_EXPECT").as_deref() == Ok("recovered") {
                let recovery = recover_exited_owned_compose_services_for_project(
                    &root,
                    &policy,
                    &policy.project_name,
                )
                .expect("a running container after a timer collision hands off to readiness");
                fs::write(root.join("warnings.txt"), recovery.warnings.join("\n"))
                    .expect("private recovery receipt");
                return;
            }
            let error = recover_exited_owned_compose_services_for_project(
                &root,
                &policy,
                &policy.project_name,
            )
            .expect_err("recurring timer must not become ready or trigger recreation");
            fs::write(root.join("refusal.txt"), error.to_string())
                .expect("private refusal receipt");
        });
    });
}

// This exclusively private adapter accepts only the expected production
// commands. Unexpected argv fail without forwarding to any installed CLI.
const FAKE_COLIMA: &str = r#"#!/bin/sh
set -eu
printf '%s\n' "$*" >> commands
full=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
case "$*" in
  'nerdctl --profile private-health -- ps --all --filter label=com.docker.compose.project=private-health-project --format '*)
    if [ "${PRIVATE_HEALTH_RUNNING:-}" = 1 ] && [ -f started ]; then
      printf 'fixture-db\tUp 1 second\t\tprivate-health-project\t\tdb\tfalse\n'
    else
      printf 'fixture-db\tExited (1)\t\tprivate-health-project\t\tdb\tfalse\n'
    fi
    ;;
  'nerdctl --profile private-health -- start fixture-db')
    : > started
    printf 'systemd-run: Unit %s.timer was already loaded or has a fragment file\n' "$full" >&2
    exit 1
    ;;
  'nerdctl --profile private-health -- inspect fixture-db')
    printf '[{"Id":"%s","Name":"/fixture-db","State":{"Status":"exited","Running":false},"Config":{"Labels":{"com.docker.compose.project":"private-health-project","com.docker.compose.service":"db","com.docker.compose.oneoff":"false"}},"Mounts":[{"Type":"volume","Name":"private-data","Source":"/private/data","Destination":"/data"},{"Type":"bind","Source":"/private/bind","Destination":"/bind"}]}]\n' "$full"
    ;;
  'ssh --profile private-health -- sudo systemctl show '*)
    unit="$8"
    case "$unit" in "$full.timer"|"$full.service") ;; *) exit 92 ;; esac
    if [ -f unloaded ]; then
      printf 'Id=%s\nLoadState=not-found\nTransient=no\nFragmentPath=\nUnitFileState=\n' "$unit"
    else
      printf 'Id=%s\nLoadState=loaded\nTransient=yes\nFragmentPath=/run/systemd/transient/%s\nUnitFileState=\n' "$unit" "$unit"
    fi
    ;;
  "ssh --profile private-health -- sudo systemctl stop $full.timer") ;;
  "ssh --profile private-health -- sudo systemctl reset-failed $full.service $full.timer")
    printf 'unloaded\n' > unloaded
    ;;
  *) printf 'unexpected private backend command\n' >&2; exit 93 ;;
esac
"#;
