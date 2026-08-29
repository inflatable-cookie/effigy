#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use effigy_deps::{
    apply_bun_link_plan, apply_bun_unlink_plan, bun_registration_path, inventory_bun_library,
    plan_bun_link, plan_bun_unlink, BunConsumerInventory, BunConsumerReference, BunLinkOutcome,
    BunPackageInventory, BunRegistrationIndexStore, BunUnlinkOutcome, DependencyDepth, DepsError,
    FsBunPlanObserver, ProcessOutput, ProcessRequest, ReadOnlyProcess, VerificationStatus,
};
use tempfile::TempDir;

struct IsolatedBunProcess {
    home: PathBuf,
}

impl ReadOnlyProcess for IsolatedBunProcess {
    fn run(&self, request: &ProcessRequest) -> Result<ProcessOutput, DepsError> {
        let output = Command::new(&request.program)
            .args(&request.args)
            .current_dir(&request.cwd)
            .env("HOME", &self.home)
            .env("BUN_INSTALL", self.home.join(".bun"))
            .output()
            .map_err(|source| DepsError::ProcessSpawn {
                program: request.program.clone(),
                cwd: request.cwd.clone(),
                source,
            })?;
        let result = ProcessOutput {
            status: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        };
        if output.status.success() {
            Ok(result)
        } else {
            Err(DepsError::ProcessFailed {
                program: request.program.clone(),
                cwd: request.cwd.clone(),
                status: result.status,
                stderr: result.stderr,
            })
        }
    }
}

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

fn installed_bun_version() -> Option<String> {
    let output = Command::new("bun").arg("--version").output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn run_real_fixture(root_only: bool) {
    let Some(version) = installed_bun_version() else {
        eprintln!("skipping real Bun link proof because Bun is unavailable");
        return;
    };
    eprintln!("real Bun link proof using Bun {version}");
    let consumer_temp = TempDir::new().unwrap();
    let consumer = fs::canonicalize(consumer_temp.path()).unwrap();
    let library_temp = TempDir::new().unwrap();
    let library = fs::canonicalize(library_temp.path()).unwrap();
    let home_temp = TempDir::new().unwrap();
    let home = fs::canonicalize(home_temp.path()).unwrap();

    let package_names = if root_only {
        write(
            &library.join("package.json"),
            "{\"name\":\"@effigy-fixture/root\",\"version\":\"0.1.0\"}\n",
        );
        vec!["@effigy-fixture/root"]
    } else {
        write(
            &library.join("package.json"),
            "{\"private\":true,\"workspaces\":[\"packages/*\"]}\n",
        );
        write(
            &library.join("packages/core/package.json"),
            "{\"name\":\"@effigy-fixture/core\",\"version\":\"0.1.0\"}\n",
        );
        write(
            &library.join("packages/protocol/package.json"),
            "{\"name\":\"@effigy-fixture/protocol\",\"version\":\"0.1.0\"}\n",
        );
        vec!["@effigy-fixture/core", "@effigy-fixture/protocol"]
    };
    let dependencies = format!("\"{}\":\"1.2.3\"", package_names[0]);
    write(
        &consumer.join("package.json"),
        &format!("{{\"name\":\"consumer\",\"dependencies\":{{{dependencies}}}}}\n"),
    );
    let package_before = fs::read(consumer.join("package.json")).unwrap();
    let library_packages = inventory_bun_library(&library).unwrap();
    let consumer_inventory = BunConsumerInventory {
        root: consumer.clone(),
        packages: package_names
            .iter()
            .map(|name| BunPackageInventory {
                name: (*name).to_owned(),
                package_path: consumer.join("node_modules").join(name),
                version: Some("1.2.3".to_owned()),
            })
            .collect(),
        direct_dependencies: vec![package_names[0].to_owned()],
        library_matches: package_names
            .iter()
            .enumerate()
            .map(|(index, name)| {
                (
                    BunPackageInventory {
                        name: (*name).to_owned(),
                        package_path: consumer.join("node_modules").join(name),
                        version: Some("1.2.3".to_owned()),
                    },
                    if index == 0 {
                        DependencyDepth::Direct
                    } else {
                        DependencyDepth::Transitive
                    },
                )
            })
            .collect(),
    };
    let plan = plan_bun_link(
        &consumer,
        &library,
        &library_packages,
        &consumer_inventory,
        &home,
        false,
        &FsBunPlanObserver,
    )
    .unwrap();
    let report = apply_bun_link_plan(
        plan,
        &home,
        &IsolatedBunProcess { home: home.clone() },
        &FsBunPlanObserver,
    )
    .unwrap();

    assert_eq!(
        report.outcome,
        BunLinkOutcome::Applied,
        "{:?}",
        report.errors
    );
    assert_eq!(report.verification.status, VerificationStatus::Passed);
    assert!(report.immutable_files.iter().all(|item| item.unchanged));
    assert_eq!(
        fs::read(consumer.join("package.json")).unwrap(),
        package_before
    );
    assert!(!consumer.join("bun.lock").exists());
    assert!(!consumer.join("bun.lockb").exists());
    for package in &library_packages {
        assert_eq!(
            fs::canonicalize(consumer.join("node_modules").join(&package.name)).unwrap(),
            package.package_path
        );
    }

    write(
        &library_packages[0].package_path.join("edit-proof.txt"),
        "local edit visible\n",
    );
    assert_eq!(
        fs::read_to_string(
            consumer
                .join("node_modules")
                .join(&library_packages[0].name)
                .join("edit-proof.txt")
        )
        .unwrap(),
        "local edit visible\n"
    );

    if !root_only {
        let other_consumer = consumer.join("other-consumer");
        fs::create_dir_all(&other_consumer).unwrap();
        let other_consumer = fs::canonicalize(other_consumer).unwrap();
        BunRegistrationIndexStore::for_home(&home)
            .update(|index| {
                for package in &library_packages {
                    index.add_reference(
                        package.name.clone(),
                        package.package_path.clone(),
                        false,
                        BunConsumerReference {
                            consumer_repo: other_consumer.clone(),
                            library_path: library.clone(),
                        },
                    )?;
                }
                Ok(())
            })
            .unwrap();
    }

    let unlink_plan =
        plan_bun_unlink(&consumer, &library, &home, false, &FsBunPlanObserver).unwrap();
    let unlink = apply_bun_unlink_plan(
        unlink_plan,
        &home,
        &IsolatedBunProcess { home: home.clone() },
        &FsBunPlanObserver,
    )
    .unwrap();
    assert_eq!(
        unlink.outcome,
        BunUnlinkOutcome::Unlinked,
        "{:?}",
        unlink.errors
    );
    assert_eq!(unlink.verification.status, VerificationStatus::Passed);
    assert!(unlink.immutable_files.iter().all(|item| item.unchanged));
    assert_eq!(
        fs::read(consumer.join("package.json")).unwrap(),
        package_before
    );
    assert!(!consumer.join("bun.lock").exists());
    assert!(!consumer.join("bun.lockb").exists());
    for package in &library_packages {
        assert!(!consumer.join("node_modules").join(&package.name).exists());
        if root_only {
            assert!(!bun_registration_path(&home, &package.name).exists());
        } else {
            assert_eq!(
                fs::canonicalize(bun_registration_path(&home, &package.name)).unwrap(),
                package.package_path
            );
        }
    }

    let relink_plan = plan_bun_link(
        &consumer,
        &library,
        &library_packages,
        &consumer_inventory,
        &home,
        false,
        &FsBunPlanObserver,
    )
    .unwrap();
    let relink = apply_bun_link_plan(
        relink_plan,
        &home,
        &IsolatedBunProcess { home: home.clone() },
        &FsBunPlanObserver,
    )
    .unwrap();
    assert_eq!(
        relink.outcome,
        BunLinkOutcome::Applied,
        "{:?}",
        relink.errors
    );
    for package in &library_packages {
        assert_eq!(
            fs::canonicalize(consumer.join("node_modules").join(&package.name)).unwrap(),
            package.package_path
        );
    }
}

/// Link and unlink a repo whose Bun tree sits below the checkout.
///
/// The link is keyed by `studio/`, but the ledger belongs to the checkout. If
/// the two identities are conflated, unlink looks for the ledger under
/// `studio/` and silently leaves the desired link behind.
fn run_nested_root_fixture(requested_root: NestedRequest) {
    let Some(version) = installed_bun_version() else {
        eprintln!("skipping nested-root Bun link proof because Bun is unavailable");
        return;
    };
    eprintln!("nested-root Bun link proof using Bun {version}");
    let repo_temp = TempDir::new().unwrap();
    let repo = fs::canonicalize(repo_temp.path()).unwrap();
    let library_temp = TempDir::new().unwrap();
    let library = fs::canonicalize(library_temp.path()).unwrap();
    let home_temp = TempDir::new().unwrap();
    let home = fs::canonicalize(home_temp.path()).unwrap();

    // Figmatic shape: a checkout with no root manifest and Bun under `studio/`
    // alongside sibling roots that do not declare the library.
    fs::create_dir_all(repo.join(".git")).unwrap();
    write(
        &library.join("package.json"),
        "{\"name\":\"@effigy-fixture/nested\",\"version\":\"0.1.0\"}\n",
    );
    write(
        &repo.join("studio/package.json"),
        "{\"name\":\"studio\",\"dependencies\":{\"@effigy-fixture/nested\":\"1.2.3\"}}\n",
    );
    write(
        &repo.join("harness/package.json"),
        "{\"name\":\"harness\"}\n",
    );
    let studio = fs::canonicalize(repo.join("studio")).unwrap();
    let package_before = fs::read(studio.join("package.json")).unwrap();

    let library_packages = inventory_bun_library(&library).unwrap();
    let consumer_inventory = BunConsumerInventory {
        root: studio.clone(),
        packages: vec![BunPackageInventory {
            name: "@effigy-fixture/nested".to_owned(),
            package_path: studio.join("node_modules/@effigy-fixture/nested"),
            version: Some("1.2.3".to_owned()),
        }],
        direct_dependencies: vec!["@effigy-fixture/nested".to_owned()],
        library_matches: vec![(
            BunPackageInventory {
                name: "@effigy-fixture/nested".to_owned(),
                package_path: studio.join("node_modules/@effigy-fixture/nested"),
                version: Some("1.2.3".to_owned()),
            },
            DependencyDepth::Direct,
        )],
    };
    let invoked_root = match requested_root {
        NestedRequest::Checkout => repo.clone(),
        NestedRequest::NestedRoot => studio.clone(),
    };

    let plan = plan_bun_link(
        &invoked_root,
        &library,
        &library_packages,
        &consumer_inventory,
        &home,
        false,
        &FsBunPlanObserver,
    )
    .unwrap();

    // Either entry point resolves to one identity pair.
    assert_eq!(plan.repo_root, repo);
    assert_eq!(plan.operation.key.consumer_repo, studio);

    let repo_ledger = repo.join(".effigy/local/dependency-links.json");
    let nested_ledger = studio.join(".effigy/local/dependency-links.json");
    let report = apply_bun_link_plan(
        plan,
        &home,
        &IsolatedBunProcess { home: home.clone() },
        &FsBunPlanObserver,
    )
    .unwrap();
    assert_eq!(
        report.outcome,
        BunLinkOutcome::Applied,
        "{:?}",
        report.errors
    );
    assert!(repo_ledger.exists(), "ledger belongs to the checkout");
    assert!(!nested_ledger.exists(), "no ledger under the Bun root");
    assert!(repo.join(".gitignore").exists());
    assert_eq!(
        fs::canonicalize(studio.join("node_modules/@effigy-fixture/nested")).unwrap(),
        library_packages[0].package_path
    );
    assert_eq!(
        fs::read(studio.join("package.json")).unwrap(),
        package_before
    );

    let unlink_plan =
        plan_bun_unlink(&invoked_root, &library, &home, false, &FsBunPlanObserver).unwrap();
    assert_eq!(unlink_plan.repo_root, repo);
    assert_eq!(unlink_plan.operation.key.consumer_repo, studio);
    let unlink = apply_bun_unlink_plan(
        unlink_plan,
        &home,
        &IsolatedBunProcess { home: home.clone() },
        &FsBunPlanObserver,
    )
    .unwrap();
    assert_eq!(
        unlink.outcome,
        BunUnlinkOutcome::Unlinked,
        "{:?}",
        unlink.errors
    );
    assert!(
        !repo_ledger.exists(),
        "unlink must remove the checkout ledger it wrote"
    );
    assert!(!studio.join("node_modules/@effigy-fixture/nested").exists());
    assert_eq!(
        fs::read(studio.join("package.json")).unwrap(),
        package_before
    );
}

enum NestedRequest {
    Checkout,
    NestedRoot,
}

#[test]
fn real_bun_round_trips_a_nested_bun_root_against_the_checkout_ledger() {
    run_nested_root_fixture(NestedRequest::Checkout);
}

#[test]
fn real_bun_nested_root_repo_override_resolves_the_same_link_identity() {
    run_nested_root_fixture(NestedRequest::NestedRoot);
}

#[test]
fn real_bun_round_trips_a_root_package_without_manifest_or_lock_churn() {
    run_real_fixture(true);
}

#[test]
fn real_bun_round_trips_a_shared_multi_package_closure_without_manifest_or_lock_churn() {
    run_real_fixture(false);
}
