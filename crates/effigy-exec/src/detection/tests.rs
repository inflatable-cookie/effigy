use super::*;

fn capabilities_with_effigy() -> ContainerCapabilities {
    ContainerCapabilities {
        has_effigy: true,
        effigy_version: Some("0.2.13".to_string()),
        shell: "/bin/bash".to_string(),
        tools: HashMap::new(),
        probed_at: Instant::now(),
    }
}

fn capabilities_without_effigy() -> ContainerCapabilities {
    ContainerCapabilities {
        has_effigy: false,
        effigy_version: None,
        shell: "/bin/bash".to_string(),
        tools: HashMap::new(),
        probed_at: Instant::now(),
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

#[test]
fn handoff_compose_args() {
    let strategy = ExecStrategy::Handoff {
        args: vec!["test".to_string(), "--verbose".to_string()],
    };
    let args = strategy.compose_exec_args();
    assert_eq!(args, vec!["effigy", "test", "--verbose"]);
}

#[test]
fn raw_exec_compose_args() {
    let strategy = ExecStrategy::RawExec {
        working_dir: "/var/www/html".to_string(),
        command: vec!["php".to_string(), "artisan".to_string(), "test".to_string()],
    };
    let args = strategy.compose_exec_args();
    assert_eq!(args, vec!["-w", "/var/www/html", "php", "artisan", "test"]);
}

#[test]
fn standard_probe_has_expected_checks() {
    let spec = standard_probe_spec();
    let descriptions: Vec<&str> = spec.checks.iter().map(|c| c.description.as_str()).collect();
    assert_eq!(
        descriptions,
        [
            "effigy installation",
            "effigy version",
            "bash availability",
            "sh availability",
        ]
    );
}

#[test]
fn capabilities_from_probe_results_prefer_bash_and_parse_version() {
    let mut results = HashMap::new();
    results.insert(
        "effigy installation".to_string(),
        ProbeResult {
            success: true,
            output: String::new(),
        },
    );
    results.insert(
        "effigy version".to_string(),
        ProbeResult {
            success: true,
            output: "0.2.13".to_string(),
        },
    );
    results.insert(
        "bash availability".to_string(),
        ProbeResult {
            success: false,
            output: String::new(),
        },
    );
    let caps = build_capabilities_from_results(&results);
    assert!(caps.has_effigy);
    assert_eq!(caps.effigy_version.as_deref(), Some("0.2.13"));
    assert_eq!(caps.shell, "/bin/sh");
    assert_eq!(caps.tools.get("bash"), Some(&false));
}
