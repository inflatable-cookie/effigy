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
/// The owned directory one published Compose render generation is staged in, so
/// a Compose client running inside the VM can read the files it is given.
pub const STAGE_RENDER_DIR: &str = "render";
/// The digest inventory inside the bound tree. It lists every staged file and
/// its digest, so the container can prove the whole closure itself.
pub const STAGE_INVENTORY_FILE: &str = ".effigy-builder-source.inventory";
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
    /// The owned directory that holds one published render generation's files.
    pub fn render_directory(&self, digest: &str) -> PathBuf {
        self.stage_root.join(STAGE_RENDER_DIR).join(digest)
    }

    /// The digest inventory of the bound closure, inside the bound tree.
    pub fn inventory(&self) -> PathBuf {
        self.source.join(STAGE_INVENTORY_FILE)
    }

    /// Whether `path` is a Compose project directory of this checkout's own
    /// staged render: a digest generation of the owned render directory. A
    /// Compose container started from the staged render reports this as its
    /// working directory, and it is never another checkout's or another
    /// environment's.
    pub fn owns_staged_render_directory(&self, path: &Path) -> bool {
        let Ok(relative) = path.strip_prefix(self.stage_root.join(STAGE_RENDER_DIR)) else {
            return false;
        };
        let Some(generation) = relative
            .components()
            .next()
            .and_then(|part| part.as_os_str().to_str())
        else {
            return false;
        };
        path.is_dir()
            && generation.len() == 64
            && generation.bytes().all(|byte| byte.is_ascii_hexdigit())
    }

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

/// Move this environment's Compose inputs into the owned stage so the client
/// that runs inside the VM can read them.
///
/// A `colima nerdctl -- compose` client reads `-f` files and the project
/// directory from inside the VM. Render objects live under the checkout's
/// `.effigy/state/objects`, which a profile that shares only its own mount
/// cannot reach, so a staged source alone would still fail before the build.
/// The published closure's own bytes are written into
/// `<stage>/render/<digest>`, the compose files are repointed there, and the
/// rest of the policy (project identity, labels, world-readable paths, bind
/// sources) is untouched. Nothing is published to the environment generation:
/// the digest is unchanged, so admission, records and reuse are unaffected.
///
/// `None` means nothing was staged: the environment does not declare staging,
/// or the profile reaches the checkout, and today's paths stay in force.
pub fn stage_compose_render(
    policy: &mut EffectiveContainerPolicy,
) -> Result<Option<PathBuf>, ContainerPolicyError> {
    let Some(plan) = policy_stage(policy)? else {
        return Ok(None);
    };
    let Some(render) = policy.render.clone() else {
        return Err(refusal(format!(
            "container `{}` declares `workspace_source = \"staged\"`, but its Compose inputs are not a rendered closure; a staged source requires the generated render",
            policy.name
        )));
    };
    if !render.shared.is_empty() {
        return Err(refusal(format!(
            "container `{}` declares `workspace_source = \"staged\"` with shared services; a staged Compose render cannot carry a shared render",
            policy.name
        )));
    }
    let digest = render.closure.digest().to_owned();
    let directory = plan.render_directory(&digest);
    // Ownership first: the caller binding must own this staged child (or the
    // child must be absent/empty) before a render file is written or an older
    // generation is removed.
    let binding = StageBinding::resolve(
        &policy.repo_root,
        &policy.project_name,
        &policy.profile,
        effigy_core::worktree_scope::load(&policy.repo_root)
            .map_err(|error| refusal(format!("cannot resolve this checkout's run scope: {error}")))?
            .as_deref(),
    )?;
    ensure_owned_stage_directory(&plan, &plan.stage_root, &binding)?;
    // Record who owns the child before writing into it, so an interrupted run
    // leaves this checkout's own record rather than an unrecorded tree.
    record_render_staging(&plan, &binding, &digest)?;
    write_render_closure(&plan, &directory, &binding, &render.closure)?;
    // One generation at a time: the render is content-addressed and the current
    // digest names the files every activation and exec of this environment use.
    prune_other_render_generations(&plan, &digest)?;
    let mut compose_files = Vec::with_capacity(render.closure.files().len().max(1));
    for file in &policy.compose_files {
        let relative = file.strip_prefix(&render.path).map_err(|_| {
            refusal(format!(
                "container `{}` has a Compose input `{}` outside its rendered closure `{}`",
                policy.name,
                file.display(),
                render.path.display()
            ))
        })?;
        let staged = directory.join(relative);
        if !staged.is_file() {
            return Err(refusal(format!(
                "staged Compose input `{}` is missing from the owned render `{}`",
                staged.display(),
                directory.display()
            )));
        }
        compose_files.push(staged);
    }
    if compose_files.is_empty() {
        return Err(refusal(format!(
            "container `{}` declares `workspace_source = \"staged\"` with no Compose input to stage",
            policy.name
        )));
    }
    policy.compose_files = compose_files;
    if let Some(render) = policy.render.as_mut() {
        render.path = directory.clone();
    }
    Ok(Some(directory))
}

/// Write the render closure's bytes into the owned directory, then prove the
/// directory carries them. An existing file with other bytes is refused rather
/// than replaced: the digest names exactly one generation.
fn write_render_closure(
    plan: &StagePlan,
    directory: &Path,
    binding: &StageBinding,
    closure: &effigy_kernel::objects::Closure,
) -> Result<(), ContainerPolicyError> {
    ensure_owned_stage_directory(plan, &plan.stage_root.join(STAGE_RENDER_DIR), binding)?;
    ensure_owned_stage_directory(plan, directory, binding)?;
    for (name, bytes) in closure.files() {
        let relative = Path::new(name);
        if relative.is_absolute()
            || relative
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Err(refusal(format!(
                "staged Compose render name `{name}` escapes the owned render directory"
            )));
        }
        let target = directory.join(relative);
        if let Some(parent) = target.parent() {
            ensure_owned_stage_directory(plan, parent, binding)?;
        }
        match std::fs::symlink_metadata(&target) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                std::fs::remove_file(&target).map_err(|error| {
                    refusal(format!(
                        "cannot replace the unowned staged Compose input `{}`: {error}",
                        target.display()
                    ))
                })?;
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(refusal(format!(
                    "cannot inspect the staged Compose input `{}`: {error}",
                    target.display()
                )))
            }
        }
        if let Ok(existing) = std::fs::read(&target) {
            if existing == *bytes {
                continue;
            }
            return Err(refusal(format!(
                "staged Compose input `{}` already carries other bytes; the render generation is not reused",
                target.display()
            )));
        }
        let temporary = target.with_extension(format!("staging-{}", std::process::id()));
        std::fs::write(&temporary, bytes).map_err(|error| {
            refusal(format!(
                "cannot write the staged Compose input `{}`: {error}",
                temporary.display()
            ))
        })?;
        std::fs::rename(&temporary, &target).map_err(|error| {
            refusal(format!(
                "cannot publish the staged Compose input `{}`: {error}",
                target.display()
            ))
        })?;
    }
    Ok(())
}

/// Write or refresh this checkout's binding record for the staged child before
/// any render file lands in it.
///
/// A record that already carries the source closure keeps it: render staging
/// never overwrites the closure a build recorded. A record that only carried an
/// older render is updated to the generation being staged now.
fn record_render_staging(
    plan: &StagePlan,
    binding: &StageBinding,
    digest: &str,
) -> Result<(), ContainerPolicyError> {
    let existing = match std::fs::symlink_metadata(&plan.manifest) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return Err(refusal(format!(
                "staged source record `{}` is not a regular file; this checkout's staged child is unproven",
                plan.manifest.display()
            )))
        }
        Ok(_) => {
            let record = StageManifest::read(plan).map_err(refusal)?;
            record
                .verify_binding(plan, binding)
                .map_err(|reason| refusal(format!("{reason}; it is not written or adopted")))?;
            Some(record)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(refusal(format!(
                "cannot read staged source record `{}`: {error}",
                plan.manifest.display()
            )))
        }
    };
    let mut record = existing.unwrap_or_else(|| StageManifest {
        schema: STAGE_SCHEMA.to_owned(),
        caller: binding.caller.clone(),
        scope: binding.scope.clone(),
        project: binding.project.clone(),
        profile: binding.profile.clone(),
        source: plan.source.clone(),
        closure: String::new(),
        source_staged: false,
        render_digest: None,
        head: None,
        files: BTreeMap::new(),
        submodules: BTreeMap::new(),
        excluded: Vec::new(),
    });
    if record.render_digest.as_deref() == Some(digest) {
        return Ok(());
    }
    record.render_digest = Some(digest.to_owned());
    write_manifest(plan, &record)
}

/// Publish the record atomically beside the staged trees.
fn write_manifest(plan: &StagePlan, record: &StageManifest) -> Result<(), ContainerPolicyError> {
    let body = serde_json::to_vec_pretty(record)
        .map_err(|error| refusal(format!("cannot render the staged source record: {error}")))?;
    let temporary = plan
        .manifest
        .with_extension(format!("staging-{}", std::process::id()));
    std::fs::write(&temporary, &body).map_err(|error| {
        refusal(format!(
            "cannot write the staged source record `{}`: {error}",
            temporary.display()
        ))
    })?;
    std::fs::rename(&temporary, &plan.manifest).map_err(|error| {
        refusal(format!(
            "cannot publish the staged source record `{}`: {error}",
            plan.manifest.display()
        ))
    })?;
    Ok(())
}

/// Keep exactly the generation this environment binds now.
fn prune_other_render_generations(
    plan: &StagePlan,
    current: &str,
) -> Result<(), ContainerPolicyError> {
    let root = plan.stage_root.join(STAGE_RENDER_DIR);
    let entries = match std::fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(refusal(format!(
                "cannot read the owned render directory `{}`: {error}",
                root.display()
            )))
        }
    };
    for entry in entries {
        let entry = entry.map_err(|error| {
            refusal(format!(
                "cannot read the owned render directory `{}`: {error}",
                root.display()
            ))
        })?;
        if entry.file_name().to_string_lossy() == current {
            continue;
        }
        // A link is removed as a link. Recursion never follows one, so a stale
        // alias cannot point this cleanup outside the owned directory.
        match entry.file_type() {
            Ok(file_type) if file_type.is_dir() => std::fs::remove_dir_all(entry.path()),
            _ => std::fs::remove_file(entry.path()),
        }
        .map_err(|error| {
            refusal(format!(
                "cannot remove the superseded staged render `{}`: {error}",
                entry.path().display()
            ))
        })?;
    }
    Ok(())
}

/// Create `directory` as part of this environment's owned chain below the
/// profile's shared mount, and return its canonical path.
///
/// Every component is created and checked on its own, starting at the mount
/// root: an alias that already sits between the mount and the owned leaf is
/// removed as a link, so creating a directory can never reach through it into a
/// tree Effigy does not own. The mount itself must be a real directory of its
/// own, the finished chain must be real directories all the way down, and the
/// canonical result must still be inside the mount.
pub fn ensure_owned_stage_directory(
    plan: &StagePlan,
    directory: &Path,
    binding: &StageBinding,
) -> Result<PathBuf, ContainerPolicyError> {
    let mount = real_directory_of_its_own(&plan.mount_root, "the profile's shared mount")?;
    // Ownership is proved before anything below the mount is created, replaced
    // or removed: either this checkout's record is already there and names it, or
    // the owned child does not exist yet (or is empty) and this call creates it.
    prove_stage_ownership(plan, binding)?;
    let relative = directory.strip_prefix(&plan.mount_root).map_err(|_| {
        refusal(format!(
            "the staged directory `{}` is not below the profile's mount `{}`",
            directory.display(),
            plan.mount_root.display()
        ))
    })?;
    let mut current = plan.mount_root.clone();
    for component in relative.components() {
        let std::path::Component::Normal(name) = component else {
            return Err(refusal(format!(
                "the staged directory `{}` is not a plain path below `{}`",
                directory.display(),
                plan.mount_root.display()
            )));
        };
        current.push(name);
        ensure_real_directory(&current)?;
    }
    let observed = current.canonicalize().map_err(|error| {
        refusal(format!(
            "cannot resolve the staged directory `{}`: {error}",
            current.display()
        ))
    })?;
    if !observed.starts_with(&mount) {
        return Err(refusal(format!(
            "the staged directory `{}` resolves to `{}`, outside the profile's mount `{}`",
            current.display(),
            observed.display(),
            mount.display()
        )));
    }
    Ok(observed)
}

/// Prove this environment owns the staged child before any mutation.
///
/// A directory at the derived project path is not proof: anyone can create one.
/// Ownership is this checkout's own binding record, or the absence of anything
/// to adopt (the child does not exist, or is an empty directory this producer
/// creates). Anything else — the record belongs to another checkout, scope,
/// project or profile, is unreadable, or is missing while the child already
/// holds entries — refuses before a single write, unlink or removal.
pub fn prove_stage_ownership(
    plan: &StagePlan,
    binding: &StageBinding,
) -> Result<(), ContainerPolicyError> {
    let entries = match std::fs::read_dir(&plan.stage_root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(refusal(format!(
                "cannot read the staged child `{}`: {error}",
                plan.stage_root.display()
            )))
        }
    };
    let empty = entries
        .into_iter()
        .next()
        .transpose()
        .map_err(|error| {
            refusal(format!(
                "cannot read the staged child `{}`: {error}",
                plan.stage_root.display()
            ))
        })?
        .is_none();
    if empty {
        // An empty directory carries nothing to adopt; this producer owns it.
        return Ok(());
    }
    let record = StageManifest::read(plan).map_err(|error| {
        refusal(format!(
            "{error}; the staged child `{}` already holds entries this checkout did not record, so it is not written, cleaned or adopted",
            plan.stage_root.display()
        ))
    })?;
    record
        .verify_binding(plan, binding)
        .map_err(|reason| refusal(format!("{reason}; it is not written, cleaned or adopted")))?;
    Ok(())
}

/// One directory that is real and of its own: an existing alias is unlinked, an
/// existing non-directory is replaced, and the result is verified on disk.
fn ensure_real_directory(directory: &Path) -> Result<(), ContainerPolicyError> {
    match std::fs::symlink_metadata(directory) {
        // A pre-existing entry that is not the directory this producer owns is
        // refused before it is touched. Nothing here is ever unlinked, replaced
        // or written through: ownership is proved by the binding record first.
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(refusal(format!(
                "the staged path `{}` is a link this checkout did not create; it is not followed, replaced or adopted",
                directory.display()
            )))
        }
        Ok(metadata) if !metadata.is_dir() => {
            return Err(refusal(format!(
                "the staged path `{}` is a file this checkout did not create; it is not replaced or adopted",
                directory.display()
            )))
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir(directory).map_err(|error| {
                refusal(format!(
                    "cannot create the staged directory `{}`: {error}",
                    directory.display()
                ))
            })?;
        }
        Err(error) => {
            return Err(refusal(format!(
                "cannot inspect the staged directory `{}`: {error}",
                directory.display()
            )))
        }
    }
    let metadata = std::fs::symlink_metadata(directory).map_err(|error| {
        refusal(format!(
            "cannot inspect the staged directory `{}`: {error}",
            directory.display()
        ))
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(refusal(format!(
            "the staged directory `{}` is not a directory of its own",
            directory.display()
        )));
    }
    Ok(())
}

/// The canonical path of a directory that is a real directory of its own, never
/// a link and never something that resolves through one.
fn real_directory_of_its_own(
    directory: &Path,
    what: &str,
) -> Result<PathBuf, ContainerPolicyError> {
    let metadata = std::fs::symlink_metadata(directory).map_err(|error| {
        refusal(format!(
            "cannot inspect {what} `{}`: {error}",
            directory.display()
        ))
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(refusal(format!(
            "{what} `{}` is not a directory of its own",
            directory.display()
        )));
    }
    directory.canonicalize().map_err(|error| {
        refusal(format!(
            "cannot resolve {what} `{}`: {error}",
            directory.display()
        ))
    })
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
    /// Digest over the ordered `path sha256` inventory. Empty while only the
    /// Compose render has been staged: the source closure is not established
    /// yet, and no reader may treat the record as a source binding.
    pub closure: String,
    /// Whether the source closure in this record has been staged. A record that
    /// only carries the Compose render has `false` and an empty closure.
    #[serde(default)]
    pub source_staged: bool,
    /// The Compose render generation staged beside the source, if any.
    #[serde(default)]
    pub render_digest: Option<String>,
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
            source_staged: true,
            render_digest: None,
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
