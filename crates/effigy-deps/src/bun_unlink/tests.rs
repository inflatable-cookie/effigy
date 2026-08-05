#![cfg(unix)]

use std::cell::{Cell, RefCell};
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::*;
use crate::{
    apply_bun_link_plan, plan_bun_link, BunConsumerInventory, BunConsumerReference,
    BunPackageInventory, DependencyDepth, ProcessOutput, RepoLinkStateStore,
};

struct FixtureProcess {
    home: PathBuf,
    fail_unregister: Option<String>,
    failed: Cell<bool>,
    requests: RefCell<Vec<ProcessRequest>>,
}

impl FixtureProcess {
    fn new(home: &Path) -> Self {
        Self {
            home: home.to_path_buf(),
            fail_unregister: None,
            failed: Cell::new(false),
            requests: RefCell::new(Vec::new()),
        }
    }

    fn fail_unregister_once(home: &Path, package: &str) -> Self {
        Self {
            home: home.to_path_buf(),
            fail_unregister: Some(package.to_owned()),
            failed: Cell::new(false),
            requests: RefCell::new(Vec::new()),
        }
    }
}

impl ReadOnlyProcess for FixtureProcess {
    fn run(&self, request: &ProcessRequest) -> Result<ProcessOutput, DepsError> {
        self.requests.borrow_mut().push(request.clone());
        let package_names = request
            .args
            .iter()
            .skip(1)
            .filter(|arg| !arg.starts_with("--"))
            .cloned()
            .collect::<Vec<_>>();
        let package = package_name(&request.cwd);
        if request.args == ["unlink", "--no-save"]
            && self
                .fail_unregister
                .as_ref()
                .is_some_and(|failed| package.as_deref() == Some(failed.as_str()))
            && !self.failed.replace(true)
        {
            return Err(DepsError::ProcessFailed {
                program: request.program.clone(),
                cwd: request.cwd.clone(),
                status: Some(1),
                stderr: "fixture unregister failure".to_owned(),
            });
        }

        match request.args.first().map(String::as_str) {
            Some("link") if request.args.get(1).map(String::as_str) == Some("--no-save") => {
                let package = package.expect("registration package");
                replace_symlink(&request.cwd, &bun_registration_path(&self.home, &package));
            }
            Some("link") => {
                for package in package_names {
                    let target =
                        fs::canonicalize(bun_registration_path(&self.home, &package)).unwrap();
                    replace_symlink(&target, &request.cwd.join("node_modules").join(package));
                }
            }
            Some("unlink") => {
                let package = package.expect("unregister package");
                let path = bun_registration_path(&self.home, &package);
                if fs::symlink_metadata(&path).is_ok() {
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
    let value: serde_json::Value = serde_json::from_slice(&raw).ok()?;
    value.get("name")?.as_str().map(str::to_owned)
}

fn replace_symlink(target: &Path, path: &Path) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink() || metadata.is_file() {
            fs::remove_file(path).unwrap();
        } else {
            fs::remove_dir_all(path).unwrap();
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

fn link(fixture: &Fixture, process: &FixtureProcess) {
    let plan = plan_bun_link(
        &fixture.repo,
        &fixture.library,
        &fixture.packages,
        &fixture.consumer,
        &fixture.home,
        false,
        &FsBunPlanObserver,
    )
    .unwrap();
    let report = apply_bun_link_plan(plan, &fixture.home, process, &FsBunPlanObserver).unwrap();
    assert_eq!(report.outcome, crate::BunLinkOutcome::Applied);
}

fn unlink_plan(fixture: &Fixture, dry_run: bool) -> BunDependencyPlan {
    plan_bun_unlink(
        &fixture.repo,
        &fixture.library,
        &fixture.home,
        dry_run,
        &FsBunPlanObserver,
    )
    .unwrap()
}

#[test]
fn dry_run_and_already_unlinked_are_non_mutating() {
    let fixture = fixture(&[("underlay", DependencyDepth::Direct)]);
    let process = FixtureProcess::new(&fixture.home);
    let no_op = apply_bun_unlink_plan(
        unlink_plan(&fixture, false),
        &fixture.home,
        &process,
        &FsBunPlanObserver,
    )
    .unwrap();
    assert_eq!(no_op.outcome, BunUnlinkOutcome::NoOp);
    assert!(process.requests.borrow().is_empty());

    link(&fixture, &process);
    let request_count = process.requests.borrow().len();
    let state_before = fs::read(RepoLinkStateStore::for_repo(&fixture.repo).path()).unwrap();
    let report = apply_bun_unlink_plan(
        unlink_plan(&fixture, true),
        &fixture.home,
        &process,
        &FsBunPlanObserver,
    )
    .unwrap();
    assert_eq!(report.outcome, BunUnlinkOutcome::DryRun);
    assert_eq!(process.requests.borrow().len(), request_count);
    assert_eq!(
        fs::read(RepoLinkStateStore::for_repo(&fixture.repo).path()).unwrap(),
        state_before
    );
}

#[test]
fn unlinks_complete_closure_and_owned_last_registrations() {
    let fixture = fixture(&[
        ("@signal/core", DependencyDepth::Direct),
        ("@signal/protocol", DependencyDepth::Transitive),
    ]);
    let process = FixtureProcess::new(&fixture.home);
    link(&fixture, &process);
    let package_before = fs::read(fixture.repo.join("package.json")).unwrap();
    let lock_before = fs::read(fixture.repo.join("bun.lock")).unwrap();

    let report = apply_bun_unlink_plan(
        unlink_plan(&fixture, false),
        &fixture.home,
        &process,
        &FsBunPlanObserver,
    )
    .unwrap();

    assert_eq!(report.outcome, BunUnlinkOutcome::Unlinked);
    assert_eq!(report.verification.status, VerificationStatus::Passed);
    assert_eq!(report.removed_consumer_links.len(), 2);
    assert_eq!(report.applied_processes.len(), 2);
    assert!(report.immutable_files.iter().all(|item| item.unchanged));
    assert_eq!(
        fs::read(fixture.repo.join("package.json")).unwrap(),
        package_before
    );
    assert_eq!(
        fs::read(fixture.repo.join("bun.lock")).unwrap(),
        lock_before
    );
    assert!(!RepoLinkStateStore::for_repo(&fixture.repo).path().exists());
    assert!(BunRegistrationIndexStore::for_home(&fixture.home)
        .read()
        .unwrap()
        .registrations
        .is_empty());
    for package in &fixture.packages {
        assert_eq!(
            FsBunPlanObserver
                .observe_path(&fixture.repo.join("node_modules").join(&package.name))
                .unwrap(),
            BunPathObservation::Missing
        );
        assert_eq!(
            FsBunPlanObserver
                .observe_path(&bun_registration_path(&fixture.home, &package.name))
                .unwrap(),
            BunPathObservation::Missing
        );
    }
}

#[test]
fn shared_and_foreign_registrations_are_retained() {
    let shared = fixture(&[("underlay", DependencyDepth::Direct)]);
    let shared_process = FixtureProcess::new(&shared.home);
    link(&shared, &shared_process);
    let other_repo = shared.repo.join("other-consumer");
    fs::create_dir_all(&other_repo).unwrap();
    BunRegistrationIndexStore::for_home(&shared.home)
        .update(|index| {
            index.add_reference(
                "underlay",
                shared.packages[0].package_path.clone(),
                false,
                BunConsumerReference {
                    consumer_repo: fs::canonicalize(&other_repo).unwrap(),
                    library_path: shared.library.clone(),
                },
            )
        })
        .unwrap();
    let shared_report = apply_bun_unlink_plan(
        unlink_plan(&shared, false),
        &shared.home,
        &shared_process,
        &FsBunPlanObserver,
    )
    .unwrap();
    assert_eq!(shared_report.outcome, BunUnlinkOutcome::Unlinked);
    assert!(shared_report.applied_processes.is_empty());
    assert!(matches!(
        FsBunPlanObserver
            .observe_path(&bun_registration_path(&shared.home, "underlay"))
            .unwrap(),
        BunPathObservation::Symlink { .. }
    ));
    assert_eq!(
        BunRegistrationIndexStore::for_home(&shared.home)
            .read()
            .unwrap()
            .registrations[0]
            .consumers
            .len(),
        1
    );

    let foreign = fixture(&[("poodle", DependencyDepth::Direct)]);
    replace_symlink(
        &foreign.packages[0].package_path,
        &bun_registration_path(&foreign.home, "poodle"),
    );
    let foreign_process = FixtureProcess::new(&foreign.home);
    link(&foreign, &foreign_process);
    let foreign_report = apply_bun_unlink_plan(
        unlink_plan(&foreign, false),
        &foreign.home,
        &foreign_process,
        &FsBunPlanObserver,
    )
    .unwrap();
    assert_eq!(foreign_report.outcome, BunUnlinkOutcome::Unlinked);
    assert!(foreign_report.applied_processes.is_empty());
    assert!(matches!(
        FsBunPlanObserver
            .observe_path(&bun_registration_path(&foreign.home, "poodle"))
            .unwrap(),
        BunPathObservation::Symlink { .. }
    ));
}

#[test]
fn stale_owned_registration_is_retained_as_unverifiable() {
    let fixture = fixture(&[("underlay", DependencyDepth::Direct)]);
    let process = FixtureProcess::new(&fixture.home);
    link(&fixture, &process);
    fs::remove_file(bun_registration_path(&fixture.home, "underlay")).unwrap();

    let report = apply_bun_unlink_plan(
        unlink_plan(&fixture, false),
        &fixture.home,
        &process,
        &FsBunPlanObserver,
    )
    .unwrap();
    assert_eq!(report.outcome, BunUnlinkOutcome::Unlinked);
    assert!(report.applied_processes.is_empty());
    assert_eq!(
        report.plan.packages[0].reference_release,
        Some(BunReferenceRelease::RetainedUnverifiable)
    );
    assert_eq!(
        BunRegistrationIndexStore::for_home(&fixture.home)
            .read()
            .unwrap()
            .registrations
            .len(),
        1
    );
}

#[test]
fn unregister_failure_restores_registrations_and_consumer_links() {
    let fixture = fixture(&[
        ("@signal/core", DependencyDepth::Direct),
        ("@signal/protocol", DependencyDepth::Transitive),
    ]);
    let link_process = FixtureProcess::new(&fixture.home);
    link(&fixture, &link_process);
    let process = FixtureProcess::fail_unregister_once(&fixture.home, "@signal/protocol");

    let report = apply_bun_unlink_plan(
        unlink_plan(&fixture, false),
        &fixture.home,
        &process,
        &FsBunPlanObserver,
    )
    .unwrap();
    assert_eq!(report.outcome, BunUnlinkOutcome::ApplyFailed);
    assert!(report.rollback.attempted);
    assert!(
        report.rollback.failures.is_empty(),
        "{:?}",
        report.rollback.failures
    );
    assert_eq!(report.rollback.relinked_consumer_packages.len(), 2);
    assert_eq!(report.rollback.restored_registrations.len(), 1);
    assert_eq!(
        RepoLinkStateStore::for_repo(&fixture.repo)
            .read()
            .unwrap()
            .links
            .len(),
        1
    );
    for package in &fixture.packages {
        assert_eq!(
            fs::canonicalize(fixture.repo.join("node_modules").join(&package.name)).unwrap(),
            package.package_path
        );
        assert_eq!(
            fs::canonicalize(bun_registration_path(&fixture.home, &package.name)).unwrap(),
            package.package_path
        );
    }
}
