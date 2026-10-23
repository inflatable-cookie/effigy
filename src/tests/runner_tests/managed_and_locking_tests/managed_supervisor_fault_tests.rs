//! Journal-boundary fault injection on the out-of-process supervisor
//! (effigy#235, contract 052 §12.5). A traced run lists every kernel crossing
//! the supervisor makes from admission to settle; each crossing is then armed
//! in turn and the supervisor SIGKILLed there. Oracles per crossing: a stop
//! settles the run with complete cleanup, no workload survives, and the next
//! run starts and stops cleanly.
use crate::runner::tests::prelude::{lock_test, run_dev, write_root_manifest, EnvGuard};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
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

const SHARDS: usize = 6;
const MATRIX_ROOT: &str = "EFFIGY_TEST_SUPERVISOR_MATRIX_ROOT";
const MATRIX_SHARD: &str = "EFFIGY_TEST_SUPERVISOR_MATRIX_SHARD";
const WORKER: &str = "runner::tests::managed_and_locking_tests::managed_supervisor_fault_tests::supervisor_fault_shard";
const REQUIRED: [&str; 4] = [
    "BeginAdmitted#0",
    "IntendAfter#0",
    "AppliedAfter#0",
    "SettleBefore#0",
];

#[test]
fn supervisor_killed_at_every_journal_crossing_settles_without_orphans() {
    let _guard = lock_test();
    let fixture = effigy_test_support::Fixture::workspace("effigy-supervisor-faults-")
        .expect("private matrix root");
    eprintln!("supervisor matrix fixture: {}", fixture.path().display());
    let root = fixture.path().join("baseline");
    fs::create_dir(&root).expect("baseline checkout");
    write_manifest(&root);
    let trace = fixture.path().join("trace");
    {
        let _env = EnvGuard::set_many(&[(TRACE, Some(trace.display().to_string()))]);
        one_run(&root, "trace").expect("a traced run stops cleanly");
    }
    let armed = crossings(&trace);
    for required in REQUIRED {
        assert!(armed.iter().any(|point| point == required), "{armed:?}");
    }
    fs::write(
        fixture.path().join("plan.json"),
        serde_json::to_vec(&armed).expect("crossing plan"),
    )
    .expect("write crossing plan");
    let workers: Vec<_> = (0..SHARDS)
        .map(|shard| {
            let child = Command::new(std::env::current_exe().expect("test executable"))
                .args([
                    WORKER,
                    "--exact",
                    "--ignored",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env(MATRIX_ROOT, fixture.path())
                .env(MATRIX_SHARD, shard.to_string())
                .env_remove(FAULT)
                .env_remove(TRACE)
                .stdin(Stdio::null())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .spawn()
                .expect("matrix worker");
            eprintln!("supervisor matrix shard {shard}: pid {}", child.id());
            (shard, child)
        })
        .collect();
    // Reap all exact children before reading results or reporting failures.
    // Their checkout journals and immutable plan remain available throughout.
    let outcomes: Vec<_> = workers
        .into_iter()
        .map(|(shard, mut worker)| (shard, worker.wait().expect("matrix worker wait")))
        .collect();
    let mut failed = Vec::new();
    for (shard, status) in &outcomes {
        if !status.success() {
            failed.push(format!("shard {shard}: {status:?}"));
        }
    }
    for checkout in std::iter::once(root)
        .chain((0..SHARDS).map(|shard| fixture.path().join(format!("shard-{shard}"))))
    {
        if let Err(error) = effigy_test_support::process::wait_kernel_helpers(&checkout) {
            failed.push(format!("{}: {error}", checkout.display()));
        }
    }
    if !failed.is_empty() {
        let retained = fixture.path().to_path_buf();
        // Unknown custody cannot authorize directory deletion. Retain only
        // this exact fixture for diagnosis if a worker or helper did not close.
        std::mem::forget(fixture);
        panic!(
            "matrix failures: {failed:?}; retained {}",
            retained.display()
        );
    }
    let mut missed = Vec::new();
    let mut completed = 0;
    for (shard, _) in outcomes {
        let result = fs::read(fixture.path().join(format!("shard-{shard}/result.json")))
            .expect("worker result");
        let (count, absent): (usize, Vec<String>) =
            serde_json::from_slice(&result).expect("worker result JSON");
        completed += count;
        missed.extend(absent);
    }
    assert_eq!(
        completed,
        armed.len(),
        "every crossing assigned exactly once"
    );
    println!(
        "{} crossings armed across {SHARDS} workers, {} never fired: {missed:?}\n{armed:?}",
        armed.len(),
        missed.len()
    );
    for required in REQUIRED {
        assert!(
            !missed.iter().any(|point| point == required),
            "{required} never fired"
        );
    }
    // Only timing-dependent poll crossings may vary, as in the unsharded proof.
    assert!(
        missed.len() * 10 <= armed.len(),
        "faults never fired: {missed:?}"
    );
}

#[test]
#[ignore = "owned supervisor matrix worker"]
fn supervisor_fault_shard() {
    let _guard = lock_test();
    let base = PathBuf::from(std::env::var_os(MATRIX_ROOT).expect("matrix root"));
    let shard: usize = std::env::var(MATRIX_SHARD)
        .expect("matrix shard")
        .parse()
        .expect("matrix shard number");
    assert!(shard < SHARDS);
    let armed: Vec<String> =
        serde_json::from_slice(&fs::read(base.join("plan.json")).expect("read plan"))
            .expect("crossing plan JSON");
    let root = base.join(format!("shard-{shard}"));
    fs::create_dir(&root).expect("worker checkout");
    write_manifest(&root);
    let proof = root.join("proof");
    fs::create_dir(&proof).expect("worker proof directory");
    let mut missed = Vec::new();
    let mut completed = 0;
    for (case, point) in armed
        .iter()
        .enumerate()
        .filter(|(case, _)| case % SHARDS == shard)
    {
        eprintln!(
            "supervisor shard {shard}: crossing {case}/{} {point}",
            armed.len()
        );
        let case_trace = proof.join(format!("case-{case}"));
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
        let recovered = one_run(&root, &format!("{point} recovery"));
        assert!(
            recovered
                .as_deref()
                .is_ok_and(|out| out.contains("): stopped")),
            "{point} recovery: {recovered:?}"
        );
        completed += 1;
    }
    fs::write(
        root.join("result.json"),
        serde_json::to_vec(&(completed, missed)).expect("worker result JSON"),
    )
    .expect("write worker result");
}
