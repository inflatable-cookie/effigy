//! Where the Linux artifact builder's `/workspace` source comes from.
//!
//! A builder compiles this checkout's source. The VM only reaches host paths a
//! Colima profile shares, so a checkout outside every shared mount cannot be
//! bound directly: Docker would silently create an empty VM directory for an
//! inaccessible host path and the build would compile nothing. An environment
//! that declares `workspace_source = "staged"` therefore binds the checkout's
//! staged source closure under one of the profile's own writable mounts.
//!
//! This module owns only where that source lives and how it is proven to be
//! this checkout's. It never copies, publishes, activates or removes anything:
//! staging is the artifact builder's maintained producer step, and the profile
//! config is read, never written.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{ContainerPolicyError, EffectiveContainerPolicy};

/// The owned directory below one writable profile mount. Every staged source
/// lives in a child of it, named by the environment's project identity.
pub const STAGE_ROOT_DIR: &str = "effigy-builder-source";
/// The staged source closure itself: the directory bound at `/workspace`.
pub const STAGE_SOURCE_DIR: &str = "source";
/// The binding record beside the staged source, outside the bound tree.
pub const STAGE_MANIFEST_FILE: &str = "stage.json";
/// Schema of that record. A record naming another schema is not this one.
pub const STAGE_SCHEMA: &str = "effigy.builder-source-stage.v1";
/// The path components a staged closure never carries: Effigy's private state,
/// local secret files and the build's own output. They are excluded by rule and
/// recorded as excluded, never copied and never claimed.
pub const STAGE_EXCLUDED_PREFIXES: [&str; 3] = [".effigy/", ".env", ".git/"];

/// How an environment declares its `/workspace` source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorkspaceSource {
    /// Bind this checkout's own host path, exactly as before. This is the
    /// default and the only source a profile that shares the checkout reaches.
    #[default]
    Checkout,
    /// Bind the checkout's staged source closure under the profile's writable
    /// mount whenever the profile does not share the checkout's host path.
    Staged,
}

impl WorkspaceSource {
    pub fn is_staged(self) -> bool {
        matches!(self, Self::Staged)
    }
}

/// The exact owned directory one checkout's staged source is bound from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagePlan {
    /// The profile mount the staged source lives under, as the profile spells
    /// it: the VM mounts it at this same guest path.
    pub mount_root: PathBuf,
    /// `mount_root/effigy-builder-source/<project>`: this checkout's own child.
    pub stage_root: PathBuf,
    /// The directory bound at `/workspace`.
    pub source: PathBuf,
    /// The binding record beside it.
    pub manifest: PathBuf,
}

impl StagePlan {
    fn under(mount_root: &Path, project_name: &str) -> Self {
        let stage_root = mount_root.join(STAGE_ROOT_DIR).join(project_name.trim());
        Self {
            source: stage_root.join(STAGE_SOURCE_DIR),
            manifest: stage_root.join(STAGE_MANIFEST_FILE),
            stage_root,
            mount_root: mount_root.to_path_buf(),
        }
    }
}

fn refusal(reason: impl std::fmt::Display) -> ContainerPolicyError {
    ContainerPolicyError::TaskInvocation(reason.to_string())
}

/// The staged source plan this environment binds, if any.
///
/// `None` means the checkout's own host path is the source: either the
/// environment did not declare staging, the profile's config is unknown, or a
/// writable profile mount already reaches the checkout. `Some(plan)` names the
/// one owned directory the builder binds instead.
pub fn resolve_stage(
    repo_root: &Path,
    profile: &str,
    project_name: &str,
    declared: WorkspaceSource,
) -> Result<Option<StagePlan>, ContainerPolicyError> {
    if !declared.is_staged() {
        return Ok(None);
    }
    let Some(mounts) = crate::colima::colima_profile_mounts(profile) else {
        // An absent or unreadable profile config is not proof that the profile
        // shares nothing. The declared bind stays, exactly as before staging.
        return Ok(None);
    };
    let checkout = repo_root.canonicalize().map_err(|error| {
        refusal(format!(
            "container source staging cannot canonicalize checkout `{}`: {error}",
            repo_root.display()
        ))
    })?;
    let mut writable = Vec::new();
    for mount in mounts.iter().filter(|mount| mount.writable) {
        let canonical = mount.location.canonicalize().ok();
        // A writable mount that reaches this checkout needs no staging: the VM
        // sees the real source tree, including everything git does not track.
        if canonical
            .as_deref()
            .is_some_and(|root| checkout.starts_with(root))
        {
            return Ok(None);
        }
        // A mount the host cannot read is not a stage candidate; naming it as
        // one would stage into a directory the VM may not see either.
        if canonical.is_some() {
            writable.push(mount.location.clone());
        }
    }
    match writable.as_slice() {
        [mount_root] => Ok(Some(StagePlan::under(mount_root, project_name))),
        [] => Err(refusal(format!(
            "Colima profile `{profile}` shares no writable host mount that exists, and `{}` is not inside one, so the Linux artifact builder has no source it can reach; add one writable mount to the profile (for example `{base}/release-source`) or run the build where the profile shares the checkout",
            checkout.display(),
            base = default_stage_hint(),
        ))),
        mounts => Err(refusal(format!(
            "Colima profile `{profile}` shares {} writable host mounts ({}) and none of them reaches `{}`; Effigy stages the builder source under exactly one of them, so keep one writable mount or place the checkout inside one",
            mounts.len(),
            mounts
                .iter()
                .map(|mount| format!("`{}`", mount.display()))
                .collect::<Vec<_>>()
                .join(", "),
            checkout.display(),
        ))),
    }
}

fn default_stage_hint() -> String {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join("effigy-release-source").display().to_string())
        .unwrap_or_else(|| "$HOME/effigy-release-source".to_owned())
}

/// The staged source plan for a loaded policy. The verifier and the producer
/// both resolve it here so the bind and the proof can never disagree.
pub fn policy_stage(
    policy: &EffectiveContainerPolicy,
) -> Result<Option<StagePlan>, ContainerPolicyError> {
    resolve_stage(
        &policy.repo_root,
        &policy.profile,
        &policy.project_name,
        policy.workspace_source,
    )
}

/// The file a staged closure staged for one repo-relative path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StagedFile {
    pub sha256: String,
    pub executable: bool,
}

/// The binding record of one staged source.
///
/// It names exactly which caller source inputs the producer snapshotted: the
/// canonical checkout, its scope and project identity, the closure digest and
/// every file's digest. A build refuses when the record, the staged tree or the
/// caller's current closure disagree, so a different source is never reused as
/// if it were this one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageManifest {
    pub schema: String,
    pub caller: PathBuf,
    pub scope: Option<String>,
    pub project: String,
    pub profile: String,
    /// The directory bound at `/workspace`.
    pub source: PathBuf,
    /// Digest over the ordered `path sha256` inventory.
    pub closure: String,
    /// The caller's `HEAD` when the closure was recorded, as evidence only:
    /// the closure is the working tree's content, committed or not.
    pub head: Option<String>,
    pub files: BTreeMap<String, StagedFile>,
    /// Submodule paths and the commits the caller records for them. Their
    /// contents are never copied.
    pub submodules: BTreeMap<String, String>,
    /// The exclusion rules that kept private state out of the closure.
    pub excluded: Vec<String>,
}

impl StageManifest {
    /// The record must describe this exact caller, scope and project, and the
    /// plan's own directory. Anything else is another checkout's staged source.
    pub fn verify_binding(&self, plan: &StagePlan, expected: &StageBinding) -> Result<(), String> {
        if self.schema != STAGE_SCHEMA {
            return Err(format!(
                "staged source record `{}` has schema `{}`, not `{STAGE_SCHEMA}`",
                plan.manifest.display(),
                self.schema
            ));
        }
        if self.caller != expected.caller {
            return Err(format!(
                "staged source `{}` belongs to checkout `{}`, not `{}`",
                plan.stage_root.display(),
                self.caller.display(),
                expected.caller.display()
            ));
        }
        if self.scope != expected.scope {
            return Err(format!(
                "staged source `{}` belongs to scope `{}`, not `{}`",
                plan.stage_root.display(),
                self.scope.as_deref().unwrap_or("primary"),
                expected.scope.as_deref().unwrap_or("primary")
            ));
        }
        if self.project != expected.project {
            return Err(format!(
                "staged source `{}` belongs to project `{}`, not `{}`",
                plan.stage_root.display(),
                self.project,
                expected.project
            ));
        }
        if self.profile != expected.profile {
            return Err(format!(
                "staged source `{}` belongs to Colima profile `{}`, not `{}`",
                plan.stage_root.display(),
                self.profile,
                expected.profile
            ));
        }
        if self.source != plan.source {
            return Err(format!(
                "staged source record names `{}`, but this checkout binds `{}`",
                self.source.display(),
                plan.source.display()
            ));
        }
        Ok(())
    }

    pub fn read(plan: &StagePlan) -> Result<Self, String> {
        let raw = std::fs::read(&plan.manifest).map_err(|error| {
            format!(
                "staged source record `{}` is unreadable: {error}",
                plan.manifest.display()
            )
        })?;
        serde_json::from_slice(&raw).map_err(|error| {
            format!(
                "staged source record `{}` is not a binding record: {error}",
                plan.manifest.display()
            )
        })
    }
}

/// The caller facts a staged source record must match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageBinding {
    pub caller: PathBuf,
    pub scope: Option<String>,
    pub project: String,
    pub profile: String,
}

impl StageBinding {
    /// Resolve the caller facts a stage record is bound to. The caller is the
    /// canonical checkout — the same identity the builder's project and labels
    /// carry — and the scope is the checkout's runtime scope, if it has one.
    pub fn resolve(
        repo_root: &Path,
        project: &str,
        profile: &str,
        scope: Option<&str>,
    ) -> Result<Self, ContainerPolicyError> {
        let caller = effigy_core::worktree_scope::checkout_root(repo_root)
            .canonicalize()
            .map_err(|error| {
                refusal(format!(
                    "cannot canonicalize the owning checkout of `{}`: {error}",
                    repo_root.display()
                ))
            })?;
        Ok(Self {
            caller,
            scope: scope.map(str::to_owned),
            project: project.to_owned(),
            profile: profile.to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(mounts: &str) -> String {
        format!("cpu: 2\nmounts:\n{mounts}")
    }

    #[test]
    fn profile_mounts_read_the_mapping_and_short_forms() {
        let mounts = super::super::colima::colima_profile_mounts_in_config(&config(
            "  - location: /Volumes/Stash/scratch/effigy/release-source\n    writable: true\n  - /Volumes/Builds/readonly\n",
        ))
        .expect("profile mounts");
        assert_eq!(
            mounts,
            vec![
                super::super::colima::ColimaProfileMount {
                    location: PathBuf::from("/Volumes/Stash/scratch/effigy/release-source"),
                    writable: true,
                },
                super::super::colima::ColimaProfileMount {
                    location: PathBuf::from("/Volumes/Builds/readonly"),
                    writable: false,
                },
            ]
        );
    }

    #[test]
    fn undeclared_staging_never_consults_the_profile() {
        let root = tempfile::tempdir().expect("checkout");
        assert_eq!(
            resolve_stage(
                root.path(),
                "effigy-release",
                "builder",
                WorkspaceSource::Checkout
            )
            .expect("no stage"),
            None
        );
    }

    #[test]
    fn a_stage_child_is_named_by_the_project_under_the_mount() {
        let plan = StagePlan::under(
            Path::new("/Volumes/Stash/scratch/effigy/release-source"),
            "effigy-artifact-builder-v1-co-0123456789ab",
        );
        assert_eq!(
            plan.source,
            Path::new(
                "/Volumes/Stash/scratch/effigy/release-source/effigy-builder-source/effigy-artifact-builder-v1-co-0123456789ab/source"
            )
        );
        assert_eq!(plan.manifest.parent(), Some(plan.stage_root.as_path()));
    }

    fn manifest(caller: &Path, plan: &StagePlan) -> StageManifest {
        StageManifest {
            schema: STAGE_SCHEMA.to_owned(),
            caller: caller.to_path_buf(),
            scope: None,
            project: "app-dev".to_owned(),
            profile: "effigy-release".to_owned(),
            source: plan.source.clone(),
            closure: "a".repeat(64),
            head: None,
            files: BTreeMap::new(),
            submodules: BTreeMap::new(),
            excluded: Vec::new(),
        }
    }

    #[test]
    fn a_record_for_another_caller_scope_or_project_never_binds() {
        let plan = StagePlan::under(Path::new("/mnt/release-source"), "app-dev");
        let here = Path::new("/work/app");
        let elsewhere = Path::new("/work/other");
        let expected = StageBinding {
            caller: here.to_path_buf(),
            scope: None,
            project: "app-dev".to_owned(),
            profile: "effigy-release".to_owned(),
        };
        manifest(here, &plan)
            .verify_binding(&plan, &expected)
            .expect("own record");
        let mut record = manifest(elsewhere, &plan);
        assert!(record
            .verify_binding(&plan, &expected)
            .expect_err("another checkout's record")
            .contains("belongs to checkout"));
        record = manifest(here, &plan);
        record.scope = Some("7288dc2b3e8400000000000000000000".to_owned());
        assert!(record
            .verify_binding(&plan, &expected)
            .expect_err("another scope's record")
            .contains("belongs to scope"));
        record = manifest(here, &plan);
        record.project = "app-other".to_owned();
        assert!(record
            .verify_binding(&plan, &expected)
            .expect_err("another project's record")
            .contains("belongs to project"));
        record = manifest(here, &plan);
        record.schema = "effigy.builder-source-stage.v2".to_owned();
        assert!(record
            .verify_binding(&plan, &expected)
            .expect_err("another schema")
            .contains("schema"));
    }
}
