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

/// The same health wait, read on a fake clock: the attempt, the probe and the
/// elapsed wait move, and the last answer the check gave sticks.
#[test]
fn a_health_wait_frame_names_the_probe_attempt_elapsed_and_last_answer() {
    let (progress, step) = health_wait_progress("web", "http://127.0.0.1:8080/healthz");
    progress.update(
        "probing `http://127.0.0.1:8080/healthz` for web (attempt 3)",
        None,
        None,
    );
    assert_eq!(
        progress.frame_at(std::time::Duration::from_secs(4)),
        "probing `http://127.0.0.1:8080/healthz` for web (attempt 3); waiting for `http://127.0.0.1:8080/healthz` to report web healthy; 4s elapsed"
    );
    progress.event("`http://127.0.0.1:8080/healthz` did not answer healthy");
    assert_eq!(
        progress.frame_at(std::time::Duration::from_secs(9)),
        "probing `http://127.0.0.1:8080/healthz` for web (attempt 3); last: `http://127.0.0.1:8080/healthz` did not answer healthy; 9s elapsed"
    );
    step.finish_clear();
}

#[test]
fn a_failed_health_wait_returns_its_error_and_draws_no_step_output() {
    // Timeout 0 keeps the wait from probing: the contract under test is the
    // error a caller reports, and that a non-terminal run draws no step text.
    let error = wait_for_ready("container `web`", Some("tcp://127.0.0.1:9"), 0, None)
        .expect_err("an unreachable health check must fail the wait");
    assert_eq!(error.outcome(), "not_ready");
    assert!(error.to_string().contains(
        "the health check of container `web` (`tcp://127.0.0.1:9`) did not become ready within 0s"
    ));
    assert_eq!(error.subject(), "container `web`");
    assert_eq!(effigy_ui::steps::mode(), effigy_ui::StepMode::Silent);
}

/// One in-process HTTP answer on a loopback port, so the probe's own reading of
/// a status line is exercised without a container runtime.
fn answer_once(status_line: &str) -> u16 {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().expect("local addr").port();
    let answer = status_line.to_owned();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut request = [0u8; 256];
            let _ = stream.read(&mut request);
            let _ = stream.write_all(
                format!("{answer}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes(),
            );
        }
    });
    port
}

#[test]
fn a_definite_status_mismatch_ends_the_health_wait_at_once() {
    let port = answer_once("HTTP/1.1 500 Internal Server Error");
    let check = format!("http://127.0.0.1:{port}/health");
    let started = std::time::Instant::now();
    // A long declared budget: a definite answer must not spend it.
    let error = wait_for_ready("container `web`", Some(&check), 300, None)
        .expect_err("a 500 is a definite refusal, not a slow start");
    assert_eq!(error.observed_status(), Some(500));
    assert_eq!(error.outcome(), "status_mismatch");
    assert!(
        error.to_string().contains("answered HTTP 500")
            && error.to_string().contains("status_mismatch"),
        "{error}"
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "a definite status is terminal, not a 300s wait: {:?}",
        started.elapsed()
    );
}

#[test]
fn an_http_health_check_keeps_the_status_it_read() {
    assert_eq!(http_status(b"HTTP/1.1 200 OK\r\n"), Some(200));
    assert_eq!(http_status(b"HTTP/1.0 302 Found\r\n"), Some(302));
    assert_eq!(http_status(b"HTTP/1.1 404 Not Found\n"), Some(404));
    assert_eq!(
        http_status(b"HTTP/1.1 500 Internal Server Error\r\n"),
        Some(500)
    );
    // An empty reason phrase is still a complete line (RFC 9112 requires the
    // space after the code and allows an empty reason).
    assert_eq!(http_status(b"HTTP/1.1 204 \r\n"), Some(204));
    // No complete status line: an incomplete or foreign answer stays in the
    // budgeted wait instead of becoming a refusal or a false readiness.
    assert_eq!(http_status(b"HTTP/1.1"), None);
    assert_eq!(http_status(b"HTTP/1.1 50"), None);
    assert_eq!(http_status(b"HTTP/1.1 abc nope\r\n"), None);
    assert_eq!(http_status(b"SSH-2.0-OpenSSH_9.0\r\n"), None);
    // A prefix without its terminator, or without the space and reason after
    // the code, is a partial response even when the three digits are there.
    assert_eq!(http_status(b"HTTP/1.1 500"), None);
    assert_eq!(http_status(b"HTTP/1.1 500 Internal Server Error"), None);
    assert_eq!(http_status(b"HTTP/1.1 200"), None);
    assert_eq!(http_status(b"HTTP/1.1 200\r\n"), None);
    // A line with a control byte in its reason is malformed.
    assert_eq!(http_status(b"HTTP/1.1 200 OK\x00\r\n"), None);
}

/// A loopback server that answers `responses.len()` connections. Each response
/// is written in parts with a short pause between them, so a fragmented status
/// line and a peer that closes after a prefix are both exercised.
fn scripted_http_server(responses: Vec<Vec<&'static [u8]>>) -> u16 {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().expect("local addr").port();
    std::thread::spawn(move || {
        for parts in responses {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut request = [0u8; 256];
            let _ = stream.read(&mut request);
            for part in parts {
                if stream.write_all(part).is_err() {
                    break;
                }
                let _ = stream.flush();
                std::thread::sleep(std::time::Duration::from_millis(30));
            }
            // Dropping the stream ends this connection; a response whose parts
            // ended without a terminator is a truncated one.
        }
    });
    port
}

#[test]
fn a_fragmented_complete_status_line_is_still_a_definite_answer() {
    let port = scripted_http_server(vec![vec![
        b"HTTP/1.1 500 Internal",
        b" Server Error\r\nContent-Length: 0\r\n\r\n",
    ]]);
    let check = format!("http://127.0.0.1:{port}/health");
    assert_eq!(
        probe_health_detail(&check),
        HealthProbe::StatusMismatch(500),
        "a status line split across reads is read to its terminator"
    );
}

#[test]
fn a_truncated_mismatch_prefix_stays_in_the_wait_instead_of_ending_it() {
    // The first connection answers `HTTP/1.1 500` and closes: incomplete, so
    // the wait keeps its budget. The second connection answers ready.
    let port = scripted_http_server(vec![
        vec![b"HTTP/1.1 500"],
        vec![b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n"],
    ]);
    let check = format!("http://127.0.0.1:{port}/health");
    let started = std::time::Instant::now();
    let outcome = wait_for_ready("container `web`", Some(&check), 10, None)
        .expect("an incomplete prefix is not a terminal refusal");
    assert_eq!(outcome, Some("ready"));
    assert!(
        started.elapsed() < std::time::Duration::from_secs(10),
        "the wait continued instead of spending its whole budget: {:?}",
        started.elapsed()
    );
}

#[test]
fn a_truncated_ready_prefix_is_not_readiness() {
    // The first connection answers `HTTP/1.1 200` and closes: incomplete, so it
    // proves nothing. The second connection answers a definite 500.
    let port = scripted_http_server(vec![
        vec![b"HTTP/1.1 200"],
        vec![b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n"],
    ]);
    let check = format!("http://127.0.0.1:{port}/health");
    let error = wait_for_ready("container `web`", Some(&check), 10, None)
        .expect_err("an incomplete 200 prefix never reports readiness");
    assert_eq!(error.observed_status(), Some(500));
    assert_eq!(error.outcome(), "status_mismatch");
}

#[test]
fn a_probe_with_no_answer_is_unanswered_not_a_mismatch() {
    assert_eq!(
        probe_health_detail("tcp://127.0.0.1:9"),
        HealthProbe::Unanswered
    );
    assert!(!probe_health("tcp://127.0.0.1:9"));
}
