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
//! After: each holder names its task, pid, age and whether a terminal is
//! attached, and the exact commands are `` `effigy run stop <id>` ``,
//! `` `effigy run status <id>` `` and `` `effigy runs` ``.
//!
//! A refusal that points at `effigy tasks status --all` or
//! `effigy tasks status <name>` is worse than no advice: neither form parses
//! for a managed profile, and the person is left guessing (effigy#313). Each
//! suggestion here is parsed through the same grammar the CLI uses.

use effigy_containers::environment::{EnvError, HeldHolder};
use effigy_kernel::id::RunId;
use effigy_kernel::observe::{Evidence, Observed};
use effigy_kernel::run::Liveness;

use crate::tests::prelude::parse_command;

fn run_id(milliseconds: u64, random: u8) -> String {
    RunId::new(milliseconds, [random; 10])
        .expect("run id")
        .to_string()
}

fn evidence() -> Evidence {
    Evidence::new("fixture".to_owned(), None).expect("fixed evidence")
}

fn holder(run: &str, task: &str, pid: u32, ago_ms: i64, foreground: bool) -> HeldHolder {
    HeldHolder {
        run: run.parse::<RunId>().expect("run id"),
        liveness: Observed::Verified(Liveness::Live, evidence()),
        task: Some(task.to_owned()),
        pid: Some(pid),
        started_at_ms: Some(now_ms() - ago_ms),
        foreground,
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_millis() as i64
}

fn held_environment_refusal(holders: Vec<HeldHolder>) -> String {
    EnvError::HeldBy {
        env: "inventors".to_owned(),
        holders,
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
    let text = held_environment_refusal(vec![
        holder(&held, "dev", 99_290, 66 * 60 * 1_000, true),
        holder(&background, "watch", 40_112, 5_000, false),
    ]);

    assert!(
        text.contains(
            "refused to stop, restart or recreate environment `inventors` while 2 runs hold it"
        ),
        "{text}"
    );
    // Each holder: id, task, pid, age, and whether a terminal is attached.
    assert!(text.contains(&format!("run {held}")), "{text}");
    assert!(text.contains("`dev`"), "{text}");
    assert!(text.contains("pid 99290"), "{text}");
    assert!(text.contains("started 1h06m ago"), "{text}");
    assert!(text.contains("attached to a terminal"), "{text}");
    assert!(text.contains(&format!("run {background}")), "{text}");
    assert!(text.contains("`watch`"), "{text}");
    assert!(text.contains("not attached to a terminal"), "{text}");

    // The exact commands: stop through the run's own owner, or Ctrl-C in its
    // terminal; inspect a run or list them all.
    assert!(
        text.contains(&format!("`effigy run stop {held}`")),
        "{text}"
    );
    assert!(text.contains("Ctrl-C"), "{text}");
    assert!(
        text.contains(&format!("`effigy run status {held}`")),
        "{text}"
    );
    assert!(text.contains("`effigy runs`"), "{text}");
    assert!(!text.contains("effigy tasks status"), "{text}");
    assert!(
        !text.contains("End those runs through their own stop first"),
        "{text}"
    );
}

#[test]
fn every_command_a_held_environment_refusal_suggests_parses() {
    let held = run_id(1_760_000_000_000, 7);
    let background = run_id(1_760_000_100_000, 9);
    let text = held_environment_refusal(vec![
        holder(&held, "dev", 99_290, 66 * 60 * 1_000, true),
        holder(&background, "watch", 40_112, 5_000, false),
    ]);
    let commands = suggested_commands(&text);
    assert!(
        commands
            .iter()
            .any(|command| command == &format!("effigy run stop {held}")),
        "{commands:?}"
    );
    for command in commands {
        parse_command(
            command
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<String>>(),
        )
        .unwrap_or_else(|error| panic!("`{command}` does not parse: {error}"));
    }
}

#[test]
fn an_unobserved_holder_still_names_the_run_and_the_commands() {
    // A refusal raised without an observation context keeps the correctness of
    // its advice: the run id and the commands, without invented facts.
    let held = run_id(1_760_000_000_000, 7);
    let text = held_environment_refusal(vec![HeldHolder {
        run: held.parse().expect("run id"),
        liveness: Observed::Unknown(effigy_kernel::observe::UnknownReason::Inconsistent(
            "not observed".to_owned(),
        )),
        task: None,
        pid: None,
        started_at_ms: None,
        foreground: false,
    }]);
    assert!(text.contains(&format!("run {held}")), "{text}");
    assert!(
        text.contains(&format!("`effigy run stop {held}`")),
        "{text}"
    );
    assert!(!text.contains("pid"), "{text}");
}
