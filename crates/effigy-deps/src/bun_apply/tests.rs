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
fn matching_committed_override_refuses_link_without_local_state_or_processes() {
    let fixture = fixture(&[("@acme/core", DependencyDepth::Direct)]);
    let package = &fixture.packages[0];
    let relative = format!(
        "../{}/{}",
        fixture.library.file_name().unwrap().to_string_lossy(),
        package.package_path.file_name().unwrap().to_string_lossy()
    );
    write(
        &fixture.repo.join("package.json"),
        &format!(
            "{{\"name\":\"consumer\",\"overrides\":{{\"@acme/core\":\"file:{relative}\"}}}}\n"
        ),
    );
    let process = FixtureProcess::new(&fixture.home);

    let report = apply_bun_link_plan(
        plan(&fixture, false),
        &fixture.home,
        &process,
        &FsBunPlanObserver,
    )
    .unwrap();

    assert_eq!(report.outcome, BunLinkOutcome::CommittedPinActive);
    assert!(report.plan.operation.changes.is_empty());
    assert!(report.applied_processes.is_empty());
    assert!(report.errors[0].contains("effigy deps unpin bun"));
    assert!(process.requests.borrow().is_empty());
    assert!(!RepoLinkStateStore::for_repo(&fixture.repo).path().exists());
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
                crate::BunConsumerReference::claim(fixture.repo.clone(), fixture.library.clone()),
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

fn link(fixture: &Fixture, process: &FixtureProcess) {
    let report = apply_bun_link_plan(
        plan(fixture, false),
        &fixture.home,
        process,
        &FsBunPlanObserver,
    )
    .unwrap();
    assert_eq!(report.outcome, BunLinkOutcome::Applied);
}

fn registration_consumers(fixture: &Fixture) -> Vec<PathBuf> {
    BunRegistrationIndexStore::for_home(&fixture.home)
        .read()
        .unwrap()
        .registrations
        .into_iter()
        .flat_map(|registration| registration.consumers)
        .map(|consumer| consumer.consumer_repo)
        .collect()
}

#[test]
fn retirement_unregisters_a_last_owned_claim_and_keeps_the_consumer_link() {
    let fixture = fixture(&[("underlay", DependencyDepth::Direct)]);
    let process = FixtureProcess::new(&fixture.home);
    link(&fixture, &process);
    assert!(matches!(
        FsBunPlanObserver
            .observe_path(&bun_registration_path(&fixture.home, "underlay"))
            .unwrap(),
        BunPathObservation::Symlink { .. }
    ));

    let retirement = retire_bun_registration_claims(
        &fixture.home,
        &fixture.repo,
        &fixture.library,
        &process,
        &FsBunPlanObserver,
    );

    assert_eq!(retirement.unregistered, ["underlay"]);
    assert!(retirement.failures.is_empty(), "{:?}", retirement.failures);
    assert_eq!(
        FsBunPlanObserver
            .observe_path(&bun_registration_path(&fixture.home, "underlay"))
            .unwrap(),
        BunPathObservation::Missing
    );
    assert!(registration_consumers(&fixture).is_empty());
    // The consumer symlink and the ledger are the link; only the machine-wide
    // registration is retired.
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

    // An owner that runs twice (completed then interrupted, or a watcher and
    // the command) has nothing left to retire.
    let again = retire_bun_registration_claims(
        &fixture.home,
        &fixture.repo,
        &fixture.library,
        &process,
        &FsBunPlanObserver,
    );
    assert!(again.is_empty(), "{again:?}");
}

#[test]
fn retirement_releases_only_the_owning_consumers_claim() {
    let fixture = fixture(&[("underlay", DependencyDepth::Direct)]);
    let process = FixtureProcess::new(&fixture.home);
    link(&fixture, &process);
    let sibling = TempDir::new().unwrap();
    let sibling_root = fs::canonicalize(sibling.path()).unwrap();
    BunRegistrationIndexStore::for_home(&fixture.home)
        .update(|index| {
            index.add_reference(
                "underlay",
                fixture.packages[0].package_path.clone(),
                true,
                BunConsumerReference::claim(sibling_root.clone(), fixture.library.clone()),
            )
        })
        .unwrap();

    let retirement = retire_bun_registration_claims(
        &fixture.home,
        &fixture.repo,
        &fixture.library,
        &process,
        &FsBunPlanObserver,
    );

    assert_eq!(retirement.released_shared, ["underlay"]);
    assert!(retirement.unregistered.is_empty());
    assert_eq!(registration_consumers(&fixture), [sibling_root]);
    assert!(matches!(
        FsBunPlanObserver
            .observe_path(&bun_registration_path(&fixture.home, "underlay"))
            .unwrap(),
        BunPathObservation::Symlink { .. }
    ));
}

#[test]
fn retirement_keeps_a_foreign_registration_and_its_claim() {
    let fixture = fixture(&[("underlay", DependencyDepth::Direct)]);
    let process = FixtureProcess::new(&fixture.home);
    replace_symlink(
        &fixture.packages[0].package_path,
        &bun_registration_path(&fixture.home, "underlay"),
    );
    link(&fixture, &process);
    assert!(
        !BunRegistrationIndexStore::for_home(&fixture.home)
            .read()
            .unwrap()
            .registrations[0]
            .effigy_created
    );

    let retirement = retire_bun_registration_claims(
        &fixture.home,
        &fixture.repo,
        &fixture.library,
        &process,
        &FsBunPlanObserver,
    );

    assert_eq!(retirement.retained, ["underlay"]);
    // A registration Effigy did not create is never removed, and its claim is
    // not Effigy's to keep either: the ledger remains the desired link.
    assert!(registration_consumers(&fixture).is_empty());
    assert!(matches!(
        FsBunPlanObserver
            .observe_path(&bun_registration_path(&fixture.home, "underlay"))
            .unwrap(),
        BunPathObservation::Symlink { .. }
    ));
}

#[test]
fn a_failed_retirement_keeps_the_claim_for_unlink() {
    let fixture = fixture(&[("underlay", DependencyDepth::Direct)]);
    let process = FixtureProcess::new(&fixture.home);
    link(&fixture, &process);
    let failing = FixtureProcess::failing(&fixture.home, "underlay");

    let retirement = retire_bun_registration_claims(
        &fixture.home,
        &fixture.repo,
        &fixture.library,
        &failing,
        &FsBunPlanObserver,
    );

    assert_eq!(retirement.failures.len(), 1, "{retirement:?}");
    assert_eq!(
        registration_consumers(&fixture),
        std::slice::from_ref(&fixture.repo)
    );
    assert!(matches!(
        FsBunPlanObserver
            .observe_path(&bun_registration_path(&fixture.home, "underlay"))
            .unwrap(),
        BunPathObservation::Symlink { .. }
    ));

    // Reversible: the claim a failed retirement left behind is the same one a
    // later owner retires, and `deps unlink` reports it until then.
    let retried = retire_bun_registration_claims(
        &fixture.home,
        &fixture.repo,
        &fixture.library,
        &process,
        &FsBunPlanObserver,
    );
    assert_eq!(retried.unregistered, ["underlay"]);
    assert!(registration_consumers(&fixture).is_empty());
}

#[test]
fn retirement_of_one_bun_root_keeps_a_sibling_roots_claim() {
    let fixture = fixture(&[("underlay", DependencyDepth::Direct)]);
    fs::create_dir_all(fixture.repo.join(".git")).unwrap();
    let process = FixtureProcess::new(&fixture.home);
    link(&fixture, &process);
    // A second Bun package root in the same checkout: its own consumer, its own
    // claim, sharing one checkout with the root this link named.
    let harness = fixture.repo.join("harness");
    fs::create_dir_all(&harness).unwrap();
    let harness = fs::canonicalize(&harness).unwrap();
    BunRegistrationIndexStore::for_home(&fixture.home)
        .update(|index| {
            index.add_reference(
                "underlay",
                fixture.packages[0].package_path.clone(),
                true,
                BunConsumerReference::claim(harness.clone(), fixture.library.clone()),
            )
        })
        .unwrap();

    let retirement = retire_bun_registration_claims(
        &fixture.home,
        &fixture.repo,
        &fixture.library,
        &process,
        &FsBunPlanObserver,
    );

    assert_eq!(retirement.released_shared, ["underlay"]);
    assert!(retirement.unregistered.is_empty());
    assert_eq!(registration_consumers(&fixture), [harness]);
    assert!(matches!(
        FsBunPlanObserver
            .observe_path(&bun_registration_path(&fixture.home, "underlay"))
            .unwrap(),
        BunPathObservation::Symlink { .. }
    ));
}

#[test]
fn an_interrupted_owner_keeps_a_registration_another_consumer_claims() {
    let fixture = fixture(&[("underlay", DependencyDepth::Direct)]);
    let process = FixtureProcess::new(&fixture.home);
    link(&fixture, &process);
    let sibling = TempDir::new().unwrap();
    let sibling_root = fs::canonicalize(sibling.path()).unwrap();
    BunRegistrationIndexStore::for_home(&fixture.home)
        .update(|index| {
            index.add_reference(
                "underlay",
                fixture.packages[0].package_path.clone(),
                true,
                BunConsumerReference::claim(sibling_root.clone(), fixture.library.clone()),
            )
        })
        .unwrap();

    let retirement = retire_owned_bun_registrations(
        &fixture.home,
        &fixture.repo,
        &fixture.library,
        &[(
            "underlay".to_owned(),
            fixture.packages[0].package_path.clone(),
        )],
        true,
        &process,
        &FsBunPlanObserver,
    );

    // The dying owner holds the index lock, so it reads the index instead of
    // waiting for it -- and still sees the other consumer.
    assert_eq!(retirement.retained, ["underlay"]);
    assert!(retirement.unregistered.is_empty());
    assert!(retirement.failures.is_empty(), "{:?}", retirement.failures);
    // The interrupted owner leaves its own claim behind (it cannot rewrite the
    // index it dies under) and never touches the other consumer's.
    let mut consumers = registration_consumers(&fixture);
    consumers.sort();
    let mut expected = vec![fixture.repo.clone(), sibling_root];
    expected.sort();
    assert_eq!(consumers, expected);
    assert!(matches!(
        FsBunPlanObserver
            .observe_path(&bun_registration_path(&fixture.home, "underlay"))
            .unwrap(),
        BunPathObservation::Symlink { .. }
    ));
}

#[test]
fn an_interrupted_owner_retires_an_unclaimed_registration() {
    let fixture = fixture(&[("underlay", DependencyDepth::Direct)]);
    let process = FixtureProcess::new(&fixture.home);
    link(&fixture, &process);

    let retirement = retire_owned_bun_registrations(
        &fixture.home,
        &fixture.repo,
        &fixture.library,
        &[(
            "underlay".to_owned(),
            fixture.packages[0].package_path.clone(),
        )],
        true,
        &process,
        &FsBunPlanObserver,
    );

    assert_eq!(retirement.unregistered, ["underlay"]);
    assert_eq!(
        FsBunPlanObserver
            .observe_path(&bun_registration_path(&fixture.home, "underlay"))
            .unwrap(),
        BunPathObservation::Missing
    );
}

#[test]
fn an_interrupted_owner_keeps_an_unverified_registration_when_the_index_cannot_be_read() {
    let fixture = fixture(&[("underlay", DependencyDepth::Direct)]);
    let process = FixtureProcess::new(&fixture.home);
    link(&fixture, &process);
    write(
        BunRegistrationIndexStore::for_home(&fixture.home).path(),
        "{\"schema\":\"effigy.deps.bun-registrations.v1\",\"truncated\":",
    );

    let retirement = retire_owned_bun_registrations(
        &fixture.home,
        &fixture.repo,
        &fixture.library,
        &[(
            "underlay".to_owned(),
            fixture.packages[0].package_path.clone(),
        )],
        true,
        &process,
        &FsBunPlanObserver,
    );

    // No index means no proof that another consumer does not claim it.
    assert_eq!(retirement.retained, ["underlay"]);
    assert!(retirement.unregistered.is_empty());
    assert_eq!(retirement.failures.len(), 1, "{retirement:?}");
    assert!(matches!(
        FsBunPlanObserver
            .observe_path(&bun_registration_path(&fixture.home, "underlay"))
            .unwrap(),
        BunPathObservation::Symlink { .. }
    ));
}

#[test]
fn an_interrupted_owner_that_never_started_retires_nothing() {
    let fixture = fixture(&[("underlay", DependencyDepth::Direct)]);
    let process = FixtureProcess::new(&fixture.home);
    link(&fixture, &process);

    // A link that only wanted the index lock planned its registrations before
    // this one created anything; it has nothing of its own to retire.
    let retirement = retire_owned_bun_registrations(
        &fixture.home,
        &fixture.repo,
        &fixture.library,
        &[(
            "underlay".to_owned(),
            fixture.packages[0].package_path.clone(),
        )],
        false,
        &process,
        &FsBunPlanObserver,
    );

    assert_eq!(retirement.retained, ["underlay"]);
    assert!(retirement.unregistered.is_empty());
    assert!(retirement.failures.is_empty(), "{:?}", retirement.failures);
    assert!(matches!(
        FsBunPlanObserver
            .observe_path(&bun_registration_path(&fixture.home, "underlay"))
            .unwrap(),
        BunPathObservation::Symlink { .. }
    ));
}

#[test]
fn an_interrupted_owner_retires_after_the_apply_released_the_index_lock() {
    let fixture = fixture(&[("underlay", DependencyDepth::Direct)]);
    let process = FixtureProcess::new(&fixture.home);
    link(&fixture, &process);
    // Stopping the in-flight `bun link` lets the apply fail and release its own
    // index transaction; the retiral must still retire the registration.
    drop(
        crate::state::StateLock::acquire(
            BunRegistrationIndexStore::for_home(&fixture.home).lock_path(),
        )
        .expect("the apply released the index lock"),
    );

    let retirement = retire_owned_bun_registrations(
        &fixture.home,
        &fixture.repo,
        &fixture.library,
        &[(
            "underlay".to_owned(),
            fixture.packages[0].package_path.clone(),
        )],
        true,
        &process,
        &FsBunPlanObserver,
    );

    assert_eq!(retirement.unregistered, ["underlay"]);
    assert!(retirement.failures.is_empty(), "{:?}", retirement.failures);
    assert_eq!(
        FsBunPlanObserver
            .observe_path(&bun_registration_path(&fixture.home, "underlay"))
            .unwrap(),
        BunPathObservation::Missing
    );
}

#[test]
fn an_owned_link_retires_its_registration_inside_the_apply() {
    let fixture = fixture(&[("underlay", DependencyDepth::Direct)]);
    let process = FixtureProcess::new(&fixture.home);

    let report = super::apply_bun_link_plan_owned(
        plan(&fixture, false),
        &fixture.home,
        &process,
        &FsBunPlanObserver,
        None,
    )
    .unwrap();

    assert_eq!(report.outcome, BunLinkOutcome::Applied);
    assert_eq!(report.registration_retirement.unregistered, ["underlay"]);
    // The apply retires the registration inside its own index transaction, so
    // nothing is left for a later step to retire and no interrupt can land
    // after the lock is released with a registration still owned.
    assert_eq!(
        FsBunPlanObserver
            .observe_path(&bun_registration_path(&fixture.home, "underlay"))
            .unwrap(),
        BunPathObservation::Missing
    );
    assert!(registration_consumers(&fixture).is_empty());
    // The link itself is the consumer symlink plus the ledger.
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
}

#[test]
fn a_relink_retires_an_existing_claim_instead_of_leaving_it_in_the_index() {
    let fixture = fixture(&[("underlay", DependencyDepth::Direct)]);
    let process = FixtureProcess::new(&fixture.home);
    // The fixture link leaves the claim and registration a recovery reads.
    link(&fixture, &process);
    assert_eq!(
        registration_consumers(&fixture),
        std::slice::from_ref(&fixture.repo)
    );

    // A re-link over that claim plans no index change of its own, so the
    // retirement alone has to decide that the index is written.
    let report = super::apply_bun_link_plan_owned(
        plan(&fixture, false),
        &fixture.home,
        &process,
        &FsBunPlanObserver,
        None,
    )
    .unwrap();

    assert_eq!(report.outcome, BunLinkOutcome::Applied);
    assert_eq!(report.registration_retirement.unregistered, ["underlay"]);
    assert!(
        registration_consumers(&fixture).is_empty(),
        "the retired claim stayed in the index"
    );
    assert_eq!(
        FsBunPlanObserver
            .observe_path(&bun_registration_path(&fixture.home, "underlay"))
            .unwrap(),
        BunPathObservation::Missing
    );
}

#[test]
fn a_ledger_write_failure_leaves_the_link_exactly_as_it_was() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = fixture(&[("underlay", DependencyDepth::Direct)]);
    let process = FixtureProcess::new(&fixture.home);
    // Start from the claimed state a fixture link leaves, then drop the ledger
    // so the plan has a ledger write to attempt.
    link(&fixture, &process);
    let ledger = fixture.repo.join(".effigy/local/dependency-links.json");
    fs::remove_file(&ledger).unwrap();
    let planned = plan(&fixture, false);
    let ledger_dir = fixture.repo.join(".effigy/local");
    fs::set_permissions(&ledger_dir, fs::Permissions::from_mode(0o555)).unwrap();

    let report = super::apply_bun_link_plan_owned(
        planned,
        &fixture.home,
        &process,
        &FsBunPlanObserver,
        None,
    )
    .unwrap();
    fs::set_permissions(&ledger_dir, fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(report.outcome, BunLinkOutcome::ApplyFailed);
    assert!(!report.errors.is_empty());
    assert!(report.registration_retirement.is_empty());
    assert!(
        report.rollback.failures.is_empty(),
        "{:?}",
        report.rollback.failures
    );
    // A failed transaction leaves the claim, the registration and the ledger
    // exactly as they were: nothing is retired before the ledger commits.
    assert_eq!(
        registration_consumers(&fixture),
        std::slice::from_ref(&fixture.repo)
    );
    assert!(matches!(
        FsBunPlanObserver
            .observe_path(&bun_registration_path(&fixture.home, "underlay"))
            .unwrap(),
        BunPathObservation::Symlink { .. }
    ));
    assert!(!ledger.exists());
}

#[test]
fn an_index_commit_failure_rolls_the_link_back() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = fixture(&[("underlay", DependencyDepth::Direct)]);
    let process = FixtureProcess::new(&fixture.home);
    // The transaction takes the index lock and commits under it, so the lock
    // file has to exist for the injected write failure to be the commit itself.
    let index_dir = fixture.home.join(".effigy/deps");
    write(&index_dir.join("bun-registrations.lock"), "");
    let planned = plan(&fixture, false);
    fs::set_permissions(&index_dir, fs::Permissions::from_mode(0o555)).unwrap();

    let report =
        super::apply_bun_link_plan(planned, &fixture.home, &process, &FsBunPlanObserver).unwrap();
    fs::set_permissions(&index_dir, fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(report.outcome, BunLinkOutcome::ApplyFailed);
    assert!(
        report
            .errors
            .iter()
            .any(|error| error.contains("bun-registrations.json")),
        "{:?}",
        report.errors
    );
    assert!(report.registration_retirement.is_empty());
    assert!(
        report.rollback.failures.is_empty(),
        "{:?}",
        report.rollback.failures
    );
    // The commit failed, so the transaction left no ledger, no consumer link
    // and no registration behind.
    assert!(!fixture
        .repo
        .join(".effigy/local/dependency-links.json")
        .exists());
    assert!(fs::symlink_metadata(fixture.repo.join("node_modules/underlay")).is_err());
    assert_eq!(
        FsBunPlanObserver
            .observe_path(&bun_registration_path(&fixture.home, "underlay"))
            .unwrap(),
        BunPathObservation::Missing
    );
    assert!(registration_consumers(&fixture).is_empty());
}

#[test]
fn an_index_commit_failure_restores_a_pre_existing_registration() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = fixture(&[("underlay", DependencyDepth::Direct)]);
    let process = FixtureProcess::new(&fixture.home);
    // The claimed state a recovery reads: the registration and its claim exist
    // before this attempt, so a failed commit has to put the registration back
    // for the claim the index still holds.
    link(&fixture, &process);
    let ledger = fixture.repo.join(".effigy/local/dependency-links.json");
    let index_before = fs::read(BunRegistrationIndexStore::for_home(&fixture.home).path()).unwrap();
    let planned = plan(&fixture, false);
    let index_dir = fixture.home.join(".effigy/deps");
    fs::set_permissions(&index_dir, fs::Permissions::from_mode(0o555)).unwrap();

    let report = super::apply_bun_link_plan_owned(
        planned,
        &fixture.home,
        &process,
        &FsBunPlanObserver,
        None,
    )
    .unwrap();
    fs::set_permissions(&index_dir, fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(report.outcome, BunLinkOutcome::ApplyFailed);
    assert!(report.registration_retirement.is_empty());
    // Nothing committed: the claim the index still holds has its registration.
    assert_eq!(
        fs::read(BunRegistrationIndexStore::for_home(&fixture.home).path()).unwrap(),
        index_before
    );
    assert_eq!(
        registration_consumers(&fixture),
        std::slice::from_ref(&fixture.repo)
    );
    assert!(
        matches!(
            FsBunPlanObserver
                .observe_path(&bun_registration_path(&fixture.home, "underlay"))
                .unwrap(),
            BunPathObservation::Symlink { .. }
        ),
        "the retired registration was not restored"
    );
    assert!(ledger.exists());
    assert!(
        report.rollback.failures.is_empty(),
        "{:?}",
        report.rollback.failures
    );
}
