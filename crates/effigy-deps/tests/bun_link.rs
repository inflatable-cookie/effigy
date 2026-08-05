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

#[test]
fn real_bun_round_trips_a_root_package_without_manifest_or_lock_churn() {
    run_real_fixture(true);
}

#[test]
fn real_bun_round_trips_a_shared_multi_package_closure_without_manifest_or_lock_churn() {
    run_real_fixture(false);
}
