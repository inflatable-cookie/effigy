use super::*;

fn capabilities_with_effigy() -> ContainerCapabilities {
    ContainerCapabilities {
        has_effigy: true,
        effigy_version: Some("0.2.13".to_string()),
        shell: Some("/bin/bash".to_string()),
    }
}

fn capabilities_without_effigy() -> ContainerCapabilities {
    ContainerCapabilities {
        has_effigy: false,
        effigy_version: None,
        shell: Some("/bin/bash".to_string()),
    }
}

fn probe(success: bool, output: &str) -> ProbeResult {
    ProbeResult {
        success,
        output: output.to_string(),
    }
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
            ProbeKind::EffigyVersion,
            ProbeKind::Bash,
            ProbeKind::Sh,
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
            vec!["effigy".to_string(), "version".to_string()],
            vec!["which".to_string(), "bash".to_string()],
            vec!["which".to_string(), "sh".to_string()],
        ]
    );
    assert_eq!(
        ProbeKind::EffigyVersion.interpreter(),
        ProbeInterpreter::VersionOutput
    );
    assert_eq!(
        ProbeKind::EffigyInstallation.interpreter(),
        ProbeInterpreter::ExitCode
    );
}

#[test]
fn capabilities_from_probe_results_parse_version_and_fall_back_to_sh() {
    let mut results = HashMap::new();
    results.insert(ProbeKind::EffigyInstallation, probe(true, ""));
    results.insert(ProbeKind::EffigyVersion, probe(true, "0.2.13"));
    results.insert(ProbeKind::Bash, probe(false, ""));
    results.insert(ProbeKind::Sh, probe(true, "/bin/sh"));
    let caps = build_capabilities_from_results(&results);
    assert!(caps.has_effigy);
    assert_eq!(caps.effigy_version.as_deref(), Some("0.2.13"));
    assert_eq!(caps.shell.as_deref(), Some("/bin/sh"));
}

/// A container that answers the bash probe runs through `/bin/bash`; the
/// bash-miss fallback above must not be the only branch under test.
#[test]
fn capabilities_from_probe_results_prefer_bash_when_present() {
    let mut results = HashMap::new();
    results.insert(ProbeKind::Bash, probe(true, ""));
    results.insert(ProbeKind::Sh, probe(true, ""));
    let caps = build_capabilities_from_results(&results);
    assert!(!caps.has_effigy);
    assert_eq!(caps.effigy_version, None);
    assert_eq!(caps.shell.as_deref(), Some("/bin/bash"));
}

/// Both shell probes ran and failed: the container offered neither, so the
/// capabilities claim none. Defaulting to `/bin/sh` here reported a shell the
/// probe had just disproved.
#[test]
fn neither_shell_probe_answering_claims_no_shell() {
    let mut results = HashMap::new();
    results.insert(ProbeKind::Bash, probe(false, ""));
    results.insert(ProbeKind::Sh, probe(false, ""));
    let caps = build_capabilities_from_results(&results);
    assert_eq!(caps.shell, None);
}

/// A missing shell probe is not an answer either: silence is not `/bin/sh`.
#[test]
fn an_unrun_shell_probe_claims_no_shell() {
    let caps = build_capabilities_from_results(&HashMap::new());
    assert_eq!(caps.shell, None);
    assert!(!caps.has_effigy);
    assert_eq!(caps.effigy_version, None);
}

/// The version is read through the kind's own interpreter: a version probe that
/// says nothing reports nothing, and an exit-code probe's stdout never becomes
/// a version.
#[test]
fn only_a_version_probe_reports_a_version() {
    let mut results = HashMap::new();
    results.insert(ProbeKind::EffigyInstallation, probe(true, "0.2.13"));
    results.insert(ProbeKind::EffigyVersion, probe(true, "  "));
    let caps = build_capabilities_from_results(&results);
    assert!(caps.has_effigy);
    assert_eq!(caps.effigy_version, None);

    let mut results = HashMap::new();
    results.insert(ProbeKind::EffigyVersion, probe(false, "0.2.13"));
    let caps = build_capabilities_from_results(&results);
    assert_eq!(
        caps.effigy_version, None,
        "a failed version probe reports no version"
    );
}
