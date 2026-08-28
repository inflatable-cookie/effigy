use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use crate::{
    canonical_existing_path, BunConsumerInventory, BunConsumerLinkDisposition,
    BunConsumerReference, BunDependencyPlan, BunImmutableFileSnapshot, BunPackageInventory,
    BunPackagePlan, BunPathObservation, BunPhysicalPrecondition, BunProcessAction,
    BunProcessIntent, BunReferenceRelease, BunRegistration, BunRegistrationDisposition,
    BunRegistrationIndex, BunRegistrationIndexStore, BunStateFileSnapshot, BunSymlinkAction,
    BunSymlinkIntent, CommittedSource, CommittedSourceKind, ConsumerRoot, DependencyLinkKey,
    DependencyLinkPlan, DependencyPackage, DepsError, DesiredDependencyLink, LinkMechanism,
    PackageManager, PlanAction, PlannedChange, PlannedChangeAction, RepoLinkStateStore,
};

pub trait BunPlanObserver {
    fn observe_path(&self, path: &Path) -> Result<BunPathObservation, DepsError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct FsBunPlanObserver;

impl BunPlanObserver for FsBunPlanObserver {
    fn observe_path(&self, path: &Path) -> Result<BunPathObservation, DepsError> {
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(BunPathObservation::Missing);
            }
            Err(error) => return Err(DepsError::io("inspect Bun link", path, error)),
        };
        if !metadata.file_type().is_symlink() {
            return Ok(BunPathObservation::NonSymlink);
        }
        let target =
            fs::read_link(path).map_err(|error| DepsError::io("read Bun link", path, error))?;
        let target = if target.is_absolute() {
            target
        } else {
            path.parent().unwrap_or_else(|| Path::new(".")).join(target)
        };
        Ok(BunPathObservation::Symlink {
            target: canonical_or_original(&target),
        })
    }
}

pub fn bun_registration_path(home: impl AsRef<Path>, package_name: &str) -> PathBuf {
    home.as_ref()
        .join(".bun/install/global/node_modules")
        .join(package_name)
}

pub fn plan_bun_link(
    repo_root: impl AsRef<Path>,
    library_root: impl AsRef<Path>,
    library_packages: &[BunPackageInventory],
    consumer: &BunConsumerInventory,
    home: impl AsRef<Path>,
    dry_run: bool,
    observer: &impl BunPlanObserver,
) -> Result<BunDependencyPlan, DepsError> {
    let repo_root = crate::state::repo_state_root(&canonical_existing_path(repo_root)?);
    let library_root = canonical_existing_path(library_root)?;
    // The repo identity owns the ledger and `.gitignore`; the Bun package root
    // owns `package.json`, `node_modules`, and every `bun` invocation. They are
    // the same directory only when the repo keeps Bun at its root.
    let consumer_root = canonical_existing_path(&consumer.root)?;
    if !consumer_root.starts_with(&repo_root) {
        return Err(DepsError::invalid(
            &consumer.root,
            format!(
                "Bun consumer inventory belongs to `{}` rather than a package root inside requested repo `{}`",
                consumer_root.display(),
                repo_root.display()
            ),
        ));
    }
    let key = DependencyLinkKey {
        manager: PackageManager::Bun,
        consumer_repo: consumer_root.clone(),
        library_path: library_root.clone(),
    };
    let closure = bun_closure(&consumer_root, library_packages, consumer)?;
    let committed_pin_packages = crate::bun_pin::matching_committed_overrides(
        &consumer_root,
        closure
            .iter()
            .map(|package| (package.name.as_str(), package.local_path.as_path())),
    )?;
    if !committed_pin_packages.is_empty() {
        return Ok(BunDependencyPlan {
            repo_root: repo_root.clone(),
            desired: None,
            operation: DependencyLinkPlan {
                action: PlanAction::Link,
                dry_run,
                key,
                changes: Vec::new(),
                warnings: vec![format!(
                    "committed Bun override already selects local package(s) {}; use the pin plus `bun install`, or run `effigy deps unpin bun {}` before linking",
                    committed_pin_packages.join(", "),
                    library_root.display()
                )],
            },
            packages: closure
                .iter()
                .map(|package| BunPackagePlan {
                    name: package.name.clone(),
                    local_path: package.local_path.clone(),
                    depth: Some(package.depth),
                    committed_version: package.committed_version.clone(),
                    registration: BunRegistrationDisposition::Conflict,
                    consumer_link: BunConsumerLinkDisposition::Conflict,
                    reference_release: None,
                })
                .collect(),
            process_intents: Vec::new(),
            symlink_intents: Vec::new(),
            physical_preconditions: Vec::new(),
            state_preconditions: Vec::new(),
            immutable_files: immutable_bun_files(
                &consumer_root,
                closure.iter().map(|package| package.local_path.as_path()),
            )?,
        });
    }
    let reference = BunConsumerReference {
        consumer_repo: consumer_root.clone(),
        library_path: library_root.clone(),
    };
    let state_store = RepoLinkStateStore::for_repo(&repo_root);
    let state = state_store.read()?;
    let index_store = BunRegistrationIndexStore::for_home(home);
    let index = index_store.read()?;
    let state_preconditions = state_file_snapshots([
        repo_root.join(".gitignore"),
        state_store.path().to_path_buf(),
        index_store.path().to_path_buf(),
    ])?;

    let mut package_plans = Vec::new();
    let mut registration_intents = Vec::new();
    let mut consumer_observations = Vec::new();
    let mut physical_preconditions = Vec::new();
    for package in &closure {
        let registration_path = bun_registration_path_from_store(&index_store, &package.name)?;
        let registration_observation = observer.observe_path(&registration_path)?;
        physical_preconditions.push(BunPhysicalPrecondition {
            path: registration_path.clone(),
            observation: registration_observation.clone(),
        });
        let registration = classify_registration(
            &package.name,
            &package.local_path,
            &reference,
            &registration_observation,
            find_registration(&index, &package.name),
        )?;
        if registration == BunRegistrationDisposition::StaleForeign {
            return Err(DepsError::invalid(
                &registration_path,
                format!(
                    "foreign Bun registration `{}` is missing; recreate it manually before linking so Effigy does not claim it",
                    package.name
                ),
            ));
        }
        if matches!(
            registration,
            BunRegistrationDisposition::Absent | BunRegistrationDisposition::StaleOwned
        ) {
            registration_intents.push(register_intent(package));
        }

        let consumer_link_path = consumer_root.join("node_modules").join(&package.name);
        let consumer_observation = observer.observe_path(&consumer_link_path)?;
        physical_preconditions.push(BunPhysicalPrecondition {
            path: consumer_link_path.clone(),
            observation: consumer_observation.clone(),
        });
        let consumer_link = classify_consumer_link(
            &package.name,
            &package.local_path,
            &consumer_root,
            &consumer_link_path,
            &consumer_observation,
        )?;
        consumer_observations.push(consumer_link);
        package_plans.push(BunPackagePlan {
            name: package.name.clone(),
            local_path: package.local_path.clone(),
            depth: Some(package.depth),
            committed_version: package.committed_version.clone(),
            registration,
            consumer_link,
            reference_release: None,
        });
    }
    let managed_relink = state.links.iter().any(|link| link.key == key);
    refuse_unmanaged_partial_consumer_closure(&consumer_root, &package_plans, managed_relink)?;

    let all_linked = consumer_observations
        .iter()
        .all(|state| *state == BunConsumerLinkDisposition::Linked);
    let mut process_intents = registration_intents;
    if !all_linked {
        process_intents.push(link_consumer_intent(&consumer_root, &closure));
    }

    let desired = DesiredDependencyLink {
        key: key.clone(),
        mechanism: LinkMechanism::BunLink,
        consumer_roots: vec![ConsumerRoot {
            canonical_path: consumer_root.clone(),
        }],
        packages: closure
            .iter()
            .map(|package| DependencyPackage {
                name: package.name.clone(),
                local_path: package.local_path.clone(),
                committed_sources: package
                    .committed_version
                    .as_ref()
                    .map(|version| {
                        vec![CommittedSource {
                            kind: CommittedSourceKind::Registry,
                            identity: version.clone(),
                        }]
                    })
                    .unwrap_or_default(),
            })
            .collect(),
        cargo_resolutions: Vec::new(),
        cargo_ownership: None,
    };

    let mut planned_state = state.clone();
    planned_state.links.retain(|link| link.key != key);
    planned_state.links.push(desired.clone());
    planned_state.normalize();
    let mut planned_index = index.clone();
    for package in &package_plans {
        let created = matches!(
            package.registration,
            BunRegistrationDisposition::Absent | BunRegistrationDisposition::StaleOwned
        );
        planned_index.add_reference(
            package.name.clone(),
            package.local_path.clone(),
            created,
            reference.clone(),
        )?;
    }

    let mut changes = Vec::new();
    let (gitignore_before, gitignore_after) = plan_link_gitignore(&repo_root)?;
    push_file_change(
        &mut changes,
        repo_root.join(".gitignore"),
        "ensure machine-local dependency state is ignored",
        gitignore_before,
        gitignore_after,
    );
    push_json_change(
        &mut changes,
        state_store.path(),
        "record the desired Bun dependency link",
        &planned_state,
    )?;
    push_json_change(
        &mut changes,
        index_store.path(),
        "record Bun registration ownership and consumer references",
        &planned_index,
    )?;

    Ok(BunDependencyPlan {
        repo_root: repo_root.clone(),
        desired: Some(desired),
        operation: DependencyLinkPlan {
            action: PlanAction::Link,
            dry_run,
            key,
            changes,
            warnings: vec![
                "Bun process intents explicitly use --no-save; package.json and Bun lockfiles must remain byte-for-byte unchanged"
                    .to_owned(),
                "Bun consumer links are ephemeral and may need repair after bun install".to_owned(),
            ],
        },
        packages: package_plans,
        process_intents,
        symlink_intents: Vec::new(),
        physical_preconditions,
        state_preconditions,
        immutable_files: immutable_bun_files(
            &consumer_root,
            closure.iter().map(|package| package.local_path.as_path()),
        )?,
    })
}

pub fn plan_bun_unlink(
    repo_root: impl AsRef<Path>,
    library_path: impl AsRef<Path>,
    home: impl AsRef<Path>,
    dry_run: bool,
    observer: &impl BunPlanObserver,
) -> Result<BunDependencyPlan, DepsError> {
    let requested_root = canonical_existing_path(repo_root)?;
    let library_path = resolve_unlink_library_path(&requested_root, library_path.as_ref())?;
    let repo_root = crate::state::repo_state_root(&requested_root);
    let absent_key = DependencyLinkKey {
        manager: PackageManager::Bun,
        consumer_repo: requested_root.clone(),
        library_path: library_path.clone(),
    };
    let state_store = RepoLinkStateStore::for_repo(&repo_root);
    let state = state_store.read()?;
    let Some(desired) = select_unlink_link(&requested_root, &state, &library_path)?.cloned() else {
        return Ok(BunDependencyPlan {
            repo_root: repo_root.clone(),
            desired: None,
            operation: DependencyLinkPlan {
                action: PlanAction::Unlink,
                dry_run,
                key: absent_key,
                changes: Vec::new(),
                warnings: vec![
                    "Bun dependency link is already absent; nothing to remove".to_owned()
                ],
            },
            packages: Vec::new(),
            process_intents: Vec::new(),
            symlink_intents: Vec::new(),
            physical_preconditions: Vec::new(),
            state_preconditions: Vec::new(),
            immutable_files: Vec::new(),
        });
    };
    let key = desired.key.clone();
    // The link records the Bun package root it was made against, so unlink
    // repairs that tree even when the repo keeps Bun below its root.
    let consumer_root = desired
        .consumer_roots
        .first()
        .map(|root| root.canonical_path.clone())
        .unwrap_or_else(|| key.consumer_repo.clone());
    let reference = BunConsumerReference {
        consumer_repo: key.consumer_repo.clone(),
        library_path: key.library_path.clone(),
    };
    let index_store = BunRegistrationIndexStore::for_home(home);
    let index = index_store.read()?;
    let state_preconditions = state_file_snapshots([
        state_store.path().to_path_buf(),
        index_store.path().to_path_buf(),
    ])?;
    let mut planned_index = index.clone();
    let mut package_plans = Vec::new();
    let mut process_intents = Vec::new();
    let mut symlink_intents = Vec::new();
    let mut physical_preconditions = Vec::new();
    let mut warnings =
        vec!["Bun unlink must preserve package.json and Bun lockfiles byte-for-byte".to_owned()];

    for package in &desired.packages {
        let registration_path = bun_registration_path_from_store(&index_store, &package.name)?;
        let registration_observation = observer.observe_path(&registration_path)?;
        physical_preconditions.push(BunPhysicalPrecondition {
            path: registration_path.clone(),
            observation: registration_observation.clone(),
        });
        let indexed = find_registration(&index, &package.name);
        let registration = classify_registration(
            &package.name,
            &package.local_path,
            &reference,
            &registration_observation,
            indexed,
        )?;

        let consumer_path = consumer_root.join("node_modules").join(&package.name);
        let consumer_observation = observer.observe_path(&consumer_path)?;
        physical_preconditions.push(BunPhysicalPrecondition {
            path: consumer_path.clone(),
            observation: consumer_observation.clone(),
        });
        let consumer_link = match &consumer_observation {
            BunPathObservation::Missing => BunConsumerLinkDisposition::Missing,
            BunPathObservation::NonSymlink => BunConsumerLinkDisposition::Registry,
            BunPathObservation::Symlink { target } if same_path(target, &package.local_path) => {
                symlink_intents.push(BunSymlinkIntent {
                    action: BunSymlinkAction::RemoveConsumerLink,
                    package: package.name.clone(),
                    path: consumer_path,
                    expected_target: package.local_path.clone(),
                });
                BunConsumerLinkDisposition::Linked
            }
            BunPathObservation::Symlink { target } => {
                warnings.push(format!(
                    "consumer link for `{}` points to `{}`; leave the foreign symlink untouched",
                    package.name,
                    target.display()
                ));
                BunConsumerLinkDisposition::Conflict
            }
        };

        let release = plan_reference_release(
            &mut planned_index,
            indexed,
            &package.name,
            &package.local_path,
            &reference,
            &registration_observation,
        );
        if release == BunReferenceRelease::RemoveOwned {
            process_intents.push(unregister_intent(package));
        } else if release == BunReferenceRelease::RetainedUnverifiable {
            warnings.push(format!(
                "retain the ownership reference for `{}` because its global registration target is stale or unverifiable",
                package.name
            ));
        }
        package_plans.push(BunPackagePlan {
            name: package.name.clone(),
            local_path: package.local_path.clone(),
            depth: None,
            committed_version: package
                .committed_sources
                .iter()
                .find(|source| source.kind == CommittedSourceKind::Registry)
                .map(|source| source.identity.clone()),
            registration,
            consumer_link,
            reference_release: Some(release),
        });
    }

    let mut planned_state = state.clone();
    planned_state.links.retain(|link| link.key != key);
    planned_state.normalize();
    let mut changes = Vec::new();
    push_optional_json_change(
        &mut changes,
        state_store.path(),
        "remove the desired Bun dependency link",
        (!planned_state.links.is_empty()).then_some(&planned_state),
    )?;
    push_optional_json_change(
        &mut changes,
        index_store.path(),
        "release only this consumer's Bun registration references",
        Some(&planned_index),
    )?;

    Ok(BunDependencyPlan {
        repo_root: repo_root.clone(),
        desired: None,
        operation: DependencyLinkPlan {
            action: PlanAction::Unlink,
            dry_run,
            key,
            changes,
            warnings,
        },
        packages: package_plans,
        process_intents,
        symlink_intents,
        physical_preconditions,
        state_preconditions,
        immutable_files: immutable_bun_files(
            &consumer_root,
            desired
                .packages
                .iter()
                .map(|package| package.local_path.as_path()),
        )?,
    })
}

#[derive(Debug, Clone)]
struct BunClosurePackage {
    name: String,
    local_path: PathBuf,
    committed_version: Option<String>,
    depth: crate::DependencyDepth,
}

fn bun_closure(
    repo_root: &Path,
    library_packages: &[BunPackageInventory],
    consumer: &BunConsumerInventory,
) -> Result<Vec<BunClosurePackage>, DepsError> {
    let mut local = BTreeMap::new();
    for package in library_packages {
        let local_path = canonical_existing_path(&package.package_path)?;
        if let Some(existing) = local.insert(package.name.clone(), local_path.clone()) {
            if existing != local_path {
                return Err(DepsError::invalid(
                    &package.package_path,
                    format!(
                        "library contains duplicate Bun package name `{}`",
                        package.name
                    ),
                ));
            }
        }
    }

    let mut matches = BTreeMap::new();
    let mut duplicate_names = BTreeSet::new();
    for (package, depth) in &consumer.library_matches {
        let Some(local_path) = local.get(&package.name) else {
            return Err(DepsError::invalid(
                repo_root,
                format!(
                    "resolved Bun package `{}` is absent from the local library inventory",
                    package.name
                ),
            ));
        };
        if matches
            .insert(
                package.name.clone(),
                BunClosurePackage {
                    name: package.name.clone(),
                    local_path: local_path.clone(),
                    committed_version: package.version.clone(),
                    depth: *depth,
                },
            )
            .is_some()
        {
            duplicate_names.insert(package.name.clone());
        }
    }
    if !duplicate_names.is_empty() {
        let guidance = bun_override_guidance(
            repo_root,
            matches
                .values()
                .map(|package| (package.name.as_str(), package.local_path.as_path())),
        );
        return Err(DepsError::invalid(
            repo_root,
            format!(
                "Bun dependency closure contains multiple resolved copies of {}; mixed local/registry linking is unsafe and no plan was produced\n\n{}",
                duplicate_names.into_iter().collect::<Vec<_>>().join(", "),
                guidance,
            ),
        ));
    }
    if matches.is_empty() {
        return Err(DepsError::invalid(
            repo_root,
            "consumer has no dependencies matching the local Bun library; no plan was produced",
        ));
    }
    Ok(matches.into_values().collect())
}

fn bun_override_guidance<'a>(
    consumer_root: &Path,
    packages: impl Iterator<Item = (&'a str, &'a Path)>,
) -> String {
    let entries = packages
        .map(|(name, local_path)| {
            format!(
                "  {}: {}",
                serde_json::to_string(name).expect("package name is JSON-serializable"),
                serde_json::to_string(&bun_file_spec(consumer_root, local_path))
                    .expect("package path is JSON-serializable")
            )
        })
        .collect::<Vec<_>>();
    let override_block = format!("\"overrides\": {{\n{}\n}}", entries.join(",\n"));
    format!(
        "A consumer-level Bun override is the transitive mechanism for dependency graphs that cross `file:` or repository boundaries. Add or merge this block in `{}`, then run `bun install`:\n\n{}\n\n`deps link bun` remains save-less and did not modify the manifest",
        consumer_root.join("package.json").display(),
        override_block,
    )
}

fn bun_file_spec(consumer_root: &Path, package_path: &Path) -> String {
    let consumer_components = consumer_root.components().collect::<Vec<_>>();
    let package_components = package_path.components().collect::<Vec<_>>();
    let common = consumer_components
        .iter()
        .zip(&package_components)
        .take_while(|(left, right)| left == right)
        .count();

    if common == 0 {
        return format!("file:{}", package_path.to_string_lossy().replace('\\', "/"));
    }

    let mut relative = PathBuf::new();
    for _ in common..consumer_components.len() {
        relative.push("..");
    }
    for component in &package_components[common..] {
        relative.push(component.as_os_str());
    }
    let relative = relative.to_string_lossy().replace('\\', "/");
    let relative = if relative.starts_with("../") || relative == ".." {
        relative
    } else {
        format!("./{relative}")
    };
    format!("file:{relative}")
}

fn classify_registration(
    package_name: &str,
    expected: &Path,
    reference: &BunConsumerReference,
    observed: &BunPathObservation,
    indexed: Option<&BunRegistration>,
) -> Result<BunRegistrationDisposition, DepsError> {
    if let Some(indexed) = indexed {
        if !same_path(&indexed.package_path, expected) {
            return Err(DepsError::RegistrationConflict {
                package_name: package_name.to_owned(),
                existing_path: indexed.package_path.clone(),
                requested_path: expected.to_path_buf(),
            });
        }
    }
    match observed {
        BunPathObservation::Missing => match indexed {
            None => Ok(BunRegistrationDisposition::Absent),
            Some(indexed) if indexed.effigy_created => Ok(BunRegistrationDisposition::StaleOwned),
            Some(_) => Ok(BunRegistrationDisposition::StaleForeign),
        },
        BunPathObservation::NonSymlink => Err(DepsError::invalid(
            expected,
            format!("Bun registration `{package_name}` is not a symlink; refusing to replace it"),
        )),
        BunPathObservation::Symlink { target } if same_path(target, expected) => match indexed {
            None => Ok(BunRegistrationDisposition::MatchingForeign),
            Some(indexed) if !indexed.effigy_created => {
                Ok(BunRegistrationDisposition::MatchingForeign)
            }
            Some(indexed)
                if indexed
                    .consumers
                    .iter()
                    .any(|consumer| consumer != reference) =>
            {
                Ok(BunRegistrationDisposition::MatchingOwnedShared)
            }
            Some(_) => Ok(BunRegistrationDisposition::MatchingOwned),
        },
        BunPathObservation::Symlink { target } => Err(DepsError::RegistrationConflict {
            package_name: package_name.to_owned(),
            existing_path: target.clone(),
            requested_path: expected.to_path_buf(),
        }),
    }
}

fn classify_consumer_link(
    package_name: &str,
    expected: &Path,
    repo_root: &Path,
    link_path: &Path,
    observed: &BunPathObservation,
) -> Result<BunConsumerLinkDisposition, DepsError> {
    match observed {
        BunPathObservation::Missing => Ok(BunConsumerLinkDisposition::Missing),
        BunPathObservation::NonSymlink => Ok(BunConsumerLinkDisposition::Registry),
        BunPathObservation::Symlink { target } if same_path(target, expected) => {
            Ok(BunConsumerLinkDisposition::Linked)
        }
        BunPathObservation::Symlink { target }
            if is_consumer_bun_registry_store_path(repo_root, link_path, target) =>
        {
            Ok(BunConsumerLinkDisposition::Registry)
        }
        BunPathObservation::Symlink { target } => Err(DepsError::invalid(
            link_path,
            format!(
                "consumer package `{package_name}` points to conflicting symlink target `{}`; no plan was produced",
                target.display()
            ),
        )),
    }
}

fn is_consumer_bun_registry_store_path(repo_root: &Path, link_path: &Path, target: &Path) -> bool {
    let resolved = if target.is_absolute() {
        target.to_path_buf()
    } else {
        link_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(target)
    };
    let normalized = lexically_normalize(&resolved);
    let store = lexically_normalize(&repo_root.join("node_modules").join(".bun"));
    normalized.starts_with(&store)
}

fn lexically_normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push(component.as_os_str());
                }
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

fn refuse_unmanaged_partial_consumer_closure(
    repo_root: &Path,
    packages: &[BunPackagePlan],
    managed_relink: bool,
) -> Result<(), DepsError> {
    let linked = packages
        .iter()
        .filter(|package| package.consumer_link == BunConsumerLinkDisposition::Linked)
        .count();
    if linked == 0 || linked == packages.len() || managed_relink {
        return Ok(());
    }
    let guidance = bun_override_guidance(
        repo_root,
        packages
            .iter()
            .map(|package| (package.name.as_str(), package.local_path.as_path())),
    );
    Err(DepsError::invalid(
        repo_root,
        format!(
            "Bun consumer has a partial local closure ({linked} of {} packages linked); repair or remove the mixed state before planning\n\n{}",
            packages.len(),
            guidance,
        ),
    ))
}

fn plan_reference_release(
    planned_index: &mut BunRegistrationIndex,
    indexed: Option<&BunRegistration>,
    package_name: &str,
    expected: &Path,
    reference: &BunConsumerReference,
    observed: &BunPathObservation,
) -> BunReferenceRelease {
    let Some(indexed) = indexed else {
        return BunReferenceRelease::Missing;
    };
    if !indexed.consumers.contains(reference) {
        return BunReferenceRelease::Missing;
    }
    let has_other_references = indexed
        .consumers
        .iter()
        .any(|consumer| consumer != reference);
    if has_other_references || !indexed.effigy_created {
        return planned_index.release_reference(package_name, reference);
    }
    if matches!(observed, BunPathObservation::Symlink { target } if same_path(target, expected)) {
        return planned_index.release_reference(package_name, reference);
    }
    BunReferenceRelease::RetainedUnverifiable
}

fn register_intent(package: &BunClosurePackage) -> BunProcessIntent {
    BunProcessIntent {
        action: BunProcessAction::Register,
        packages: vec![package.name.clone()],
        cwd: package.local_path.clone(),
        program: "bun".to_owned(),
        args: vec!["link".to_owned(), "--no-save".to_owned()],
    }
}

fn link_consumer_intent(repo_root: &Path, packages: &[BunClosurePackage]) -> BunProcessIntent {
    let package_names = packages
        .iter()
        .map(|package| package.name.clone())
        .collect::<Vec<_>>();
    let mut args = vec!["link".to_owned()];
    args.extend(package_names.iter().cloned());
    args.push("--no-save".to_owned());
    BunProcessIntent {
        action: BunProcessAction::LinkConsumer,
        packages: package_names,
        cwd: repo_root.to_path_buf(),
        program: "bun".to_owned(),
        args,
    }
}

fn unregister_intent(package: &DependencyPackage) -> BunProcessIntent {
    BunProcessIntent {
        action: BunProcessAction::Unregister,
        packages: vec![package.name.clone()],
        cwd: package.local_path.clone(),
        program: "bun".to_owned(),
        args: vec!["unlink".to_owned(), "--no-save".to_owned()],
    }
}

fn immutable_bun_files<'a>(
    repo_root: &Path,
    library_packages: impl Iterator<Item = &'a Path>,
) -> Result<Vec<BunImmutableFileSnapshot>, DepsError> {
    let mut paths = BTreeSet::from([
        repo_root.join("package.json"),
        repo_root.join("bun.lock"),
        repo_root.join("bun.lockb"),
    ]);
    paths.extend(library_packages.map(|path| path.join("package.json")));
    paths
        .into_iter()
        .map(|path| {
            Ok(BunImmutableFileSnapshot {
                contents: read_optional_bytes(&path)?,
                path,
            })
        })
        .collect()
}

fn state_file_snapshots(
    paths: impl IntoIterator<Item = PathBuf>,
) -> Result<Vec<BunStateFileSnapshot>, DepsError> {
    paths
        .into_iter()
        .map(|path| {
            Ok(BunStateFileSnapshot {
                contents: read_optional_string(&path)?,
                path,
            })
        })
        .collect()
}

fn find_registration<'a>(
    index: &'a BunRegistrationIndex,
    package_name: &str,
) -> Option<&'a BunRegistration> {
    index
        .registrations
        .iter()
        .find(|registration| registration.package_name == package_name)
}

fn bun_registration_path_from_store(
    store: &BunRegistrationIndexStore,
    package_name: &str,
) -> Result<PathBuf, DepsError> {
    let effigy_dir = store.path().ancestors().nth(3).ok_or_else(|| {
        DepsError::invalid(store.path(), "Bun registration index has no home directory")
    })?;
    Ok(bun_registration_path(effigy_dir, package_name))
}

fn plan_link_gitignore(repo_root: &Path) -> Result<(Option<String>, Option<String>), DepsError> {
    let path = repo_root.join(".gitignore");
    let before = read_optional_string(&path)?;
    let mut after = before.clone().unwrap_or_default();
    let covered = after.lines().map(str::trim).any(|line| {
        matches!(
            line,
            ".effigy/"
                | "/.effigy/"
                | ".effigy"
                | "/.effigy"
                | ".effigy/**"
                | "/.effigy/**"
                | "**/.effigy/"
        )
    });
    if !covered {
        if !after.is_empty() && !after.ends_with('\n') {
            after.push('\n');
        }
        after.push_str(".effigy/\n");
    }
    Ok((before, Some(after)))
}

fn push_json_change<T: serde::Serialize>(
    changes: &mut Vec<PlannedChange>,
    path: &Path,
    description: &str,
    after: &T,
) -> Result<(), DepsError> {
    push_optional_json_change(changes, path, description, Some(after))
}

fn push_optional_json_change<T: serde::Serialize>(
    changes: &mut Vec<PlannedChange>,
    path: &Path,
    description: &str,
    after: Option<&T>,
) -> Result<(), DepsError> {
    let before = read_optional_string(path)?;
    let after = after.map(|value| render_json(value, path)).transpose()?;
    push_file_change(changes, path.to_path_buf(), description, before, after);
    Ok(())
}

fn render_json<T: serde::Serialize>(value: &T, path: &Path) -> Result<String, DepsError> {
    let mut rendered = serde_json::to_string_pretty(value)
        .map_err(|error| DepsError::json("render", path, error))?;
    rendered.push('\n');
    Ok(rendered)
}

fn push_file_change(
    changes: &mut Vec<PlannedChange>,
    target: PathBuf,
    description: &str,
    before: Option<String>,
    after: Option<String>,
) {
    if before == after {
        return;
    }
    let action = match (&before, &after) {
        (None, Some(_)) => PlannedChangeAction::Create,
        (Some(_), None) => PlannedChangeAction::Delete,
        (Some(_), Some(_)) => PlannedChangeAction::Update,
        (None, None) => return,
    };
    changes.push(PlannedChange {
        target,
        action,
        description: description.to_owned(),
        before,
        after,
    });
}

fn read_optional_string(path: &Path) -> Result<Option<String>, DepsError> {
    match fs::read_to_string(path) {
        Ok(raw) => Ok(Some(raw)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(DepsError::io("read", path, error)),
    }
}

fn read_optional_bytes(path: &Path) -> Result<Option<Vec<u8>>, DepsError> {
    match fs::read(path) {
        Ok(raw) => Ok(Some(raw)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(DepsError::io("read", path, error)),
    }
}

/// The recorded Bun link an unlink request names.
///
/// Links are keyed by Bun package root, so a repo whose Bun tree sits below
/// the git root records `studio/` rather than the repo. Prefer an exact repo
/// match, accept a single recorded root, and refuse a genuinely ambiguous
/// choice instead of unlinking a tree the caller did not name.
fn select_unlink_link<'a>(
    requested_root: &Path,
    state: &'a crate::RepoLinkState,
    library_path: &Path,
) -> Result<Option<&'a DesiredDependencyLink>, DepsError> {
    let matches = state
        .links
        .iter()
        .filter(|link| {
            link.key.manager == PackageManager::Bun && link.key.library_path == library_path
        })
        .collect::<Vec<_>>();
    if let Some(exact) = matches
        .iter()
        .find(|link| link.key.consumer_repo == requested_root)
    {
        return Ok(Some(exact));
    }
    match matches.as_slice() {
        [] => Ok(None),
        [single] => Ok(Some(single)),
        several => Err(DepsError::invalid(
            requested_root,
            format!(
                "`{}` is linked from {} Bun package roots ({}); re-run with `--repo <PATH>` naming one root",
                library_path.display(),
                several.len(),
                several
                    .iter()
                    .map(|link| link.key.consumer_repo.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )),
    }
}

fn resolve_unlink_library_path(
    repo_root: &Path,
    library_path: &Path,
) -> Result<PathBuf, DepsError> {
    let absolute = if library_path.is_absolute() {
        library_path.to_path_buf()
    } else {
        repo_root.join(library_path)
    };
    if let Ok(path) = canonical_existing_path(&absolute) {
        return Ok(path);
    }
    if let (Some(parent), Some(name)) = (absolute.parent(), absolute.file_name()) {
        if let Ok(parent) = canonical_existing_path(parent) {
            return Ok(parent.join(name));
        }
    }
    if absolute.is_absolute() {
        Ok(absolute)
    } else {
        Err(DepsError::invalid(
            library_path,
            "unlink library path could not be resolved to an absolute identity",
        ))
    }
}

fn same_path(left: &Path, right: &Path) -> bool {
    canonical_or_original(left) == canonical_or_original(right)
}

fn canonical_or_original(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use tempfile::TempDir;

    use super::*;
    use crate::{DependencyDepth, RepoLinkStateStore};

    #[derive(Default)]
    struct FixtureObserver {
        paths: BTreeMap<PathBuf, BunPathObservation>,
        observations: RefCell<Vec<PathBuf>>,
    }

    impl BunPlanObserver for FixtureObserver {
        fn observe_path(&self, path: &Path) -> Result<BunPathObservation, DepsError> {
            self.observations.borrow_mut().push(path.to_path_buf());
            Ok(self
                .paths
                .get(path)
                .cloned()
                .unwrap_or(BunPathObservation::Missing))
        }
    }

    fn write(path: &Path, contents: &[u8]) {
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
        write(&repo.join("package.json"), b"{\"name\":\"consumer\"}\n");
        write(&repo.join("bun.lock"), b"lock-v1\n");
        let library_temp = TempDir::new().unwrap();
        let library = fs::canonicalize(library_temp.path()).unwrap();
        let packages = names
            .iter()
            .map(|(name, _)| {
                let package_path = library.join(name.trim_start_matches('@').replace('/', "-"));
                fs::create_dir_all(&package_path).unwrap();
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

    fn apply_state_changes(plan: &BunDependencyPlan) {
        for change in &plan.operation.changes {
            match &change.after {
                Some(after) => write(&change.target, after.as_bytes()),
                None => fs::remove_file(&change.target).unwrap(),
            }
        }
    }

    #[test]
    fn plans_root_only_library_without_mutation_and_with_exact_invariants() {
        let mut fixture = fixture(&[("underlay", DependencyDepth::Direct)]);
        fixture.packages[0].package_path = fixture.library.clone();
        let package_before = fs::read(fixture.repo.join("package.json")).unwrap();
        let lock_before = fs::read(fixture.repo.join("bun.lock")).unwrap();

        let plan = plan_bun_link(
            &fixture.repo,
            &fixture.library,
            &fixture.packages,
            &fixture.consumer,
            &fixture.home,
            true,
            &FixtureObserver::default(),
        )
        .unwrap();

        assert_eq!(plan.packages.len(), 1);
        assert_eq!(
            plan.packages[0].registration,
            BunRegistrationDisposition::Absent
        );
        assert_eq!(plan.process_intents.len(), 2);
        assert!(plan.process_intents.iter().all(|intent| {
            intent.args.contains(&"--no-save".to_owned())
                && !intent.args.contains(&"--save".to_owned())
        }));
        assert_eq!(plan.immutable_files.len(), 4);
        assert_eq!(
            plan.immutable_files
                .iter()
                .find(|snapshot| snapshot.path == fixture.repo.join("package.json"))
                .unwrap()
                .contents,
            Some(package_before.clone())
        );
        assert_eq!(
            plan.immutable_files
                .iter()
                .find(|snapshot| snapshot.path == fixture.repo.join("bun.lock"))
                .unwrap()
                .contents,
            Some(lock_before.clone())
        );
        assert!(plan
            .immutable_files
            .iter()
            .find(|snapshot| snapshot.path == fixture.repo.join("bun.lockb"))
            .unwrap()
            .contents
            .is_none());
        assert!(plan
            .operation
            .changes
            .iter()
            .any(|change| { change.target == RepoLinkStateStore::for_repo(&fixture.repo).path() }));
        assert!(plan.operation.changes.iter().any(|change| {
            change.target == BunRegistrationIndexStore::for_home(&fixture.home).path()
        }));
        assert_eq!(
            fs::read(fixture.repo.join("package.json")).unwrap(),
            package_before
        );
        assert_eq!(
            fs::read(fixture.repo.join("bun.lock")).unwrap(),
            lock_before
        );
        assert!(!RepoLinkStateStore::for_repo(&fixture.repo).path().exists());
        assert!(!BunRegistrationIndexStore::for_home(&fixture.home)
            .path()
            .exists());
    }

    #[test]
    fn plans_complete_direct_and_transitive_closure_deterministically() {
        let fixture = fixture(&[
            ("@signal/core", DependencyDepth::Direct),
            ("@signal/protocol", DependencyDepth::Transitive),
        ]);
        let plan = plan_bun_link(
            &fixture.repo,
            &fixture.library,
            &fixture.packages,
            &fixture.consumer,
            &fixture.home,
            true,
            &FixtureObserver::default(),
        )
        .unwrap();

        assert_eq!(
            plan.packages
                .iter()
                .map(|package| (&package.name, package.depth))
                .collect::<Vec<_>>(),
            [
                (&"@signal/core".to_owned(), Some(DependencyDepth::Direct)),
                (
                    &"@signal/protocol".to_owned(),
                    Some(DependencyDepth::Transitive)
                ),
            ]
        );
        assert_eq!(
            plan.process_intents
                .iter()
                .filter(|intent| intent.action == BunProcessAction::LinkConsumer)
                .count(),
            1
        );
        let consumer_intent = plan
            .process_intents
            .iter()
            .find(|intent| intent.action == BunProcessAction::LinkConsumer)
            .unwrap();
        assert_eq!(
            consumer_intent.args,
            ["link", "@signal/core", "@signal/protocol", "--no-save"]
        );
    }

    #[test]
    fn no_match_duplicate_resolution_and_partial_link_return_no_plan() {
        let mut no_match = fixture(&[("@signal/core", DependencyDepth::Direct)]);
        no_match.consumer.library_matches.clear();
        assert!(plan_bun_link(
            &no_match.repo,
            &no_match.library,
            &no_match.packages,
            &no_match.consumer,
            &no_match.home,
            true,
            &FixtureObserver::default(),
        )
        .is_err());
        assert!(!RepoLinkStateStore::for_repo(&no_match.repo).path().exists());

        let mut duplicate = fixture(&[
            ("@signal/core", DependencyDepth::Direct),
            ("@signal/protocol", DependencyDepth::Transitive),
        ]);
        duplicate
            .consumer
            .library_matches
            .push(duplicate.consumer.library_matches[0].clone());
        duplicate.consumer.library_matches[2].0.version = Some("2.0.0".to_owned());
        let error = plan_bun_link(
            &duplicate.repo,
            &duplicate.library,
            &duplicate.packages,
            &duplicate.consumer,
            &duplicate.home,
            true,
            &FixtureObserver::default(),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("A consumer-level Bun override is the transitive mechanism"));
        for package in &duplicate.packages {
            assert!(error.contains(&format!(
                "{}: {}",
                serde_json::to_string(&package.name).unwrap(),
                serde_json::to_string(&bun_file_spec(&duplicate.repo, &package.package_path))
                    .unwrap()
            )));
        }
        assert!(error.contains("then run `bun install`"));
        assert!(error.contains("remains save-less and did not modify the manifest"));

        let partial = fixture(&[
            ("@signal/core", DependencyDepth::Direct),
            ("@signal/protocol", DependencyDepth::Transitive),
        ]);
        let observer = FixtureObserver {
            paths: BTreeMap::from([(
                partial.repo.join("node_modules/@signal/core"),
                BunPathObservation::Symlink {
                    target: partial.packages[0].package_path.clone(),
                },
            )]),
            ..FixtureObserver::default()
        };
        let error = plan_bun_link(
            &partial.repo,
            &partial.library,
            &partial.packages,
            &partial.consumer,
            &partial.home,
            true,
            &observer,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("partial local closure (1 of 2 packages linked)"));
        assert!(error.contains("A consumer-level Bun override is the transitive mechanism"));
        for package in &partial.packages {
            assert!(error.contains(&format!(
                "{}: {}",
                serde_json::to_string(&package.name).unwrap(),
                serde_json::to_string(&bun_file_spec(&partial.repo, &package.package_path))
                    .unwrap()
            )));
        }
    }

    #[test]
    fn bun_file_spec_is_relative_to_the_consumer_manifest() {
        assert_eq!(
            bun_file_spec(
                Path::new("/projects/soundcheck"),
                Path::new("/projects/poodle/packages/core")
            ),
            "file:../poodle/packages/core"
        );
        assert_eq!(
            bun_file_spec(
                Path::new("/projects/soundcheck"),
                Path::new("/projects/soundcheck/packages/local")
            ),
            "file:./packages/local"
        );
    }

    #[test]
    fn matching_foreign_registration_is_used_but_never_claimed() {
        let fixture = fixture(&[("underlay", DependencyDepth::Direct)]);
        let observer = FixtureObserver {
            paths: BTreeMap::from([(
                bun_registration_path(&fixture.home, "underlay"),
                BunPathObservation::Symlink {
                    target: fixture.packages[0].package_path.clone(),
                },
            )]),
            ..FixtureObserver::default()
        };
        let plan = plan_bun_link(
            &fixture.repo,
            &fixture.library,
            &fixture.packages,
            &fixture.consumer,
            &fixture.home,
            true,
            &observer,
        )
        .unwrap();
        assert_eq!(
            plan.packages[0].registration,
            BunRegistrationDisposition::MatchingForeign
        );
        assert!(!plan
            .process_intents
            .iter()
            .any(|intent| intent.action == BunProcessAction::Register));
        let index_change = plan
            .operation
            .changes
            .iter()
            .find(|change| {
                change.target == BunRegistrationIndexStore::for_home(&fixture.home).path()
            })
            .unwrap();
        let index: BunRegistrationIndex =
            serde_json::from_str(index_change.after.as_ref().unwrap()).unwrap();
        assert!(!index.registrations[0].effigy_created);
    }

    #[test]
    fn registry_package_symlinks_are_replaceable() {
        let fixture = fixture(&[("underlay", DependencyDepth::Direct)]);
        let observer = FixtureObserver {
            paths: BTreeMap::from([(
                fixture.repo.join("node_modules").join("underlay"),
                BunPathObservation::Symlink {
                    target: fixture
                        .repo
                        .join("node_modules/.bun/underlay@1.2.3/node_modules/underlay"),
                },
            )]),
            ..FixtureObserver::default()
        };

        let plan = plan_bun_link(
            &fixture.repo,
            &fixture.library,
            &fixture.packages,
            &fixture.consumer,
            &fixture.home,
            true,
            &observer,
        )
        .expect("registry symlink should be replaceable");
        assert_eq!(
            plan.packages[0].consumer_link,
            BunConsumerLinkDisposition::Registry
        );
        assert!(plan
            .process_intents
            .iter()
            .any(|intent| intent.action == BunProcessAction::LinkConsumer));
    }

    #[test]
    fn foreign_bun_store_symlinks_remain_conflicts() {
        let fixture = fixture(&[("underlay", DependencyDepth::Direct)]);
        let observer = FixtureObserver {
            paths: BTreeMap::from([(
                fixture.repo.join("node_modules").join("underlay"),
                BunPathObservation::Symlink {
                    target: PathBuf::from("/foreign/node_modules/.bun/underlay"),
                },
            )]),
            ..FixtureObserver::default()
        };

        let error = plan_bun_link(
            &fixture.repo,
            &fixture.library,
            &fixture.packages,
            &fixture.consumer,
            &fixture.home,
            true,
            &observer,
        )
        .expect_err("foreign .bun symlink must stay a conflict");
        assert!(
            error
                .to_string()
                .contains("conflicting symlink target `/foreign/node_modules/.bun/underlay`"),
            "got {error}"
        );
    }

    #[test]
    fn escaped_bun_store_symlink_targets_remain_conflicts() {
        let fixture = fixture(&[("underlay", DependencyDepth::Direct)]);
        let escaped = fixture.repo.join("node_modules/.bun/../../foreign-package");
        let observer = FixtureObserver {
            paths: BTreeMap::from([(
                fixture.repo.join("node_modules").join("underlay"),
                BunPathObservation::Symlink {
                    target: escaped.clone(),
                },
            )]),
            ..FixtureObserver::default()
        };

        let error = plan_bun_link(
            &fixture.repo,
            &fixture.library,
            &fixture.packages,
            &fixture.consumer,
            &fixture.home,
            true,
            &observer,
        )
        .expect_err("escaped .bun symlink must stay a conflict");
        assert!(
            error.to_string().contains("conflicting symlink target"),
            "got {error}"
        );
        assert!(error.to_string().contains("foreign-package"), "got {error}");
    }

    #[test]
    fn conflicting_registration_refuses_every_delta() {
        let fixture = fixture(&[("underlay", DependencyDepth::Direct)]);
        let observer = FixtureObserver {
            paths: BTreeMap::from([(
                bun_registration_path(&fixture.home, "underlay"),
                BunPathObservation::Symlink {
                    target: fixture.library.join("other"),
                },
            )]),
            ..FixtureObserver::default()
        };
        assert!(plan_bun_link(
            &fixture.repo,
            &fixture.library,
            &fixture.packages,
            &fixture.consumer,
            &fixture.home,
            true,
            &observer,
        )
        .is_err());
        assert!(!RepoLinkStateStore::for_repo(&fixture.repo).path().exists());
        assert!(!BunRegistrationIndexStore::for_home(&fixture.home)
            .path()
            .exists());
    }

    #[test]
    fn stale_foreign_registration_is_reported_without_being_claimed() {
        let fixture = fixture(&[("underlay", DependencyDepth::Direct)]);
        let store = BunRegistrationIndexStore::for_home(&fixture.home);
        store
            .update(|index| {
                index.add_reference(
                    "underlay",
                    fixture.packages[0].package_path.clone(),
                    false,
                    BunConsumerReference {
                        consumer_repo: fixture.repo.join("other-consumer"),
                        library_path: fixture.library.clone(),
                    },
                )
            })
            .unwrap();
        let before = fs::read(store.path()).unwrap();

        let error = plan_bun_link(
            &fixture.repo,
            &fixture.library,
            &fixture.packages,
            &fixture.consumer,
            &fixture.home,
            true,
            &FixtureObserver::default(),
        )
        .unwrap_err();

        assert!(error.to_string().contains("recreate it manually"));
        assert_eq!(fs::read(store.path()).unwrap(), before);
        assert!(!RepoLinkStateStore::for_repo(&fixture.repo).path().exists());
    }

    #[test]
    fn unlink_releases_only_selected_reference_and_preserves_shared_registration() {
        let fixture = fixture(&[("underlay", DependencyDepth::Direct)]);
        let link = plan_bun_link(
            &fixture.repo,
            &fixture.library,
            &fixture.packages,
            &fixture.consumer,
            &fixture.home,
            true,
            &FixtureObserver::default(),
        )
        .unwrap();
        apply_state_changes(&link);
        let other = BunConsumerReference {
            consumer_repo: fixture.repo.join("other-consumer"),
            library_path: fixture.library.clone(),
        };
        let index_store = BunRegistrationIndexStore::for_home(&fixture.home);
        index_store
            .update(|index| {
                index.add_reference(
                    "underlay",
                    fixture.packages[0].package_path.clone(),
                    true,
                    other.clone(),
                )
            })
            .unwrap();
        let observer = FixtureObserver {
            paths: BTreeMap::from([
                (
                    bun_registration_path(&fixture.home, "underlay"),
                    BunPathObservation::Symlink {
                        target: fixture.packages[0].package_path.clone(),
                    },
                ),
                (
                    fixture.repo.join("node_modules/underlay"),
                    BunPathObservation::Symlink {
                        target: fixture.packages[0].package_path.clone(),
                    },
                ),
            ]),
            ..FixtureObserver::default()
        };
        let unlink = plan_bun_unlink(
            &fixture.repo,
            &fixture.library,
            &fixture.home,
            true,
            &observer,
        )
        .unwrap();

        assert_eq!(
            unlink.packages[0].reference_release,
            Some(BunReferenceRelease::RetainedShared)
        );
        assert_eq!(
            unlink.packages[0].registration,
            BunRegistrationDisposition::MatchingOwnedShared
        );
        assert!(unlink.process_intents.is_empty());
        assert_eq!(unlink.symlink_intents.len(), 1);
        let index_change = unlink
            .operation
            .changes
            .iter()
            .find(|change| change.target == index_store.path())
            .unwrap();
        let index: BunRegistrationIndex =
            serde_json::from_str(index_change.after.as_ref().unwrap()).unwrap();
        assert_eq!(index.registrations[0].consumers, [other]);
    }

    #[test]
    fn unlink_removes_owned_last_registration_but_retains_foreign_and_stale() {
        for (foreign, registration_present, expected_release, unregisters) in [
            (false, true, BunReferenceRelease::RemoveOwned, true),
            (true, true, BunReferenceRelease::RetainedForeign, false),
            (
                false,
                false,
                BunReferenceRelease::RetainedUnverifiable,
                false,
            ),
        ] {
            let fixture = fixture(&[("underlay", DependencyDepth::Direct)]);
            let link = plan_bun_link(
                &fixture.repo,
                &fixture.library,
                &fixture.packages,
                &fixture.consumer,
                &fixture.home,
                true,
                &FixtureObserver::default(),
            )
            .unwrap();
            apply_state_changes(&link);
            if foreign {
                let store = BunRegistrationIndexStore::for_home(&fixture.home);
                store
                    .update(|index| {
                        index.registrations[0].effigy_created = false;
                        Ok(())
                    })
                    .unwrap();
            }
            let mut observer = FixtureObserver::default();
            if registration_present {
                observer.paths.insert(
                    bun_registration_path(&fixture.home, "underlay"),
                    BunPathObservation::Symlink {
                        target: fixture.packages[0].package_path.clone(),
                    },
                );
            }
            let unlink = plan_bun_unlink(
                &fixture.repo,
                &fixture.library,
                &fixture.home,
                true,
                &observer,
            )
            .unwrap();
            assert_eq!(unlink.packages[0].reference_release, Some(expected_release));
            assert_eq!(
                unlink
                    .process_intents
                    .iter()
                    .any(|intent| intent.action == BunProcessAction::Unregister),
                unregisters
            );
        }
    }
}
