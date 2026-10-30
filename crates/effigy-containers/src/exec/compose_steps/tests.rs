use super::{ComposeSteps, TimedStep};
use crate::exec::ComposeStepEvent;
use effigy_ui::steps::ProgressSource;

#[test]
fn a_duration_is_named_the_way_a_person_reads_it() {
    assert_eq!(
        super::with_duration("started web", std::time::Duration::from_millis(800)),
        "started web (800ms)"
    );
    assert_eq!(
        super::with_duration("started web", std::time::Duration::from_millis(1_400)),
        "started web (1s)"
    );
}

/// The step lines a terminal shows for one detached activation: one per image,
/// build and container, each ending with what now holds. Piped and `--json`
/// output draws nothing, so the lines are captured instead.
#[test]
fn an_activation_draws_one_finished_line_per_service_move() {
    let (_, lines) = effigy_ui::steps::capture_lines(|| {
        let mut steps = ComposeSteps::new("demo-web-dev");
        let events = [
            ComposeStepEvent::Pulling {
                subject: "postgres:16".to_owned(),
            },
            ComposeStepEvent::Pulled {
                subject: "postgres:16".to_owned(),
            },
            ComposeStepEvent::Building {
                subject: "demo-web-dev-web-1".to_owned(),
            },
            ComposeStepEvent::Built {
                subject: "demo-web-dev-web-1".to_owned(),
            },
            ComposeStepEvent::Creating {
                subject: "demo-web-dev-web-1".to_owned(),
            },
            ComposeStepEvent::Created {
                subject: "demo-web-dev-web-1".to_owned(),
            },
            ComposeStepEvent::Starting {
                subject: "demo-web-dev-web-1".to_owned(),
            },
            ComposeStepEvent::Started {
                subject: "demo-web-dev-web-1".to_owned(),
            },
            // A container the runtime reports but never started is still
            // finished, so no spinner is left behind.
            ComposeStepEvent::Creating {
                subject: "demo-web-dev-pma-1".to_owned(),
            },
        ];
        for event in &events {
            steps.observe(event);
        }
        steps.finish_unfinished(true, "the runtime reported the stack as reconciled");
    });
    let rendered = lines.join("\n");
    for expected in [
        "✓ pulled postgres:16",
        "✓ built web",
        "✓ started web",
        "✓ the runtime reported the stack as reconciled",
    ] {
        assert!(
            rendered.contains(expected),
            "{expected} missing from {rendered}"
        );
    }
    assert_eq!(
        lines.len(),
        4,
        "one line per image, build and container: {rendered}"
    );
}

/// `container down` draws the runtime's own stopping and removing, one step per
/// service, while it happens (effigy#313). The reviewer's finding was that down
/// opened every step before the teardown and closed them after it, so a slow
/// stop could never name the service holding it up.
#[test]
fn a_teardown_draws_stopping_and_removing_as_the_runtime_reports_them() {
    let (_, lines) = effigy_ui::steps::capture_lines(|| {
        let mut steps = ComposeSteps::new("demo-web-dev");
        for event in [
            ComposeStepEvent::Stopping {
                subject: "demo-web-dev-web-1".to_owned(),
            },
            ComposeStepEvent::Stopped {
                subject: "demo-web-dev-web-1".to_owned(),
            },
            ComposeStepEvent::Removing {
                subject: "demo-web-dev-web-1".to_owned(),
            },
            ComposeStepEvent::Removed {
                subject: "demo-web-dev-web-1".to_owned(),
            },
        ] {
            // The label moves while the service is stopping; nothing is
            // finished until the runtime says so.
            steps.observe(&event);
            if !event.is_final() {
                assert_eq!(steps.touched(), 1);
            }
        }
        steps.finish_unfinished_teardown(None);
    });
    assert_eq!(
        lines.len(),
        1,
        "one step per service, whatever moves it: {lines:?}"
    );
    assert!(
        lines[0].starts_with("✓ removed web ("),
        "the step names the removed service and its duration: {lines:?}"
    );
}

#[test]
fn a_teardown_step_the_runtime_did_not_narrate_still_says_what_happened() {
    let (_, lines) = effigy_ui::steps::capture_lines(|| {
        let mut steps = ComposeSteps::new("demo-web-dev");
        steps.observe(&ComposeStepEvent::Stopping {
            subject: "demo-web-dev-web-1".to_owned(),
        });
        // The service is still there afterwards: the step says so instead of
        // claiming a removal the runtime did not report.
        steps.finish_unfinished_teardown(Some(&["demo-web-dev-web-1".to_owned()]));
    });
    assert_eq!(lines.len(), 1);
    assert!(
        lines[0].starts_with("✗ web is still present after the teardown ("),
        "{lines:?}"
    );
    assert!(
        lines[0].contains("`effigy container logs --service web`"),
        "{lines:?}"
    );
}

#[test]
fn a_finished_step_is_never_rewritten_by_a_repeated_runtime_line() {
    let (_, lines) = effigy_ui::steps::capture_lines(|| {
        let mut steps = ComposeSteps::new("demo-web-dev");
        let removed = ComposeStepEvent::Removed {
            subject: "demo-web-dev-web-1".to_owned(),
        };
        steps.observe(&removed);
        steps.observe(&removed);
        let step = TimedStep::live(ProgressSource::waiting_for("the stack check"));
        step.succeed("the stack is checked");
        step.fail("a second ending would be wrong");
        step.finish_clear();
    });
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(lines[0].starts_with("✓ removed web ("), "{lines:?}");
    assert!(
        lines[1].starts_with("✓ the stack is checked ("),
        "the second ending is the only one drawn: {lines:?}"
    );
}

#[test]
fn image_build_and_container_steps_stay_distinct() {
    let mut steps = ComposeSteps::new("demo");
    steps.observe(&ComposeStepEvent::Pulling {
        subject: "postgres:16".to_owned(),
    });
    steps.observe(&ComposeStepEvent::Building {
        subject: "demo-web-1".to_owned(),
    });
    steps.observe(&ComposeStepEvent::Creating {
        subject: "demo-web-1".to_owned(),
    });
    assert_eq!(steps.touched(), 3);
}

#[test]
fn a_runtime_step_frame_moves_with_the_runtime_and_names_its_wait() {
    let step = super::runtime_wait("pulling postgres:16");
    let frame = step.label();
    assert!(
        frame.starts_with("pulling postgres:16; waiting for the runtime's next report; "),
        "{frame}"
    );
    assert!(frame.ends_with("s elapsed"), "{frame}");
    // A new runtime line replaces the label; nothing waits on a fixed one.
    step.set_label("starting web");
    assert!(
        step.label()
            .starts_with("starting web; waiting for the runtime's next report; "),
        "{}",
        step.label()
    );
}
