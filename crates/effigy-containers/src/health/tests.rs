use super::*;

/// Health-check URLs: an HTTP check keeps its path (and defaults to `/` on
/// port 80), a TCP check needs an explicit port, and the wrong scheme for each
/// is refused.
#[test]
fn health_check_url_cases() {
    let http = [
        (
            "http://localhost:8080/health",
            Some(("localhost", 8080, "/health")),
        ),
        ("http://localhost:8080", Some(("localhost", 8080, "/"))),
        ("http://localhost", Some(("localhost", 80, "/"))),
        ("tcp://localhost:3306", None),
    ];
    for (url, expected) in http {
        assert_eq!(
            parse_http_health_check(url),
            expected.map(|(host, port, path)| (host.to_owned(), port, path.to_owned())),
            "{url}"
        );
    }

    let tcp = [
        ("tcp://127.0.0.1:5432", Some(("127.0.0.1", 5432))),
        ("tcp://localhost", None),
        ("http://localhost:80", None),
    ];
    for (url, expected) in tcp {
        assert_eq!(
            parse_tcp_health_check(url),
            expected.map(|(host, port)| (host.to_owned(), port)),
            "{url}"
        );
    }
}

/// `host:port` with a fallback default: an explicit port wins, the default
/// applies when one is given, and a zero default requires an explicit port.
#[test]
fn split_host_port_cases() {
    let cases = [
        ("localhost:3000", 80, Some(("localhost", 3000))),
        ("localhost", 80, Some(("localhost", 80))),
        ("localhost", 0, None),
    ];
    for (raw, default, expected) in cases {
        assert_eq!(
            split_host_port(raw, default),
            expected.map(|(host, port)| (host.to_owned(), port)),
            "{raw} with default {default}"
        );
    }
}

#[test]
fn a_failed_health_wait_returns_its_error_and_draws_no_step_output() {
    // Timeout 0 keeps the wait from probing: the contract under test is the
    // error a caller reports, and that a non-terminal run draws no step text.
    let error = wait_for_ready("web", Some("tcp://127.0.0.1:9"), 0, None)
        .expect_err("an unreachable health check must fail the wait");
    assert!(
        error.contains("container `web` health check `tcp://127.0.0.1:9` did not become ready"),
        "{error}"
    );
    assert_eq!(effigy_ui::steps::mode(), effigy_ui::StepMode::Silent);
}
