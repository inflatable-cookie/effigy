//! The commands a refusal suggests are commands this CLI accepts.
//!
//! Before (effigy#313, the inventors report):
//!
//! ```text
//! refused to stop, restart or recreate environment `web` while runs hold it:
//! run agqw465xzsu66h77ejdxon3hde. End those runs through their own stop first
//! (`effigy tasks status --all` lists them)
//! ```
//!
//! `effigy tasks status --all` parses, but it lists declared tasks and never
//! the run the refusal named; nothing in the text says which command stops it.
//! A refusal that points at `effigy tasks status --all` or
//! `effigy tasks status <name>` is worse than no advice: neither form parses
//! for a managed profile, and the person is left guessing (effigy#313). Each
//! suggestion here is parsed through the same grammar the CLI uses.

use effigy_containers::environment::EnvError;
use effigy_kernel::id::RunId;

use crate::tests::prelude::parse_command;

fn run_id(milliseconds: u64, random: u8) -> String {
    RunId::new(milliseconds, [random; 10])
        .expect("run id")
        .to_string()
}

fn held_environment_refusal(holders: Vec<&str>) -> String {
    EnvError::HeldBy {
        env: "inventors".to_owned(),
        holders: holders
            .into_iter()
            .map(|run| run.parse::<RunId>().expect("run id"))
            .collect(),
    }
    .to_string()
}

/// The `effigy …` commands a refusal suggests, in backticks.
fn suggested_commands(text: &str) -> Vec<String> {
    text.split('`')
        .skip(1)
        .step_by(2)
        .filter(|span| span.starts_with("effigy "))
        .map(str::to_owned)
        .collect()
}

#[test]
fn a_held_environment_names_each_holder_and_how_to_end_it() {
    let held = run_id(1_760_000_000_000, 7);
    let background = run_id(1_760_000_100_000, 9);
    let text = held_environment_refusal(vec![&held, &background]);

    assert!(
        text.contains(
            "refused to stop, restart or recreate environment `inventors` while 2 runs hold it"
        ),
        "{text}"
    );
    assert!(text.contains(&held), "{text}");
    assert!(text.contains(&background), "{text}");
    // One sentence naming the command that ends a holder, and the observation
    // that names each run's task and pid.
    assert!(text.contains("`effigy run stop <id>`"), "{text}");
    assert!(text.contains("effigy container inventors status"), "{text}");
    assert!(!text.contains("effigy tasks status"), "{text}");
    assert!(
        !text.contains("End those runs through their own stop first"),
        "{text}"
    );
}

#[test]
fn a_busy_environment_is_one_sentence_naming_pid_and_task() {
    let text = EnvError::Busy(
        "environment `stack` is busy: run abc (pid 99290, task `dev`) is transitioning it. Nothing was changed and no backend was contacted; inspect it with `effigy container stack status`, then retry"
            .to_owned(),
    )
    .to_string();
    assert!(text.contains("pid 99290"), "{text}");
    assert!(text.contains("task `dev`"), "{text}");
    assert!(text.contains("`effigy container stack status`"), "{text}");
}

#[test]
fn every_command_a_refusal_suggests_parses() {
    let held = run_id(1_760_000_000_000, 7);
    let background = run_id(1_760_000_100_000, 9);
    let mut texts = vec![
        held_environment_refusal(vec![&held, &background]),
        EnvError::Busy(
            "environment `stack` is busy: run abc (pid 99290, task `dev`) is transitioning it. Nothing was changed and no backend was contacted; inspect it with `effigy container stack status`, then retry"
                .to_owned(),
        )
        .to_string(),
    ];
    let mut commands = Vec::new();
    for text in texts.drain(..) {
        commands.extend(suggested_commands(&text));
    }
    assert!(
        commands
            .iter()
            .any(|command| command == "effigy container stack status"),
        "{commands:?}"
    );
    for command in commands {
        if command.contains('<') {
            // A documented placeholder names the shape, not a runnable argv.
            continue;
        }
        parse_command(
            command
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<String>>(),
        )
        .unwrap_or_else(|error| panic!("`{command}` does not parse: {error}"));
    }
}
