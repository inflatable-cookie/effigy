use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::{
    canonical_existing_path, CargoDependencyPlan, CargoExpectedResolution, CargoLibraryInventory,
    CargoLinkOwnership, CargoWorkspaceInventory, CommittedSource, CommittedSourceKind,
    ConsumerRoot, DependencyLinkKey, DependencyLinkPlan, DependencyPackage, DepsError,
    DesiredDependencyLink, LinkMechanism, MatchDisposition, PackageManager, PlanAction,
    PlannedChange, PlannedChangeAction, ProcessRequest, ReadOnlyProcess, RepoLinkState,
    RepoLinkStateStore,
};

const CARGO_MARKER_START_PREFIX: &str = "# >>> effigy deps cargo ";
const CARGO_MARKER_START_SUFFIX: &str = " >>>";
const CARGO_MARKER_END_PREFIX: &str = "# <<< effigy deps cargo ";
const CARGO_MARKER_END_SUFFIX: &str = " <<<";

pub trait CargoPlanObserver {
    fn is_tracked(&self, repo_root: &Path, path: &Path) -> Result<bool, DepsError>;
    fn is_dirty(&self, repo_root: &Path, path: &Path) -> Result<bool, DepsError>;
}

pub struct GitCargoPlanObserver<'a, P> {
    process: &'a P,
}

impl<'a, P> GitCargoPlanObserver<'a, P> {
    pub fn new(process: &'a P) -> Self {
        Self { process }
    }
}

impl<P: ReadOnlyProcess> CargoPlanObserver for GitCargoPlanObserver<'_, P> {
    fn is_tracked(&self, repo_root: &Path, path: &Path) -> Result<bool, DepsError> {
        let relative = repo_relative(repo_root, path)?;
        let output = self.process.run(&ProcessRequest {
            program: "git".to_owned(),
            args: vec![
                "ls-files".to_owned(),
                "--full-name".to_owned(),
                "--".to_owned(),
                relative.display().to_string(),
            ],
            cwd: repo_root.to_path_buf(),
        })?;
        Ok(!output.stdout.trim().is_empty())
    }

    fn is_dirty(&self, repo_root: &Path, path: &Path) -> Result<bool, DepsError> {
        let relative = repo_relative(repo_root, path)?;
        let output = self.process.run(&ProcessRequest {
            program: "git".to_owned(),
            args: vec![
                "status".to_owned(),
                "--porcelain=v1".to_owned(),
                "--".to_owned(),
                relative.display().to_string(),
            ],
            cwd: repo_root.to_path_buf(),
        })?;
        Ok(!output.stdout.trim().is_empty())
    }
}

pub fn plan_cargo_link(
    repo_root: impl AsRef<Path>,
    library: &CargoLibraryInventory,
    workspaces: &[CargoWorkspaceInventory],
    dry_run: bool,
    observer: &impl CargoPlanObserver,
) -> Result<CargoDependencyPlan, DepsError> {
    let repo_root = canonical_existing_path(repo_root)?;
    let library_root = canonical_existing_path(&library.root)?;
    let key = DependencyLinkKey {
        manager: PackageManager::Cargo,
        consumer_repo: repo_root.clone(),
        library_path: library_root.clone(),
    };
    let (consumer_roots, packages, patch_groups, expected_resolutions) =
        cargo_closure(&repo_root, library, workspaces)?;
    let state_store = RepoLinkStateStore::for_checkout(&repo_root);
    let state = state_store.read()?;
    let lockfile_guard_packages = cargo_link_package_names(state.links.iter());
    let previous = state.links.iter().find(|link| link.key == key);
    let config_path = repo_root.join(".cargo/config.toml");
    let cargo_dir = repo_root.join(".cargo");
    let config_before = read_optional_string(&config_path)?;
    refuse_tracked_config(&repo_root, &config_path, observer)?;
    let blocks = parse_managed_blocks(config_before.as_deref().unwrap_or(""), &config_path)?;
    let own_block = select_owned_block(&blocks, &library_root, &config_path)?;
    if own_block.is_some() && previous.is_none() {
        return Err(DepsError::invalid(
            &config_path,
            format!(
                "managed Cargo block for `{}` has no desired-state ledger entry; refusing to claim it",
                library_root.display()
            ),
        ));
    }
    let config_without_own = remove_block(config_before.as_deref().unwrap_or(""), own_block);
    let (config_without_adopted, adopted_patch_tables) = adopt_compatible_patch_tables(
        &repo_root,
        &config_without_own,
        &patch_groups,
        &config_path,
    )?;
    refuse_patch_collisions(&config_without_adopted, &patch_groups, &config_path)?;
    let rendered_block = render_managed_block(&library_root, &patch_groups);
    let config_after = Some(match own_block {
        Some(block) => replace_block(
            config_before.as_deref().unwrap_or(""),
            block,
            &rendered_block,
        ),
        None => append_block(&config_without_adopted, &rendered_block),
    });

    let cargo_ownership =
        previous
            .and_then(|link| link.cargo_ownership)
            .unwrap_or(CargoLinkOwnership {
                config_created_by_effigy: config_before.is_none(),
                cargo_dir_created_by_effigy: !cargo_dir.exists(),
            });
    let desired = DesiredDependencyLink {
        key: key.clone(),
        mechanism: LinkMechanism::CargoPatch,
        consumer_roots,
        packages,
        cargo_resolutions: expected_resolutions.clone(),
        cargo_ownership: Some(cargo_ownership),
    };

    let mut planned_state = state.clone();
    planned_state.links.retain(|link| link.key != key);
    planned_state.links.push(desired.clone());
    planned_state.normalize();
    let ledger_before = read_optional_string(state_store.path())?;
    let ledger_after = Some(render_repo_state(&planned_state, state_store.path())?);
    let (gitignore_before, gitignore_after) = plan_link_gitignore(&repo_root)?;
    let affected_lockfiles = affected_lockfiles_for_workspaces(
        &repo_root,
        desired
            .consumer_roots
            .iter()
            .map(|root| root.canonical_path.as_path()),
        lockfile_guard_packages.is_empty(),
        observer,
    )?;

    let mut changes = Vec::new();
    push_file_change(
        &mut changes,
        config_path,
        "refresh the Effigy-managed Cargo patch block",
        config_before,
        config_after,
    );
    push_file_change(
        &mut changes,
        repo_root.join(".gitignore"),
        "ensure machine-local dependency state and Cargo config are ignored",
        gitignore_before,
        gitignore_after,
    );
    push_file_change(
        &mut changes,
        state_store.path().to_path_buf(),
        "record the desired Cargo dependency link",
        ledger_before,
        ledger_after,
    );

    let mut warnings = vec![
        "Cargo verification or builds may rewrite affected Cargo.lock entries to local path sources; do not commit that linked lock state"
            .to_owned(),
    ];
    if adopted_patch_tables > 0 {
        warnings.push(format!(
            "adopting {adopted_patch_tables} compatible hand-managed Cargo patch table(s) into Effigy-managed state"
        ));
    }

    Ok(CargoDependencyPlan {
        desired: Some(desired),
        operation: DependencyLinkPlan {
            action: PlanAction::Link,
            dry_run,
            key,
            changes,
            warnings,
        },
        expected_resolutions,
        affected_lockfiles,
        lockfile_guard_packages,
        remaining_linked_packages: Vec::new(),
        remove_empty_directories: Vec::new(),
    })
}

pub fn plan_cargo_unlink(
    repo_root: impl AsRef<Path>,
    library_path: impl AsRef<Path>,
    dry_run: bool,
    observer: &impl CargoPlanObserver,
) -> Result<CargoDependencyPlan, DepsError> {
    let repo_root = canonical_existing_path(repo_root)?;
    let library_path = resolve_unlink_library_path(&repo_root, library_path.as_ref())?;
    let key = DependencyLinkKey {
        manager: PackageManager::Cargo,
        consumer_repo: repo_root.clone(),
        library_path: library_path.clone(),
    };
    let state_store = RepoLinkStateStore::for_checkout(&repo_root);
    let state = state_store.read()?;
    let desired = state.links.iter().find(|link| link.key == key).cloned();
    let config_path = repo_root.join(".cargo/config.toml");
    let config_before = read_optional_string(&config_path)?;
    refuse_tracked_config(&repo_root, &config_path, observer)?;
    let blocks = parse_managed_blocks(config_before.as_deref().unwrap_or(""), &config_path)?;
    let own_block = select_owned_block(&blocks, &library_path, &config_path)?;

    if desired.is_none() {
        if own_block.is_some() {
            return Err(DepsError::invalid(
                &config_path,
                format!(
                    "managed Cargo block for `{}` has no desired-state ledger entry; refusing unowned removal",
                    library_path.display()
                ),
            ));
        }
        return Ok(CargoDependencyPlan {
            desired: None,
            operation: DependencyLinkPlan {
                action: PlanAction::Unlink,
                dry_run,
                key,
                changes: Vec::new(),
                warnings: vec![
                    "Cargo dependency link is already absent; nothing to remove".to_owned()
                ],
            },
            expected_resolutions: Vec::new(),
            affected_lockfiles: Vec::new(),
            lockfile_guard_packages: Vec::new(),
            remaining_linked_packages: Vec::new(),
            remove_empty_directories: Vec::new(),
        });
    }
    let desired = desired.expect("checked desired state");
    let expected_resolutions = desired_cargo_resolutions(&desired);
    let lockfile_guard_packages = cargo_link_package_names(state.links.iter());
    let ownership = desired.cargo_ownership.unwrap_or(CargoLinkOwnership {
        config_created_by_effigy: false,
        cargo_dir_created_by_effigy: false,
    });
    let config_remainder = own_block
        .map(|block| remove_block(config_before.as_deref().unwrap_or(""), Some(block)))
        .unwrap_or_else(|| config_before.clone().unwrap_or_default());
    let config_after = if config_before.is_some()
        && config_remainder.trim().is_empty()
        && ownership.config_created_by_effigy
    {
        None
    } else {
        config_before.as_ref().map(|_| config_remainder)
    };

    let mut planned_state = state.clone();
    planned_state.links.retain(|link| link.key != key);
    planned_state.normalize();
    let remaining_linked_packages = cargo_link_package_names(planned_state.links.iter());
    let ledger_before = read_optional_string(state_store.path())?;
    let ledger_after = if planned_state.links.is_empty() {
        None
    } else {
        Some(render_repo_state(&planned_state, state_store.path())?)
    };
    let affected_lockfiles = affected_lockfiles_for_workspaces(
        &repo_root,
        desired
            .consumer_roots
            .iter()
            .map(|root| root.canonical_path.as_path()),
        false,
        observer,
    )?;

    let mut changes = Vec::new();
    push_file_change(
        &mut changes,
        config_path.clone(),
        "remove only the Effigy-managed Cargo patch block",
        config_before,
        config_after.clone(),
    );
    push_file_change(
        &mut changes,
        state_store.path().to_path_buf(),
        "remove the desired Cargo dependency link",
        ledger_before,
        ledger_after,
    );
    let remove_empty_directories = if config_after.is_none()
        && ownership.cargo_dir_created_by_effigy
        && cargo_dir_has_no_foreign_entries(&repo_root.join(".cargo"))?
    {
        vec![repo_root.join(".cargo")]
    } else {
        Vec::new()
    };

    Ok(CargoDependencyPlan {
        desired: None,
        operation: DependencyLinkPlan {
            action: PlanAction::Unlink,
            dry_run,
            key,
            changes,
            warnings: vec![
                "unlink apply must re-resolve affected lockfiles from committed git sources without Git restore commands"
                    .to_owned(),
            ],
        },
        expected_resolutions,
        affected_lockfiles,
        lockfile_guard_packages,
        remaining_linked_packages,
        remove_empty_directories,
    })
}

pub(crate) fn cargo_config_patches_library(
    repo_root: &Path,
    library_root: &Path,
) -> Result<bool, DepsError> {
    let config_path = repo_root.join(".cargo/config.toml");
    let Some(raw) = read_optional_string(&config_path)? else {
        return Ok(false);
    };
    let config: toml::Value = toml::from_str(&raw).map_err(|error| {
        DepsError::invalid(
            &config_path,
            format!("failed to parse existing Cargo config: {error}"),
        )
    })?;
    let Some(patch) = config.get("patch").and_then(toml::Value::as_table) else {
        return Ok(false);
    };
    for source in patch.values().filter_map(toml::Value::as_table) {
        for entry in source.values().filter_map(toml::Value::as_table) {
            let Some(path) = entry.get("path").and_then(toml::Value::as_str) else {
                continue;
            };
            let candidate = if Path::new(path).is_absolute() {
                PathBuf::from(path)
            } else {
                repo_root.join(path)
            };
            if canonical_existing_path(candidate).is_ok_and(|path| path.starts_with(library_root)) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

pub(crate) fn plan_adopted_cargo_unlink(
    link_plan: CargoDependencyPlan,
) -> Result<CargoDependencyPlan, DepsError> {
    let desired = link_plan.desired.as_ref().ok_or_else(|| {
        DepsError::invalid(
            &link_plan.operation.key.consumer_repo,
            "manual Cargo patch adoption produced no desired link state",
        )
    })?;
    let config_path = link_plan
        .operation
        .key
        .consumer_repo
        .join(".cargo/config.toml");
    let config_change = link_plan
        .operation
        .changes
        .iter()
        .find(|change| change.target == config_path)
        .ok_or_else(|| {
            DepsError::invalid(
                &config_path,
                "manual Cargo patch adoption produced no config change",
            )
        })?;
    let managed = config_change.after.as_deref().ok_or_else(|| {
        DepsError::invalid(
            &config_path,
            "manual Cargo patch adoption did not produce managed config",
        )
    })?;
    let blocks = parse_managed_blocks(managed, &config_path)?;
    let block = select_owned_block(&blocks, &link_plan.operation.key.library_path, &config_path)?;
    let block = block.ok_or_else(|| {
        DepsError::invalid(
            &config_path,
            "manual Cargo patch adoption did not produce an owned block",
        )
    })?;
    let config_after = remove_block(managed, Some(block));
    let mut changes = Vec::new();
    push_file_change(
        &mut changes,
        config_path,
        "remove the compatible hand-managed Cargo patch",
        config_change.before.clone(),
        Some(config_after),
    );
    let lockfile_guard_packages = desired
        .packages
        .iter()
        .map(|package| package.name.clone())
        .collect();

    Ok(CargoDependencyPlan {
        desired: None,
        operation: DependencyLinkPlan {
            action: PlanAction::Unlink,
            dry_run: link_plan.operation.dry_run,
            key: link_plan.operation.key,
            changes,
            warnings: vec![
                "removing a compatible pre-Effigy Cargo patch that resolves only to the requested local library"
                    .to_owned(),
                "unlink apply must re-resolve affected lockfiles from committed git sources without Git restore commands"
                    .to_owned(),
            ],
        },
        expected_resolutions: link_plan.expected_resolutions,
        affected_lockfiles: link_plan.affected_lockfiles,
        lockfile_guard_packages,
        remaining_linked_packages: Vec::new(),
        remove_empty_directories: Vec::new(),
    })
}

fn cargo_link_package_names<'a>(
    links: impl Iterator<Item = &'a DesiredDependencyLink>,
) -> Vec<String> {
    links
        .filter(|link| link.mechanism == LinkMechanism::CargoPatch)
        .flat_map(|link| link.packages.iter().map(|package| package.name.clone()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn desired_cargo_resolutions(desired: &DesiredDependencyLink) -> Vec<CargoExpectedResolution> {
    if !desired.cargo_resolutions.is_empty() {
        return desired.cargo_resolutions.clone();
    }

    desired
        .consumer_roots
        .iter()
        .flat_map(|root| {
            desired.packages.iter().flat_map(move |package| {
                package
                    .committed_sources
                    .iter()
                    .filter(|source| source.kind == CommittedSourceKind::Git)
                    .map(move |source| CargoExpectedResolution {
                        consumer_root: root.canonical_path.clone(),
                        package: package.name.clone(),
                        committed_source: source.clone(),
                        local_path: package.local_path.clone(),
                    })
            })
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

type PatchGroups = BTreeMap<String, BTreeMap<String, PathBuf>>;
type CargoClosure = (
    Vec<ConsumerRoot>,
    Vec<DependencyPackage>,
    PatchGroups,
    Vec<CargoExpectedResolution>,
);

fn cargo_closure(
    repo_root: &Path,
    library: &CargoLibraryInventory,
    workspaces: &[CargoWorkspaceInventory],
) -> Result<CargoClosure, DepsError> {
    let mut local_packages = BTreeMap::<String, PathBuf>::new();
    for package in &library.packages {
        let package_path = package.manifest_path.parent().ok_or_else(|| {
            DepsError::invalid(
                &package.manifest_path,
                "Cargo package manifest has no parent",
            )
        })?;
        let package_path = canonical_existing_path(package_path)?;
        if let Some(existing) = local_packages.insert(package.name.clone(), package_path.clone()) {
            if existing != package_path {
                return Err(DepsError::invalid(
                    &library.root,
                    format!(
                        "library contains duplicate Cargo package name `{}`",
                        package.name
                    ),
                ));
            }
        }
    }

    let mut bad_matches = BTreeMap::<MatchDisposition, BTreeSet<String>>::new();
    let mut consumer_roots = BTreeSet::new();
    let mut package_sources = BTreeMap::<String, BTreeSet<CommittedSource>>::new();
    let mut patch_groups = PatchGroups::new();
    let mut expected_resolutions = BTreeSet::new();
    for workspace in workspaces {
        repo_relative(repo_root, &workspace.root)?;
        let mut workspace_matched = false;
        for candidate in &workspace.library_matches {
            if candidate.disposition != MatchDisposition::Git {
                bad_matches
                    .entry(candidate.disposition)
                    .or_default()
                    .insert(candidate.package.name.clone());
                continue;
            }
            let local_path = local_packages.get(&candidate.package.name).ok_or_else(|| {
                DepsError::invalid(
                    &library.root,
                    format!(
                        "resolved Cargo package `{}` is absent from the local library inventory",
                        candidate.package.name
                    ),
                )
            })?;
            let source = candidate.package.source.clone().ok_or_else(|| {
                DepsError::invalid(
                    &candidate.package.manifest_path,
                    format!(
                        "Cargo package `{}` has no committed source",
                        candidate.package.name
                    ),
                )
            })?;
            if source.kind != CommittedSourceKind::Git {
                return Err(DepsError::invalid(
                    &candidate.package.manifest_path,
                    format!(
                        "Cargo package `{}` is not resolved from git",
                        candidate.package.name
                    ),
                ));
            }
            patch_groups
                .entry(source.identity.clone())
                .or_default()
                .insert(candidate.package.name.clone(), local_path.clone());
            package_sources
                .entry(candidate.package.name.clone())
                .or_default()
                .insert(source.clone());
            expected_resolutions.insert(CargoExpectedResolution {
                consumer_root: workspace.root.clone(),
                package: candidate.package.name.clone(),
                committed_source: source,
                local_path: local_path.clone(),
            });
            workspace_matched = true;
        }
        if workspace_matched {
            consumer_roots.insert(workspace.root.clone());
        }
    }
    if !bad_matches.is_empty() {
        let details = bad_matches
            .into_iter()
            .map(|(disposition, packages)| {
                format!(
                    "{}: {}",
                    disposition_label(disposition),
                    packages.into_iter().collect::<Vec<_>>().join(", ")
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        return Err(DepsError::invalid(
            repo_root,
            format!("Cargo dependency closure is not linkable ({details}); no plan was produced"),
        ));
    }
    if patch_groups.is_empty() {
        return Err(DepsError::invalid(
            repo_root,
            "consumer has no git dependencies matching the local Cargo library; no plan was produced",
        ));
    }

    let packages = package_sources
        .into_iter()
        .map(|(name, committed_sources)| DependencyPackage {
            local_path: local_packages[&name].clone(),
            name,
            committed_sources: committed_sources.into_iter().collect(),
        })
        .collect();
    let consumer_roots = consumer_roots
        .into_iter()
        .map(|canonical_path| ConsumerRoot { canonical_path })
        .collect();
    Ok((
        consumer_roots,
        packages,
        patch_groups,
        expected_resolutions.into_iter().collect(),
    ))
}

fn disposition_label(disposition: MatchDisposition) -> &'static str {
    match disposition {
        MatchDisposition::Git => "git",
        MatchDisposition::PreMigrationPath => "pre-migration path dependency",
        MatchDisposition::Registry => "registry dependency",
        MatchDisposition::Unmatched => "unmatched name collision",
    }
}

fn refuse_tracked_config(
    repo_root: &Path,
    config_path: &Path,
    observer: &impl CargoPlanObserver,
) -> Result<(), DepsError> {
    if observer.is_tracked(repo_root, config_path)? {
        return Err(DepsError::invalid(
            config_path,
            "`.cargo/config.toml` is tracked by Git; refusing machine-local patch planning",
        ));
    }
    Ok(())
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

fn affected_lockfiles_for_workspaces<'a>(
    repo_root: &Path,
    roots: impl Iterator<Item = &'a Path>,
    refuse_dirty: bool,
    observer: &impl CargoPlanObserver,
) -> Result<Vec<PathBuf>, DepsError> {
    let mut lockfiles = BTreeSet::new();
    for root in roots {
        let lockfile = root.join("Cargo.lock");
        repo_relative(repo_root, &lockfile)?;
        if observer.is_tracked(repo_root, &lockfile)? {
            if refuse_dirty && observer.is_dirty(repo_root, &lockfile)? {
                return Err(DepsError::invalid(
                    &lockfile,
                    "affected tracked Cargo.lock is already dirty; commit, stash, or resolve it before linking",
                ));
            }
            lockfiles.insert(lockfile);
        }
    }
    Ok(lockfiles.into_iter().collect())
}

#[derive(Debug, Clone, Copy)]
struct ManagedBlock<'a> {
    identity: &'a str,
    start: usize,
    end: usize,
}

fn parse_managed_blocks<'a>(raw: &'a str, path: &Path) -> Result<Vec<ManagedBlock<'a>>, DepsError> {
    let mut blocks = Vec::new();
    let mut open: Option<(&str, usize)> = None;
    let mut offset = 0;
    for line in raw.split_inclusive('\n') {
        let trimmed = line.trim();
        if let Some(identity) = marker_identity(
            trimmed,
            CARGO_MARKER_START_PREFIX,
            CARGO_MARKER_START_SUFFIX,
        ) {
            if open.replace((identity, offset)).is_some() {
                return Err(malformed_markers(path));
            }
        } else if trimmed.starts_with("# >>> effigy deps cargo") {
            return Err(malformed_markers(path));
        } else if let Some(identity) =
            marker_identity(trimmed, CARGO_MARKER_END_PREFIX, CARGO_MARKER_END_SUFFIX)
        {
            let Some((opened_identity, start)) = open.take() else {
                return Err(malformed_markers(path));
            };
            if opened_identity != identity {
                return Err(malformed_markers(path));
            }
            blocks.push(ManagedBlock {
                identity,
                start,
                end: offset + line.len(),
            });
        } else if trimmed.starts_with("# <<< effigy deps cargo") {
            return Err(malformed_markers(path));
        }
        offset += line.len();
    }
    if open.is_some() {
        return Err(malformed_markers(path));
    }
    Ok(blocks)
}

fn marker_identity<'a>(line: &'a str, prefix: &str, suffix: &str) -> Option<&'a str> {
    line.strip_prefix(prefix)
        .and_then(|line| line.strip_suffix(suffix))
        .filter(|identity| !identity.is_empty())
}

fn malformed_markers(path: &Path) -> DepsError {
    DepsError::invalid(
        path,
        "Effigy-managed Cargo block markers are malformed, nested, duplicated, or mismatched",
    )
}

fn select_owned_block<'a>(
    blocks: &'a [ManagedBlock<'a>],
    library_path: &Path,
    config_path: &Path,
) -> Result<Option<ManagedBlock<'a>>, DepsError> {
    let identity = library_path.display().to_string();
    let matches = blocks
        .iter()
        .copied()
        .filter(|block| block.identity == identity)
        .collect::<Vec<_>>();
    if matches.len() > 1 {
        return Err(malformed_markers(config_path));
    }
    Ok(matches.into_iter().next())
}

fn remove_block(raw: &str, block: Option<ManagedBlock<'_>>) -> String {
    let Some(block) = block else {
        return raw.to_owned();
    };
    format!("{}{}", &raw[..block.start], &raw[block.end..])
}

fn replace_block(raw: &str, block: ManagedBlock<'_>, replacement: &str) -> String {
    format!(
        "{}{}{}",
        &raw[..block.start],
        replacement,
        &raw[block.end..]
    )
}

fn append_block(raw: &str, block: &str) -> String {
    let mut rendered = raw.to_owned();
    if !rendered.is_empty() {
        if !rendered.ends_with('\n') {
            rendered.push('\n');
        }
        if !rendered.ends_with("\n\n") {
            rendered.push('\n');
        }
    }
    rendered.push_str(block);
    rendered
}

fn render_managed_block(library_path: &Path, patch_groups: &PatchGroups) -> String {
    let identity = library_path.display();
    let mut lines = vec![format!(
        "{CARGO_MARKER_START_PREFIX}{identity}{CARGO_MARKER_START_SUFFIX}"
    )];
    for (source, packages) in patch_groups {
        lines.push(format!("[patch.{}]", toml_string(source)));
        for (name, path) in packages {
            lines.push(format!(
                "{} = {{ path = {} }}",
                toml_string(name),
                toml_string(&path.display().to_string())
            ));
        }
    }
    lines.push(format!(
        "{CARGO_MARKER_END_PREFIX}{identity}{CARGO_MARKER_END_SUFFIX}"
    ));
    format!("{}\n", lines.join("\n"))
}

fn toml_string(value: &str) -> String {
    toml::Value::String(value.to_owned()).to_string()
}

fn refuse_patch_collisions(
    raw: &str,
    patch_groups: &PatchGroups,
    config_path: &Path,
) -> Result<(), DepsError> {
    if raw.trim().is_empty() {
        return Ok(());
    }
    let config: toml::Value = toml::from_str(raw).map_err(|error| {
        DepsError::invalid(
            config_path,
            format!("failed to parse existing Cargo config: {error}"),
        )
    })?;
    let Some(patch) = config.get("patch") else {
        return Ok(());
    };
    for (source, packages) in patch_groups {
        let Some(source_table) = patch.get(source) else {
            continue;
        };
        for name in packages.keys() {
            if source_table.get(name).is_some() {
                return Err(DepsError::invalid(
                    config_path,
                    format!(
                        "hand-managed Cargo patch collision for crate `{name}` under exact source `{source}`"
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn adopt_compatible_patch_tables(
    repo_root: &Path,
    raw: &str,
    patch_groups: &PatchGroups,
    config_path: &Path,
) -> Result<(String, usize), DepsError> {
    if raw.trim().is_empty() {
        return Ok((raw.to_owned(), 0));
    }
    let config: toml::Value = toml::from_str(raw).map_err(|error| {
        DepsError::invalid(
            config_path,
            format!("failed to parse existing Cargo config: {error}"),
        )
    })?;
    let Some(patch) = config.get("patch") else {
        return Ok((raw.to_owned(), 0));
    };

    let mut adopted_sources = BTreeSet::new();
    for (source, packages) in patch_groups {
        let Some(source_table) = patch.get(source).and_then(toml::Value::as_table) else {
            continue;
        };
        let overlaps = source_table.keys().any(|name| packages.contains_key(name));
        if !overlaps {
            return Err(DepsError::invalid(
                config_path,
                format!(
                    "hand-managed Cargo patch table already exists for exact source `{source}`; refusing an unsafe table merge"
                ),
            ));
        }
        for (name, value) in source_table {
            let Some(expected_path) = packages.get(name) else {
                return Err(DepsError::invalid(
                    config_path,
                    format!(
                        "hand-managed Cargo patch table for exact source `{source}` also contains unrelated crate `{name}`; refusing to claim it"
                    ),
                ));
            };
            let Some(entry) = value.as_table() else {
                return Err(incompatible_patch(config_path, source, name));
            };
            if entry.len() != 1 {
                return Err(incompatible_patch(config_path, source, name));
            }
            let Some(path) = entry.get("path").and_then(toml::Value::as_str) else {
                return Err(incompatible_patch(config_path, source, name));
            };
            let candidate = if Path::new(path).is_absolute() {
                PathBuf::from(path)
            } else {
                repo_root.join(path)
            };
            let candidate = canonical_existing_path(&candidate)
                .map_err(|_| incompatible_patch(config_path, source, name))?;
            if &candidate != expected_path {
                return Err(incompatible_patch(config_path, source, name));
            }
        }
        adopted_sources.insert(source.clone());
    }

    let mut spans = adopted_sources
        .iter()
        .map(|source| patch_table_span(raw, source, config_path))
        .collect::<Result<Vec<_>, _>>()?;
    spans.sort_unstable();
    let mut rendered = raw.to_owned();
    for (start, end) in spans.into_iter().rev() {
        rendered.replace_range(start..end, "");
    }
    Ok((rendered, adopted_sources.len()))
}

fn incompatible_patch(config_path: &Path, source: &str, name: &str) -> DepsError {
    DepsError::invalid(
        config_path,
        format!(
            "hand-managed Cargo patch collision for crate `{name}` under exact source `{source}` does not point only at the requested local crate"
        ),
    )
}

fn patch_table_span(
    raw: &str,
    source: &str,
    config_path: &Path,
) -> Result<(usize, usize), DepsError> {
    let mut headers = Vec::new();
    let mut offset = 0;
    for line in raw.split_inclusive('\n') {
        if is_table_header(line) {
            headers.push((offset, patch_header_matches(line, source)));
        }
        offset += line.len();
    }
    let matches = headers
        .iter()
        .enumerate()
        .filter(|(_, (_, matches))| *matches)
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(DepsError::invalid(
            config_path,
            format!(
                "could not isolate the hand-managed Cargo patch table for exact source `{source}`"
            ),
        ));
    }
    let (index, (start, _)) = matches[0];
    let table_limit = headers
        .get(index + 1)
        .map(|(offset, _)| *offset)
        .unwrap_or(raw.len());
    let mut end = *start;
    let mut offset = *start;
    for line in raw[*start..table_limit].split_inclusive('\n') {
        let trimmed = line.trim();
        if !trimmed.is_empty() && !trimmed.starts_with('#') {
            end = offset + line.len();
        }
        offset += line.len();
    }
    Ok((*start, end))
}

fn is_table_header(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with('[') && !trimmed.starts_with("[[")
}

fn patch_header_matches(line: &str, source: &str) -> bool {
    let trimmed = line.trim();
    let Some(end) = trimmed.find(']') else {
        return false;
    };
    let header = &trimmed[..=end];
    let Ok(value) = toml::from_str::<toml::Value>(header) else {
        return false;
    };
    value
        .get("patch")
        .and_then(|patch| patch.get(source))
        .is_some()
}

fn plan_link_gitignore(repo_root: &Path) -> Result<(Option<String>, Option<String>), DepsError> {
    let path = repo_root.join(".gitignore");
    let before = read_optional_string(&path)?;
    let mut after = before.clone().unwrap_or_default();
    if !effigy_ignore_present(&after) {
        append_ignore_pattern(&mut after, ".effigy/");
    }
    if !cargo_config_ignore_present(&after) {
        append_ignore_pattern(&mut after, ".cargo/config.toml");
    }
    Ok((before, Some(after)))
}

fn append_ignore_pattern(raw: &mut String, pattern: &str) {
    if !raw.is_empty() && !raw.ends_with('\n') {
        raw.push('\n');
    }
    raw.push_str(pattern);
    raw.push('\n');
}

fn effigy_ignore_present(raw: &str) -> bool {
    raw.lines().map(str::trim).any(|line| {
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
    })
}

fn cargo_config_ignore_present(raw: &str) -> bool {
    raw.lines().map(str::trim).any(|line| {
        matches!(
            line,
            ".cargo/config.toml"
                | "/.cargo/config.toml"
                | ".cargo/"
                | "/.cargo/"
                | ".cargo/**"
                | "/.cargo/**"
                | "**/.cargo/config.toml"
        )
    })
}

fn render_repo_state(state: &RepoLinkState, path: &Path) -> Result<String, DepsError> {
    let mut rendered = serde_json::to_string_pretty(state)
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

fn cargo_dir_has_no_foreign_entries(path: &Path) -> Result<bool, DepsError> {
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(true),
        Err(error) => return Err(DepsError::io("read directory", path, error)),
    };
    for entry in entries {
        let entry = entry.map_err(|error| DepsError::io("read directory entry", path, error))?;
        if entry.file_name() != "config.toml" {
            return Ok(false);
        }
    }
    Ok(true)
}

fn read_optional_string(path: &Path) -> Result<Option<String>, DepsError> {
    match fs::read_to_string(path) {
        Ok(raw) => Ok(Some(raw)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(DepsError::io("read", path, error)),
    }
}

fn repo_relative<'a>(repo_root: &'a Path, path: &'a Path) -> Result<&'a Path, DepsError> {
    path.strip_prefix(repo_root).map_err(|_| {
        DepsError::invalid(
            path,
            format!("path is outside consumer repo `{}`", repo_root.display()),
        )
    })
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use tempfile::TempDir;

    use super::*;
    use crate::{
        CargoPackageInventory, CargoPackageMatch, DependencyDepth, ProcessOutput,
        REPO_LINK_STATE_SCHEMA, REPO_LINK_STATE_SCHEMA_VERSION,
    };

    const SOURCE_A: &str = "https://example.test/signal.git";
    const SOURCE_B: &str = "ssh://git@example.test/signal.git";

    #[derive(Default)]
    struct FixtureObserver {
        tracked: BTreeSet<PathBuf>,
        dirty: BTreeSet<PathBuf>,
        observations: RefCell<Vec<(&'static str, PathBuf)>>,
    }

    impl CargoPlanObserver for FixtureObserver {
        fn is_tracked(&self, _repo_root: &Path, path: &Path) -> Result<bool, DepsError> {
            self.observations
                .borrow_mut()
                .push(("tracked", path.to_path_buf()));
            Ok(self.tracked.contains(path))
        }

        fn is_dirty(&self, _repo_root: &Path, path: &Path) -> Result<bool, DepsError> {
            self.observations
                .borrow_mut()
                .push(("dirty", path.to_path_buf()));
            Ok(self.dirty.contains(path))
        }
    }

    fn write(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    fn setup() -> (TempDir, PathBuf, TempDir, CargoLibraryInventory) {
        let consumer = TempDir::new().unwrap();
        let repo = fs::canonicalize(consumer.path()).unwrap();
        fs::create_dir_all(repo.join("nested")).unwrap();
        let library = TempDir::new().unwrap();
        let library_root = fs::canonicalize(library.path()).unwrap();
        let core_manifest = library_root.join("packages/core/Cargo.toml");
        let protocol_manifest = library_root.join("packages/protocol/Cargo.toml");
        write(
            &core_manifest,
            "[package]\nname='signal-core'\nversion='0.1.0'\n",
        );
        write(
            &protocol_manifest,
            "[package]\nname='signal-protocol'\nversion='0.1.0'\n",
        );
        let inventory = CargoLibraryInventory {
            root: library_root,
            packages: vec![
                CargoPackageInventory {
                    id: "local-core".to_owned(),
                    name: "signal-core".to_owned(),
                    manifest_path: core_manifest,
                    source: None,
                },
                CargoPackageInventory {
                    id: "local-protocol".to_owned(),
                    name: "signal-protocol".to_owned(),
                    manifest_path: protocol_manifest,
                    source: None,
                },
            ],
        };
        (consumer, repo, library, inventory)
    }

    fn matched(name: &str, source: &str, depth: DependencyDepth) -> CargoPackageMatch {
        CargoPackageMatch {
            package: CargoPackageInventory {
                id: format!("remote-{name}"),
                name: name.to_owned(),
                manifest_path: PathBuf::from(format!("/cargo/git/{name}/Cargo.toml")),
                source: Some(CommittedSource {
                    kind: CommittedSourceKind::Git,
                    identity: source.to_owned(),
                }),
            },
            depth,
            disposition: MatchDisposition::Git,
        }
    }

    fn workspace(root: &Path, matches: Vec<CargoPackageMatch>) -> CargoWorkspaceInventory {
        CargoWorkspaceInventory {
            root: root.to_path_buf(),
            workspace_packages: Vec::new(),
            resolved_packages: matches
                .iter()
                .map(|candidate| candidate.package.clone())
                .collect(),
            library_matches: matches,
        }
    }

    fn change<'a>(plan: &'a CargoDependencyPlan, target: &Path) -> &'a PlannedChange {
        plan.operation
            .changes
            .iter()
            .find(|change| change.target == target)
            .unwrap()
    }

    fn apply_planned_files(plan: &CargoDependencyPlan) {
        for change in &plan.operation.changes {
            match &change.after {
                Some(after) => write(&change.target, after),
                None => {
                    fs::remove_file(&change.target).unwrap();
                }
            }
        }
    }

    #[test]
    fn plans_full_flat_and_nested_closure_with_exact_sources_without_writes() {
        let (_consumer, repo, _library, library) = setup();
        let root_lock = repo.join("Cargo.lock");
        let nested_lock = repo.join("nested/Cargo.lock");
        let observer = FixtureObserver {
            tracked: BTreeSet::from([root_lock.clone(), nested_lock.clone()]),
            ..FixtureObserver::default()
        };
        let workspaces = vec![
            workspace(
                &repo,
                vec![
                    matched("signal-core", SOURCE_A, DependencyDepth::Direct),
                    matched("signal-protocol", SOURCE_A, DependencyDepth::Transitive),
                ],
            ),
            workspace(
                &repo.join("nested"),
                vec![matched("signal-core", SOURCE_B, DependencyDepth::Direct)],
            ),
        ];

        let plan = plan_cargo_link(&repo, &library, &workspaces, true, &observer).unwrap();

        assert!(plan.operation.dry_run);
        assert_eq!(plan.affected_lockfiles, [root_lock, nested_lock]);
        let desired = plan.desired.as_ref().unwrap();
        assert_eq!(desired.consumer_roots.len(), 2);
        assert_eq!(desired.packages.len(), 2);
        assert_eq!(desired.packages[0].name, "signal-core");
        assert_eq!(desired.packages[0].committed_sources.len(), 2);
        let config_path = repo.join(".cargo/config.toml");
        let config_change = change(&plan, &config_path);
        assert_eq!(config_change.action, PlannedChangeAction::Create);
        let config: toml::Value = toml::from_str(config_change.after.as_ref().unwrap()).unwrap();
        assert_eq!(
            config["patch"][SOURCE_A]["signal-core"]["path"].as_str(),
            Some(
                library.packages[0]
                    .manifest_path
                    .parent()
                    .unwrap()
                    .to_str()
                    .unwrap()
            )
        );
        assert_eq!(
            config["patch"][SOURCE_B]["signal-core"]["path"].as_str(),
            config["patch"][SOURCE_A]["signal-core"]["path"].as_str()
        );
        let json = serde_json::to_value(&plan).unwrap();
        assert_eq!(json["operation"]["dry_run"], true);
        assert_eq!(json["operation"]["changes"][0]["action"], "create");
        assert_eq!(json["affected_lockfiles"].as_array().unwrap().len(), 2);
        assert!(!config_path.exists());
        assert!(!repo.join(".gitignore").exists());
        assert!(!RepoLinkStateStore::for_repo(&repo).path().exists());
    }

    #[test]
    fn link_plan_is_idempotent_after_its_exact_deltas_are_present() {
        let (_consumer, repo, _library, library) = setup();
        let workspaces = vec![workspace(
            &repo,
            vec![matched("signal-core", SOURCE_A, DependencyDepth::Direct)],
        )];
        let observer = FixtureObserver::default();
        let first = plan_cargo_link(&repo, &library, &workspaces, true, &observer).unwrap();
        apply_planned_files(&first);

        let second = plan_cargo_link(&repo, &library, &workspaces, true, &observer).unwrap();

        assert!(second.operation.changes.is_empty());
        assert_eq!(first.desired, second.desired);
    }

    #[test]
    fn unlink_preserves_foreign_config_and_removes_only_owned_state() {
        let (_consumer, repo, _library, library) = setup();
        let config_path = repo.join(".cargo/config.toml");
        write(
            &config_path,
            "# keep this comment\n[build]\ntarget-dir = 'custom-target'\n",
        );
        write(&repo.join(".gitignore"), ".effigy/\n.cargo/config.toml\n");
        let workspaces = vec![workspace(
            &repo,
            vec![matched("signal-core", SOURCE_A, DependencyDepth::Direct)],
        )];
        let observer = FixtureObserver::default();
        let link = plan_cargo_link(&repo, &library, &workspaces, true, &observer).unwrap();
        apply_planned_files(&link);
        let before_unlink = fs::read_to_string(&config_path).unwrap();

        let unlink = plan_cargo_unlink(&repo, &library.root, true, &observer).unwrap();

        let config = change(&unlink, &config_path);
        assert_eq!(config.action, PlannedChangeAction::Update);
        assert!(config
            .before
            .as_ref()
            .unwrap()
            .contains("effigy deps cargo"));
        assert_eq!(config.before.as_ref().unwrap(), &before_unlink);
        assert!(config
            .after
            .as_ref()
            .unwrap()
            .contains("target-dir = 'custom-target'"));
        assert!(config
            .after
            .as_ref()
            .unwrap()
            .contains("# keep this comment"));
        assert!(!config.after.as_ref().unwrap().contains("effigy deps cargo"));
        assert!(unlink.remove_empty_directories.is_empty());
        assert_eq!(
            change(&unlink, RepoLinkStateStore::for_repo(&repo).path()).action,
            PlannedChangeAction::Delete
        );
        assert_eq!(fs::read_to_string(&config_path).unwrap(), before_unlink);
    }

    #[test]
    fn unlink_deletes_effigy_created_empty_config_and_plans_directory_cleanup() {
        let (_consumer, repo, _library, library) = setup();
        let workspaces = vec![workspace(
            &repo,
            vec![matched("signal-core", SOURCE_A, DependencyDepth::Direct)],
        )];
        let observer = FixtureObserver::default();
        let link = plan_cargo_link(&repo, &library, &workspaces, true, &observer).unwrap();
        apply_planned_files(&link);

        let unlink = plan_cargo_unlink(&repo, &library.root, true, &observer).unwrap();

        assert_eq!(
            change(&unlink, &repo.join(".cargo/config.toml")).action,
            PlannedChangeAction::Delete
        );
        assert_eq!(unlink.remove_empty_directories, [repo.join(".cargo")]);
        assert!(repo.join(".cargo/config.toml").exists());
    }

    #[test]
    fn tracked_config_foreign_collision_and_malformed_markers_are_refused_unchanged() {
        let (_consumer, repo, _library, library) = setup();
        let workspaces = vec![workspace(
            &repo,
            vec![matched("signal-core", SOURCE_A, DependencyDepth::Direct)],
        )];
        let config_path = repo.join(".cargo/config.toml");
        write(&config_path, "[build]\ntarget-dir='tracked'\n");
        let tracked = FixtureObserver {
            tracked: BTreeSet::from([config_path.clone()]),
            ..FixtureObserver::default()
        };
        let error = plan_cargo_link(&repo, &library, &workspaces, true, &tracked).unwrap_err();
        assert!(error.to_string().contains("tracked by Git"));

        write(
            &config_path,
            &format!(
                "[patch.{}]\nsignal-core={{path='/foreign'}}\n",
                toml_string(SOURCE_A)
            ),
        );
        let error = plan_cargo_link(
            &repo,
            &library,
            &workspaces,
            true,
            &FixtureObserver::default(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("patch collision"));

        write(
            &config_path,
            &format!(
                "{CARGO_MARKER_START_PREFIX}{}{CARGO_MARKER_START_SUFFIX}\n",
                library.root.display()
            ),
        );
        let before = fs::read_to_string(&config_path).unwrap();
        let error = plan_cargo_link(
            &repo,
            &library,
            &workspaces,
            true,
            &FixtureObserver::default(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("markers are malformed"));
        assert_eq!(fs::read_to_string(&config_path).unwrap(), before);
        assert!(!repo.join(".gitignore").exists());
    }

    #[test]
    fn path_registry_unmatched_and_no_match_outcomes_never_produce_a_plan() {
        let (_consumer, repo, _library, library) = setup();
        for disposition in [
            MatchDisposition::PreMigrationPath,
            MatchDisposition::Registry,
            MatchDisposition::Unmatched,
        ] {
            let mut candidate = matched("signal-core", SOURCE_A, DependencyDepth::Direct);
            candidate.disposition = disposition;
            let error = plan_cargo_link(
                &repo,
                &library,
                &[workspace(&repo, vec![candidate])],
                true,
                &FixtureObserver::default(),
            )
            .unwrap_err();
            assert!(error.to_string().contains("no plan was produced"));
        }
        let error = plan_cargo_link(
            &repo,
            &library,
            &[workspace(&repo, Vec::new())],
            true,
            &FixtureObserver::default(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("no git dependencies matching"));
        assert!(!repo.join(".cargo/config.toml").exists());
        assert!(!RepoLinkStateStore::for_repo(&repo).path().exists());
    }

    #[test]
    fn pre_dirty_affected_lockfile_is_refused_without_planned_writes() {
        let (_consumer, repo, _library, library) = setup();
        let lockfile = repo.join("Cargo.lock");
        let observer = FixtureObserver {
            tracked: BTreeSet::from([lockfile.clone()]),
            dirty: BTreeSet::from([lockfile]),
            ..FixtureObserver::default()
        };
        let workspaces = vec![workspace(
            &repo,
            vec![matched("signal-core", SOURCE_A, DependencyDepth::Direct)],
        )];

        let error = plan_cargo_link(&repo, &library, &workspaces, true, &observer).unwrap_err();

        assert!(error.to_string().contains("Cargo.lock is already dirty"));
        assert!(!repo.join(".cargo/config.toml").exists());
        assert!(!repo.join(".gitignore").exists());
    }

    #[test]
    fn unlink_of_absent_link_is_a_non_mutating_noop() {
        let (_consumer, repo, _library, library) = setup();
        let plan =
            plan_cargo_unlink(&repo, &library.root, true, &FixtureObserver::default()).unwrap();
        assert!(plan.operation.changes.is_empty());
        assert!(plan.operation.warnings[0].contains("already absent"));
    }

    #[test]
    fn tracked_missing_config_is_refused_before_create_planning() {
        let (_consumer, repo, _library, library) = setup();
        let config_path = repo.join(".cargo/config.toml");
        let observer = FixtureObserver {
            tracked: BTreeSet::from([config_path]),
            ..FixtureObserver::default()
        };
        let error = plan_cargo_link(
            &repo,
            &library,
            &[workspace(
                &repo,
                vec![matched("signal-core", SOURCE_A, DependencyDepth::Direct)],
            )],
            true,
            &observer,
        )
        .unwrap_err();
        assert!(error.to_string().contains("tracked by Git"));
        assert!(!repo.join(".cargo/config.toml").exists());
    }

    #[test]
    fn unlink_can_use_the_ledger_identity_after_the_library_disappears() {
        let (_consumer, repo, library_temp, library) = setup();
        let observer = FixtureObserver::default();
        let link = plan_cargo_link(
            &repo,
            &library,
            &[workspace(
                &repo,
                vec![matched("signal-core", SOURCE_A, DependencyDepth::Direct)],
            )],
            true,
            &observer,
        )
        .unwrap();
        apply_planned_files(&link);
        let missing_path = library.root.clone();
        fs::remove_dir_all(library_temp.path()).unwrap();

        let unlink = plan_cargo_unlink(&repo, &missing_path, true, &observer).unwrap();

        assert_eq!(unlink.operation.key.library_path, missing_path);
        assert!(!unlink.operation.changes.is_empty());
    }

    #[test]
    fn git_observer_uses_only_read_only_tracking_and_status_commands() {
        struct FixtureProcess {
            requests: RefCell<Vec<ProcessRequest>>,
        }
        impl ReadOnlyProcess for FixtureProcess {
            fn run(&self, request: &ProcessRequest) -> Result<ProcessOutput, DepsError> {
                self.requests.borrow_mut().push(request.clone());
                Ok(ProcessOutput {
                    status: Some(0),
                    stdout: "Cargo.lock\n".to_owned(),
                    stderr: String::new(),
                })
            }
        }
        let temp = TempDir::new().unwrap();
        let process = FixtureProcess {
            requests: RefCell::new(Vec::new()),
        };
        let observer = GitCargoPlanObserver::new(&process);
        assert!(observer
            .is_tracked(temp.path(), &temp.path().join("Cargo.lock"))
            .unwrap());
        assert!(observer
            .is_dirty(temp.path(), &temp.path().join("Cargo.lock"))
            .unwrap());
        let requests = process.requests.borrow();
        assert_eq!(requests[0].args[0], "ls-files");
        assert_eq!(requests[1].args[0], "status");
        assert!(requests.iter().all(|request| request.program == "git"));
    }

    #[test]
    fn planned_ledger_is_versioned_and_contains_cargo_ownership() {
        let (_consumer, repo, _library, library) = setup();
        let plan = plan_cargo_link(
            &repo,
            &library,
            &[workspace(
                &repo,
                vec![matched("signal-core", SOURCE_A, DependencyDepth::Direct)],
            )],
            true,
            &FixtureObserver::default(),
        )
        .unwrap();
        let ledger = change(&plan, RepoLinkStateStore::for_repo(&repo).path());
        let state: RepoLinkState = serde_json::from_str(ledger.after.as_ref().unwrap()).unwrap();
        assert_eq!(state.schema, REPO_LINK_STATE_SCHEMA);
        assert_eq!(state.schema_version, REPO_LINK_STATE_SCHEMA_VERSION);
        assert_eq!(
            state.links[0].cargo_ownership,
            Some(CargoLinkOwnership {
                config_created_by_effigy: true,
                cargo_dir_created_by_effigy: true,
            })
        );
    }
}
