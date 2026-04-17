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

// ── Strategy determination ────────────────────────────────────────

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

// ── Compose exec args ────────────────────────────────────────────

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

// ── Capability cache ─────────────────────────────────────────────

#[test]
fn cache_stores_and_retrieves() {
    let cache = CapabilityCache::new();
    cache.put("web".to_string(), capabilities_with_effigy());

    let cached = cache.get("web");
    assert!(cached.is_some());
    assert!(cached.unwrap().has_effigy);
}

#[test]
fn cache_returns_none_for_missing() {
    let cache = CapabilityCache::new();
    assert!(cache.get("nonexistent").is_none());
}

#[test]
fn cache_expires_stale_entries() {
    let cache = CapabilityCache::with_max_age(Duration::from_millis(1));
    cache.put("web".to_string(), capabilities_with_effigy());

    // Wait for expiry.
    std::thread::sleep(Duration::from_millis(10));

    assert!(cache.get("web").is_none());
}

#[test]
fn cache_invalidate_specific() {
    let cache = CapabilityCache::new();
    cache.put("web".to_string(), capabilities_with_effigy());
    cache.put("api".to_string(), capabilities_without_effigy());

    cache.invalidate("web");

    assert!(cache.get("web").is_none());
    assert!(cache.get("api").is_some());
}

#[test]
fn cache_clear_all() {
    let cache = CapabilityCache::new();
    cache.put("web".to_string(), capabilities_with_effigy());
    cache.put("api".to_string(), capabilities_without_effigy());

    cache.clear();

    assert!(cache.get("web").is_none());
    assert!(cache.get("api").is_none());
}

// ── Probe result building ────────────────────────────────────────

#[test]
fn build_capabilities_with_effigy_present() {
    let mut results = HashMap::new();
    results.insert(
        "effigy installation".to_string(),
        ProbeResult {
            success: true,
            output: "/usr/local/bin/effigy".to_string(),
        },
    );
    results.insert(
        "effigy version".to_string(),
        ProbeResult {
            success: true,
            output: "effigy 0.2.13".to_string(),
        },
    );
    results.insert(
        "bash availability".to_string(),
        ProbeResult {
            success: true,
            output: "/bin/bash".to_string(),
        },
    );
    results.insert(
        "sh availability".to_string(),
        ProbeResult {
            success: true,
            output: "/bin/sh".to_string(),
        },
    );

    let caps = build_capabilities_from_results(&results);
    assert!(caps.has_effigy);
    assert_eq!(caps.effigy_version.as_deref(), Some("effigy 0.2.13"));
    assert_eq!(caps.shell, "/bin/bash");
    assert!(caps.supports_handoff());
}

#[test]
fn build_capabilities_without_effigy() {
    let mut results = HashMap::new();
    results.insert(
        "effigy installation".to_string(),
        ProbeResult {
            success: false,
            output: String::new(),
        },
    );
    results.insert(
        "bash availability".to_string(),
        ProbeResult {
            success: true,
            output: "/bin/bash".to_string(),
        },
    );

    let caps = build_capabilities_from_results(&results);
    assert!(!caps.has_effigy);
    assert!(caps.effigy_version.is_none());
    assert!(!caps.supports_handoff());
}

#[test]
fn build_capabilities_sh_only() {
    let mut results = HashMap::new();
    results.insert(
        "effigy installation".to_string(),
        ProbeResult {
            success: false,
            output: String::new(),
        },
    );
    results.insert(
        "bash availability".to_string(),
        ProbeResult {
            success: false,
            output: String::new(),
        },
    );
    results.insert(
        "sh availability".to_string(),
        ProbeResult {
            success: true,
            output: "/bin/sh".to_string(),
        },
    );

    let caps = build_capabilities_from_results(&results);
    assert_eq!(caps.shell, "/bin/sh");
}

// ── Probe spec ───────────────────────────────────────────────────

#[test]
fn standard_probe_has_expected_checks() {
    let spec = standard_probe_spec();
    let descriptions: Vec<&str> = spec.checks.iter().map(|c| c.description.as_str()).collect();
    assert!(descriptions.contains(&"effigy installation"));
    assert!(descriptions.contains(&"effigy version"));
    assert!(descriptions.contains(&"bash availability"));
}

// ── Staleness ────────────────────────────────────────────────────

#[test]
fn fresh_capabilities_not_stale() {
    let caps = capabilities_with_effigy();
    assert!(!caps.is_stale(Duration::from_secs(60)));
}

#[test]
fn old_capabilities_are_stale() {
    let mut caps = capabilities_with_effigy();
    caps.probed_at = Instant::now() - Duration::from_secs(600);
    assert!(caps.is_stale(Duration::from_secs(300)));
}
