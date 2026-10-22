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

use super::support::{
    git_commit_all, git_stdout, init_git_repo, parse_stdout_json, run_json_cli_command,
    temp_workspace,
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
fn named_identities_refuse_the_shorthand_only_prepare_path() {
    let root = workspace("named-identities");
    let output = run_json_cli_command(&root, &["release", "prepare", "--plan"]);
    let parsed = parse_stdout_json(&output);

    assert!(!output.status.success(), "{parsed}");
    let message = parsed["error"]["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("single-identity shorthand"),
        "got: {message}"
    );
    assert!(message.contains("effigy release --plan"), "got: {message}");
}
