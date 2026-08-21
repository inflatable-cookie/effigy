use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::bun::inventory_bun_consumer_from_text_lock;
use crate::state::write_atomic;
use crate::{
    canonical_existing_path, inventory_bun_consumer, inventory_bun_library, BunPackageInventory,
    DependencyDepth, DepsError, LinkMechanism, ReadOnlyProcess, RepoLinkStateStore,
};

mod manifest;

use manifest::{add_overrides, remove_overrides, validate_editable_manifest};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BunPinOperation {
    Pin,
    Unpin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BunPinPlanDisposition {
    Apply,
    AlreadyApplied,
    NoMatch,
    Conflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BunPinPackageAction {
    Add,
    Remove,
    AlreadyApplied,
    Conflict,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BunPinPackagePlan {
    pub name: String,
    pub local_path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<DependencyDepth>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    pub action: BunPinPackageAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BunPinWarning {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BunPinPlan {
    pub operation: BunPinOperation,
    pub dry_run: bool,
    pub disposition: BunPinPlanDisposition,
    pub repo_root: PathBuf,
    pub manifest_path: PathBuf,
    pub library_path: PathBuf,
    pub packages: Vec<BunPinPackagePlan>,
    pub warnings: Vec<BunPinWarning>,
    #[serde(skip)]
    manifest_before: Vec<u8>,
    #[serde(skip)]
    manifest_after: Option<Vec<u8>>,
    #[serde(skip)]
    immutable_files: Vec<FileSnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BunPinOutcome {
    DryRun,
    Applied,
    AlreadyApplied,
    NoMatch,
    Conflict,
    ApplyFailed,
}

impl BunPinOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DryRun => "dry-run",
            Self::Applied => "applied",
            Self::AlreadyApplied => "already-applied",
            Self::NoMatch => "no-match",
            Self::Conflict => "conflict",
            Self::ApplyFailed => "apply-failed",
        }
    }

    pub fn is_success(self) -> bool {
        !matches!(self, Self::Conflict | Self::ApplyFailed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BunPinWriteAction {
    Update,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BunPinWrite {
    pub path: PathBuf,
    pub action: BunPinWriteAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BunPinVerificationStatus {
    NotRun,
    ManifestVerified,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BunPinImmutableFileEvidence {
    pub path: PathBuf,
    pub unchanged: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BunPinVerification {
    pub status: BunPinVerificationStatus,
    pub install_pending: bool,
    pub immutable_files: Vec<BunPinImmutableFileEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BunPinOperationReport {
    pub plan: BunPinPlan,
    pub outcome: BunPinOutcome,
    pub writes: Vec<BunPinWrite>,
    pub verification: BunPinVerification,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct FileSnapshot {
    path: PathBuf,
    contents: Option<Vec<u8>>,
}

pub fn plan_bun_pin(
    repo_root: impl AsRef<Path>,
    library_path: impl AsRef<Path>,
    dry_run: bool,
    process: &impl ReadOnlyProcess,
) -> Result<BunPinPlan, DepsError> {
    let repo_root = canonical_existing_path(repo_root)?;
    let library_path = canonical_existing_path(library_path)?;
    let library_packages = unique_library_packages(inventory_bun_library(&library_path)?)?;
    let manifest_path = repo_root.join("package.json");
    let manifest_before = read_manifest(&manifest_path)?;
    let manifest = parse_root_manifest(&manifest_path, &manifest_before)?;
    let overrides = overrides(&manifest_path, &manifest)?;
    validate_editable_manifest(&manifest_path, &manifest_before)?;
    let active_link_packages = active_bun_link_overlap(
        &repo_root,
        &library_path,
        &library_packages,
        &RepoLinkStateStore::for_repo(&repo_root).read()?,
    );
    if !active_link_packages.is_empty() {
        let mut packages = Vec::new();
        let mut warnings = vec![BunPinWarning {
            code: "active-effigy-bun-link".to_owned(),
            message: format!(
                "Effigy-managed Bun link state overlaps package(s) {}; run `effigy deps unlink bun {}` before pinning",
                active_link_packages.iter().cloned().collect::<Vec<_>>().join(", "),
                library_path.display()
            ),
            package: None,
        }];
        for package in library_packages
            .iter()
            .filter(|package| active_link_packages.contains(&package.name))
        {
            let relative = relative_path(&repo_root, &package.package_path)?;
            if !package.package_path.starts_with(&repo_root) {
                warnings.push(portability_warning(&package.name));
            }
            packages.push(BunPinPackagePlan {
                name: package.name.clone(),
                local_path: package.package_path.clone(),
                depth: None,
                before: overrides.get(&package.name).map(json_value_text),
                after: Some(file_specifier(&relative)),
                action: BunPinPackageAction::Conflict,
            });
        }
        return finalize_plan(BunPinPlan {
            operation: BunPinOperation::Pin,
            dry_run,
            disposition: BunPinPlanDisposition::Conflict,
            repo_root,
            manifest_path,
            library_path,
            packages,
            warnings,
            manifest_before,
            manifest_after: None,
            immutable_files: Vec::new(),
        });
    }

    let (consumer, inventory_warning) = match inventory_bun_consumer(
        &repo_root,
        &library_packages,
        process,
    ) {
        Ok(consumer) => (consumer, None),
        Err(process_error @ (DepsError::ProcessFailed { .. } | DepsError::ProcessSpawn { .. })) => {
            let lock_path = repo_root.join("bun.lock");
            let process_message = process_error.to_string();
            let consumer = inventory_bun_consumer_from_text_lock(&repo_root, &library_packages)
                .map_err(|fallback_error| {
                    DepsError::invalid(
                        &lock_path,
                        format!(
                            "{process_message}; text lockfile fallback failed: {fallback_error}"
                        ),
                    )
                })?;
            (
                consumer,
                Some(BunPinWarning {
                    code: "lockfile-enumeration-fallback".to_owned(),
                    message: format!(
                        "{process_message}; pin planning used read-only package inventory from `{}`",
                        lock_path.display()
                    ),
                    package: None,
                }),
            )
        }
        Err(error) => return Err(error),
    };
    let mut matches = BTreeMap::new();
    for (package, depth) in consumer.library_matches {
        matches
            .entry(package.name)
            .and_modify(|current| {
                if depth == DependencyDepth::Direct {
                    *current = depth;
                }
            })
            .or_insert(depth);
    }
    if matches.is_empty() {
        return finalize_plan(BunPinPlan {
            operation: BunPinOperation::Pin,
            dry_run,
            disposition: BunPinPlanDisposition::NoMatch,
            repo_root,
            manifest_path,
            library_path,
            packages: Vec::new(),
            warnings: inventory_warning.into_iter().collect(),
            manifest_before,
            manifest_after: None,
            immutable_files: Vec::new(),
        });
    }

    let mut packages = Vec::new();
    let mut warnings = inventory_warning.into_iter().collect::<Vec<_>>();
    let mut additions = BTreeMap::new();
    let mut conflict = false;
    for (name, depth) in matches {
        let package = library_packages
            .iter()
            .find(|package| package.name == name)
            .expect("consumer matches were filtered through library package names");
        let relative = relative_path(&repo_root, &package.package_path)?;
        let specifier = file_specifier(&relative);
        if !package.package_path.starts_with(&repo_root) {
            warnings.push(portability_warning(&name));
        }
        let before = overrides.get(&name).map(json_value_text);
        let action = match overrides.get(&name) {
            None => {
                additions.insert(name.clone(), specifier.clone());
                BunPinPackageAction::Add
            }
            Some(value) if override_matches(value, &repo_root, &package.package_path) => {
                BunPinPackageAction::AlreadyApplied
            }
            Some(_) => {
                conflict = true;
                warnings.push(BunPinWarning {
                    code: "conflicting-override".to_owned(),
                    message: format!(
                        "override for `{name}` already points elsewhere; resolve or remove it before pinning"
                    ),
                    package: Some(name.clone()),
                });
                BunPinPackageAction::Conflict
            }
        };
        packages.push(BunPinPackagePlan {
            name,
            local_path: package.package_path.clone(),
            depth: Some(depth),
            before,
            after: Some(specifier),
            action,
        });
    }

    if conflict {
        for package in &mut packages {
            if package.action == BunPinPackageAction::Add {
                package.action = BunPinPackageAction::Conflict;
            }
        }
        return finalize_plan(BunPinPlan {
            operation: BunPinOperation::Pin,
            dry_run,
            disposition: BunPinPlanDisposition::Conflict,
            repo_root,
            manifest_path,
            library_path,
            packages,
            warnings,
            manifest_before,
            manifest_after: None,
            immutable_files: Vec::new(),
        });
    }

    let disposition = if additions.is_empty() {
        BunPinPlanDisposition::AlreadyApplied
    } else {
        BunPinPlanDisposition::Apply
    };
    let manifest_after = if additions.is_empty() {
        None
    } else {
        Some(add_overrides(&manifest_path, &manifest_before, &additions)?)
    };
    finalize_plan(BunPinPlan {
        operation: BunPinOperation::Pin,
        dry_run,
        disposition,
        repo_root,
        manifest_path,
        library_path,
        packages,
        warnings,
        manifest_before,
        manifest_after,
        immutable_files: Vec::new(),
    })
}

pub fn plan_bun_unpin(
    repo_root: impl AsRef<Path>,
    library_path: impl AsRef<Path>,
    dry_run: bool,
) -> Result<BunPinPlan, DepsError> {
    let repo_root = canonical_existing_path(repo_root)?;
    let library_path = canonical_existing_path(library_path)?;
    let library_packages = unique_library_packages(inventory_bun_library(&library_path)?)?;
    let manifest_path = repo_root.join("package.json");
    let manifest_before = read_manifest(&manifest_path)?;
    let manifest = parse_root_manifest(&manifest_path, &manifest_before)?;
    let overrides = overrides(&manifest_path, &manifest)?;
    validate_editable_manifest(&manifest_path, &manifest_before)?;
    let mut packages = Vec::new();
    let mut removals = BTreeSet::new();
    for package in library_packages {
        let before = overrides.get(&package.name).map(json_value_text);
        let remove = overrides
            .get(&package.name)
            .is_some_and(|value| override_matches(value, &repo_root, &package.package_path));
        if remove {
            removals.insert(package.name.clone());
        }
        packages.push(BunPinPackagePlan {
            name: package.name,
            local_path: package.package_path,
            depth: None,
            before,
            after: None,
            action: if remove {
                BunPinPackageAction::Remove
            } else {
                BunPinPackageAction::AlreadyApplied
            },
        });
    }
    let disposition = if removals.is_empty() {
        BunPinPlanDisposition::AlreadyApplied
    } else {
        BunPinPlanDisposition::Apply
    };
    let manifest_after = if removals.is_empty() {
        None
    } else {
        Some(remove_overrides(
            &manifest_path,
            &manifest_before,
            &removals,
        )?)
    };
    finalize_plan(BunPinPlan {
        operation: BunPinOperation::Unpin,
        dry_run,
        disposition,
        repo_root,
        manifest_path,
        library_path,
        packages,
        warnings: Vec::new(),
        manifest_before,
        manifest_after,
        immutable_files: Vec::new(),
    })
}

pub fn apply_bun_pin_plan(plan: BunPinPlan) -> BunPinOperationReport {
    apply_with_writer(plan, &FsManifestWriter)
}

fn apply_with_writer(plan: BunPinPlan, writer: &impl ManifestWriter) -> BunPinOperationReport {
    let outcome = match plan.disposition {
        BunPinPlanDisposition::NoMatch => BunPinOutcome::NoMatch,
        BunPinPlanDisposition::AlreadyApplied => BunPinOutcome::AlreadyApplied,
        BunPinPlanDisposition::Conflict => BunPinOutcome::Conflict,
        BunPinPlanDisposition::Apply if plan.dry_run => BunPinOutcome::DryRun,
        BunPinPlanDisposition::Apply => BunPinOutcome::Applied,
    };
    if outcome != BunPinOutcome::Applied {
        let errors = if outcome == BunPinOutcome::Conflict {
            plan.warnings
                .iter()
                .filter(|warning| {
                    matches!(
                        warning.code.as_str(),
                        "active-effigy-bun-link" | "conflicting-override"
                    )
                })
                .map(|warning| warning.message.clone())
                .collect()
        } else {
            Vec::new()
        };
        return BunPinOperationReport {
            plan,
            outcome,
            writes: Vec::new(),
            verification: not_run_verification(),
            errors,
        };
    }

    let Some(after) = plan.manifest_after.as_deref() else {
        return failed_report(
            plan,
            Vec::new(),
            "apply plan has no manifest after-state".to_owned(),
        );
    };
    match fs::read(&plan.manifest_path) {
        Ok(current) if current == plan.manifest_before => {}
        Ok(_) => {
            return failed_report(
                plan,
                Vec::new(),
                "planned manifest before-state is stale; no dependency files were changed"
                    .to_owned(),
            );
        }
        Err(error) => {
            return failed_report(plan, Vec::new(), error.to_string());
        }
    }
    if let Err(error) = verify_immutable_snapshots(&plan.immutable_files) {
        return failed_report(plan, Vec::new(), error);
    }
    if let Err(error) = writer.write(&plan.manifest_path, after) {
        return failed_report(plan, Vec::new(), error.to_string());
    }

    let writes = vec![BunPinWrite {
        path: plan.manifest_path.clone(),
        action: BunPinWriteAction::Update,
    }];
    let current_manifest = fs::read(&plan.manifest_path);
    let immutable_files = immutable_evidence(&plan.immutable_files);
    let verified = current_manifest.is_ok_and(|current| current == after)
        && immutable_files.iter().all(|item| item.unchanged);
    if !verified {
        return BunPinOperationReport {
            plan,
            outcome: BunPinOutcome::ApplyFailed,
            writes,
            verification: BunPinVerification {
                status: BunPinVerificationStatus::Failed,
                install_pending: false,
                immutable_files,
            },
            errors: vec!["manifest or Bun lockfile verification failed after apply".to_owned()],
        };
    }
    BunPinOperationReport {
        plan,
        outcome: BunPinOutcome::Applied,
        writes,
        verification: BunPinVerification {
            status: BunPinVerificationStatus::ManifestVerified,
            install_pending: true,
            immutable_files,
        },
        errors: Vec::new(),
    }
}

fn finalize_plan(mut plan: BunPinPlan) -> Result<BunPinPlan, DepsError> {
    let immutable_files = ["bun.lock", "bun.lockb"]
        .into_iter()
        .map(|name| snapshot(plan.repo_root.join(name)))
        .collect::<Result<Vec<_>, _>>()?;
    plan.immutable_files = immutable_files;
    Ok(plan)
}

fn failed_report(
    plan: BunPinPlan,
    writes: Vec<BunPinWrite>,
    error: String,
) -> BunPinOperationReport {
    let immutable_files = immutable_evidence(&plan.immutable_files);
    BunPinOperationReport {
        plan,
        outcome: BunPinOutcome::ApplyFailed,
        writes,
        verification: BunPinVerification {
            status: BunPinVerificationStatus::Failed,
            install_pending: false,
            immutable_files,
        },
        errors: vec![error],
    }
}

fn not_run_verification() -> BunPinVerification {
    BunPinVerification {
        status: BunPinVerificationStatus::NotRun,
        install_pending: false,
        immutable_files: Vec::new(),
    }
}

fn snapshot(path: PathBuf) -> Result<FileSnapshot, DepsError> {
    let contents = match fs::read(&path) {
        Ok(contents) => Some(contents),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(DepsError::io("read Bun lockfile", &path, error)),
    };
    Ok(FileSnapshot { path, contents })
}

fn verify_immutable_snapshots(snapshots: &[FileSnapshot]) -> Result<(), String> {
    for snapshot in snapshots {
        let current = match fs::read(&snapshot.path) {
            Ok(contents) => Some(contents),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.to_string()),
        };
        if current != snapshot.contents {
            return Err(format!(
                "Bun lockfile `{}` changed after planning; no manifest write was attempted",
                snapshot.path.display()
            ));
        }
    }
    Ok(())
}

fn immutable_evidence(snapshots: &[FileSnapshot]) -> Vec<BunPinImmutableFileEvidence> {
    snapshots
        .iter()
        .map(|snapshot| {
            let unchanged = match fs::read(&snapshot.path) {
                Ok(contents) => snapshot.contents.as_deref() == Some(contents.as_slice()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    snapshot.contents.is_none()
                }
                Err(_) => false,
            };
            BunPinImmutableFileEvidence {
                path: snapshot.path.clone(),
                unchanged,
            }
        })
        .collect()
}

fn unique_library_packages(
    packages: Vec<BunPackageInventory>,
) -> Result<Vec<BunPackageInventory>, DepsError> {
    let mut by_name = BTreeMap::new();
    for package in packages {
        if let Some(existing) = by_name.insert(package.name.clone(), package.clone()) {
            return Err(DepsError::invalid(
                &package.package_path,
                format!(
                    "library package name `{}` is declared by both `{}` and `{}`",
                    package.name,
                    existing.package_path.display(),
                    package.package_path.display()
                ),
            ));
        }
    }
    Ok(by_name.into_values().collect())
}

fn read_manifest(path: &Path) -> Result<Vec<u8>, DepsError> {
    fs::read(path).map_err(|error| DepsError::io("read package manifest", path, error))
}

fn parse_root_manifest(
    path: &Path,
    raw: &[u8],
) -> Result<serde_json::Map<String, serde_json::Value>, DepsError> {
    let value: serde_json::Value = serde_json::from_slice(raw)
        .map_err(|error| DepsError::json("parse package manifest", path, error))?;
    value
        .as_object()
        .cloned()
        .ok_or_else(|| DepsError::invalid(path, "package manifest root must be an object"))
}

fn overrides<'a>(
    path: &Path,
    manifest: &'a serde_json::Map<String, serde_json::Value>,
) -> Result<&'a serde_json::Map<String, serde_json::Value>, DepsError> {
    match manifest.get("overrides") {
        Some(value) => value
            .as_object()
            .ok_or_else(|| DepsError::invalid(path, "top-level `overrides` must be an object")),
        None => Ok(empty_json_object()),
    }
}

fn empty_json_object() -> &'static serde_json::Map<String, serde_json::Value> {
    static EMPTY: std::sync::OnceLock<serde_json::Map<String, serde_json::Value>> =
        std::sync::OnceLock::new();
    EMPTY.get_or_init(serde_json::Map::new)
}

fn json_value_text(value: &serde_json::Value) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string())
}

pub(crate) fn matching_committed_overrides<'a>(
    repo_root: &Path,
    packages: impl IntoIterator<Item = (&'a str, &'a Path)>,
) -> Result<Vec<String>, DepsError> {
    let manifest_path = repo_root.join("package.json");
    let raw = read_manifest(&manifest_path)?;
    let manifest = parse_root_manifest(&manifest_path, &raw)?;
    let overrides = overrides(&manifest_path, &manifest)?;
    validate_editable_manifest(&manifest_path, &raw)?;
    Ok(packages
        .into_iter()
        .filter(|(name, package_path)| {
            overrides
                .get(*name)
                .is_some_and(|value| override_matches(value, repo_root, package_path))
        })
        .map(|(name, _)| name.to_owned())
        .collect())
}

fn active_bun_link_overlap(
    repo_root: &Path,
    library_path: &Path,
    library_packages: &[BunPackageInventory],
    state: &crate::RepoLinkState,
) -> BTreeSet<String> {
    let library_names = library_packages
        .iter()
        .map(|package| package.name.as_str())
        .collect::<BTreeSet<_>>();
    let mut overlap = BTreeSet::new();
    for link in state.links.iter().filter(|link| {
        link.mechanism == LinkMechanism::BunLink && link.key.consumer_repo == repo_root
    }) {
        let before = overlap.len();
        overlap.extend(
            link.packages
                .iter()
                .map(|package| package.name.as_str())
                .filter(|name| library_names.contains(name))
                .map(str::to_owned),
        );
        if link.key.library_path == library_path && overlap.len() == before {
            overlap.extend(library_names.iter().map(|name| (*name).to_owned()));
        }
    }
    overlap
}

fn portability_warning(name: &str) -> BunPinWarning {
    BunPinWarning {
        code: "checkout-topology-portability".to_owned(),
        message: format!(
            "pin for `{name}` escapes the consumer repository; CI and teammates need the same relative checkout topology"
        ),
        package: Some(name.to_owned()),
    }
}

fn override_matches(value: &serde_json::Value, manifest_root: &Path, package_root: &Path) -> bool {
    let Some(specifier) = value.as_str() else {
        return false;
    };
    let Some(path) = specifier.strip_prefix("file:") else {
        return false;
    };
    let path = Path::new(path);
    if path.is_absolute() {
        return false;
    }
    fs::canonicalize(manifest_root.join(path)).is_ok_and(|path| path == package_root)
}

fn relative_path(from: &Path, to: &Path) -> Result<PathBuf, DepsError> {
    let from = from.components().collect::<Vec<_>>();
    let to_components = to.components().collect::<Vec<_>>();
    let common = from
        .iter()
        .zip(&to_components)
        .take_while(|(left, right)| left == right)
        .count();
    if common == 0 {
        return Err(DepsError::invalid(
            to,
            "consumer and library package do not share a filesystem root",
        ));
    }
    let mut relative = PathBuf::new();
    for component in &from[common..] {
        if matches!(component, Component::Normal(_)) {
            relative.push("..");
        }
    }
    for component in &to_components[common..] {
        relative.push(component.as_os_str());
    }
    if relative.as_os_str().is_empty() {
        relative.push(".");
    }
    Ok(relative)
}

fn file_specifier(relative: &Path) -> String {
    format!("file:{}", relative.to_string_lossy().replace('\\', "/"))
}

trait ManifestWriter {
    fn write(&self, path: &Path, contents: &[u8]) -> Result<(), DepsError>;
}

struct FsManifestWriter;

impl ManifestWriter for FsManifestWriter {
    fn write(&self, path: &Path, contents: &[u8]) -> Result<(), DepsError> {
        write_atomic(path, contents, false)
    }
}

#[cfg(test)]
mod tests;
