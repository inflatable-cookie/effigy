use std::collections::BTreeMap;
use std::time::Duration;

use super::{
    bounded_last_output, parse_inspect_health, readiness_diagnostic, repeated_failure_diagnostic,
    shared_failure, unready, HealthState, ServiceHealth, SharedFailureTracker,
    LAST_OUTPUT_MAX_CHARS, SHARED_FAILURE_MIN_ELAPSED, SHARED_FAILURE_OBSERVATIONS,
};

/// A `docker inspect` record shaped like the one from the inventors report:
/// `web` (nginx) unhealthy because its check treats a 404 as failure, and
/// `pma` unhealthy because the image has no `wget`.
const INSPECT_TWO_UNHEALTHY: &str = r#"[
  {
    "Name": "/demo-web-dev-web-1",
    "State": {
      "Status": "running",
      "Running": true,
      "ExitCode": 0,
      "Health": {
        "Status": "unhealthy",
        "FailingStreak": 12,
        "Log": [
          {"Output": "GET / HTTP/1.1 404\n"},
          {"Output": "GET / HTTP/1.1 404\n"}
        ]
      }
    },
    "Config": {"Labels": {"com.docker.compose.service": "web"}}
  },
  {
    "Name": "/demo-web-dev-pma-1",
    "State": {
      "Status": "running",
      "Running": true,
      "ExitCode": 0,
      "Health": {
        "Status": "unhealthy",
        "FailingStreak": 3,
        "Log": [
          {"Output": "/bin/sh: wget: not found\n"}
        ]
      }
    },
    "Config": {"Labels": {"com.docker.compose.service": "pma"}}
  },
  {
    "Name": "/demo-web-dev-db-1",
    "State": {
      "Status": "running",
      "Running": true,
      "ExitCode": 0,
      "Health": {"Status": "healthy", "FailingStreak": 0, "Log": []}
    },
    "Config": {"Labels": {"com.docker.compose.service": "db"}}
  }
]"#;

#[test]
fn an_inspect_record_names_each_service_status_and_last_check_output() {
    let services = parse_inspect_health(INSPECT_TWO_UNHEALTHY.as_bytes()).expect("inspect parses");
    assert_eq!(
        services
            .iter()
            .map(|service| service.service.as_str())
            .collect::<Vec<_>>(),
        vec!["web", "pma", "db"]
    );
    let web = &services[0];
    assert_eq!(web.container, "demo-web-dev-web-1");
    assert_eq!(web.health, Some(HealthState::Unhealthy));
    assert_eq!(web.failing_streak, 12);
    assert_eq!(web.last_output.as_deref(), Some("GET / HTTP/1.1 404"));
    assert!(!web.is_ready());

    let db = &services[2];
    assert!(db.is_ready());
    assert_eq!(unready(&services).len(), 2);
}

#[test]
fn a_container_without_a_check_is_ready_once_it_runs() {
    let services = parse_inspect_health(
        br#"[{"Name":"/demo-app-1","State":{"Status":"running","ExitCode":0},"Config":{"Labels":{"com.docker.compose.service":"app"}}}]"#,
    )
    .expect("inspect parses");
    assert_eq!(services[0].health, None);
    assert!(services[0].is_ready());
}

#[test]
fn an_exited_service_names_its_exit_code() {
    let services = parse_inspect_health(
        br#"[{"Name":"/demo-db-1","State":{"Status":"exited","ExitCode":1},"Config":{"Labels":{"com.docker.compose.service":"db"}}}]"#,
    )
    .expect("inspect parses");
    assert!(services[0].has_exited());
    assert_eq!(
        services[0].problem(),
        "db exited with exit code 1",
        "an exited container names its exit code"
    );
}

#[test]
fn an_unreadable_inspect_is_never_readiness() {
    assert!(parse_inspect_health(b"not json").is_err());
    assert!(parse_inspect_health(b"[]").is_err());
}

#[test]
fn the_readiness_failure_names_every_service_its_check_and_the_next_commands() {
    let services = parse_inspect_health(INSPECT_TWO_UNHEALTHY.as_bytes()).expect("inspect parses");
    let diagnostic = readiness_diagnostic("inventors", &services, 60, &BTreeMap::new());

    assert!(diagnostic.starts_with("environment `inventors` did not become ready within 60s"));
    assert!(
        diagnostic.contains("- web is unhealthy after 12 failed checks (demo-web-dev-web-1)"),
        "{diagnostic}"
    );
    assert!(
        diagnostic.contains("its last health check said: `GET / HTTP/1.1 404`"),
        "{diagnostic}"
    );
    assert!(
        diagnostic.contains("- pma is unhealthy after 3 failed checks (demo-web-dev-pma-1)"),
        "{diagnostic}"
    );
    assert!(
        diagnostic.contains("its last health check said: `/bin/sh: wget: not found`"),
        "{diagnostic}"
    );
    assert!(
        diagnostic.contains("`effigy container logs --service web`"),
        "{diagnostic}"
    );
    assert!(
        diagnostic.contains("`effigy container status`"),
        "{diagnostic}"
    );
    assert!(
        diagnostic.contains("Retrying unchanged will fail the same way"),
        "a person is told whether to retry or fix the definition: {diagnostic}"
    );
    // The healthy service is not part of the failure.
    assert!(!diagnostic.contains("- db "), "{diagnostic}");
}

#[test]
fn an_exited_service_reports_its_last_log_line() {
    let services = parse_inspect_health(
        br#"[{"Name":"/demo-db-1","State":{"Status":"exited","ExitCode":1},"Config":{"Labels":{"com.docker.compose.service":"db"}}}]"#,
    )
    .expect("inspect parses");
    let mut logs = BTreeMap::new();
    logs.insert(
        "db".to_owned(),
        "FATAL: password authentication failed for user \"app\"".to_owned(),
    );
    let diagnostic = readiness_diagnostic("demo", &services, 60, &logs);
    assert!(
        diagnostic.contains("db exited with exit code 1 (demo-db-1)"),
        "{diagnostic}"
    );
    assert!(
        diagnostic.contains("its last log line: `FATAL: password authentication failed"),
        "{diagnostic}"
    );
    assert!(
        diagnostic.contains("`effigy container logs --service db`"),
        "{diagnostic}"
    );
    assert!(
        diagnostic.contains("needs its own configuration fixed"),
        "{diagnostic}"
    );
}

#[test]
fn a_shared_failure_is_recognised_only_when_every_unready_service_agrees() {
    let shared = vec![
        service(
            "web",
            Some(HealthState::Unhealthy),
            4,
            "/bin/sh: wget: not found",
        ),
        service(
            "pma",
            Some(HealthState::Unhealthy),
            4,
            "/bin/sh: wget: not found",
        ),
        service_ready("db"),
    ];
    let failure = shared_failure(&shared).expect("identical failures are shared");
    assert_eq!(failure.services, vec!["web", "pma"]);
    assert_eq!(failure.signature, "/bin/sh: wget: not found");
    assert!(repeated_failure_diagnostic(&failure)
        .contains("web, pma fail the same health check: `/bin/sh: wget: not found`"));

    let different = vec![
        service("web", Some(HealthState::Unhealthy), 4, "GET / 404"),
        service(
            "pma",
            Some(HealthState::Unhealthy),
            4,
            "/bin/sh: wget: not found",
        ),
    ];
    assert!(shared_failure(&different).is_none());
    // One failing service is a service's own problem, not a shared one.
    assert!(shared_failure(&shared[..1]).is_none());
}

#[test]
fn a_shared_failure_must_repeat_before_it_stops_the_wait() {
    let services = vec![
        service(
            "web",
            Some(HealthState::Unhealthy),
            1,
            "/bin/sh: wget: not found",
        ),
        service(
            "pma",
            Some(HealthState::Unhealthy),
            1,
            "/bin/sh: wget: not found",
        ),
    ];
    let mut tracker = SharedFailureTracker::default();
    for observation in 1..SHARED_FAILURE_OBSERVATIONS {
        assert!(
            tracker
                .observe(&services, SHARED_FAILURE_MIN_ELAPSED * 2)
                .is_none(),
            "observation {observation} is too early to be permanent"
        );
    }
    let failure = tracker
        .observe(&services, SHARED_FAILURE_MIN_ELAPSED * 2)
        .expect("the repeated failure is diagnosed");
    assert_eq!(failure.consecutive_checks, SHARED_FAILURE_OBSERVATIONS);

    // A single sample, even long after the wait began, is not enough.
    let mut tracker = SharedFailureTracker::default();
    assert!(tracker
        .observe(&services, Duration::from_secs(30))
        .is_none());
}

#[test]
fn a_different_failure_resets_the_repetition() {
    let first = vec![
        service("web", Some(HealthState::Unhealthy), 1, "GET / 404"),
        service("pma", Some(HealthState::Unhealthy), 1, "GET / 404"),
    ];
    let second = vec![
        service("web", Some(HealthState::Unhealthy), 2, "connection refused"),
        service("pma", Some(HealthState::Unhealthy), 2, "connection refused"),
    ];
    let mut tracker = SharedFailureTracker::default();
    for _ in 0..SHARED_FAILURE_OBSERVATIONS {
        tracker.observe(&first, SHARED_FAILURE_MIN_ELAPSED * 2);
    }
    assert!(
        tracker
            .observe(&second, SHARED_FAILURE_MIN_ELAPSED * 2)
            .is_none(),
        "a new signature starts its own count"
    );
}

#[test]
fn a_check_output_is_one_bounded_line() {
    let output = format!(
        "GET / 404\nsecond line that is not the first\n{}",
        "x".repeat(400)
    );
    let bounded = bounded_last_output(&output).expect("first line");
    assert_eq!(bounded, "GET / 404");
    let long = "y".repeat(LAST_OUTPUT_MAX_CHARS + 20);
    let bounded = bounded_last_output(&long).expect("bounded line");
    assert_eq!(bounded.chars().count(), LAST_OUTPUT_MAX_CHARS + 1);
    assert!(bounded.ends_with('…'));
    assert_eq!(bounded_last_output("\n  \n"), None);
}

fn service(service: &str, health: Option<HealthState>, streak: u64, output: &str) -> ServiceHealth {
    ServiceHealth {
        service: service.to_owned(),
        container: format!("demo-{service}-1"),
        state: "running".to_owned(),
        health,
        failing_streak: streak,
        last_output: Some(output.to_owned()),
        exit_code: None,
    }
}

fn service_ready(service: &str) -> ServiceHealth {
    ServiceHealth {
        service: service.to_owned(),
        container: format!("demo-{service}-1"),
        state: "running".to_owned(),
        health: Some(HealthState::Healthy),
        failing_streak: 0,
        last_output: None,
        exit_code: None,
    }
}
