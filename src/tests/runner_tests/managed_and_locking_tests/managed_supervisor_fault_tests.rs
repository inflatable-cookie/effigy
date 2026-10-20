//! Journal-boundary fault injection on the out-of-process supervisor
//! (effigy#235, contract 052 §12.5). A traced run lists every kernel crossing
//! the supervisor makes from admission to settle; each crossing is then armed
//! in turn and the supervisor SIGKILLed there. Oracles per crossing: a stop
//! settles the run with complete cleanup, no workload survives, and the next
//! run starts and stops cleanly.
use crate::runner::tests::prelude::{
    lock_test, run_dev, temp_workspace, write_root_manifest, EnvGuard,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const TRACE: &str = "EFFIGY_TEST_SUPERVISOR_TRACE";
const FAULT: &str = "EFFIGY_TEST_SUPERVISOR_FAULT";
/// A point crossed more often than this (polls, log reads) is armed at its
/// first occurrences only.
const PER_POINT: usize = 3;

fn write_manifest(root: &Path) {
    write_root_manifest(
        root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [
  { name = "api", run = "echo $$ > api.pid; while true; do sleep 1; done", start = 2 },
  { name = "front", run = "echo $$ > front.pid; while true; do sleep 1; done", start = 1 }
]
"#,
    );
}

fn start(root: &Path) -> JoinHandle<Result<String, String>> {
    let root = root.to_path_buf();
    std::thread::spawn(move || run_dev(&root, &["--headless"]).map_err(|error| error.to_string()))
}

/// Waits for the run to reach `running`, or for its client to end (the
/// supervisor died first). True when it is running.
fn wait_running(root: &Path, client: &JoinHandle<Result<String, String>>) -> bool {
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        if client.is_finished() {
            return false;
        }
        let status = run_dev(root, &["status"]).unwrap_or_default();
        if status.contains("api          running") && status.contains("front        running") {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("the run neither reached running nor ended");
}

fn join(client: JoinHandle<Result<String, String>>, case: &str) -> Result<String, String> {
    let deadline = Instant::now() + Duration::from_secs(60);
    while !client.is_finished() {
        assert!(
            Instant::now() < deadline,
            "{case}: the client outlived its supervisor"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    client.join().expect("client thread")
}

fn assert_gone(root: &Path, case: &str) {
    for file in ["api.pid", "front.pid"] {
        let Ok(raw) = fs::read_to_string(root.join(file)) else {
            continue;
        };
        let Ok(pid) = raw.trim().parse::<u32>() else {
            continue;
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        while effigy_process::process_is_running(pid) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            !effigy_process::process_is_running(pid),
            "{case}: `{file}` pid {pid} survived"
        );
    }
}

/// One run from start to settle; a stop ends it when it got as far as
/// running. Returns the client's result.
fn one_run(root: &Path, case: &str) -> Result<String, String> {
    for file in ["api.pid", "front.pid"] {
        let _ = fs::remove_file(root.join(file));
    }
    let client = start(root);
    if wait_running(root, &client) {
        let _ = run_dev(root, &["stop"]);
    }
    let result = join(client, case);
    // Idempotent: settles whatever the dead supervisor left, or reports the
    // run's settled outcome.
    let stop = run_dev(root, &["stop"]).unwrap_or_else(|error| panic!("{case}: stop: {error}"));
    assert!(
        stop.contains("settled managed task") || stop.contains("is not running"),
        "{case}: {stop}"
    );
    let status = run_dev(root, &["status"]).unwrap_or_else(|error| error.to_string());
    assert!(
        !stop.contains("cleanup incomplete"),
        "{case}: {stop}\n{status}"
    );
    assert!(
        !status.contains("): running") && !status.contains("): stopping"),
        "{case}: {status}"
    );
    assert_gone(root, case);
    result
}

/// `Name#nth` for each crossing a clean run makes, bounded per point.
fn crossings(trace: &Path) -> Vec<String> {
    let mut seen = BTreeMap::<String, usize>::new();
    let mut armed = Vec::new();
    for name in fs::read_to_string(trace).expect("trace").lines() {
        let nth = seen.entry(name.to_owned()).or_default();
        if *nth < PER_POINT {
            armed.push(format!("{name}#{nth}"));
        }
        *nth += 1;
    }
    armed
}

#[test]
fn supervisor_killed_at_every_journal_crossing_settles_without_orphans() {
    let _guard = lock_test();
    let root = temp_workspace("supervisor-faults");
    write_manifest(&root);
    let scratch = tempfile::tempdir().expect("trace dir");
    let trace: PathBuf = scratch.path().join("trace");
    {
        let _env = EnvGuard::set_many(&[(TRACE, Some(trace.display().to_string()))]);
        one_run(&root, "trace").expect("a traced run stops cleanly");
    }
    let armed = crossings(&trace);
    for required in [
        "BeginAdmitted#0",
        "IntendAfter#0",
        "AppliedAfter#0",
        "SettleBefore#0",
    ] {
        assert!(armed.iter().any(|point| point == required), "{armed:?}");
    }
    let mut missed = Vec::new();
    for (case, point) in armed.iter().enumerate() {
        let case_trace = scratch.path().join(format!("case-{case}"));
        {
            let _env = EnvGuard::set_many(&[
                (FAULT, Some(point.clone())),
                (TRACE, Some(case_trace.display().to_string())),
            ]);
            let _ = one_run(&root, point);
        }
        let fired = fs::read_to_string(&case_trace).is_ok_and(|trace| trace.contains('!'));
        if !fired {
            missed.push(point.clone());
        }
        // Recovery: nothing the killed supervisor left blocks the next run.
        let recovered = one_run(&root, &format!("{point} recovery"));
        assert!(
            recovered
                .as_deref()
                .is_ok_and(|out| out.contains("): stopped")),
            "{point} recovery: {recovered:?}"
        );
    }
    println!(
        "{} crossings armed, {} never fired: {missed:?}\n{armed:?}",
        armed.len(),
        missed.len()
    );
    // A timing-dependent crossing (a poll) may not recur in an armed run;
    // every journal boundary does.
    assert!(
        missed.len() * 10 <= armed.len(),
        "faults that never fired: {missed:?} of {armed:?}"
    );
}
