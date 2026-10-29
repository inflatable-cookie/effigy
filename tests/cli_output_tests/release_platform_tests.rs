//! `release status` and `release --plan` against the contract 054 §4 fixtures.
//!
//! Every fixture is a whole project root under
//! `tests/fixtures/release_platform/`. Each test copies one into an owned
//! temporary workspace and commits it, because resolution reads git state and
//! the plan reports the working tree. Nothing in these tests pushes, tags or
//! publishes.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use std::process::{Command, Output};

use super::support::{
    attach_bare_remote, git_commit_all, git_stdout, init_git_repo, parse_stdout_json,
    run_json_cli_command, temp_workspace,
};

fn fixture_root(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/release_platform")
        .join(name)
}

fn copy_dir(source: &Path, target: &Path) {
    fs::create_dir_all(target).expect("create fixture directory");
    for entry in fs::read_dir(source).expect("read fixture directory") {
        let entry = entry.expect("read fixture entry");
        let path = entry.path();
        let destination = target.join(entry.file_name());
        if path.is_dir() {
            copy_dir(&path, &destination);
        } else {
            fs::write(&destination, fs::read(&path).expect("read fixture file"))
                .expect("write fixture file");
        }
    }
}

/// Copy a fixture into an owned workspace and make it a committed git repo.
fn workspace(name: &str) -> PathBuf {
    let root = temp_workspace(&format!("release-platform-{name}"));
    copy_dir(&fixture_root(name), &root);
    init_git_repo(&root);
    git_commit_all(&root, "fixture");
    root
}

fn release_plan(root: &Path) -> Value {
    let output = run_json_cli_command(root, &["release", "--plan"]);
    let parsed = parse_stdout_json(&output);
    assert!(
        output.status.success(),
        "release --plan failed: {}",
        serde_json::to_string_pretty(&parsed).expect("serialize")
    );
    parsed["result"].clone()
}

fn release_status(root: &Path) -> Value {
    let output = run_json_cli_command(root, &["release", "status"]);
    let parsed = parse_stdout_json(&output);
    assert!(
        output.status.success(),
        "release status failed: {}",
        serde_json::to_string_pretty(&parsed).expect("serialize")
    );
    parsed["result"].clone()
}

fn identity<'a>(plan: &'a Value, name: &str) -> &'a Value {
    plan["identities"]
        .as_array()
        .expect("identities")
        .iter()
        .find(|identity| identity["name"] == name)
        .unwrap_or_else(|| panic!("missing identity `{name}`: {plan}"))
}

/// The shorthand names its single identity after the repository (054 §4.1),
/// so a copied fixture's identity is named after the temporary workspace.
fn shorthand_identity_name(root: &Path) -> String {
    root.file_name()
        .expect("workspace name")
        .to_string_lossy()
        .into_owned()
}

fn target<'a>(plan: &'a Value, name: &str) -> &'a Value {
    plan["targets"]
        .as_array()
        .expect("targets")
        .iter()
        .find(|target| target["name"] == name)
        .unwrap_or_else(|| panic!("missing target `{name}`: {plan}"))
}

#[test]
fn shorthand_fixture_resolves_one_semver_identity_and_one_tag_target() {
    let root = workspace("shorthand");
    let plan = release_plan(&root);

    assert_eq!(plan["schema"], "effigy.release.plan.v1");
    assert_eq!(plan["config_shape"], "single-identity-shorthand");
    assert_eq!(plan["ready"], true);

    let identities = plan["identities"].as_array().expect("identities");
    assert_eq!(identities.len(), 1);
    // Contract 054 §4.1: the shorthand's identity is named after the repo.
    assert_eq!(
        identities[0]["name"],
        root.file_name()
            .expect("repo name")
            .to_string_lossy()
            .as_ref()
    );
    assert_eq!(identities[0]["kind"], "semver");
    assert_eq!(identities[0]["current_version"], "0.2.4");
    assert_eq!(identities[0]["next_version"], "0.2.5");
    assert_eq!(identities[0]["tag"], "v0.2.5");

    // The shorthand's gates run in `certify` (§4.1).
    let gates = plan["gates"].as_array().expect("gates");
    assert_eq!(gates.len(), 2);
    assert!(gates.iter().all(|gate| gate["stage"] == "certify"));

    // One tag target, no artifacts, and no approval checkpoint (§10 ruling 3).
    assert_eq!(target(&plan, "tag")["kind"], "tag");
    assert_eq!(plan["targets"].as_array().expect("targets").len(), 1);
    assert_eq!(plan["artifacts"].as_array().expect("artifacts").len(), 0);
    assert_eq!(plan["approvals"]["checkpoints"], serde_json::json!([]));

    // Version file, changelog and the configured sync file.
    let files = plan["files"].as_array().expect("files");
    assert_eq!(files.len(), 3, "got: {files:?}");
    assert!(files.iter().any(|file| file
        .as_str()
        .is_some_and(|path| path.ends_with("Cargo.toml"))));
    assert!(files.iter().any(|file| file
        .as_str()
        .is_some_and(|path| path.ends_with("Cargo.lock"))));
    assert!(files.iter().any(|file| file
        .as_str()
        .is_some_and(|path| path.ends_with("CHANGELOG.md"))));
    assert_eq!(plan["working_tree_clean"], true);
}

#[test]
fn named_identity_fixture_resolves_identities_artifacts_targets_and_approvals() {
    let root = workspace("named-identities");
    let plan = release_plan(&root);

    assert_eq!(plan["config_shape"], "named-identities");
    assert_eq!(plan["ready"], true);

    let core = identity(&plan, "core");
    assert_eq!(core["kind"], "semver");
    assert_eq!(core["current_version"], "0.4.0");
    assert_eq!(core["next_version"], "0.4.1");
    assert_eq!(core["tag"], "core-v0.4.1");
    assert!(core["version_file"]
        .as_str()
        .expect("core version file")
        .ends_with("packages/core/package.json"));

    // `react` follows `core`, so it releases at core's version under its own
    // tag format (contract 054 §4.2).
    let react = identity(&plan, "react");
    assert_eq!(react["follows"], "core");
    assert_eq!(react["next_version"], "0.4.1");
    assert_eq!(react["tag"], "react-v0.4.1");

    // Gates keep their declared stage.
    let gates = plan["gates"].as_array().expect("gates");
    let qa = gates
        .iter()
        .find(|gate| gate["name"] == "qa")
        .expect("qa gate");
    assert_eq!(qa["stage"], "pre-gates");
    assert_eq!(qa["task"], "qa");
    assert_eq!(qa["command"], "effigy qa");
    let pack = gates
        .iter()
        .find(|gate| gate["name"] == "pack")
        .expect("pack gate");
    assert_eq!(pack["stage"], "certify");
    assert_eq!(pack["command"], "node pack.js");

    // Artifacts name their task and glob.
    let artifacts = plan["artifacts"].as_array().expect("artifacts");
    assert_eq!(artifacts.len(), 2);
    let react_artifact = artifacts
        .iter()
        .find(|artifact| artifact["name"] == "react")
        .expect("react artifact");
    assert_eq!(react_artifact["task"], "release:pack:react");
    assert_eq!(react_artifact["path"], "dist/react-*.tgz");
    assert_eq!(react_artifact["identity_kind"], "semver");

    // Targets keep their declared order, kind, check and verify tasks.
    let targets = plan["targets"].as_array().expect("targets");
    assert_eq!(
        targets
            .iter()
            .map(|target| target["name"].as_str().expect("target name"))
            .collect::<Vec<_>>(),
        vec!["tags", "npm"]
    );
    assert_eq!(target(&plan, "tags")["kind"], "tag");
    let npm = target(&plan, "npm");
    assert_eq!(npm["kind"], "task");
    assert_eq!(npm["task"], "release:publish:npm");
    assert_eq!(npm["check"], "release:check:npm");
    assert_eq!(npm["verify"], "release:verify:npm");

    // A non-`tag` target turns the §10 ruling 3 default approval on, but this
    // fixture declares it, so it is not the default.
    assert_eq!(
        plan["approvals"]["checkpoints"],
        serde_json::json!(["publish"])
    );
    assert_eq!(plan["approvals"]["defaulted"], false);
}

#[test]
fn digest_identity_fixture_has_no_version_and_needs_no_changelog() {
    let root = workspace("digest-target");
    let plan = release_plan(&root);

    // The §4.3 fixture has no `changelog` key and no CHANGELOG.md at all. A
    // release with no `semver` identity never reads one (§4.3), so that is not
    // a blocker.
    assert!(!root.join("CHANGELOG.md").exists());
    assert_eq!(plan["ready"], true, "blockers: {}", plan["blockers"]);
    assert_eq!(
        plan["blockers"],
        serde_json::json!([]),
        "a digest release has nothing to bump or validate"
    );
    assert_eq!(plan["changelog"]["used"], false);
    assert_eq!(plan["changelog"]["valid"], true);
    assert_eq!(plan["changelog"]["diagnostics"], serde_json::json!([]));

    let site = identity(&plan, "site");
    assert_eq!(site["kind"], "digest");
    assert_eq!(site["current_version"], Value::Null);
    assert_eq!(site["next_version"], Value::Null);
    assert_eq!(site["tag"], Value::Null);

    assert_eq!(plan["artifacts"][0]["name"], "site");
    assert_eq!(plan["artifacts"][0]["path"], "dist/");
    assert_eq!(plan["artifacts"][0]["identity_kind"], "digest");

    let production = target(&plan, "production");
    assert_eq!(production["kind"], "task");
    assert_eq!(production["task"], "deploy:production");
    assert_eq!(production["verify"], "deploy:health");

    // Nothing to write: no version file, and no changelog role.
    assert_eq!(plan["files"], serde_json::json!([]));

    // Approval before publish is on by default for a non-`tag` target.
    assert_eq!(
        plan["approvals"]["checkpoints"],
        serde_json::json!(["publish"])
    );
    assert_eq!(plan["approvals"]["defaulted"], true);

    // Status reports the same identity without inventing a version.
    let status = release_status(&root);
    assert_eq!(status["ready"], true);
    assert_eq!(status["current_version"], Value::Null);
    assert_eq!(status["next_version"], Value::Null);
    assert_eq!(status["identities"][0]["kind"], "digest");
    assert_eq!(status["changelog"]["used"], false);
}

#[test]
fn digest_release_does_not_parse_an_unparseable_changelog() {
    // A release with no `semver` identity never reads the changelog, so a
    // malformed one cannot block it. The file exists here on purpose.
    let root = workspace("digest-target");
    fs::write(root.join("CHANGELOG.md"), "this is not a changelog\n\n### ")
        .expect("write malformed changelog");

    let plan = release_plan(&root);
    assert_eq!(plan["ready"], true, "blockers: {}", plan["blockers"]);
    assert_eq!(plan["changelog"]["used"], false);
    assert_eq!(plan["changelog"]["diagnostics"], serde_json::json!([]));
    assert_eq!(plan["blockers"], serde_json::json!([]));
}

#[test]
fn source_release_identity_is_the_commit_and_needs_no_changelog() {
    let root = temp_workspace("release-platform-source");
    fs::write(
        root.join("effigy.toml"),
        "[release]\n\n[release.identities.build]\nkind = \"source\"\n\n[release.targets.production]\nkind = \"task\"\ntask = \"deploy:production\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "source fixture");
    let head = git_stdout(&root, &["rev-parse", "HEAD"]);
    assert!(!root.join("CHANGELOG.md").exists());

    let plan = release_plan(&root);
    assert_eq!(plan["ready"], true, "blockers: {}", plan["blockers"]);
    assert_eq!(plan["changelog"]["used"], false);
    let build = identity(&plan, "build");
    assert_eq!(build["kind"], "source");
    assert_eq!(build["identity"], head.as_str());
    assert_eq!(build["next_version"], Value::Null);
    assert_eq!(plan["files"], serde_json::json!([]));
}

#[test]
fn semver_release_still_requires_its_changelog() {
    let root = workspace("shorthand");
    fs::remove_file(root.join("CHANGELOG.md")).expect("remove changelog");

    let output = run_json_cli_command(&root, &["release", "--plan"]);
    let parsed = parse_stdout_json(&output);
    assert!(!output.status.success(), "{parsed}");
    let message = parsed["error"]["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("release changelog path does not exist"),
        "got: {message}"
    );
}

#[test]
fn longhorn_release_table_keeps_working_through_the_shorthand() {
    let root = workspace("longhorn");
    let plan = release_plan(&root);

    assert_eq!(plan["config_shape"], "single-identity-shorthand");
    assert_eq!(
        plan["blockers"],
        serde_json::json!([]),
        "Longhorn's table must resolve with no blockers"
    );

    let declared = identity(&plan, &shorthand_identity_name(&root));
    assert_eq!(declared["kind"], "semver");
    assert_eq!(declared["current_version"], "0.3.2");
    // `initial-tag-current-version` with no changelog history tags the current
    // version instead of bumping it.
    assert_eq!(declared["next_version"], "0.3.2");
    assert_eq!(declared["tag"], "v0.3.2");

    // Seven real gates, all in `certify` because the table declares no stage.
    let gates = plan["gates"].as_array().expect("gates");
    assert_eq!(gates.len(), 7);
    assert!(gates.iter().all(|gate| gate["stage"] == "certify"));

    let files = plan["files"].as_array().expect("files");
    assert!(files.iter().any(|file| file
        .as_str()
        .is_some_and(|path| path.ends_with("Cargo.lock"))));
    assert_eq!(plan["ready"], true);
}

#[test]
fn poodle_release_table_keeps_working_through_the_shorthand() {
    let root = workspace("poodle");
    let plan = release_plan(&root);

    let declared = identity(&plan, &shorthand_identity_name(&root));
    assert_eq!(declared["kind"], "semver");
    assert_eq!(declared["current_version"], "0.4.0");
    // `pre-1-0 = true`: a fixed-only changelog still takes a patch.
    assert_eq!(declared["next_version"], "0.4.1");
    assert_eq!(declared["bump"], "patch");
    assert_eq!(declared["tag"], "v0.4.1");

    // Poodle declares one detailed gate, which keeps its command, description
    // and hosted-evidence setting.
    let gates = plan["gates"].as_array().expect("gates");
    assert_eq!(gates.len(), 1);
    assert_eq!(gates[0]["name"], "headless");
    assert_eq!(gates[0]["command"], "effigy release:web-certificate");
    assert_eq!(
        gates[0]["description"],
        "Build, pack and source-free-certify the npm web archive set"
    );
    assert_eq!(plan["ready"], true);
}

#[test]
fn underlay_release_table_keeps_working_through_the_shorthand() {
    let root = workspace("underlay");
    let plan = release_plan(&root);

    let declared = identity(&plan, &shorthand_identity_name(&root));
    assert_eq!(declared["current_version"], "0.6.1");
    assert_eq!(declared["next_version"], "0.6.2");
    assert_eq!(declared["tag"], "v0.6.2");

    // Underlay's version lives under `[workspace.package]`.
    assert!(declared["version_file"]
        .as_str()
        .expect("version file")
        .ends_with("Cargo.toml"));

    // Its two sync files both resolve, alongside the version file and changelog.
    let files = plan["files"].as_array().expect("files");
    assert_eq!(files.len(), 4, "got: {files:?}");
    assert!(files.iter().any(|file| file
        .as_str()
        .is_some_and(|path| path.ends_with("Cargo.lock"))));
    assert!(files.iter().any(|file| file
        .as_str()
        .is_some_and(|path| path.ends_with("package.json"))));

    let gates = plan["gates"].as_array().expect("gates");
    assert_eq!(gates.len(), 4);
    assert_eq!(plan["ready"], true);
}

#[test]
fn retired_prepare_and_execute_name_the_run_that_replaces_them() {
    let root = workspace("named-identities");
    for retired in ["prepare", "execute"] {
        let output = run_json_cli_command(&root, &["release", retired]);
        let parsed = parse_stdout_json(&output);
        assert!(!output.status.success(), "{parsed}");
        let message = parsed["error"]["message"].as_str().unwrap_or_default();
        assert!(message.contains("retired in v0.15"), "got: {message}");
        assert!(message.contains("effigy release run"), "got: {message}");
    }

    let output = run_json_cli_command(&root, &["release", "resume"]);
    let parsed = parse_stdout_json(&output);
    assert!(!output.status.success(), "{parsed}");
    let message = parsed["error"]["message"].as_str().unwrap_or_default();
    assert!(message.contains("release Run id"), "got: {message}");
}

/// A release project whose gates and artifact task really run.
fn writable_release_project(root: &Path) {
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("Cargo.toml");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\n## [Unreleased]\n\n### Added\n- A release Run.\n\n## [0.2.4] - 2026-10-01\n\n### Fixed\n- A previous release.\n",
    )
    .expect("changelog");
    fs::write(root.join(".gitignore"), "dist/\n").expect("gitignore");
    fs::write(
        root.join("effigy.toml"),
        r#"[tasks."release:pack"]
run = "mkdir -p dist && printf certified-artifact > dist/app-0.2.5.tgz"

[release]
changelog = "CHANGELOG.md"

[release.identities.app]
version-file = "Cargo.toml"
version-path = "package.version"
tag-format = "v{version}"

[release.gates]
certify = "sh -c 'grep -q \"version = \\\"0.2.5\\\"\" Cargo.toml'"

[release.artifacts]
app = { task = "release:pack", path = "dist/app-*.tgz" }

[release.targets.tags]
kind = "tag"
"#,
    )
    .expect("effigy.toml");
}

fn run_release_cli(root: &Path, home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("--repo")
        .arg(root)
        .args(args)
        .env("HOME", home)
        .env("NO_COLOR", "1")
        // A gate builds the fixture with plain Cargo, so its output belongs in
        // the fixture, not in whatever target directory the test host set.
        .env("CARGO_TARGET_DIR", root.join("target"))
        .output()
        .expect("run effigy")
}

#[test]
fn release_run_writes_a_receipt_and_pushes_the_candidate_tag() {
    let root = temp_workspace("release-run");
    writable_release_project(&root);
    init_git_repo(&root);
    git_commit_all(&root, "fixture");
    let remote = attach_bare_remote(&root);
    let branch = git_stdout(&root, &["symbolic-ref", "--short", "HEAD"]);
    let home = temp_workspace("release-run-home");

    let output = run_release_cli(&root, &home, &["release", "run"]);
    let parsed = parse_stdout_json(&output);
    assert!(
        output.status.success(),
        "release run failed: {}",
        serde_json::to_string_pretty(&parsed).expect("serialize")
    );
    let result = &parsed["result"];
    assert_eq!(result["schema"], "effigy.release.run.v1");
    assert_eq!(result["receipt"]["schema"], "effigy.release.receipt.v1");
    assert_eq!(result["receipt"]["release"]["status"], "complete");
    assert_eq!(result["receipt"]["published"], true);

    let candidate = result["receipt"]["candidate"]["commit"]
        .as_str()
        .expect("candidate commit")
        .to_owned();
    assert_eq!(git_stdout(&root, &["rev-parse", "HEAD"]), candidate);
    assert!(fs::read_to_string(root.join("Cargo.toml"))
        .expect("Cargo.toml")
        .contains("version = \"0.2.5\""));
    let tag = git_stdout(&remote, &["rev-parse", "refs/tags/v0.2.5^{commit}"]);
    assert_eq!(tag, candidate);
    assert_eq!(
        git_stdout(&remote, &["rev-parse", &format!("refs/heads/{branch}")]),
        candidate
    );

    // The receipt belongs to this checkout’s ignored runtime journal.
    let receipt_path = result["receipt_path"].as_str().expect("receipt path");
    assert!(
        Path::new(receipt_path).starts_with(root.join(".effigy/state/release")),
        "{receipt_path}"
    );
    assert!(Path::new(receipt_path).exists());
    assert!(!root.join(".release-prepared.json").exists());

    let run_id = result["receipt"]["release"]["id"]
        .as_str()
        .expect("release id")
        .to_owned();
    let status = run_release_cli(&root, &home, &["release", "status"]);
    let status = parse_stdout_json(&status);
    // A released tree has no further version to propose, so status reports a
    // blocked next release in its error detail; the Run summary is there too.
    let status = if status["result"].is_null() {
        status["error"]["details"].clone()
    } else {
        status["result"].clone()
    };
    assert_eq!(status["run"]["id"], run_id.as_str());
    assert_eq!(status["run"]["status"], "complete");
    assert_eq!(status["run"]["active"], false);

    let receipt = run_release_cli(&root, &home, &["release", "receipt", &run_id]);
    let receipt = parse_stdout_json(&receipt);
    assert_eq!(receipt["result"]["schema"], "effigy.release.receipt.v1");
    assert_eq!(receipt["result"]["candidate"]["commit"], candidate.as_str());
}

/// The parsed on-disk `receipt.json` of a Run.
fn receipt_file(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).expect("read receipt file"))
        .expect("parse receipt file")
}

/// The parsed payload `effigy release receipt <run> --json` prints.
fn release_receipt_payload(root: &Path, home: &Path, run: &str) -> Value {
    let output = run_release_cli(root, home, &["release", "receipt", run]);
    let parsed = parse_stdout_json(&output);
    assert!(
        output.status.success(),
        "release receipt {run} failed: {parsed}"
    );
    parsed["result"].clone()
}

/// The parsed `release status` payload, whether it arrived as a result or as
/// the error detail a blocked next release reports through.
fn release_status_payload(root: &Path, home: &Path) -> Value {
    let output = run_release_cli(root, home, &["release", "status"]);
    let parsed = parse_stdout_json(&output);
    if parsed["result"].is_null() {
        parsed["error"]["details"].clone()
    } else {
        parsed["result"].clone()
    }
}

/// Hand the completed record to an invocation that never reaches settlement,
/// exactly as a process that stops after the final durable checkpoint leaves
/// it. Returns the release's id.
fn interrupted_after_closeout(root: &Path, home: &Path, receipt_path: &Path) -> String {
    let record_path = receipt_path.with_file_name("record.json");
    let mut record: effigy_release::ReleaseRecord =
        serde_json::from_slice(&fs::read(&record_path).expect("read record file"))
            .expect("parse record file");
    let session = effigy_release::ReleaseSession::open(home, root).expect("release session");
    let state = session.state().expect("release state");
    let id = effigy_release::new_run_id().expect("release run id");
    let mut adapter = effigy_release::ReportsOnly;
    let mut run = session.begin(id, &mut adapter).expect("begin release run");
    record.run_id = id.to_string();
    state.write_record(&record, &mut run).expect("write record");
    state
        .write_receipt(&record, &mut run)
        .expect("write receipt");
    run.set_engine_record(
        &serde_json::to_value(record.checkpoint(&state.record_path)).expect("checkpoint json"),
    )
    .expect("set checkpoint pointer");
    drop(run);
    record.release_id
}

/// The brief's required proof: the real CLI's `release receipt <run> --json`
/// parses exactly equal to the on-disk `receipt.json`, including the final
/// stage status and every checkpoint, and stays equal while settlement is
/// pending because interruption is a `release status` observation, never a
/// rewrite of the recorded stage outcome.
#[test]
fn release_receipt_command_matches_the_receipt_file() {
    let root = temp_workspace("release-receipt-command");
    writable_release_project(&root);
    init_git_repo(&root);
    git_commit_all(&root, "fixture");
    attach_bare_remote(&root);
    let home = temp_workspace("release-receipt-command-home");

    let output = run_release_cli(&root, &home, &["release", "run"]);
    let parsed = parse_stdout_json(&output);
    assert!(
        output.status.success(),
        "release run failed: {}",
        serde_json::to_string_pretty(&parsed).expect("serialize")
    );
    let result = &parsed["result"];
    let receipt_path = PathBuf::from(result["receipt_path"].as_str().expect("receipt path"));
    let release_id = result["receipt"]["release"]["id"]
        .as_str()
        .expect("release id")
        .to_owned();

    // A completed closeout: the command and the file agree on the final stage
    // status and all seven checkpoints.
    let complete = release_receipt_payload(&root, &home, &release_id);
    assert_eq!(
        receipt_file(&receipt_path),
        complete,
        "receipt.json must equal `release receipt {release_id} --json`"
    );
    assert_eq!(complete["release"]["status"], "complete");
    let stages = complete["checkpoints"]
        .as_array()
        .expect("checkpoints")
        .iter()
        .map(|checkpoint| checkpoint["stage"].as_str().unwrap_or_default().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        stages,
        [
            "validate",
            "pre-gates",
            "prepare",
            "certify",
            "publish",
            "verify",
            "closeout"
        ]
    );

    // Interruption after the final durable checkpoint. The record and its
    // receipt already hold the completed stage outcome; only the journal is
    // unsettled, so `release status` reports the settlement and the bounded
    // action while both receipt surfaces stay complete.
    assert_eq!(
        interrupted_after_closeout(&root, &home, &receipt_path),
        release_id
    );
    let status = release_status_payload(&root, &home);
    assert_eq!(status["run"]["status"], "complete");
    assert_eq!(status["run"]["settlement"], "pending", "{status}");
    assert_eq!(status["run"]["active"], true, "{status}");

    let pending = release_receipt_payload(&root, &home, &release_id);
    assert_eq!(pending["release"]["status"], "complete");
    assert_eq!(pending["checkpoints"].as_array().map(Vec::len), Some(7));
    assert_eq!(
        receipt_file(&receipt_path),
        pending,
        "an unsettled invocation must not rewrite either receipt surface"
    );

    // `release settle <run>` settles the journal only: the receipt file is
    // byte-identical and the settlement observation is gone.
    let settled = run_release_cli(&root, &home, &["release", "settle", &release_id]);
    assert!(
        settled.status.success(),
        "release settle failed: {}",
        parse_stdout_json(&settled)
    );
    assert_eq!(receipt_file(&receipt_path), pending);
    let status = release_status_payload(&root, &home);
    assert!(status["run"]["settlement"].is_null(), "{status}");
}

#[test]
fn release_run_refuses_a_dirty_tree_and_names_the_run_for_an_unknown_id() {
    let root = temp_workspace("release-run-dirty");
    writable_release_project(&root);
    init_git_repo(&root);
    git_commit_all(&root, "fixture");
    attach_bare_remote(&root);
    let home = temp_workspace("release-run-dirty-home");
    fs::write(root.join("pending.txt"), "pending\n").expect("pending file");

    let output = run_release_cli(&root, &home, &["release", "run"]);
    let parsed = parse_stdout_json(&output);
    assert!(!output.status.success(), "{parsed}");
    // A failed Run still prints its receipt payload as the error detail.
    let details = &parsed["error"]["details"];
    assert_eq!(details["schema"], "effigy.release.run.v1");
    let errors = details["receipt"]["errors"]
        .as_array()
        .expect("receipt errors")
        .iter()
        .filter_map(|error| error.as_str())
        .collect::<Vec<_>>()
        .join("; ");
    assert!(errors.contains("clean working tree"), "got: {errors}");

    // A failed Run keeps its record, so an unknown id names the active one.
    let output = run_release_cli(&root, &home, &["release", "resume", "not-a-run"]);
    let parsed = parse_stdout_json(&output);
    assert!(!output.status.success(), "{parsed}");
    let message = parsed["error"]["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("not this checkout's release Run"),
        "got: {message}"
    );
    assert!(message.contains("release Run is"), "got: {message}");
}

/// A release project whose only destination is a project task that reports an
/// unknown publish outcome, so the Run needs an operator decision (054 §7).
fn decision_release_project(root: &Path) {
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("Cargo.toml");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\n## [Unreleased]\n\n### Added\n- A release Run.\n\n## [0.2.4] - 2026-10-01\n\n### Fixed\n- A previous release.\n",
    )
    .expect("changelog");
    fs::write(
        root.join("publish.sh"),
        "#!/bin/sh\nprintf '{\"schema\":\"effigy.release.output.v1\",\"committed\":\"unknown\"}' > \"$EFFIGY_RELEASE_OUTPUT\"\n",
    )
    .expect("publish script");
    fs::write(
        root.join("effigy.toml"),
        r#"[tasks."release:publish:fake"]
run = "sh publish.sh"

[release]
changelog = "CHANGELOG.md"

[release.identities.app]
version-file = "Cargo.toml"
version-path = "package.version"
tag-format = "v{version}"

[release.targets.fake]
kind = "task"
task = "release:publish:fake"
"#,
    )
    .expect("effigy.toml");
}

#[test]
fn release_decide_records_an_unknown_publish_outcome_and_continues() {
    let root = temp_workspace("release-decide");
    decision_release_project(&root);
    init_git_repo(&root);
    git_commit_all(&root, "fixture");
    attach_bare_remote(&root);
    let home = temp_workspace("release-decide-home");

    // A non-`tag` target turns the publish approval checkpoint on (054 §10).
    let paused = run_release_cli(&root, &home, &["release", "run"]);
    let paused = parse_stdout_json(&paused);
    assert!(
        paused["result"]["receipt"]["release"]["status"] == "awaiting-approval",
        "{}",
        serde_json::to_string_pretty(&paused).expect("serialize")
    );
    let run_id = paused["result"]["receipt"]["release"]["id"]
        .as_str()
        .expect("release id")
        .to_owned();

    // Without a `check` task the unknown outcome stops the Run for an
    // operator ruling in the same invocation, not as a failure first.
    let waiting = run_release_cli(&root, &home, &["release", "approve", &run_id]);
    let waiting = parse_stdout_json(&waiting);
    assert!(
        waiting["result"]["receipt"]["release"]["status"] == "awaiting-decision",
        "{}",
        serde_json::to_string_pretty(&waiting).expect("serialize")
    );
    assert_eq!(
        waiting["result"]["receipt"]["targets"][0]["state"],
        "unknown"
    );

    let decided = run_release_cli(&root, &home, &["release", "decide", &run_id, "--published"]);
    let decided = parse_stdout_json(&decided);
    assert!(
        decided["result"]["receipt"]["release"]["status"] == "complete",
        "{}",
        serde_json::to_string_pretty(&decided).expect("serialize")
    );
    let receipt = &decided["result"]["receipt"];
    assert_eq!(receipt["published"], true);
    assert_eq!(receipt["decisions"][0]["target"], "fake");
    assert_eq!(receipt["decisions"][0]["decision"], "published");
}

/// Effigy's own release, on contract 054 §4.4.
///
/// The fixture copies this repository's `config/release.toml` **byte for
/// byte**, so the identity, the gate names and stages, the artifact and both
/// targets under test are Effigy's real declaration. Only what a release
/// machine cannot reproduce in a temporary workspace is stubbed, and each stub
/// is a file the config already names:
///
/// - `scripts/check-release-ci.sh` exits 0 instead of asking `gh` for a hosted
///   run on this exact commit;
/// - `scripts/release/secret-audit.sh` records the pinned arguments instead of
///   scanning the real release lineage, which a synthetic repository lacks;
/// - the fixture's `build:release`, which the real `release:artifact` task
///   composes, builds the fixture's own tiny `effigy` binary, so certify really
///   produces the `target/release/effigy` artifact the config declares;
/// - `release:publish:binaries` is the fake dispatch: it reads the candidate at
///   `$EFFIGY_RELEASE_INPUT` and records the tag, and never calls `gh`.
///
/// Everything else is the real repository code: `scripts/release/checks.sh`
/// runs the real `release:metadata` and `release:preflight` task gates, and
/// `.github/workflows/release-binaries.yml` is the real file the metadata check
/// reads. Nothing pushes to GitHub, tags a real release or dispatches a
/// workflow: the only remote is a local bare repository.
fn effigy_release_project(root: &Path, home: &Path) {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    fs::create_dir_all(root.join("config")).expect("config dir");
    fs::create_dir_all(root.join("scripts/release")).expect("scripts dir");
    fs::create_dir_all(root.join("src")).expect("src dir");
    fs::create_dir_all(root.join("docs/knowledge/contracts")).expect("docs dir");
    fs::create_dir_all(root.join(".github/workflows")).expect("workflow dir");

    for (source, destination) in [
        ("config/release.toml", "config/release.toml"),
        (
            ".github/workflows/release-binaries.yml",
            ".github/workflows/release-binaries.yml",
        ),
        ("scripts/release/checks.sh", "scripts/release/checks.sh"),
        ("scripts/release-assets.sh", "scripts/release-assets.sh"),
    ] {
        fs::copy(repo_root.join(source), root.join(destination))
            .unwrap_or_else(|error| panic!("copy {source}: {error}"));
    }

    fs::write(
        root.join("effigy.toml"),
        r#"[manifest]
include = ["config/release.toml"]

[tasks]
"qa:ci" = { run = "printf qa-ci" }
"qa:docs" = { run = "printf qa-docs" }
"smoke:release" = { run = "printf smoke-release" }
"build:release" = { run = "cargo build --release --bin effigy" }
"release:artifact" = { task = "build:release" }
"release:metadata" = { run = "sh scripts/release/checks.sh metadata {args}" }
"release:preflight" = { run = "sh scripts/release/checks.sh preflight {args}" }
"release:publish:binaries" = { run = "sh fake-dispatch.sh" }
"release:check:binaries" = { run = "sh fake-check.sh" }
"release:verify:binaries" = { run = "sh fake-verify.sh" }
"#,
    )
    .expect("fixture manifest");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"effigy\"\nversion.workspace = true\nedition = \"2021\"\n\n[workspace]\nmembers = []\n\n[workspace.package]\nversion = \"0.2.4\"\n",
    )
    .expect("fixture cargo manifest");
    fs::write(
        root.join("Cargo.lock"),
        "# This file is automatically @generated by Cargo.\n# It is not intended for manual editing.\nversion = 4\n\n[[package]]\nname = \"effigy\"\nversion = \"0.2.4\"\n",
    )
    .expect("fixture lockfile");
    fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("fixture binary");
    fs::write(root.join(".gitignore"), "target/\n").expect("fixture gitignore");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\n## [Unreleased]\n\n### Added\n- A release Run.\n\n## [0.2.4] - 2026-10-01\n\n### Fixed\n- A previous release.\n",
    )
    .expect("fixture changelog");
    fs::write(root.join("README.md"), "# Effigy\n").expect("fixture readme");
    for record in [
        "docs/knowledge/contracts/release.md",
        "docs/knowledge/contracts/054-release-platform-contract.md",
    ] {
        fs::write(root.join(record), "# Release record\n").expect("fixture release record");
    }
    fs::write(
        root.join("scripts/check-release-ci.sh"),
        "#!/bin/sh\n# fixture: the hosted CI probe needs gh and a real dispatch\necho 'CI is green for candidate commit fixture'\n",
    )
    .expect("fixture CI gate");
    fs::write(
        root.join("scripts/release/secret-audit.sh"),
        "#!/bin/sh\nset -eu\nprintf '%s\\n' \"$@\" > \"$HOME/secret-audit-invoked\"\n",
    )
    .expect("fixture secret audit gate");
    fs::write(
        root.join("fake-dispatch.sh"),
        r#"#!/bin/sh
set -eu
tag=$(jq -r '.identities[0].tag' "$EFFIGY_RELEASE_INPUT")
candidate=$(jq -r '.candidate_commit' "$EFFIGY_RELEASE_INPUT")
printf '%s %s\n' "$tag" "$candidate" >> dispatched.txt
jq -n --arg d "workflow:release-binaries.yml@$tag" \
  '{schema: "effigy.release.output.v1", schema_version: 1, committed: true, destinations: [$d]}' \
  > "$EFFIGY_RELEASE_OUTPUT"
"#,
    )
    .expect("fixture dispatch");
    fs::write(
        root.join("fake-check.sh"),
        r#"#!/bin/sh
printf '%s\n' '{"schema":"effigy.release.output.v1","schema_version":1,"committed":false}' > "$EFFIGY_RELEASE_OUTPUT"
"#,
    )
    .expect("fixture check");
    fs::write(
        root.join("fake-verify.sh"),
        "#!/bin/sh\nprintf 'verified %s\\n' \"$(jq -r '.identities[0].tag' \"$EFFIGY_RELEASE_INPUT\")\"\n",
    )
    .expect("fixture verify");

    // The real config wires two gates as `task = ...`, which the release
    // runner executes as `effigy <task>`. A private HOME whose login profile
    // puts a symlink to this test binary first on PATH makes that the same
    // binary that started the Run.
    let bin = home.join("bin");
    fs::create_dir_all(&bin).expect("fixture bin");
    // A wrapper, not a symlink: Effigy refuses to identify itself through a
    // symlinked path, and `exec` reaches the real binary.
    let wrapper = bin.join("effigy");
    fs::write(
        &wrapper,
        format!(
            "#!/bin/sh\nexec \"{}\" \"$@\"\n",
            env!("CARGO_BIN_EXE_effigy")
        ),
    )
    .expect("fixture effigy wrapper");
    let mut permissions = fs::metadata(&wrapper)
        .expect("wrapper metadata")
        .permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
    fs::set_permissions(&wrapper, permissions).expect("fixture effigy wrapper executable");
    // The release runner executes a gate as `<shell> -lc`, and the login
    // shell is zsh on macOS and bash elsewhere, so every login profile the
    // fixture HOME can see puts the symlink first.
    let path_line = format!("PATH=\"{}:$PATH\"\nexport PATH\n", bin.display());
    for profile in [".profile", ".bash_profile", ".zprofile", ".zshrc"] {
        fs::write(home.join(profile), &path_line).expect("fixture login profile");
    }
}

#[test]
fn effigy_release_plan_reports_the_contract_054_declaration() {
    let home = temp_workspace("effigy-release-plan-home");
    let root = temp_workspace("effigy-release-plan");
    effigy_release_project(&root, &home);
    init_git_repo(&root);
    git_commit_all(&root, "fixture");

    let plan = release_plan(&root);
    let identities = plan["identities"].as_array().expect("identities");
    assert_eq!(identities.len(), 1, "{plan}");
    assert_eq!(identities[0]["name"], "effigy");
    assert_eq!(identities[0]["kind"], "semver");
    assert_eq!(identities[0]["tag"], "v0.2.5");

    let gates = plan["gates"].as_array().expect("gates");
    let described = gates
        .iter()
        .map(|gate| {
            (
                gate["name"].as_str().unwrap_or_default().to_owned(),
                gate["stage"].as_str().unwrap_or_default().to_owned(),
                gate["task"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        described,
        vec![
            (
                "preflight".to_owned(),
                "pre-gates".to_owned(),
                String::new()
            ),
            (
                "secret-audit".to_owned(),
                "certify".to_owned(),
                String::new()
            ),
            ("ci".to_owned(), "certify".to_owned(), String::new()),
            ("format".to_owned(), "certify".to_owned(), String::new()),
            ("test".to_owned(), "certify".to_owned(), String::new()),
            ("qa".to_owned(), "certify".to_owned(), String::new()),
            ("build".to_owned(), "certify".to_owned(), String::new()),
            ("smoke".to_owned(), "certify".to_owned(), String::new()),
            (
                "metadata".to_owned(),
                "certify".to_owned(),
                "release:metadata".to_owned()
            ),
        ]
    );

    // The pre gate runs the repo script with `--skip-smoke`; the certify smoke
    // gate is what proves the prepared binary.
    let preflight = &gates[0];
    assert_eq!(
        preflight["command"],
        "sh scripts/release/checks.sh preflight --skip-smoke"
    );
    assert_eq!(
        gates[1]["command"],
        "sh scripts/release/secret-audit.sh --base v0.14.1 --base-sha 20465f1ff690340ee9ef461915ca769da1c76fca"
    );

    let artifacts = plan["artifacts"].as_array().expect("artifacts");
    assert_eq!(artifacts.len(), 1, "{plan}");
    assert_eq!(artifacts[0]["name"], "binary");
    assert_eq!(artifacts[0]["task"], "release:artifact");
    assert_eq!(artifacts[0]["path"], "target/release/effigy");

    let targets = plan["targets"].as_array().expect("targets");
    let described = targets
        .iter()
        .map(|target| {
            (
                target["name"].as_str().unwrap_or_default().to_owned(),
                target["kind"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        described,
        vec![
            ("tag".to_owned(), "tag".to_owned()),
            ("binaries".to_owned(), "task".to_owned())
        ]
    );
    assert_eq!(targets[1]["task"], "release:publish:binaries");
    assert_eq!(targets[1]["check"], "release:check:binaries");
    assert_eq!(targets[1]["verify"], "release:verify:binaries");

    // §10 ruling 3 turns publish into an approval checkpoint because the
    // release has a non-`tag` target.
    assert_eq!(
        plan["approvals"]["checkpoints"],
        serde_json::json!(["publish"])
    );
    assert_eq!(plan["approvals"]["defaulted"], true);
}

#[test]
fn effigy_release_run_certifies_and_then_publishes_through_the_fake_dispatch() {
    let home = temp_workspace("effigy-release-run-home");
    let root = temp_workspace("effigy-release-run");
    effigy_release_project(&root, &home);
    init_git_repo(&root);
    git_commit_all(&root, "fixture");
    let remote = attach_bare_remote(&root);
    let branch = git_stdout(&root, &["symbolic-ref", "--short", "HEAD"]);

    // Certify only: the prepared commit and the certified artifact, nothing
    // published and no workflow dispatched.
    let certified = run_release_cli(&root, &home, &["release", "run", "--to", "certify"]);
    let certified = parse_stdout_json(&certified);
    assert!(
        certified["result"]["receipt"]["release"]["status"] == "paused",
        "{}",
        serde_json::to_string_pretty(&certified).expect("serialize")
    );
    let receipt = &certified["result"]["receipt"];
    assert_eq!(receipt["published"], false);
    assert_eq!(
        fs::read_to_string(home.join("secret-audit-invoked")).expect("secret audit ran"),
        "--base\nv0.14.1\n--base-sha\n20465f1ff690340ee9ef461915ca769da1c76fca\n"
    );
    let artifact = &receipt["candidate"]["artifacts"][0];
    assert_eq!(artifact["name"], "binary");
    assert_eq!(artifact["pattern"], "target/release/effigy");
    assert_eq!(artifact["task"], "release:artifact");
    let copied = artifact["files"][0]["path"]
        .as_str()
        .expect("artifact path");
    assert!(copied.ends_with("candidate/binary/effigy"), "{copied}");
    let digest = artifact["files"][0]["sha256"]
        .as_str()
        .expect("artifact digest");
    assert_eq!(digest.len(), 64, "{digest}");
    assert!(fs::read_to_string(root.join("Cargo.toml"))
        .expect("Cargo.toml")
        .contains("version = \"0.2.5\""));
    assert!(fs::read_to_string(root.join("Cargo.lock"))
        .expect("Cargo.lock")
        .contains("version = \"0.2.5\""));
    assert!(
        !root.join("dispatched.txt").exists(),
        "certify dispatched nothing"
    );
    assert!(
        Command::new("git")
            .arg("-C")
            .arg(&remote)
            .args(["rev-parse", "--verify", "refs/tags/v0.2.5"])
            .output()
            .expect("read remote tag")
            .status
            .code()
            .is_some_and(|code| code != 0),
        "certify tagged nothing"
    );

    // Continuing stops at the publish approval checkpoint.
    let paused = run_release_cli(&root, &home, &["release", "run"]);
    let paused = parse_stdout_json(&paused);
    assert!(
        paused["result"]["receipt"]["release"]["status"] == "awaiting-approval",
        "{}",
        serde_json::to_string_pretty(&paused).expect("serialize")
    );
    assert_eq!(
        paused["result"]["receipt"]["release"]["waiting_on"],
        "publish"
    );
    assert!(
        !root.join("dispatched.txt").exists(),
        "publish waits for approval"
    );

    // Approving publishes the tag, dispatches through the task target, runs
    // the verify task and closes out against the local bare remote.
    let run_id = paused["result"]["receipt"]["release"]["id"]
        .as_str()
        .expect("release id")
        .to_owned();
    let complete = run_release_cli(&root, &home, &["release", "approve", &run_id]);
    let complete = parse_stdout_json(&complete);
    assert!(
        complete["result"]["receipt"]["release"]["status"] == "complete",
        "{}",
        serde_json::to_string_pretty(&complete).expect("serialize")
    );
    let receipt = &complete["result"]["receipt"];
    assert_eq!(receipt["published"], true);
    let candidate = receipt["candidate"]["commit"]
        .as_str()
        .expect("candidate commit");
    assert_eq!(
        git_stdout(&remote, &["rev-parse", "refs/tags/v0.2.5^{commit}"]),
        candidate
    );
    assert_eq!(
        git_stdout(&remote, &["rev-parse", &format!("refs/heads/{branch}")]),
        candidate
    );

    let dispatched = fs::read_to_string(root.join("dispatched.txt")).expect("dispatched tag");
    assert!(
        dispatched.starts_with("v0.2.5 "),
        "the task target received the candidate tag: {dispatched}"
    );
    let binaries = receipt["targets"]
        .as_array()
        .expect("targets")
        .iter()
        .find(|target| target["name"] == "binaries")
        .expect("binaries target");
    assert_eq!(binaries["state"], "published");
    assert_eq!(
        binaries["destinations"][0]["destination"],
        "workflow:release-binaries.yml@v0.2.5"
    );
    let verification = receipt["verification"]
        .as_array()
        .expect("verifications")
        .iter()
        .find(|entry| entry["target"] == "binaries")
        .expect("binaries verification");
    assert_eq!(verification["passed"], true);
}
