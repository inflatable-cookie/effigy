//! Health check polling for container readiness.
//!
//! When `effigy dev` starts a container, it needs to wait for services to
//! be healthy before showing the "ready" indicator. This module provides:
//!
//! - A health check specification (HTTP or TCP)
//! - A state machine tracking the check lifecycle
//! - Polling logic with configurable interval, timeout, and retries
//!
//! The actual network I/O (making HTTP requests, opening TCP connections)
//! is performed by the caller via a trait. This module provides the
//! decision logic and state tracking.

use std::time::{Duration, Instant};

/// How to check if a service is healthy.
#[derive(Debug, Clone)]
pub enum HealthCheck {
    /// HTTP GET request — healthy if response is 2xx or 3xx.
    Http {
        /// URL to check (e.g., "http://localhost:8080/health").
        url: String,
    },

    /// TCP connection — healthy if a connection can be established.
    Tcp {
        /// Host and port (e.g., "localhost:3306").
        addr: String,
    },
}

impl HealthCheck {
    /// Parse a health check from a string.
    ///
    /// Supports two formats:
    /// - `http://host:port/path` — HTTP check
    /// - `tcp://host:port` — TCP check
    pub fn parse(s: &str) -> Result<Self, String> {
        if s.starts_with("http://") || s.starts_with("https://") {
            Ok(Self::Http { url: s.to_string() })
        } else if s.starts_with("tcp://") {
            let addr = s
                .strip_prefix("tcp://")
                .unwrap_or(s)
                .to_string();
            if addr.is_empty() {
                return Err("tcp:// health check requires host:port".to_string());
            }
            Ok(Self::Tcp { addr })
        } else {
            // Default to HTTP if no scheme.
            Ok(Self::Http {
                url: format!("http://{s}"),
            })
        }
    }
}

/// Configuration for the health check polling loop.
#[derive(Debug, Clone)]
pub struct HealthCheckConfig {
    /// The check to perform.
    pub check: HealthCheck,

    /// How long to wait between probes.
    pub interval: Duration,

    /// Maximum time to wait for the service to become healthy.
    pub timeout: Duration,

    /// Maximum number of consecutive failures before giving up.
    /// If None, only the timeout matters.
    pub max_failures: Option<u32>,
}

impl HealthCheckConfig {
    /// Create a config with sensible defaults.
    pub fn new(check: HealthCheck) -> Self {
        Self {
            check,
            interval: Duration::from_millis(500),
            timeout: Duration::from_secs(60),
            max_failures: None,
        }
    }

    /// Set the polling interval.
    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.interval = interval;
        self
    }

    /// Set the timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set the maximum consecutive failures.
    pub fn with_max_failures(mut self, max: u32) -> Self {
        self.max_failures = Some(max);
        self
    }
}

/// The state of a health check polling session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthState {
    /// Waiting to start the first probe.
    Pending,

    /// Actively polling — not yet healthy.
    Checking {
        /// Number of probes attempted so far.
        attempts: u32,
        /// Number of consecutive failures.
        consecutive_failures: u32,
        /// Last probe result message.
        last_message: String,
    },

    /// Service is healthy.
    Healthy {
        /// How long it took to become healthy.
        elapsed: Duration,
        /// Total number of probes before success.
        attempts: u32,
    },

    /// Health check timed out.
    TimedOut {
        /// How long we waited.
        elapsed: Duration,
        /// Total probes attempted.
        attempts: u32,
        /// Last failure message.
        last_message: String,
    },

    /// Health check exhausted max failures.
    Failed {
        /// Total probes attempted.
        attempts: u32,
        /// Last failure message.
        last_message: String,
    },
}

impl HealthState {
    /// Whether this is a terminal state (healthy, timed out, or failed).
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Healthy { .. } | Self::TimedOut { .. } | Self::Failed { .. }
        )
    }

    /// Whether the service is healthy.
    pub fn is_healthy(&self) -> bool {
        matches!(self, Self::Healthy { .. })
    }

    /// Human-readable status line.
    pub fn status_line(&self) -> String {
        match self {
            Self::Pending => "waiting to start health check...".to_string(),
            Self::Checking {
                attempts,
                last_message,
                ..
            } => format!("checking... (attempt {attempts}: {last_message})"),
            Self::Healthy { elapsed, attempts } => {
                format!(
                    "healthy (took {:.1}s, {attempts} probe{})",
                    elapsed.as_secs_f64(),
                    if *attempts == 1 { "" } else { "s" }
                )
            }
            Self::TimedOut {
                elapsed,
                attempts,
                last_message,
            } => format!(
                "timed out after {:.1}s ({attempts} probes, last: {last_message})",
                elapsed.as_secs_f64()
            ),
            Self::Failed {
                attempts,
                last_message,
            } => format!("failed after {attempts} probes: {last_message}"),
        }
    }
}

/// Result of a single health probe.
#[derive(Debug, Clone)]
pub struct ProbeOutcome {
    /// Whether the probe succeeded.
    pub healthy: bool,

    /// Descriptive message (e.g., "HTTP 200 OK", "connection refused").
    pub message: String,
}

/// Tracks the state of a health check polling session.
///
/// The caller drives the poll loop by:
/// 1. Creating a `HealthPoller` with the config.
/// 2. In a loop: calling `should_probe()` to check timing, performing the
///    probe, calling `record_probe()` with the result.
/// 3. Checking `state()` for terminal conditions.
///
/// This design keeps the I/O out of this crate while providing all the
/// timing, counting, and state management logic.
pub struct HealthPoller {
    config: HealthCheckConfig,
    state: HealthState,
    started_at: Instant,
    last_probe_at: Option<Instant>,
}

impl HealthPoller {
    /// Create a new poller.
    pub fn new(config: HealthCheckConfig) -> Self {
        Self {
            config,
            state: HealthState::Pending,
            started_at: Instant::now(),
            last_probe_at: None,
        }
    }

    /// The current state.
    pub fn state(&self) -> &HealthState {
        &self.state
    }

    /// The health check specification.
    pub fn check(&self) -> &HealthCheck {
        &self.config.check
    }

    /// Whether it's time to perform another probe.
    ///
    /// Returns true if enough time has elapsed since the last probe
    /// (or if no probe has been performed yet).
    pub fn should_probe(&self) -> bool {
        if self.state.is_terminal() {
            return false;
        }

        match self.last_probe_at {
            None => true,
            Some(last) => last.elapsed() >= self.config.interval,
        }
    }

    /// Record the outcome of a probe.
    ///
    /// Updates the state based on the probe result, timeout, and failure
    /// count.
    pub fn record_probe(&mut self, outcome: ProbeOutcome) {
        self.last_probe_at = Some(Instant::now());
        let elapsed = self.started_at.elapsed();

        let (attempts, consecutive_failures) = match &self.state {
            HealthState::Checking {
                attempts,
                consecutive_failures,
                ..
            } => (*attempts + 1, *consecutive_failures),
            _ => (1, 0),
        };

        if outcome.healthy {
            self.state = HealthState::Healthy { elapsed, attempts };
            return;
        }

        let new_consecutive = consecutive_failures + 1;

        // Check max failures.
        if let Some(max) = self.config.max_failures {
            if new_consecutive >= max {
                self.state = HealthState::Failed {
                    attempts,
                    last_message: outcome.message,
                };
                return;
            }
        }

        // Check timeout.
        if elapsed >= self.config.timeout {
            self.state = HealthState::TimedOut {
                elapsed,
                attempts,
                last_message: outcome.message,
            };
            return;
        }

        // Still checking.
        self.state = HealthState::Checking {
            attempts,
            consecutive_failures: new_consecutive,
            last_message: outcome.message,
        };
    }

    /// How long until the timeout expires.
    pub fn remaining_timeout(&self) -> Duration {
        let elapsed = self.started_at.elapsed();
        self.config.timeout.saturating_sub(elapsed)
    }

    /// How long since the poller was created.
    pub fn elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── HealthCheck parsing ──────────────────────────────────────────

    #[test]
    fn parse_http_url() {
        let check = HealthCheck::parse("http://localhost:8080/health").unwrap();
        assert!(matches!(check, HealthCheck::Http { url } if url == "http://localhost:8080/health"));
    }

    #[test]
    fn parse_https_url() {
        let check = HealthCheck::parse("https://localhost:443/").unwrap();
        assert!(matches!(check, HealthCheck::Http { url } if url.starts_with("https://")));
    }

    #[test]
    fn parse_tcp_address() {
        let check = HealthCheck::parse("tcp://localhost:3306").unwrap();
        assert!(matches!(check, HealthCheck::Tcp { addr } if addr == "localhost:3306"));
    }

    #[test]
    fn parse_bare_address_defaults_to_http() {
        let check = HealthCheck::parse("localhost:8080").unwrap();
        assert!(matches!(check, HealthCheck::Http { url } if url == "http://localhost:8080"));
    }

    #[test]
    fn parse_empty_tcp_fails() {
        let result = HealthCheck::parse("tcp://");
        assert!(result.is_err());
    }

    // ── HealthState ──────────────────────────────────────────────────

    #[test]
    fn pending_is_not_terminal() {
        assert!(!HealthState::Pending.is_terminal());
        assert!(!HealthState::Pending.is_healthy());
    }

    #[test]
    fn healthy_is_terminal() {
        let state = HealthState::Healthy {
            elapsed: Duration::from_secs(2),
            attempts: 3,
        };
        assert!(state.is_terminal());
        assert!(state.is_healthy());
    }

    #[test]
    fn timed_out_is_terminal() {
        let state = HealthState::TimedOut {
            elapsed: Duration::from_secs(60),
            attempts: 120,
            last_message: "connection refused".to_string(),
        };
        assert!(state.is_terminal());
        assert!(!state.is_healthy());
    }

    #[test]
    fn status_line_formats() {
        assert!(!HealthState::Pending.status_line().is_empty());

        let healthy = HealthState::Healthy {
            elapsed: Duration::from_millis(1500),
            attempts: 3,
        };
        let line = healthy.status_line();
        assert!(line.contains("healthy"));
        assert!(line.contains("1.5s"));
        assert!(line.contains("3 probes"));

        let single = HealthState::Healthy {
            elapsed: Duration::from_millis(500),
            attempts: 1,
        };
        assert!(single.status_line().contains("1 probe"));
        assert!(!single.status_line().contains("probes"));
    }

    // ── HealthPoller ─────────────────────────────────────────────────

    #[test]
    fn poller_starts_pending() {
        let config = HealthCheckConfig::new(HealthCheck::Http {
            url: "http://localhost:80".to_string(),
        });
        let poller = HealthPoller::new(config);
        assert_eq!(*poller.state(), HealthState::Pending);
        assert!(poller.should_probe());
    }

    #[test]
    fn poller_becomes_healthy_on_success() {
        let config = HealthCheckConfig::new(HealthCheck::Http {
            url: "http://localhost:80".to_string(),
        });
        let mut poller = HealthPoller::new(config);

        poller.record_probe(ProbeOutcome {
            healthy: true,
            message: "HTTP 200 OK".to_string(),
        });

        assert!(poller.state().is_healthy());
        assert!(!poller.should_probe()); // Terminal — no more probes.
    }

    #[test]
    fn poller_tracks_failures() {
        let config = HealthCheckConfig::new(HealthCheck::Http {
            url: "http://localhost:80".to_string(),
        });
        let mut poller = HealthPoller::new(config);

        poller.record_probe(ProbeOutcome {
            healthy: false,
            message: "connection refused".to_string(),
        });

        assert!(!poller.state().is_terminal());
        if let HealthState::Checking {
            attempts,
            consecutive_failures,
            last_message,
        } = poller.state()
        {
            assert_eq!(*attempts, 1);
            assert_eq!(*consecutive_failures, 1);
            assert_eq!(last_message, "connection refused");
        } else {
            panic!("expected Checking state");
        }
    }

    #[test]
    fn poller_fails_after_max_failures() {
        let config = HealthCheckConfig::new(HealthCheck::Http {
            url: "http://localhost:80".to_string(),
        })
        .with_max_failures(3);

        let mut poller = HealthPoller::new(config);

        for i in 1..=3 {
            poller.record_probe(ProbeOutcome {
                healthy: false,
                message: format!("attempt {i} failed"),
            });
        }

        assert!(poller.state().is_terminal());
        assert!(matches!(poller.state(), HealthState::Failed { .. }));
    }

    #[test]
    fn poller_resets_consecutive_on_success() {
        let config = HealthCheckConfig::new(HealthCheck::Http {
            url: "http://localhost:80".to_string(),
        })
        .with_max_failures(3);

        let mut poller = HealthPoller::new(config);

        // Two failures.
        poller.record_probe(ProbeOutcome {
            healthy: false,
            message: "fail 1".to_string(),
        });
        poller.record_probe(ProbeOutcome {
            healthy: false,
            message: "fail 2".to_string(),
        });

        // Success — becomes healthy, doesn't hit max.
        poller.record_probe(ProbeOutcome {
            healthy: true,
            message: "HTTP 200 OK".to_string(),
        });

        assert!(poller.state().is_healthy());
    }

    #[test]
    fn poller_times_out() {
        let config = HealthCheckConfig::new(HealthCheck::Http {
            url: "http://localhost:80".to_string(),
        })
        .with_timeout(Duration::from_millis(1));

        let mut poller = HealthPoller::new(config);

        // Wait for timeout to expire.
        std::thread::sleep(Duration::from_millis(10));

        poller.record_probe(ProbeOutcome {
            healthy: false,
            message: "still down".to_string(),
        });

        assert!(poller.state().is_terminal());
        assert!(matches!(poller.state(), HealthState::TimedOut { .. }));
    }

    #[test]
    fn poller_respects_interval() {
        let config = HealthCheckConfig::new(HealthCheck::Http {
            url: "http://localhost:80".to_string(),
        })
        .with_interval(Duration::from_millis(100));

        let mut poller = HealthPoller::new(config);

        // First probe is immediate.
        assert!(poller.should_probe());

        poller.record_probe(ProbeOutcome {
            healthy: false,
            message: "not ready".to_string(),
        });

        // Too soon for next probe.
        assert!(!poller.should_probe());

        // Wait for interval.
        std::thread::sleep(Duration::from_millis(110));
        assert!(poller.should_probe());
    }

    #[test]
    fn remaining_timeout_decreases() {
        let config = HealthCheckConfig::new(HealthCheck::Http {
            url: "http://localhost:80".to_string(),
        })
        .with_timeout(Duration::from_secs(10));

        let poller = HealthPoller::new(config);
        let remaining = poller.remaining_timeout();

        // Should be close to 10 seconds.
        assert!(remaining.as_secs() >= 9);

        std::thread::sleep(Duration::from_millis(50));
        let remaining2 = poller.remaining_timeout();
        assert!(remaining2 < remaining);
    }
}
