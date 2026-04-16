//! Container capability detection.
//!
//! Probes a running container to determine:
//!
//! - Whether effigy is installed inside the container (for handoff)
//! - Which shell is available (bash, sh, etc.)
//! - Whether specific tools are available (composer, npm, etc.)
//!
//! Detection results are cached per container session to avoid repeated
//! probes during a single development session.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Result of probing a container for capabilities.
#[derive(Debug, Clone)]
pub struct ContainerCapabilities {
    /// Whether effigy is installed inside the container.
    pub has_effigy: bool,

    /// Effigy version inside the container, if installed.
    pub effigy_version: Option<String>,

    /// Available shell (first match from preferred list).
    pub shell: String,

    /// Map of tool names to their availability.
    pub tools: HashMap<String, bool>,

    /// When this probe was performed.
    pub probed_at: Instant,
}

impl ContainerCapabilities {
    /// Whether these capabilities are stale (older than the given duration).
    pub fn is_stale(&self, max_age: Duration) -> bool {
        self.probed_at.elapsed() > max_age
    }

    /// Whether the container supports effigy handoff.
    ///
    /// Handoff means the host effigy invokes the container effigy directly,
    /// letting it handle path resolution, environment, and task execution
    /// natively.
    pub fn supports_handoff(&self) -> bool {
        self.has_effigy
    }
}

/// Strategy for executing a command inside a container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecStrategy {
    /// Hand off to effigy inside the container.
    ///
    /// The host effigy runs:
    /// `docker compose exec <service> effigy <args...>`
    ///
    /// The container effigy handles everything natively — no CWD mapping,
    /// no path translation, full access to the runtime environment.
    Handoff {
        /// The effigy command and arguments to run in the container.
        args: Vec<String>,
    },

    /// Execute a raw command inside the container.
    ///
    /// The host effigy runs:
    /// `docker compose exec -w <cwd> <service> <command...>`
    ///
    /// CWD mapping and environment setup are handled by the host.
    RawExec {
        /// The container-side working directory.
        working_dir: String,
        /// The command and arguments to run.
        command: Vec<String>,
    },
}

impl ExecStrategy {
    /// Build the docker compose exec command arguments.
    ///
    /// Returns the arguments to append after `docker compose exec <service>`.
    pub fn compose_exec_args(&self) -> Vec<String> {
        match self {
            Self::Handoff { args } => {
                let mut result = vec!["effigy".to_string()];
                result.extend(args.iter().cloned());
                result
            }
            Self::RawExec {
                working_dir,
                command,
            } => {
                let mut result = vec![
                    "-w".to_string(),
                    working_dir.clone(),
                ];
                result.extend(command.iter().cloned());
                result
            }
        }
    }
}

/// Determine the execution strategy for a task invocation.
///
/// If the container has effigy installed, hands off the entire command.
/// Otherwise, falls back to raw exec with CWD mapping.
pub fn determine_strategy(
    capabilities: &ContainerCapabilities,
    task_name: &str,
    task_args: &[String],
    container_cwd: &str,
    raw_command: &[String],
) -> ExecStrategy {
    if capabilities.supports_handoff() {
        // Hand off to the container's effigy.
        let mut args = vec![task_name.to_string()];
        args.extend(task_args.iter().cloned());
        ExecStrategy::Handoff { args }
    } else {
        // Raw exec with CWD mapping.
        ExecStrategy::RawExec {
            working_dir: container_cwd.to_string(),
            command: raw_command.to_vec(),
        }
    }
}

/// Cache of container capability probes.
///
/// Avoids repeated `docker compose exec ... which effigy` calls during a
/// single development session. Cache entries expire after a configurable
/// duration (default: 5 minutes).
#[derive(Debug, Clone)]
pub struct CapabilityCache {
    entries: Arc<RwLock<HashMap<String, ContainerCapabilities>>>,
    max_age: Duration,
}

impl CapabilityCache {
    /// Create a new cache with the default max age (5 minutes).
    pub fn new() -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            max_age: Duration::from_secs(300),
        }
    }

    /// Create a new cache with a custom max age.
    pub fn with_max_age(max_age: Duration) -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            max_age,
        }
    }

    /// Get cached capabilities for a container, if fresh.
    pub fn get(&self, container_key: &str) -> Option<ContainerCapabilities> {
        let entries = self.entries.read().expect("capability cache lock poisoned");
        entries.get(container_key).and_then(|cap| {
            if cap.is_stale(self.max_age) {
                None
            } else {
                Some(cap.clone())
            }
        })
    }

    /// Store capabilities for a container.
    pub fn put(&self, container_key: String, capabilities: ContainerCapabilities) {
        let mut entries = self.entries.write().expect("capability cache lock poisoned");
        entries.insert(container_key, capabilities);
    }

    /// Invalidate all cached entries.
    pub fn clear(&self) {
        let mut entries = self.entries.write().expect("capability cache lock poisoned");
        entries.clear();
    }

    /// Invalidate a specific container's cached entry.
    pub fn invalidate(&self, container_key: &str) {
        let mut entries = self.entries.write().expect("capability cache lock poisoned");
        entries.remove(container_key);
    }
}

impl Default for CapabilityCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Probe specification for detecting container capabilities.
///
/// This struct describes what to probe and how. The actual probing
/// (running docker compose exec) is performed by the caller — this
/// crate provides the logic, not the I/O.
#[derive(Debug, Clone)]
pub struct ProbeSpec {
    /// Commands to run to detect capabilities. Each entry is
    /// (description, command_args, what_it_detects).
    pub checks: Vec<ProbeCheck>,
}

/// A single probe check.
#[derive(Debug, Clone)]
pub struct ProbeCheck {
    /// What this check detects.
    pub description: String,

    /// Command to run inside the container (via docker compose exec).
    pub command: Vec<String>,

    /// How to interpret the result.
    pub interpreter: ProbeInterpreter,
}

/// How to interpret a probe check result.
#[derive(Debug, Clone)]
pub enum ProbeInterpreter {
    /// Check exits 0 → capability present.
    ExitCode,

    /// Check outputs a version string → parse it.
    VersionOutput,
}

/// Build the standard probe spec for a container.
///
/// Returns the set of checks needed to determine capabilities. The
/// caller runs these checks and feeds the results back to
/// `build_capabilities_from_results`.
pub fn standard_probe_spec() -> ProbeSpec {
    ProbeSpec {
        checks: vec![
            ProbeCheck {
                description: "effigy installation".to_string(),
                command: vec!["which".to_string(), "effigy".to_string()],
                interpreter: ProbeInterpreter::ExitCode,
            },
            ProbeCheck {
                description: "effigy version".to_string(),
                command: vec!["effigy".to_string(), "version".to_string()],
                interpreter: ProbeInterpreter::VersionOutput,
            },
            ProbeCheck {
                description: "bash availability".to_string(),
                command: vec!["which".to_string(), "bash".to_string()],
                interpreter: ProbeInterpreter::ExitCode,
            },
            ProbeCheck {
                description: "sh availability".to_string(),
                command: vec!["which".to_string(), "sh".to_string()],
                interpreter: ProbeInterpreter::ExitCode,
            },
        ],
    }
}

/// Result of running a single probe check.
#[derive(Debug, Clone)]
pub struct ProbeResult {
    /// Whether the command exited successfully (exit code 0).
    pub success: bool,

    /// Stdout output (trimmed).
    pub output: String,
}

/// Build container capabilities from probe results.
///
/// `results` is a map of check description → probe result, matching the
/// checks from `standard_probe_spec()`.
pub fn build_capabilities_from_results(
    results: &HashMap<String, ProbeResult>,
) -> ContainerCapabilities {
    let has_effigy = results
        .get("effigy installation")
        .map(|r| r.success)
        .unwrap_or(false);

    let effigy_version = results
        .get("effigy version")
        .filter(|r| r.success)
        .map(|r| r.output.clone());

    let has_bash = results
        .get("bash availability")
        .map(|r| r.success)
        .unwrap_or(false);

    let shell = if has_bash {
        "/bin/bash".to_string()
    } else {
        "/bin/sh".to_string()
    };

    let mut tools = HashMap::new();
    tools.insert("effigy".to_string(), has_effigy);
    tools.insert("bash".to_string(), has_bash);

    ContainerCapabilities {
        has_effigy,
        effigy_version,
        shell,
        tools,
        probed_at: Instant::now(),
    }
}

#[cfg(test)]
mod tests {
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
        assert_eq!(
            args,
            vec!["-w", "/var/www/html", "php", "artisan", "test"]
        );
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
}
