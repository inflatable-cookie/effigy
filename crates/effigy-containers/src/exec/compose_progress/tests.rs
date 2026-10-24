use super::{parse_compose_step_event, service_from_container_name, ComposeStepEvent};

/// The lines docker compose v2 prints for one activation, plus the service
/// log line it prints on an attached stack, which is never a step.
const DOCKER_UP_PROGRESS: &str = "\
 Container demo-web-dev-db-1  Creating
 Container demo-web-dev-db-1  Created
 Image postgres:16 Pulling
 Image postgres:16 Pulled
 Container demo-web-dev-db-1  Starting
 Container demo-web-dev-db-1  Started
 web-1  | 172.18.0.1 - - GET / HTTP/1.1 404
";

#[test]
fn a_docker_activation_reads_as_one_step_per_service_move() {
    let events = DOCKER_UP_PROGRESS
        .lines()
        .map(|line| parse_compose_step_event(line))
        .collect::<Vec<_>>();

    assert_eq!(
        events,
        vec![
            Some(ComposeStepEvent::Creating {
                subject: "demo-web-dev-db-1".to_owned()
            }),
            Some(ComposeStepEvent::Created {
                subject: "demo-web-dev-db-1".to_owned()
            }),
            Some(ComposeStepEvent::Pulling {
                subject: "postgres:16".to_owned()
            }),
            Some(ComposeStepEvent::Pulled {
                subject: "postgres:16".to_owned()
            }),
            Some(ComposeStepEvent::Starting {
                subject: "demo-web-dev-db-1".to_owned()
            }),
            Some(ComposeStepEvent::Started {
                subject: "demo-web-dev-db-1".to_owned()
            }),
            None,
        ]
    );
}

#[test]
fn the_labels_name_the_service_the_runtime_is_working_on() {
    let creating = parse_compose_step_event(" Container demo-web-dev-db-1  Creating").unwrap();
    assert_eq!(creating.label("demo-web-dev"), "creating db");
    let started = parse_compose_step_event(" Container demo-web-dev-db-1  Started").unwrap();
    assert_eq!(
        started.done_summary("demo-web-dev"),
        Some("started db".to_owned())
    );

    let pulling = parse_compose_step_event(" Image docker.io/library/postgres:16 Pulling").unwrap();
    assert_eq!(pulling.label("demo-web-dev"), "pulling postgres:16");
    assert_eq!(
        pulling.done_summary("demo-web-dev"),
        None,
        "a pull is only finished when the runtime says so"
    );
    let pulled = parse_compose_step_event(" Image postgres:16 Pulled").unwrap();
    assert_eq!(
        pulled.done_summary("demo-web-dev"),
        Some("pulled postgres:16".to_owned())
    );
}

#[test]
fn a_teardown_reads_as_stopping_and_removing_per_service() {
    let events = [
        " Container demo-web-dev-web-1  Stopping",
        " Container demo-web-dev-web-1  Stopped",
        " Container demo-web-dev-web-1  Removing",
        " Container demo-web-dev-web-1  Removed",
    ]
    .iter()
    .filter_map(|line| parse_compose_step_event(line))
    .collect::<Vec<_>>();
    assert_eq!(events.len(), 4);
    assert_eq!(events[0].label("demo-web-dev"), "stopping web");
    assert_eq!(events[2].label("demo-web-dev"), "removing web");
    assert_eq!(
        events[3].done_summary("demo-web-dev"),
        Some("removed web".to_owned())
    );
    assert!(events.iter().all(ComposeStepEvent::is_teardown));
}

#[test]
fn terminal_glyphs_and_ansi_colour_still_read_as_progress() {
    let coloured = "\u{1b}[32m✔\u{1b}[0m Container demo-web-dev-pma-1  Created";
    assert_eq!(
        parse_compose_step_event(coloured),
        Some(ComposeStepEvent::Created {
            subject: "demo-web-dev-pma-1".to_owned()
        })
    );
    let failed = "✘ Container demo-web-dev-pma-1  Started";
    assert_eq!(
        parse_compose_step_event(failed),
        Some(ComposeStepEvent::Started {
            subject: "demo-web-dev-pma-1".to_owned()
        })
    );
}

#[test]
fn nerdctl_spelling_and_bare_service_names_are_read_too() {
    assert_eq!(
        parse_compose_step_event("demo-web-dev_web_1 Created"),
        Some(ComposeStepEvent::Created {
            subject: "demo-web-dev_web_1".to_owned()
        })
    );
    assert_eq!(
        parse_compose_step_event("web Building"),
        Some(ComposeStepEvent::Building {
            subject: "web".to_owned()
        })
    );
    assert_eq!(
        parse_compose_step_event("web Built"),
        Some(ComposeStepEvent::Built {
            subject: "web".to_owned()
        })
    );
}

#[test]
fn prose_and_log_lines_are_not_steps() {
    for line in [
        "",
        "   ",
        "[+] Running 3/3",
        " ⠿ Network demo-web-dev_default  Created",
        "time=\"2026-10-24T10:00:00Z\" level=info msg=\"starting\"",
        "waiting for web to report healthy",
        "The container is starting up and will be ready soon",
        "web-1  | Building the asset pipeline",
    ] {
        assert_eq!(
            parse_compose_step_event(line),
            None,
            "`{line}` is not a progress line"
        );
    }
}

#[test]
fn a_container_name_reduces_to_its_service() {
    assert_eq!(
        service_from_container_name("demo-web-dev", "demo-web-dev-db-1"),
        "db"
    );
    assert_eq!(
        service_from_container_name("demo-web-dev", "demo-web-dev_web_1"),
        "web"
    );
    assert_eq!(
        service_from_container_name("demo-web-dev", "demo-web-dev-worker-1"),
        "worker"
    );
    // A name from another project, or one with no index, is kept as printed:
    // a guess would be worse evidence than the runtime's own word.
    assert_eq!(
        service_from_container_name("demo-web-dev", "other-project-web-1"),
        "other-project-web-1"
    );
    assert_eq!(service_from_container_name("demo-web-dev", "web"), "web");
    assert_eq!(
        service_from_container_name("demo-web-dev", "demo-web-dev-cache"),
        "demo-web-dev-cache"
    );
}
