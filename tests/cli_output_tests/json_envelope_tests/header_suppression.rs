//! An explicit `--json` is the machine contract, wherever the invocation runs.
//!
//! `EFFIGY_INTERNAL_SUPPRESS_HEADER` marks a nested Effigy invocation so the
//! parent's header and retired-config notice are not repeated. It hides that
//! human furniture and nothing else: a caller that asked for JSON still gets
//! one `effigy.command.v1` document, including a workload that inherited the
//! marker from a host-process supervisor (effigy#330).

use super::temp_workspace;
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

const MARKER: &str = "EFFIGY_INTERNAL_SUPPRESS_HEADER";
/// The CLI header's top border. It is the one glyph no payload contains, so
/// its absence proves the header was suppressed rather than merely reworded.
const HEADER_GLYPH: &str = "╭";

fn effigy_command(root: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_effigy"));
    command
        .current_dir(root)
        .arg("--repo")
        .arg(root)
        .args(args)
        .env("NO_COLOR", "1")
        .env(MARKER, "1")
        .stdin(std::process::Stdio::null());
    command
}

fn run(root: &Path, args: &[&str]) -> Output {
    effigy_command(root, args).output().expect("run effigy")
}

fn parse_stdout(output: &Output) -> Value {
    let stdout = String::from_utf8(output.stdout.clone()).expect("utf8 stdout");
    serde_json::from_str(&stdout).expect("stdout is one JSON document")
}

fn assert_suppressed_header(output: &Output, label: &str) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stdout.contains(HEADER_GLYPH),
        "{label}: the header must stay suppressed on stdout: {stdout}"
    );
    assert!(
        !stderr.contains(HEADER_GLYPH),
        "{label}: the header must stay suppressed on stderr: {stderr}"
    );
}

fn assert_envelope(parsed: &Value, kind: &str, label: &str) {
    assert_eq!(parsed["schema"], "effigy.command.v1", "{label}: {parsed}");
    assert_eq!(parsed["schema_version"], 1, "{label}: {parsed}");
    assert_eq!(parsed["command"]["kind"], kind, "{label}: {parsed}");
}

fn workspace_with_task(name: &str) -> std::path::PathBuf {
    let root = temp_workspace(name);
    fs::write(
        root.join("effigy.toml"),
        "[tasks.dev]\nrun = \"printf dev\"\n",
    )
    .expect("write manifest");
    root
}

#[test]
fn the_internal_header_marker_never_strips_a_local_json_envelope() {
    let root = workspace_with_task("cli-json-marker-tasks-local");

    let output = run(&root, &["tasks", "--json"]);

    assert!(output.status.success(), "{output:?}");
    let parsed = parse_stdout(&output);
    assert_envelope(&parsed, "tasks", "tasks --json");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["result"]["schema"], "effigy.tasks.v1");
    assert_suppressed_header(&output, "tasks --json");
}

#[test]
fn the_internal_header_marker_never_strips_a_global_json_envelope() {
    let root = workspace_with_task("cli-json-marker-tasks-global");

    let output = run(&root, &["--json", "tasks"]);

    assert!(output.status.success(), "{output:?}");
    let parsed = parse_stdout(&output);
    assert_envelope(&parsed, "tasks", "--json tasks");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["result"]["schema"], "effigy.tasks.v1");
    assert_suppressed_header(&output, "--json tasks");
}

#[test]
fn the_internal_header_marker_never_strips_the_doctor_envelope() {
    let root = workspace_with_task("cli-json-marker-doctor");

    let output = run(&root, &["doctor", "--json"]);

    assert!(output.status.success(), "{output:?}");
    let parsed = parse_stdout(&output);
    assert_envelope(&parsed, "doctor", "doctor --json");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["result"]["schema"], "effigy.doctor.v1");
    assert_suppressed_header(&output, "doctor --json");
}

#[test]
fn the_internal_header_marker_never_strips_a_container_hosts_envelope() {
    let root = temp_workspace("cli-json-marker-container-hosts");
    fs::write(
        root.join("compose.yml"),
        "services:\n  app:\n    image: alpine\n",
    )
    .expect("write compose file");
    fs::write(
        root.join("effigy.toml"),
        "[containers]\ndefault = \"web\"\n\n\
[containers.web]\ndriver = \"colima\"\nprofile = \"effigy\"\ncompose_file = \"compose.yml\"\nworking_dir = \"/workspace\"\nprimary_service = \"app\"\n\n\
[containers.web.dns]\nroutes = [{ domain = \"app.test\", port = 80, service = \"app\" }]\n\n\
[[containers.web.host_processes]]\nname = \"worker\"\nrun = \"true\"\n",
    )
    .expect("write manifest");

    let output = run(&root, &["container", "web", "hosts", "--json"]);

    assert!(output.status.success(), "{output:?}");
    let parsed = parse_stdout(&output);
    assert_envelope(&parsed, "container", "container web hosts --json");
    assert_eq!(parsed["result"]["schema"], "effigy.container.hosts.v1");
    assert_suppressed_header(&output, "container web hosts --json");
}

#[test]
fn the_internal_header_marker_never_strips_a_run_status_envelope() {
    let root = workspace_with_task("cli-json-marker-run-status");

    // A status read for a run that does not exist is still one envelope: the
    // refusal must be machine-readable under the marker as anywhere else.
    let output = run(&root, &["--json", "run", "status", "missing-run"]);

    let parsed = parse_stdout(&output);
    assert_envelope(&parsed, "runs", "--json run status");
    if !output.status.success() {
        assert_eq!(parsed["ok"], false, "{parsed}");
        assert!(!parsed["error"]["message"].is_null(), "{parsed}");
    }
    assert_suppressed_header(&output, "--json run status");
}

/// A workload inherits whatever environment its supervisor was started with.
/// That is how a host process ran `effigy … --json` and saw a bare payload
/// (effigy#330, Acowtancy #756): the marker must not change the schema.
#[test]
fn a_workload_that_inherits_the_marker_still_gets_the_envelope() {
    let root = workspace_with_task("cli-json-marker-workload");

    let output = Command::new("/bin/sh")
        .arg("-c")
        .arg("exec \"$0\" tasks --json")
        .arg(env!("CARGO_BIN_EXE_effigy"))
        .current_dir(&root)
        .env("NO_COLOR", "1")
        .env(MARKER, "1")
        .output()
        .expect("run the workload");

    assert!(output.status.success(), "{output:?}");
    let parsed = parse_stdout(&output);
    assert_envelope(&parsed, "tasks", "workload tasks --json");
    assert_eq!(parsed["result"]["schema"], "effigy.tasks.v1");
    assert_suppressed_header(&output, "workload tasks --json");
}

/// The marker's documented job survives: a nested invocation that did not ask
/// for JSON stays quiet and prints no envelope.
#[test]
fn a_nested_text_invocation_stays_quiet_and_unenveloped() {
    let root = workspace_with_task("cli-json-marker-text");

    let output = run(&root, &["tasks"]);

    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("\"schema\": \"effigy.command.v1\""),
        "a text invocation gets no JSON envelope: {stdout}"
    );
    assert_suppressed_header(&output, "tasks");
}
