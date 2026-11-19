use super::*;

fn capabilities_with_effigy() -> ContainerCapabilities {
    ContainerCapabilities {
        has_effigy: true,
        shell: Some("/bin/bash".to_string()),
    }
}

fn capabilities_without_effigy() -> ContainerCapabilities {
    ContainerCapabilities {
        has_effigy: false,
        shell: Some("/bin/bash".to_string()),
    }
}

fn probe(success: bool) -> ProbeResult {
    ProbeResult { success }
}

#[test]
fn handoff_when_effigy_installed() {
    let caps = capabilities_with_effigy();
    let strategy = determine_strategy(
        &caps,
        "test",
        &["--verbose".to_string()],
        "/var/www/html/app",
        &["php".to_string(), "artisan".to_string(), "test".to_string()],
    );
    assert_eq!(
        strategy,
        ExecStrategy::Handoff {
            args: vec!["test".to_string(), "--verbose".to_string()]
        }
    );
}

#[test]
fn raw_exec_when_no_effigy() {
    let caps = capabilities_without_effigy();
    let strategy = determine_strategy(
        &caps,
        "test",
        &[],
        "/var/www/html",
        &["php".to_string(), "artisan".to_string(), "test".to_string()],
    );
    assert_eq!(
        strategy,
        ExecStrategy::RawExec {
            working_dir: "/var/www/html".to_string(),
            command: vec!["php".to_string(), "artisan".to_string(), "test".to_string()],
        }
    );
}

/// The spec and the results share one identity per check, so a caller cannot
/// record a result under a name the builder never reads.
#[test]
fn standard_probe_has_typed_checks() {
    let spec = standard_probe_spec();
    let kinds: Vec<ProbeKind> = spec.checks.iter().map(|check| check.kind).collect();
    assert_eq!(
        kinds,
        [
            ProbeKind::EffigyInstallation,
            ProbeKind::Bash,
            ProbeKind::Sh
        ]
    );
    let commands: Vec<Vec<String>> = spec
        .checks
        .iter()
        .map(|check| check.command.clone())
        .collect();
    assert_eq!(
        commands,
        vec![
            vec!["which".to_string(), "effigy".to_string()],
            vec!["which".to_string(), "bash".to_string()],
            vec!["which".to_string(), "sh".to_string()],
        ]
    );
}

#[test]
fn capabilities_from_probe_results_fall_back_to_sh() {
    let mut results = HashMap::new();
    results.insert(ProbeKind::EffigyInstallation, probe(true));
    results.insert(ProbeKind::Bash, probe(false));
    results.insert(ProbeKind::Sh, probe(true));
    let caps = build_capabilities_from_results(&results);
    assert!(caps.has_effigy);
    assert_eq!(caps.shell.as_deref(), Some("/bin/sh"));
}

/// A container that answers the bash probe runs through `/bin/bash`; the
/// bash-miss fallback above must not be the only branch under test.
#[test]
fn capabilities_from_probe_results_prefer_bash_when_present() {
    let mut results = HashMap::new();
    results.insert(ProbeKind::Bash, probe(true));
    results.insert(ProbeKind::Sh, probe(true));
    let caps = build_capabilities_from_results(&results);
    assert!(!caps.has_effigy);
    assert_eq!(caps.shell.as_deref(), Some("/bin/bash"));
}

/// Both shell probes ran and failed: the container offered neither, so the
/// capabilities claim none. Defaulting to `/bin/sh` here reported a shell the
/// probe had just disproved.
#[test]
fn neither_shell_probe_answering_claims_no_shell() {
    let mut results = HashMap::new();
    results.insert(ProbeKind::Bash, probe(false));
    results.insert(ProbeKind::Sh, probe(false));
    let caps = build_capabilities_from_results(&results);
    assert_eq!(caps.shell, None);
}

/// A missing shell probe is not an answer either: silence is not `/bin/sh`.
#[test]
fn an_unrun_shell_probe_claims_no_shell() {
    let caps = build_capabilities_from_results(&HashMap::new());
    assert_eq!(caps.shell, None);
    assert!(!caps.has_effigy);
}
