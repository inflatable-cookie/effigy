//! `container retire` against a scripted runtime: ownership is the labels the
//! backend reports, deletion is exact, and every leftover is named.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

const MANIFEST: &str = "[tasks]\nbuild = \"true\"\n";

struct Fixture {
    owned: effigy_test_support::Fixture,
    root: PathBuf,
    home: PathBuf,
    state: PathBuf,
    path: String,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let owned = effigy_test_support::Fixture::new(name).expect("private fixture");
        let root = owned.path().to_path_buf();
        let home = root.join("home");
        fs::create_dir_all(&home).expect("private home");
        fs::write(root.join("effigy.toml"), MANIFEST).expect("manifest");
        let bin = root.join("bin");
        let state = root.join("runtime");
        fs::create_dir_all(&bin).expect("bin dir");
        fs::create_dir_all(&state).expect("state dir");
        write_executable(&bin.join("docker"), &fake_runtime_script("docker"));
        write_executable(&bin.join("colima"), &fake_runtime_script("colima"));
        let path = format!("{}:{}", bin.display(), std::env::var("PATH").expect("PATH"));
        Self {
            owned,
            root,
            home,
            state,
            path,
        }
    }

    fn root(&self) -> &Path {
        self.owned.path()
    }

    fn checkout(&self) -> String {
        self.root
            .canonicalize()
            .expect("canonical checkout")
            .display()
            .to_string()
    }

    fn container(&self, name: &str, project: &str, scope: &str) {
        append(
            &self.state.join("containers"),
            &format!("{name}|{project}|{}||{scope}", self.checkout()),
        );
    }

    fn unowned_container(&self, name: &str, project: &str) {
        append(
            &self.state.join("containers"),
            &format!("{name}|{project}|||"),
        );
    }

    fn volume(&self, name: &str, project: &str, scope: &str, persist: bool) {
        append(
            &self.state.join("volumes"),
            &format!("{name}|{project}|{}||{scope}|{persist}", self.checkout()),
        );
    }

    /// A volume exactly as `nerdctl compose` leaves one: its Compose project
    /// label and the name Compose gave it, with no Effigy label at all.
    fn project_volume(&self, name: &str, project: &str) {
        append(
            &self.state.join("volumes"),
            &format!("{name}|{project}|||false"),
        );
    }

    fn network(&self, name: &str, project: &str) {
        append(&self.state.join("networks"), &format!("{name}|{project}"));
    }

    fn lock(&self, name: &str) {
        append(&self.state.join("locked"), name);
    }

    /// Mark a container running with `restart: unless-stopped`.
    fn running_unless_stopped(&self, name: &str) {
        append(&self.state.join("running"), name);
        append(&self.state.join("unless-stopped"), name);
    }

    fn state_lines(&self, file: &str) -> Vec<String> {
        fs::read_to_string(self.state.join(file))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_effigy"))
            .arg("--json")
            .arg("--repo")
            .arg(&self.root)
            .args(args)
            .env("NO_COLOR", "1")
            .env("HOME", &self.home)
            .env("EFFIGY_TEST_CONTAINERS_HOME", self.home.join(".effigy"))
            .env("EFFIGY_COMPOSE_BACKEND", "docker")
            .env("EFFIGY_TEST_RUNTIME_STATE", &self.state)
            .env("PATH", &self.path)
            .output()
            .expect("run effigy")
    }

    fn retire(&self, args: &[&str]) -> (bool, Value) {
        let output = self.run(args);
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let envelope: Value = serde_json::from_str(&stdout)
            .unwrap_or_else(|error| panic!("retire JSON ({error}): {stdout}\n{output:?}"));
        // The CLI wraps a command's own payload: a success keeps it under
        // `result`, a failure keeps it in the error's own JSON details.
        let report = envelope
            .get("result")
            .filter(|value| !value.is_null())
            .cloned()
            .or_else(|| {
                envelope
                    .pointer("/error/details")
                    .filter(|value| !value.is_null())
                    .cloned()
            })
            .or_else(|| {
                let text = envelope.pointer("/error/details")?.as_str()?;
                serde_json::from_str(text).ok()
            })
            .unwrap_or_else(|| envelope.clone());
        (output.status.success(), report)
    }
}

fn append(path: &Path, line: &str) {
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .expect("open state file");
    writeln!(file, "{line}").expect("write state line");
}

fn write_executable(path: &Path, contents: &str) {
    fs::write(path, contents).expect("write executable");
    let mut perms = fs::metadata(path).expect("stat").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).expect("chmod");
}

/// A scripted backend. It answers the three listings retirement reads, records
/// every call, and refuses to delete a name that is marked in use.
fn fake_runtime_script(program: &str) -> String {
    format!(
        r#"#!/bin/sh
set -u
state="$EFFIGY_TEST_RUNTIME_STATE"
printf '%s %s\n' "{program}" "$*" >> "$state/calls.log"
case "{program}" in
  colima)
    case "${{1:-}}" in
      list) printf '[]\n' ;;
      status) printf 'Running\n' ;;
      ssh) printf 'worker:containerd\n' ;;
    esac
    exit 0
    ;;
esac
contains() {{ printf '%s' "$1" | grep -q -- "$2"; }}
case "$*" in
  "ps -a "*)
    sed -n 's/^\([^|]*\)|\([^|]*\)|\([^|]*\)|\([^|]*\)|\([^|]*\)$/\1\t\2\t\3\t\4\t\5/p' "$state/containers" 2>/dev/null
    ;;
  "ps --all"*|"ps -a"|"version"*|"info"*)
    exit 0
    ;;
  "volume ls"*)
    cut -d'|' -f1 "$state/volumes" 2>/dev/null
    ;;
  "volume inspect "*)
    shift
    first=1
    printf '['
    for name in "$@"; do
      line=$(grep "^$name|" "$state/volumes" 2>/dev/null | head -1)
      [ -n "$line" ] || continue
      project=$(printf '%s' "$line" | cut -d'|' -f2)
      checkout=$(printf '%s' "$line" | cut -d'|' -f3)
      repo=$(printf '%s' "$line" | cut -d'|' -f4)
      scope=$(printf '%s' "$line" | cut -d'|' -f5)
      persist=$(printf '%s' "$line" | cut -d'|' -f6)
      labels=""
      [ -n "$project" ] && labels="$labels,\"com.effigy.project\":\"$project\",\"com.docker.compose.project\":\"$project\""
      [ -n "$checkout" ] && labels="$labels,\"com.effigy.checkout\":\"$checkout\""
      [ -n "$repo" ] && labels="$labels,\"com.effigy.repo-root\":\"$repo\""
      [ -n "$scope" ] && labels="$labels,\"com.effigy.scope\":\"$scope\""
      labels="$labels,\"com.effigy.persist\":\"$persist\""
      [ $first -eq 0 ] && printf ','
      first=0
      printf '{{"Name":"%s","Labels":{{%s}}}}' "$name" "$(printf '%s' "$labels" | sed 's/^,//')"
    done
    printf ']\n'
    ;;
  "network ls"*)
    project=""
    for arg in "$@"; do
      case "$arg" in
        label=com.docker.compose.project=*) project="${{arg#label=com.docker.compose.project=}}" ;;
      esac
    done
    sed -n "s/^\([^|]*\)|$project\$/\1/p" "$state/networks" 2>/dev/null
    ;;
  "stop "*)
    name="${{2:-}}"
    grep -vx "$name" "$state/running" > "$state/running.tmp" 2>/dev/null || true
    mv "$state/running.tmp" "$state/running" 2>/dev/null || true
    ;;
  "rm -f "*)
    name="${{3:-}}"
    # A kill of a running `restart: unless-stopped` container is an exit the
    # runtime undoes: the container comes back under the same name.
    if grep -qx "$name" "$state/running" 2>/dev/null && grep -qx "$name" "$state/unless-stopped" 2>/dev/null; then
      printf '%s\n' "$name" >> "$state/restarts"
      exit 0
    fi
    if grep -qx "$name" "$state/locked" 2>/dev/null; then
      printf 'container removal failed\n' >&2
      exit 1
    fi
    grep -v "^$name|" "$state/containers" > "$state/containers.tmp" 2>/dev/null || true
    mv "$state/containers.tmp" "$state/containers" 2>/dev/null || true
    ;;
  "volume rm "*)
    name="${{3:-}}"
    if grep -qx "$name" "$state/locked" 2>/dev/null; then
      printf 'Error response from daemon: remove %s: volume is in use\n' "$name" >&2
      exit 1
    fi
    grep -v "^$name|" "$state/volumes" > "$state/volumes.tmp" 2>/dev/null || true
    mv "$state/volumes.tmp" "$state/volumes" 2>/dev/null || true
    ;;
  "network rm "*)
    name="${{3:-}}"
    if grep -qx "$name" "$state/locked" 2>/dev/null; then
      printf 'network removal failed\n' >&2
      exit 1
    fi
    grep -v "^$name|" "$state/networks" > "$state/networks.tmp" 2>/dev/null || true
    mv "$state/networks.tmp" "$state/networks" 2>/dev/null || true
    ;;
esac
exit 0
"#
    )
}

fn names(values: &Value) -> Vec<String> {
    values
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|row| row["name"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn retire_removes_every_labelled_resource_and_says_what_it_removed() {
    let fixture = Fixture::new("effigy-retire-labels-");
    let scope = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let project = "app-web-wt-aaaaaaaaaaaa";
    fixture.container("web-1", project, scope);
    fixture.container("db-1", project, scope);
    fixture.volume("app-web-wt-aaaaaaaaaaaa-target", project, scope, false);
    fixture.volume("app-web-wt-aaaaaaaaaaaa-db-data", project, scope, true);
    fixture.network("app-web-wt-aaaaaaaaaaaa_default", project);

    let (ok, report) = fixture.retire(&["container", "retire", "--yes"]);
    assert!(ok, "{report}");
    assert_eq!(report["schema"], "effigy.container.retire.v1");
    assert_eq!(report["ok"], true);
    assert_eq!(
        names(&report["removed"]),
        vec![
            "db-1",
            "web-1",
            "app-web-wt-aaaaaaaaaaaa-db-data",
            "app-web-wt-aaaaaaaaaaaa-target",
            "app-web-wt-aaaaaaaaaaaa_default",
        ]
    );
    assert!(names(&report["kept"]).is_empty(), "{report}");
    assert!(fixture.state_lines("containers").is_empty());
    assert!(fixture.state_lines("volumes").is_empty());
    assert!(fixture.state_lines("networks").is_empty());
    // Repeating it changes nothing and still succeeds.
    let (ok, again) = fixture.retire(&["container", "retire", "--yes"]);
    assert!(ok, "{again}");
    assert_eq!(again["idempotent"], true);
}

#[test]
fn keep_data_keeps_the_named_volumes_and_lists_them() {
    let fixture = Fixture::new("effigy-retire-keep-data-");
    let scope = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let project = "app-web-wt-bbbbbbbbbbbb";
    fixture.container("web-1", project, scope);
    fixture.volume("app-web-wt-bbbbbbbbbbbb-target", project, scope, false);
    fixture.volume("app-web-wt-bbbbbbbbbbbb-db-data", project, scope, true);

    let (ok, report) = fixture.retire(&["container", "retire", "--keep-data", "--yes"]);
    assert!(ok, "{report}");
    let kept = report["kept"].as_array().expect("kept rows");
    let kept_names = names(&report["kept"]);
    assert!(kept_names.contains(&"app-web-wt-bbbbbbbbbbbb-target".to_owned()));
    assert!(kept_names.contains(&"app-web-wt-bbbbbbbbbbbb-db-data".to_owned()));
    assert!(kept
        .iter()
        .any(|row| row["reason"] == "kept by `--keep-data`"));
    assert!(kept.iter().any(|row| row["reason"]
        == "named data volume (`com.effigy.persist=true`), kept by `--keep-data`"));
    assert!(names(&report["removed"]).contains(&"web-1".to_owned()));
    assert_eq!(fixture.state_lines("volumes").len(), 2);
}

#[test]
fn a_volume_the_runtime_refuses_to_remove_is_named_and_retire_fails() {
    let fixture = Fixture::new("effigy-retire-failure-");
    let scope = "cccccccccccccccccccccccccccccccc";
    let project = "app-web-wt-cccccccccccc";
    fixture.container("web-1", project, scope);
    fixture.volume("app-web-wt-cccccccccccc-stuck", project, scope, false);
    fixture.lock("app-web-wt-cccccccccccc-stuck");

    let (ok, report) = fixture.retire(&["container", "retire", "--yes"]);
    assert!(!ok, "a leftover is never reported clean: {report}");
    assert_eq!(report["ok"], false);
    assert_eq!(
        names(&report["remaining"]),
        vec!["app-web-wt-cccccccccccc-stuck".to_owned()]
    );
    let remaining = report["remaining"].as_array().expect("remaining rows");
    assert!(
        remaining[0]["error"]
            .as_str()
            .is_some_and(|error| error.contains("app-web-wt-cccccccccccc-stuck")
                && error.contains("volume is in use")),
        "{remaining:?}"
    );
    assert_eq!(
        fixture.state_lines("volumes"),
        vec![format!(
            "app-web-wt-cccccccccccc-stuck|{project}|{}||{scope}|false",
            fixture.checkout()
        )]
    );
}

#[test]
fn dead_retire_removes_only_the_stacks_whose_checkout_is_gone() {
    let fixture = Fixture::new("effigy-retire-dead-labels-");
    let dead_scope = "dddddddddddddddddddddddddddddddd";
    let live_scope = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
    let dead_checkout = fixture.root().join("dead-worktree");
    fs::create_dir_all(&dead_checkout).expect("dead checkout before retirement");
    let dead = dead_checkout.display().to_string();
    append(
        &fixture.state.join("containers"),
        &format!(
            "dead-web-1|app-dead-wt-dddddddddddd|{dead}||{dead_scope}\n\
             live-web-1|app-live-wt-eeeeeeeeeeee|{}||{live_scope}",
            fixture.checkout()
        ),
    );
    append(
        &fixture.state.join("volumes"),
        &format!(
            "app-dead-wt-dddddddddddd-target|app-dead-wt-dddddddddddd|{dead}||{dead_scope}|false"
        ),
    );
    append(
        &fixture.state.join("networks"),
        "app-dead-wt-dddddddddddd_default|app-dead-wt-dddddddddddd",
    );
    fs::remove_dir_all(&dead_checkout).expect("remove dead worktree");

    let (ok, report) = fixture.retire(&["container", "retire", "--dead", "--yes"]);
    assert!(ok, "{report}");
    assert_eq!(report["schema"], "effigy.container.retire-dead.v1");
    let scopes = report["scopes"].as_array().expect("scope rows");
    assert_eq!(scopes.len(), 1, "{report}");
    assert_eq!(scopes[0]["checkout"], dead);
    assert_eq!(scopes[0]["ok"], true);
    assert!(names(&scopes[0]["report"]["removed"]).contains(&"dead-web-1".to_owned()));
    assert_eq!(
        fixture.state_lines("containers"),
        vec![format!(
            "live-web-1|app-live-wt-eeeeeeeeeeee|{}||{live_scope}",
            fixture.checkout()
        )]
    );
    assert!(fixture.state_lines("volumes").is_empty());
    assert!(fixture.state_lines("networks").is_empty());
    assert!(
        !dead_checkout.exists(),
        "retirement never recreates a checkout"
    );

    // A checkout that exists again is not dead, and its stack is left alone.
    fs::create_dir_all(&dead_checkout).expect("re-cloned checkout");
    let (ok, report) = fixture.retire(&["container", "retire", "--dead", "--yes"]);
    assert!(ok, "{report}");
    assert!(report["scopes"].as_array().expect("scope rows").is_empty());
    fs::remove_dir_all(&dead_checkout).ok();
}

#[test]
fn a_volume_carrying_only_its_compose_project_label_is_retired_by_name() {
    // What nerdctl compose actually leaves on a named volume: the project label
    // and no Effigy label. The name Compose gave it is the ownership proof.
    let fixture = Fixture::new("effigy-retire-project-volume-");
    let scope = "22222222222222222222222222222222";
    let project = "c2raed464";
    fixture.container("c2raed464-web-1", project, scope);
    fixture.project_volume("c2raed464-cache-data", project);
    // Another project's volume is not this checkout's.
    fixture.project_volume("c2raed464-elsewhere", "c2raed464-other");
    // A look-alike that does not carry the project's name form is left alone.
    fixture.project_volume("efv-51bb06807c269fec", project);

    let (ok, report) = fixture.retire(&["container", "retire", "--yes"]);
    assert!(ok, "{report}");
    assert_eq!(
        names(&report["removed"]),
        vec![
            "c2raed464-web-1".to_owned(),
            "c2raed464-cache-data".to_owned()
        ]
    );
    assert_eq!(
        fixture.state_lines("volumes"),
        vec![
            format!("c2raed464-elsewhere|c2raed464-other|||false"),
            format!("efv-51bb06807c269fec|{project}|||false"),
        ]
    );
}

#[test]
fn a_recloned_checkout_retires_again_with_no_generation_refusal() {
    let fixture = Fixture::new("effigy-retire-reclone-");
    let scope = "ffffffffffffffffffffffffffffffff";
    let project = "app-web-wt-ffffffffffff";
    fixture.container("web-1", project, scope);
    let (ok, report) = fixture.retire(&["container", "retire", "--yes"]);
    assert!(ok, "{report}");
    // A fresh container at the same path and scope is this checkout's again:
    // no recorded generation stands between the two.
    fixture.container("web-1", project, scope);
    let (ok, report) = fixture.retire(&["container", "retire", "--yes"]);
    assert!(ok, "{report}");
    assert_eq!(names(&report["removed"]), vec!["web-1".to_owned()]);
}

#[test]
fn unlabelled_lookalikes_are_reported_and_kept() {
    let fixture = Fixture::new("effigy-retire-unlabelled-");
    let scope = "11111111111111111111111111111111";
    let project = "app-web-wt-111111111111";
    fixture.container("web-1", project, scope);
    fixture.unowned_container("legacy-web-1", project);

    let (ok, report) = fixture.retire(&["container", "retire", "--yes"]);
    assert!(ok, "{report}");
    assert_eq!(names(&report["removed"]), vec!["web-1".to_owned()]);
    let kept = names(&report["kept"]);
    assert_eq!(kept, vec!["legacy-web-1".to_owned()], "{report}");
    assert_eq!(fixture.state_lines("containers").len(), 1);
}

#[test]
fn retire_of_a_checkout_with_no_labelled_resources_is_a_no_op() {
    let fixture = Fixture::new("effigy-retire-noop-");
    let (ok, report) = fixture.retire(&["container", "retire", "--yes"]);
    assert!(ok, "{report}");
    assert_eq!(report["ok"], true);
    assert_eq!(report["idempotent"], true);
    assert!(report["removed"]
        .as_array()
        .expect("removed rows")
        .is_empty());
    // No ownership record is written by reading or retiring.
    assert!(!fixture.root().join(".effigy/state/runtime-scopes").exists());
    assert!(!fixture.home.join(".effigy/runtime-scopes").exists());
    // Nothing reached a mutating backend command.
    let calls = fixture.state_lines("calls.log");
    assert!(calls.iter().all(|call| !call.contains(" rm ")), "{calls:?}");
}

/// The scripted runtime's own proof: `rm -f` of a running
/// `restart: unless-stopped` container is undone by a restart, which is what
/// happened to a milestone-QA stack (effigy#376).
#[test]
fn the_scripted_runtime_restarts_a_killed_unless_stopped_container() {
    let fixture = Fixture::new("effigy-retire-race-model-");
    let scope = "33333333333333333333333333333333";
    fixture.container("web-1", "app-web-wt-333333333333", scope);
    fixture.running_unless_stopped("web-1");
    let output = Command::new("docker")
        .args(["rm", "-f", "web-1"])
        .env("EFFIGY_TEST_RUNTIME_STATE", &fixture.state)
        .env("PATH", &fixture.path)
        .output()
        .expect("scripted docker");
    assert!(output.status.success(), "{output:?}");
    assert_eq!(fixture.state_lines("restarts"), vec!["web-1".to_owned()]);
    assert_eq!(fixture.state_lines("containers").len(), 1);
}

#[test]
fn retire_stops_before_removing_so_unless_stopped_never_resurrects() {
    let fixture = Fixture::new("effigy-retire-restart-policy-");
    let scope = "44444444444444444444444444444444";
    let project = "app-web-wt-444444444444";
    fixture.container("web-1", project, scope);
    fixture.container("mail-1", project, scope);
    fixture.running_unless_stopped("web-1");
    fixture.running_unless_stopped("mail-1");

    let (ok, report) = fixture.retire(&["container", "retire", "--yes"]);
    assert!(ok, "{report}");
    assert_eq!(names(&report["removed"]), vec!["mail-1", "web-1"]);
    assert!(fixture.state_lines("containers").is_empty());
    assert!(
        fixture.state_lines("restarts").is_empty(),
        "a removal never restarts what it is removing"
    );
    let calls = fixture.state_lines("calls.log");
    for name in ["web-1", "mail-1"] {
        let stop = calls
            .iter()
            .position(|call| call == &format!("docker stop {name}"))
            .unwrap_or_else(|| panic!("no stop of {name}: {calls:?}"));
        let remove = calls
            .iter()
            .position(|call| call == &format!("docker rm -f {name}"))
            .unwrap_or_else(|| panic!("no removal of {name}: {calls:?}"));
        assert!(stop < remove, "{name} is stopped before it is removed");
    }
}
