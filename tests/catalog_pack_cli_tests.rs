//! End-to-end proofs for the catalog-pack surface that only the real binary
//! can give: visible fallback reaching ordinary catalog-backed consumers, and
//! genuine cross-process serialization of durable store mutation.
//!
//! Every test drives an isolated `HOME`, so none of them read or write a
//! developer's real `~/.effigy`.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("mkdir");
    }
    std::fs::write(path, contents).expect("write");
}

/// A valid pack directory carrying one `postgres` fragment.
fn candidate_pack(root: &Path, version: &str, image_tag: &str) -> PathBuf {
    let pack_root = root.join(format!("pack-{version}"));
    write(
        &pack_root.join("pack.toml"),
        &format!(
            "schema_version = 1\n\n[pack]\nid = \"effigy-default-catalog\"\n\
             version = \"{version}\"\n\n[compatibility]\neffigy = \">=0.1\"\n"
        ),
    );
    write(
        &pack_root.join("postgres/service.toml"),
        "[service]\nname = \"postgres\"\ndescription = \"pack postgres\"\n",
    );
    write(
        &pack_root.join("postgres/compose.fragment.yml"),
        &format!("image: postgres:{image_tag}\n"),
    );
    pack_root
}

/// A repo whose default container consumes a catalog-backed service.
fn catalog_repo(root: &Path) -> PathBuf {
    let repo = root.join("repo");
    write(
        &repo.join("effigy.toml"),
        "[catalog]\nalias = \"demo\"\n\n[containers]\ndefault = \"web\"\n\n\
         [containers.web]\nprimary_service = \"db\"\n\n\
         [containers.web.services.db]\ncatalog = \"postgres\"\n",
    );
    repo
}

struct Run {
    stdout: String,
    stderr: String,
    success: bool,
}

fn effigy(home: &Path, args: &[&str]) -> Run {
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .args(args)
        .env("HOME", home)
        // The Colima temp-root guard would otherwise reject a tempdir repo
        // before catalog resolution is ever reached.
        .env("EFFIGY_TEST_SKIP_COLIMA_TEMP_ROOT_CHECK", "1")
        .stdin(Stdio::null())
        .output()
        .expect("run effigy");
    Run {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        success: output.status.success(),
    }
}

/// Install a pack, then corrupt a non-manifest byte of the stored content so
/// the active selection is unhealthy without the manifest changing.
fn install_then_corrupt(home: &Path, source: &Path) {
    let install = effigy(
        home,
        &[
            "service",
            "pack",
            "install",
            "--path",
            source.to_str().expect("utf-8 path"),
        ],
    );
    assert!(install.success, "install failed: {}", install.stderr);

    let installs = home.join(".effigy/catalog-packs/v1/installs");
    let entry = std::fs::read_dir(&installs)
        .expect("read installs")
        .flatten()
        .map(|entry| entry.path())
        .find(|path| path.is_dir())
        .expect("one install directory");
    write(
        &entry.join("postgres/compose.fragment.yml"),
        "image: postgres:tampered\n",
    );
}

#[test]
fn an_unhealthy_pack_warns_visibly_in_both_text_and_json() {
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path().join("home");
    std::fs::create_dir_all(&home).expect("mkdir home");
    let repo = catalog_repo(temp.path());
    install_then_corrupt(&home, &candidate_pack(temp.path(), "1.0.0", "16"));
    let repo_arg = repo.to_str().expect("utf-8 path");

    let text = effigy(&home, &["service", "list", "--repo", repo_arg]);
    assert!(text.success, "service list failed: {}", text.stderr);
    assert!(
        text.stderr
            .contains("[warn] active catalog pack is unhealthy"),
        "catalog source changed silently; stderr was:\n{}",
        text.stderr
    );
    assert!(
        text.stderr.contains("fallback-content-changed"),
        "the structured reason was omitted:\n{}",
        text.stderr
    );
    assert!(
        text.stderr.contains("effigy service pack rollback"),
        "the repair was omitted:\n{}",
        text.stderr
    );
    // The fallback is a source change, so the baseline fragment is what the
    // operator actually gets.
    assert!(
        text.stdout.contains("postgres [bundled]"),
        "an unhealthy pack still supplied content:\n{}",
        text.stdout
    );

    let json = effigy(&home, &["--json", "service", "list", "--repo", repo_arg]);
    assert!(json.success, "json service list failed: {}", json.stderr);
    let notice: serde_json::Value = json
        .stderr
        .lines()
        .find(|line| line.contains("effigy.catalog-pack.fallback.v1"))
        .map(|line| serde_json::from_str(line).expect("notice is JSON"))
        .unwrap_or_else(|| panic!("no structured notice on stderr:\n{}", json.stderr));
    assert_eq!(notice["fallback"], true);
    assert_eq!(notice["reason"], "fallback-content-changed");
    assert_eq!(notice["layer"], "compiled-baseline");
    assert_eq!(
        notice["repair"],
        serde_json::json!(["effigy service pack rollback", "effigy service pack reset"])
    );

    // stdout stays exactly one clean envelope: the notice never contaminates
    // an existing JSON contract.
    let envelope: serde_json::Value =
        serde_json::from_str(json.stdout.trim()).expect("stdout is one envelope");
    assert_eq!(envelope["schema"], "effigy.command.v1");
    assert_eq!(
        envelope["result"]["selection"]["reason"],
        "fallback-content-changed"
    );
    assert_eq!(envelope["result"]["selection"]["fallback"], true);
}

// The container/system/workspace boundary is proven in
// `effigy-containers::tests::catalog_pack_fallback`, not here: under
// `cargo test`, feature unification enables `effigy-containers/test-support`,
// which pins that crate's `~/.effigy` resolution to a synthetic home so
// container tests can never touch a developer's real one. This harness binary
// therefore cannot observe a `HOME`-based pack store on the container path.

#[test]
fn a_healthy_machine_emits_no_fallback_notice() {
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path().join("home");
    std::fs::create_dir_all(&home).expect("mkdir home");
    let repo = catalog_repo(temp.path());
    let pack = candidate_pack(temp.path(), "1.0.0", "16");
    let install = effigy(
        &home,
        &[
            "service",
            "pack",
            "install",
            "--path",
            pack.to_str().expect("utf-8 path"),
        ],
    );
    assert!(install.success, "install failed: {}", install.stderr);

    let run = effigy(
        &home,
        &[
            "service",
            "list",
            "--repo",
            repo.to_str().expect("utf-8 path"),
        ],
    );
    assert!(run.success);
    assert!(
        !run.stderr.contains("unhealthy"),
        "a healthy pack announced a fallback:\n{}",
        run.stderr
    );
    assert!(
        run.stdout.contains("postgres [installed-pack"),
        "the healthy pack was not selected:\n{}",
        run.stdout
    );
}

#[test]
fn concurrent_installs_from_separate_processes_keep_every_record() {
    // Genuinely cross-process: separate `effigy` invocations racing one store.
    // Without the durable lock the read-modify-write of `state.json` drops
    // records; the assertion is exact, not statistical.
    const WORKERS: usize = 6;
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path().join("home");
    std::fs::create_dir_all(&home).expect("mkdir home");

    let packs: Vec<PathBuf> = (0..WORKERS)
        .map(|index| {
            candidate_pack(
                temp.path(),
                &format!("{}.0.0", index + 1),
                &format!("{}", 16 + index),
            )
        })
        .collect();

    let children: Vec<_> = packs
        .iter()
        .map(|pack| {
            Command::new(env!("CARGO_BIN_EXE_effigy"))
                .args([
                    "service",
                    "pack",
                    "install",
                    "--path",
                    pack.to_str().expect("utf-8 path"),
                ])
                .env("HOME", &home)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .expect("spawn install")
        })
        .collect();

    for child in children {
        let output = child.wait_with_output().expect("wait install");
        assert!(
            output.status.success(),
            "a concurrent install failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let state: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(home.join(".effigy/catalog-packs/v1/state.json"))
            .expect("read state"),
    )
    .expect("parse state");
    let installs = state["installs"].as_array().expect("installs");
    assert_eq!(
        installs.len(),
        WORKERS,
        "concurrent installs lost lineage: {state}"
    );

    let active = state["active"].as_str().expect("active");
    assert!(
        installs.iter().any(|record| record["install_id"] == active),
        "active names no record: {state}"
    );
    for record in installs {
        let id = record["install_id"].as_str().expect("install id");
        assert!(
            home.join(".effigy/catalog-packs/v1/installs")
                .join(id)
                .is_dir(),
            "content for {id} is missing"
        );
    }

    // Retention is settled: nothing was pruned, and status agrees.
    let status = effigy(&home, &["--json", "service", "pack", "status"]);
    assert!(status.success, "status failed: {}", status.stderr);
    let envelope: serde_json::Value =
        serde_json::from_str(status.stdout.trim()).expect("status envelope");
    assert_eq!(
        envelope["result"]["installs"]
            .as_array()
            .expect("installs")
            .len(),
        WORKERS
    );
}
