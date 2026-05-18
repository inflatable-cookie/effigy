use std::fs;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::Value;

use super::support::{temp_workspace, write_manifest_task};

#[test]
fn cli_graph_watch_json_streams_started_and_refresh_events() {
    let root = temp_workspace("graph-watch-json");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest_task(&root, "build", "echo ok");
    fs::write(root.join("src/lib.rs"), "pub fn alpha() {}\n").expect("write rust");

    let mut child = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("graph")
        .arg("watch")
        .arg("--debounce-ms")
        .arg("100")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn graph watch");

    let stdout = child.stdout.take().expect("watch stdout");
    let (tx, rx) = mpsc::channel::<Value>();
    let join = std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            let line = match line {
                Ok(line) => line,
                Err(_) => break,
            };
            if line.trim().is_empty() {
                continue;
            }
            let parsed = serde_json::from_str::<Value>(&line).expect("watch event json");
            if tx.send(parsed).is_err() {
                break;
            }
        }
    });

    let started = recv_event(&rx, Duration::from_secs(5), "started");
    assert_eq!(started["schema"], "effigy.graph.watch.event.v1");
    assert_eq!(started["payload"]["kind"], "started");
    assert_eq!(started["payload"]["debounce_ms"], 100);

    fs::write(
        root.join("src/lib.rs"),
        "pub fn alpha() {}\npub fn beta() {}\n",
    )
    .expect("rewrite rust");

    let refresh = recv_until_kind(&rx, "refresh", Duration::from_secs(5));
    assert_eq!(refresh["schema"], "effigy.graph.watch.event.v1");
    assert_eq!(refresh["payload"]["kind"], "refresh");
    assert_eq!(refresh["payload"]["debounce_ms"], 100);
    assert!(refresh["payload"]["changed_paths"]
        .as_array()
        .is_some_and(|paths| paths
            .iter()
            .any(|value| value.as_str() == Some("src/lib.rs"))));
    assert!(
        refresh["payload"]["refresh_duration_ms"]
            .as_u64()
            .unwrap_or(0)
            > 0
    );
    assert_eq!(
        refresh["payload"]["index"]["failed_paths"],
        serde_json::json!([])
    );

    let _ = child.kill();
    let _ = child.wait();
    let _ = join.join();
}

fn recv_event(rx: &mpsc::Receiver<Value>, timeout: Duration, label: &str) -> Value {
    rx.recv_timeout(timeout)
        .unwrap_or_else(|_| panic!("timed out waiting for {label} event"))
}

fn recv_until_kind(rx: &mpsc::Receiver<Value>, expected_kind: &str, timeout: Duration) -> Value {
    let started = std::time::Instant::now();
    loop {
        let remaining = timeout.saturating_sub(started.elapsed());
        let value = recv_event(rx, remaining, expected_kind);
        if value["payload"]["kind"].as_str() == Some(expected_kind) {
            return value;
        }
    }
}
