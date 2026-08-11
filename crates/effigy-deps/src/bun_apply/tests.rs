#![cfg(unix)]

use std::cell::RefCell;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::*;
use crate::{
    plan_bun_link, BunConsumerInventory, BunPackageInventory, DependencyDepth, ProcessOutput,
    RepoLinkStateStore,
};

struct FixtureProcess {
    home: PathBuf,
    fail_package: Option<String>,
    requests: RefCell<Vec<ProcessRequest>>,
}

impl FixtureProcess {
    fn new(home: &Path) -> Self {
        Self {
            home: home.to_path_buf(),
            fail_package: None,
            requests: RefCell::new(Vec::new()),
        }
    }

    fn failing(home: &Path, package: &str) -> Self {
        Self {
            home: home.to_path_buf(),
            fail_package: Some(package.to_owned()),
            requests: RefCell::new(Vec::new()),
        }
    }
}

impl ReadOnlyProcess for FixtureProcess {
    fn run(&self, request: &ProcessRequest) -> Result<ProcessOutput, DepsError> {
        self.requests.borrow_mut().push(request.clone());
        let packages = if request.args.first().map(String::as_str) == Some("link")
            && request.args.get(1).map(String::as_str) != Some("--no-save")
        {
            request
                .args
                .iter()
                .skip(1)
                .filter(|arg| !arg.starts_with("--"))
                .cloned()
                .collect::<Vec<_>>()
        } else {
            package_name(&request.cwd).into_iter().collect()
        };
        if self
            .fail_package
            .as_ref()
            .is_some_and(|failed| packages.contains(failed))
        {
            return Err(DepsError::ProcessFailed {
                program: request.program.clone(),
                cwd: request.cwd.clone(),
                status: Some(1),
                stderr: "fixture failure".to_owned(),
            });
        }
        match request.args.first().map(String::as_str) {
            Some("link") if request.args.get(1).map(String::as_str) == Some("--no-save") => {
                let package = packages.first().expect("registration package");
                replace_symlink(&request.cwd, &bun_registration_path(&self.home, package));
            }
            Some("link") => {
                for package in &packages {
                    let target =
                        fs::canonicalize(bun_registration_path(&self.home, package)).unwrap();
                    replace_symlink(&target, &request.cwd.join("node_modules").join(package));
                }
            }
            Some("unlink") => {
                let package = packages.first().expect("unlink package");
                let path = bun_registration_path(&self.home, package);
                if path.exists() || fs::symlink_metadata(&path).is_ok() {
                    fs::remove_file(path).unwrap();
                }
            }
            other => panic!("unexpected fixture process: {other:?} {:?}", request.args),
        }
        Ok(ProcessOutput {
            status: Some(0),
            stdout: String::new(),
            stderr: String::new(),
        })
    }
}

fn package_name(root: &Path) -> Option<String> {
    let raw = fs::read(root.join("package.json")).ok()?;
    serde_json::from_slice::<serde_json::Value>(&raw)
        .ok()?
        .get("name")?
        .as_str()
        .map(str::to_owned)
}

fn replace_symlink(target: &Path, path: &Path) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    if fs::symlink_metadata(path).is_ok() {
        if path.is_dir() && !path.is_symlink() {
            fs::remove_dir_all(path).unwrap();
        } else {
            fs::remove_file(path).unwrap();
        }
    }
    symlink(target, path).unwrap();
}

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

struct Fixture {
    _repo_temp: TempDir,
    repo: PathBuf,
    _library_temp: TempDir,
    library: PathBuf,
    _home_temp: TempDir,
    home: PathBuf,
    packages: Vec<BunPackageInventory>,
    consumer: BunConsumerInventory,
}

fn fixture(names: &[(&str, DependencyDepth)]) -> Fixture {
    let repo_temp = TempDir::new().unwrap();
    let repo = fs::canonicalize(repo_temp.path()).unwrap();
    write(&repo.join("package.json"), "{\"name\":\"consumer\"}\n");
    write(&repo.join("bun.lock"), "fixture-lock\n");
    let library_temp = TempDir::new().unwrap();
    let library = fs::canonicalize(library_temp.path()).unwrap();
    let packages = names
        .iter()
        .map(|(name, _)| {
            let package_path = library.join(name.trim_start_matches('@').replace('/', "-"));
            write(
                &package_path.join("package.json"),
                &format!("{{\"name\":\"{name}\",\"version\":\"0.1.0\"}}\n"),
            );
            BunPackageInventory {
                name: (*name).to_owned(),
                package_path,
                version: Some("0.1.0".to_owned()),
            }
        })
        .collect::<Vec<_>>();
    let consumer = BunConsumerInventory {
        root: repo.clone(),
        packages: names
            .iter()
            .map(|(name, _)| BunPackageInventory {
                name: (*name).to_owned(),
                package_path: repo.join("node_modules").join(name),
                version: Some("1.2.3".to_owned()),
            })
            .collect(),
        direct_dependencies: names
            .iter()
            .filter(|(_, depth)| *depth == DependencyDepth::Direct)
            .map(|(name, _)| (*name).to_owned())
            .collect(),
        library_matches: names
            .iter()
            .map(|(name, depth)| {
                (
                    BunPackageInventory {
                        name: (*name).to_owned(),
                        package_path: repo.join("node_modules").join(name),
                        version: Some("1.2.3".to_owned()),
                    },
                    *depth,
                )
            })
            .collect(),
    };
    let home_temp = TempDir::new().unwrap();
    let home = fs::canonicalize(home_temp.path()).unwrap();
    Fixture {
        _repo_temp: repo_temp,
        repo,
        _library_temp: library_temp,
        library,
        _home_temp: home_temp,
        home,
        packages,
        consumer,
    }
}

fn plan(fixture: &Fixture, dry_run: bool) -> BunDependencyPlan {
    plan_bun_link(
        &fixture.repo,
        &fixture.library,
        &fixture.packages,
        &fixture.consumer,
        &fixture.home,
        dry_run,
        &FsBunPlanObserver,
    )
    .unwrap()
}

#[test]
fn dry_run_executes_no_mutating_process_or_state_write() {
    let fixture = fixture(&[("underlay", DependencyDepth::Direct)]);
    let process = FixtureProcess::new(&fixture.home);
    let report = apply_bun_link_plan(
        plan(&fixture, true),
        &fixture.home,
        &process,
        &FsBunPlanObserver,
    )
    .unwrap();

    assert_eq!(report.outcome, BunLinkOutcome::DryRun);
    assert!(process.requests.borrow().is_empty());
    assert!(!RepoLinkStateStore::for_repo(&fixture.repo).path().exists());
    assert!(!BunRegistrationIndexStore::for_home(&fixture.home)
        .path()
        .exists());
}

#[test]
fn applies_and_verifies_every_direct_and_transitive_package() {
    let fixture = fixture(&[
        ("@signal/core", DependencyDepth::Direct),
        ("@signal/protocol", DependencyDepth::Transitive),
    ]);
    let process = FixtureProcess::new(&fixture.home);
    let package_before = fs::read(fixture.repo.join("package.json")).unwrap();
    let lock_before = fs::read(fixture.repo.join("bun.lock")).unwrap();
    let report = apply_bun_link_plan(
        plan(&fixture, false),
        &fixture.home,
        &process,
        &FsBunPlanObserver,
    )
    .unwrap();

    assert_eq!(report.outcome, BunLinkOutcome::Applied);
    assert_eq!(report.verification.status, VerificationStatus::Passed);
    assert_eq!(report.verification.evidence.len(), 2);
    assert!(report.immutable_files.iter().all(|item| item.unchanged));
    assert_eq!(report.applied_processes.len(), 3);
    assert!(process.requests.borrow().iter().all(|request| {
        request.args.contains(&"--no-save".to_owned())
            && !request.args.contains(&"--save".to_owned())
    }));
    assert_eq!(
        fs::read(fixture.repo.join("package.json")).unwrap(),
        package_before
    );
    assert_eq!(
        fs::read(fixture.repo.join("bun.lock")).unwrap(),
        lock_before
    );
    for package in &fixture.packages {
        assert_eq!(
            fs::canonicalize(fixture.repo.join("node_modules").join(&package.name)).unwrap(),
            package.package_path
        );
    }
    assert_eq!(
        RepoLinkStateStore::for_repo(&fixture.repo)
            .read()
            .unwrap()
            .links
            .len(),
        1
    );
    assert_eq!(
        BunRegistrationIndexStore::for_home(&fixture.home)
            .read()
            .unwrap()
            .registrations
            .len(),
        2
    );
}

#[test]
fn process_failure_rolls_back_links_registrations_files_and_registry_copies() {
    let fixture = fixture(&[
        ("@signal/core", DependencyDepth::Direct),
        ("@signal/protocol", DependencyDepth::Transitive),
    ]);
    for package in &fixture.packages {
        write(
            &fixture
                .repo
                .join("node_modules")
                .join(&package.name)
                .join("registry.txt"),
            "registry copy\n",
        );
    }
    let process = FixtureProcess::failing(&fixture.home, "@signal/protocol");
    let report = apply_bun_link_plan(
        plan(&fixture, false),
        &fixture.home,
        &process,
        &FsBunPlanObserver,
    )
    .unwrap();

    assert_eq!(report.outcome, BunLinkOutcome::ApplyFailed);
    assert!(report.rollback.attempted);
    assert!(
        report.rollback.failures.is_empty(),
        "{:?}",
        report.rollback.failures
    );
    assert!(!fixture.repo.join(".gitignore").exists());
    assert!(!RepoLinkStateStore::for_repo(&fixture.repo).path().exists());
    assert!(!BunRegistrationIndexStore::for_home(&fixture.home)
        .path()
        .exists());
    for package in &fixture.packages {
        assert_eq!(
            fs::read_to_string(
                fixture
                    .repo
                    .join("node_modules")
                    .join(&package.name)
                    .join("registry.txt")
            )
            .unwrap(),
            "registry copy\n"
        );
        assert!(matches!(
            FsBunPlanObserver
                .observe_path(&bun_registration_path(&fixture.home, &package.name))
                .unwrap(),
            BunPathObservation::Missing
        ));
    }
}

#[test]
fn stale_index_or_physical_state_fails_before_process_mutation() {
    let fixture = fixture(&[("underlay", DependencyDepth::Direct)]);
    let stale_index_plan = plan(&fixture, false);
    let index_store = BunRegistrationIndexStore::for_home(&fixture.home);
    index_store
        .update(|index| {
            index.add_reference(
                "foreign",
                fixture.library.clone(),
                false,
                crate::BunConsumerReference {
                    consumer_repo: fixture.repo.clone(),
                    library_path: fixture.library.clone(),
                },
            )
        })
        .unwrap();
    let process = FixtureProcess::new(&fixture.home);
    assert!(apply_bun_link_plan(
        stale_index_plan,
        &fixture.home,
        &process,
        &FsBunPlanObserver,
    )
    .is_err());
    assert!(process.requests.borrow().is_empty());

    fs::remove_file(index_store.path()).unwrap();
    let stale_physical_plan = plan(&fixture, false);
    replace_symlink(
        &fixture.library,
        &fixture.repo.join("node_modules/underlay"),
    );
    assert!(apply_bun_link_plan(
        stale_physical_plan,
        &fixture.home,
        &process,
        &FsBunPlanObserver,
    )
    .is_err());
    assert!(process.requests.borrow().is_empty());
}

#[test]
fn matching_foreign_registration_is_linked_without_registration_mutation() {
    let fixture = fixture(&[("underlay", DependencyDepth::Direct)]);
    replace_symlink(
        &fixture.packages[0].package_path,
        &bun_registration_path(&fixture.home, "underlay"),
    );
    let process = FixtureProcess::new(&fixture.home);
    let report = apply_bun_link_plan(
        plan(&fixture, false),
        &fixture.home,
        &process,
        &FsBunPlanObserver,
    )
    .unwrap();

    assert_eq!(report.outcome, BunLinkOutcome::Applied);
    assert_eq!(
        process
            .requests
            .borrow()
            .iter()
            .filter(|request| request.args == ["link", "--no-save"])
            .count(),
        0
    );
    assert!(
        !BunRegistrationIndexStore::for_home(&fixture.home)
            .read()
            .unwrap()
            .registrations[0]
            .effigy_created
    );
}

#[test]
fn duplicate_peer_resolution_fails_verification_and_rolls_back_link() {
    let fixture = fixture(&[("underlay", DependencyDepth::Direct)]);
    write(
        &fixture.packages[0].package_path.join("package.json"),
        "{\"name\":\"underlay\",\"version\":\"0.1.0\",\"peerDependencies\":{\"svelte\":\"^5\"}}\n",
    );
    write(
        &fixture.repo.join("node_modules/svelte/package.json"),
        "{\"name\":\"svelte\",\"version\":\"5.56.8\"}\n",
    );
    write(
        &fixture.packages[0]
            .package_path
            .join("node_modules/svelte/package.json"),
        "{\"name\":\"svelte\",\"version\":\"5.53.10\"}\n",
    );
    let process = FixtureProcess::new(&fixture.home);
    let report = apply_bun_link_plan(
        plan(&fixture, false),
        &fixture.home,
        &process,
        &FsBunPlanObserver,
    )
    .unwrap();

    assert_eq!(report.outcome, BunLinkOutcome::VerificationFailed);
    assert_eq!(report.peer_diagnostics.len(), 1);
    assert_eq!(
        report.peer_diagnostics[0].status,
        BunPeerResolutionStatus::Duplicate
    );
    assert!(report.rollback.attempted);
    assert!(report.rollback.failures.is_empty());
    assert_eq!(
        FsBunPlanObserver
            .observe_path(&fixture.repo.join("node_modules/underlay"))
            .unwrap(),
        BunPathObservation::Missing
    );
    assert_eq!(
        FsBunPlanObserver
            .observe_path(&bun_registration_path(&fixture.home, "underlay"))
            .unwrap(),
        BunPathObservation::Missing
    );
}

#[test]
fn same_version_peer_paths_across_repos_verify_as_shared() {
    let fixture = fixture(&[("underlay", DependencyDepth::Direct)]);
    write(
        &fixture.packages[0].package_path.join("package.json"),
        "{\"name\":\"underlay\",\"version\":\"0.1.0\",\"peerDependencies\":{\"svelte\":\"^5\"}}\n",
    );
    write(
        &fixture.repo.join("node_modules/svelte/package.json"),
        "{\"name\":\"svelte\",\"version\":\"5.56.8\"}\n",
    );
    write(
        &fixture.packages[0]
            .package_path
            .join("node_modules/.bun/svelte@5.56.8/node_modules/svelte/package.json"),
        "{\"name\":\"svelte\",\"version\":\"5.56.8\"}\n",
    );
    symlink(
        fixture.packages[0]
            .package_path
            .join("node_modules/.bun/svelte@5.56.8/node_modules/svelte"),
        fixture.packages[0].package_path.join("node_modules/svelte"),
    )
    .unwrap();
    let process = FixtureProcess::new(&fixture.home);
    let report = apply_bun_link_plan(
        plan(&fixture, false),
        &fixture.home,
        &process,
        &FsBunPlanObserver,
    )
    .unwrap();

    assert_eq!(report.outcome, BunLinkOutcome::Applied);
    assert_eq!(report.peer_diagnostics.len(), 1);
    assert_eq!(
        report.peer_diagnostics[0].status,
        BunPeerResolutionStatus::Shared
    );
    assert!(!report.rollback.attempted);
}

#[test]
fn relink_repairs_partial_consumer_link_loss_without_duplicate_state() {
    let fixture = fixture(&[
        ("@signal/core", DependencyDepth::Direct),
        ("@signal/protocol", DependencyDepth::Transitive),
    ]);
    let first_process = FixtureProcess::new(&fixture.home);
    let first = apply_bun_link_plan(
        plan(&fixture, false),
        &fixture.home,
        &first_process,
        &FsBunPlanObserver,
    )
    .unwrap();
    assert_eq!(first.outcome, BunLinkOutcome::Applied);
    fs::remove_file(
        fixture
            .repo
            .join("node_modules")
            .join(&fixture.packages[0].name),
    )
    .unwrap();

    let relink_plan = plan(&fixture, false);
    assert!(relink_plan
        .process_intents
        .iter()
        .all(|intent| intent.action == BunProcessAction::LinkConsumer));
    let relink_process = FixtureProcess::new(&fixture.home);
    let relink = apply_bun_link_plan(
        relink_plan,
        &fixture.home,
        &relink_process,
        &FsBunPlanObserver,
    )
    .unwrap();

    assert_eq!(relink.outcome, BunLinkOutcome::Applied);
    assert_eq!(
        RepoLinkStateStore::for_repo(&fixture.repo)
            .read()
            .unwrap()
            .links
            .len(),
        1
    );
    assert_eq!(
        BunRegistrationIndexStore::for_home(&fixture.home)
            .read()
            .unwrap()
            .registrations
            .iter()
            .flat_map(|registration| &registration.consumers)
            .count(),
        2
    );
}
