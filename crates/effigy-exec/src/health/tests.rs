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
