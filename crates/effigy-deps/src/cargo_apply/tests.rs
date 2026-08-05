use std::cell::RefCell;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;
use tempfile::TempDir;

use super::*;
use crate::{
    plan_cargo_link, CargoPackageMatch, CargoPlanObserver, CargoWorkspaceInventory,
    CommittedSource, DependencyDepth, MatchDisposition, ProcessOutput,
};

const SOURCE: &str = "https://example.test/signal.git";

#[derive(Default)]
struct FixtureObserver;

impl CargoPlanObserver for FixtureObserver {
    fn is_tracked(&self, _repo_root: &Path, _path: &Path) -> Result<bool, DepsError> {
        Ok(false)
    }

    fn is_dirty(&self, _repo_root: &Path, _path: &Path) -> Result<bool, DepsError> {
        Ok(false)
    }
}

struct FixtureProcess {
    roots: Vec<PathBuf>,
    local_path: PathBuf,
    requests: RefCell<Vec<ProcessRequest>>,
    fail_tree: bool,
}

impl ReadOnlyProcess for FixtureProcess {
    fn run(&self, request: &ProcessRequest) -> Result<ProcessOutput, DepsError> {
        self.requests.borrow_mut().push(request.clone());
        if request.args.first().map(String::as_str) == Some("tree") {
            if self.fail_tree {
                return Err(DepsError::ProcessFailed {
                    program: "cargo".to_owned(),
                    cwd: request.cwd.clone(),
                    status: Some(1),
                    stderr: "fixture tree failure".to_owned(),
                });
            }
            return Ok(ProcessOutput {
                status: Some(0),
                stdout: format!("signal-core v0.1.0 ({})\n", self.local_path.display()),
                stderr: String::new(),
            });
        }
        let manifest = request
            .args
            .iter()
            .position(|arg| arg == "--manifest-path")
            .map(|index| PathBuf::from(&request.args[index + 1]))
            .expect("metadata manifest path");
        let root = self
            .roots
            .iter()
            .find(|root| manifest.starts_with(root))
            .expect("known fixture workspace");
        let consumer_id = format!("path+file://{}#consumer@0.1.0", root.display());
        let local_id = format!(
            "path+file://{}#signal-core@0.1.0",
            self.local_path.display()
        );
        let output = json!({
            "packages": [
                {
                    "id": consumer_id,
                    "name": "consumer",
                    "manifest_path": root.join("Cargo.toml"),
                    "source": null
                },
                {
                    "id": local_id,
                    "name": "signal-core",
                    "manifest_path": self.local_path.join("Cargo.toml"),
                    "source": null
                }
            ],
            "workspace_members": [consumer_id],
            "workspace_root": root,
            "resolve": {
                "nodes": [
                    { "id": consumer_id, "deps": [{ "pkg": local_id }] },
                    { "id": local_id, "deps": [] }
                ]
            }
        });
        Ok(ProcessOutput {
            status: Some(0),
            stdout: output.to_string(),
            stderr: String::new(),
        })
    }
}

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

fn fixture(
    nested: bool,
) -> (
    TempDir,
    TempDir,
    CargoLibraryInventory,
    Vec<CargoWorkspaceInventory>,
) {
    let repo_temp = TempDir::new().unwrap();
    let repo = fs::canonicalize(repo_temp.path()).unwrap();
    let library_temp = TempDir::new().unwrap();
    let library_root = fs::canonicalize(library_temp.path()).unwrap();
    let library_path = library_root.join("signal-core");
    write(
        &library_path.join("Cargo.toml"),
        "[package]\nname='signal-core'\nversion='0.1.0'\n",
    );
    let roots = if nested {
        vec![repo.join("apps/one"), repo.join("apps/two")]
    } else {
        vec![repo.clone()]
    };
    let library = CargoLibraryInventory {
        root: library_root,
        packages: vec![CargoPackageInventory {
            id: "local-signal-core".to_owned(),
            name: "signal-core".to_owned(),
            manifest_path: library_path.join("Cargo.toml"),
            source: None,
        }],
    };
    let workspaces = roots
        .iter()
        .map(|root| {
            write(
                &root.join("Cargo.toml"),
                &format!(
                    "[package]\nname='consumer'\nversion='0.1.0'\n[dependencies]\nsignal-core={{git='{SOURCE}'}}\n"
                ),
            );
            let package = CargoPackageInventory {
                id: format!("git-signal-core-{}", root.display()),
                name: "signal-core".to_owned(),
                manifest_path: PathBuf::from("/cargo/git/signal-core/Cargo.toml"),
                source: Some(CommittedSource {
                    kind: CommittedSourceKind::Git,
                    identity: SOURCE.to_owned(),
                }),
            };
            CargoWorkspaceInventory {
                root: root.clone(),
                workspace_packages: Vec::new(),
                resolved_packages: vec![package.clone()],
                library_matches: vec![CargoPackageMatch {
                    package,
                    depth: DependencyDepth::Direct,
                    disposition: MatchDisposition::Git,
                }],
            }
        })
        .collect();
    (repo_temp, library_temp, library, workspaces)
}

fn process_for(
    library: &CargoLibraryInventory,
    workspaces: &[CargoWorkspaceInventory],
    fail_tree: bool,
) -> FixtureProcess {
    FixtureProcess {
        roots: workspaces
            .iter()
            .map(|workspace| workspace.root.clone())
            .collect(),
        local_path: library.packages[0]
            .manifest_path
            .parent()
            .unwrap()
            .to_path_buf(),
        requests: RefCell::new(Vec::new()),
        fail_tree,
    }
}

#[test]
fn dry_run_returns_exact_plan_without_writes_or_verification_processes() {
    let (repo_temp, _library_temp, library, workspaces) = fixture(false);
    let repo = fs::canonicalize(repo_temp.path()).unwrap();
    let plan = plan_cargo_link(&repo, &library, &workspaces, true, &FixtureObserver).unwrap();
    let process = process_for(&library, &workspaces, false);

    let report = apply_cargo_link_plan(plan.clone(), &process).unwrap();

    assert_eq!(report.outcome, CargoLinkOutcome::DryRun);
    assert_eq!(report.plan, plan);
    assert_eq!(report.verification.status, VerificationStatus::NotRun);
    assert!(process.requests.borrow().is_empty());
    assert!(!repo.join(".cargo/config.toml").exists());
    assert!(!repo.join(".gitignore").exists());
    assert!(!RepoLinkStateStore::for_repo(&repo).path().exists());
}

#[test]
fn stale_precondition_is_refused_before_the_first_write() {
    let (repo_temp, _library_temp, library, workspaces) = fixture(false);
    let repo = fs::canonicalize(repo_temp.path()).unwrap();
    let plan = plan_cargo_link(&repo, &library, &workspaces, false, &FixtureObserver).unwrap();
    write(&repo.join(".gitignore"), "changed-after-plan\n");
    let process = process_for(&library, &workspaces, false);

    let error = apply_cargo_link_plan(plan, &process).unwrap_err();

    assert!(error.to_string().contains("before-state is stale"));
    assert!(!repo.join(".cargo/config.toml").exists());
    assert!(!RepoLinkStateStore::for_repo(&repo).path().exists());
    assert!(process.requests.borrow().is_empty());
}

#[test]
fn verification_failure_rolls_back_only_applied_config_and_ignore_files() {
    let (repo_temp, _library_temp, library, workspaces) = fixture(false);
    let repo = fs::canonicalize(repo_temp.path()).unwrap();
    let plan = plan_cargo_link(&repo, &library, &workspaces, false, &FixtureObserver).unwrap();
    let process = process_for(&library, &workspaces, true);

    let report = apply_cargo_link_plan(plan, &process).unwrap();

    assert_eq!(report.outcome, CargoLinkOutcome::VerificationFailed);
    assert_eq!(report.verification.status, VerificationStatus::Failed);
    assert!(report.rollback.attempted);
    assert!(report.rollback.failures.is_empty());
    assert!(!repo.join(".cargo/config.toml").exists());
    assert!(!repo.join(".cargo").exists());
    assert!(!repo.join(".gitignore").exists());
    assert!(!RepoLinkStateStore::for_repo(&repo).path().exists());
}

#[test]
fn successful_nested_apply_verifies_each_workspace_then_persists_the_ledger() {
    let (repo_temp, _library_temp, library, workspaces) = fixture(true);
    let repo = fs::canonicalize(repo_temp.path()).unwrap();
    let plan = plan_cargo_link(&repo, &library, &workspaces, false, &FixtureObserver).unwrap();
    let process = process_for(&library, &workspaces, false);

    let report = apply_cargo_link_plan(plan, &process).unwrap();

    assert_eq!(report.outcome, CargoLinkOutcome::Applied);
    assert_eq!(report.verification.status, VerificationStatus::Passed);
    assert_eq!(report.verification.evidence.len(), 2);
    assert!(report
        .verification
        .evidence
        .iter()
        .all(|evidence| evidence.consumer_root.is_some()));
    assert!(RepoLinkStateStore::for_repo(&repo).path().exists());
    assert!(repo.join(".cargo/config.toml").exists());
    assert_eq!(
        process
            .requests
            .borrow()
            .iter()
            .filter(|request| request.args.first().map(String::as_str) == Some("tree"))
            .count(),
        2
    );
}

#[test]
fn relink_repairs_a_missing_owned_block_without_duplicating_desired_state() {
    let (repo_temp, _library_temp, library, workspaces) = fixture(false);
    let repo = fs::canonicalize(repo_temp.path()).unwrap();
    let first = plan_cargo_link(&repo, &library, &workspaces, false, &FixtureObserver).unwrap();
    let process = process_for(&library, &workspaces, false);
    assert_eq!(
        apply_cargo_link_plan(first, &process).unwrap().outcome,
        CargoLinkOutcome::Applied
    );
    fs::remove_file(repo.join(".cargo/config.toml")).unwrap();

    let refresh = plan_cargo_link(&repo, &library, &workspaces, false, &FixtureObserver).unwrap();
    let report = apply_cargo_link_plan(refresh, &process).unwrap();

    assert_eq!(report.outcome, CargoLinkOutcome::Applied);
    let state = RepoLinkStateStore::for_repo(&repo).read().unwrap();
    assert_eq!(state.links.len(), 1);
    let config = fs::read_to_string(repo.join(".cargo/config.toml")).unwrap();
    assert_eq!(config.matches("# >>> effigy deps cargo").count(), 1);
}

#[test]
fn already_linked_no_change_plan_still_verifies_as_an_idempotent_refresh() {
    let (repo_temp, _library_temp, library, workspaces) = fixture(false);
    let repo = fs::canonicalize(repo_temp.path()).unwrap();
    let process = process_for(&library, &workspaces, false);
    let first = plan_cargo_link(&repo, &library, &workspaces, false, &FixtureObserver).unwrap();
    assert_eq!(
        apply_cargo_link_plan(first, &process).unwrap().outcome,
        CargoLinkOutcome::Applied
    );
    process.requests.borrow_mut().clear();
    let refresh = plan_cargo_link(&repo, &library, &workspaces, false, &FixtureObserver).unwrap();
    assert!(refresh.operation.changes.is_empty());

    let report = apply_cargo_link_plan(refresh, &process).unwrap();

    assert_eq!(report.outcome, CargoLinkOutcome::Applied);
    assert!(report.applied_files.is_empty());
    assert_eq!(report.verification.status, VerificationStatus::Passed);
    assert!(process.requests.borrow().iter().any(|request| request
        .args
        .first()
        .map(String::as_str)
        == Some("tree")));
}

#[test]
fn verification_requires_one_local_copy_per_planned_resolution() {
    let (repo_temp, _library_temp, library, workspaces) = fixture(false);
    let repo = fs::canonicalize(repo_temp.path()).unwrap();
    let plan = plan_cargo_link(&repo, &library, &workspaces, false, &FixtureObserver).unwrap();
    let mut process = process_for(&library, &workspaces, false);
    process.local_path = repo.join("wrong-copy");

    let report = apply_cargo_link_plan(plan, &process).unwrap();

    assert_eq!(report.outcome, CargoLinkOutcome::VerificationFailed);
    assert!(report.verification.evidence[0]
        .message
        .as_deref()
        .unwrap()
        .contains("canonical local path"));
}

#[test]
fn expected_resolution_set_retains_exact_source_and_workspace_pairs() {
    let (repo_temp, _library_temp, library, mut workspaces) = fixture(true);
    let repo = fs::canonicalize(repo_temp.path()).unwrap();
    workspaces[1].library_matches[0]
        .package
        .source
        .as_mut()
        .unwrap()
        .identity = "ssh://git@example.test/signal.git".to_owned();
    let plan = plan_cargo_link(&repo, &library, &workspaces, true, &FixtureObserver).unwrap();

    assert_eq!(plan.expected_resolutions.len(), 2);
    assert_eq!(
        plan.expected_resolutions
            .iter()
            .map(|expected| expected.committed_source.identity.as_str())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([SOURCE, "ssh://git@example.test/signal.git"])
    );
}
