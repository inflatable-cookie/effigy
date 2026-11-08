//! The Linux artifact builder's staged source closure (effigy#715).
//!
//! A development checkout outside every mount the release Colima profile shares
//! has no source the VM can compile: a bind of its host path silently becomes an
//! empty VM directory. This module stages the exact source closure the build
//! needs into the checkout's own child of the profile's writable mount and
//! records what it staged, so the builder compiles the caller's current bytes
//! and the caller gets the artifact back.
//!
//! The closure is the checkout's tracked working tree: every path `git ls-files`
//! reports, read from the working tree so uncommitted edits are part of it. It
//! is never a recursive copy: Effigy's private state, local secret files,
//! ignored and untracked files, and submodule contents are excluded by rule and
//! recorded as excluded. A symlink, a missing tracked file or a path that cannot
//! be read refuses with the concrete cause instead of staging something else.

use std::collections::BTreeMap;
use std::path::Path;

use effigy_containers::artifact_source::{
    ensure_owned_stage_directory, StageBinding, StageManifest, StagePlan, StageState, StagedFile,
    ARTIFACT_BINARY_CAP_BYTES, ARTIFACT_EXPORT_DIR, ARTIFACT_RECEIPT_CAP_BYTES,
    ARTIFACT_SIDECAR_CAP_BYTES, STAGED_SOURCE_CAP_BYTES, STAGE_EXCLUDED_PREFIXES,
    STAGE_INVENTORY_FILE, STAGE_SCHEMA,
};

use crate::runner::error::RunnerError;

/// The staged closure this build compiles.
#[derive(Debug, Clone)]
pub(in crate::runner) struct StagedSource {
    pub manifest: StageManifest,
    /// Whether this staging wrote or removed anything. An unchanged closure
    /// keeps the builder's warm cargo target and needs no rewrite.
    pub changed: bool,
    /// Digest of the inventory file inside the bound tree, and the number of
    /// source files it lists. The container proves both before compiling.
    pub inventory_digest: String,
    /// Exact bytes this checkout's closure carries: the whole of this staging's
    /// host write, and the figure the write-boundary record reports.
    pub bytes: u64,
}

fn refusal(reason: impl std::fmt::Display) -> RunnerError {
    RunnerError::task_invocation(reason.to_string())
}

/// Stage this checkout's source closure into its owned directory under the
/// release profile's writable mount.
///
/// A record naming another checkout, scope or project is refused before any
/// write: staging never clobbers another owner's source. Ordinary incremental
/// builds keep the same directory, so the builder's cargo target stays warm and
/// the caller's project identity does not move.
pub(in crate::runner) fn stage(
    repo_root: &Path,
    policy: &effigy_containers::EffectiveContainerPolicy,
    plan: &StagePlan,
) -> Result<StagedSource, RunnerError> {
    let scope = effigy_core::worktree_scope::load(repo_root)
        .map_err(|error| refusal(format!("cannot resolve this checkout's run scope: {error}")))?;
    let binding = StageBinding::resolve(
        repo_root,
        &policy.project_name,
        &policy.profile,
        scope.as_deref(),
    )?;
    let caller = binding.caller.clone();
    // Ownership is proved before anything is created, written or removed: this
    // checkout's own record, or a child with nothing in it to adopt. A real
    // directory at the derived path proves nothing on its own.
    let previous = prove_owned_child(repo_root, plan, &binding)?;
    let closure = workspace_closure(&caller)?;
    // This staging's host writes are exactly the closure's bytes, and the
    // closure is known before the first file is written (effigy#743). Refusing
    // here keeps the shared mount's write surface finite without inventing a
    // quota: a source tree larger than the declared cap is never written.
    let closure_bytes = closure.bytes;
    if closure_bytes > STAGED_SOURCE_CAP_BYTES {
        return Err(refusal(format!(
            "this checkout's source closure is {closure_bytes} bytes, above the declared {STAGED_SOURCE_CAP_BYTES}-byte staged-source cap; nothing was staged"
        )));
    }
    // Only now, with ownership proved, is the owned leaf created: it is this
    // producer's child, not an existing directory this checkout never recorded.
    ensure_owned_stage_directory(plan, &plan.stage_root, &binding)?;
    let mut changed = false;
    // Record the binding before the first byte lands, so an interrupted run
    // leaves this checkout's own record instead of an unrecorded tree that a
    // later run would have to guess about.
    let staged_manifest = StageManifest {
        schema: STAGE_SCHEMA.to_owned(),
        caller: caller.clone(),
        scope: binding.scope.clone(),
        project: binding.project.clone(),
        profile: binding.profile.clone(),
        source: plan.source.clone(),
        closure: closure.digest.clone(),
        source_staged: true,
        // Written before the first file so an interrupted run leaves this
        // checkout's own record. It claims no closure until every file and the
        // inventory are on disk, so a crash here is recoverable rather than a
        // record that describes bytes nobody wrote.
        state: StageState::Pending,
        render_digest: previous
            .as_ref()
            .and_then(|record| record.render_digest.clone()),
        head: closure.head.clone(),
        files: closure.files.clone(),
        submodules: closure.submodules.clone(),
        excluded: STAGE_EXCLUDED_PREFIXES
            .iter()
            .map(|prefix| (*prefix).to_owned())
            .collect(),
    };
    write_manifest(plan, &staged_manifest)?;
    let bound = ensure_owned_stage_directory(plan, &plan.source, &binding)?;
    // Write the closure, then remove what it no longer names. The staged tree is
    // this checkout's own build source, so a deleted or renamed file must not
    // survive as stale input.
    for (relative, file) in &closure.files {
        let target = plan.source.join(relative);
        // The staged bytes decide, never a record: after an interrupted run the
        // record can describe a closure the tree does not carry, and trusting it
        // would leave that file stale forever.
        if staged_file_matches(&target, file) {
            continue;
        }
        let source = caller.join(relative);
        let bytes = std::fs::read(&source).map_err(|error| {
            refusal(format!(
                "cannot read tracked source `{}` for staging: {error}",
                source.display()
            ))
        })?;
        if sha256_hex(&bytes) != file.sha256 {
            return Err(refusal(format!(
                "tracked source `{}` changed while it was staged; nothing was bound",
                source.display()
            )));
        }
        if let Some(parent) = target.parent() {
            ensure_owned_directory_chain(&bound, parent)?;
        }
        replace_unowned_entry(&target)?;
        write_file_atomically(&target, &bytes, file.executable)?;
        changed = true;
    }
    changed |= remove_unaccounted(&plan.source, &closure.files)?;

    let mut manifest = staged_manifest;
    // A record written ahead of the files keeps this checkout's ownership; a
    // later run completes whatever an interruption left unfinished.
    if previous.as_ref() != Some(&manifest) {
        changed = true;
    }
    // `caller` is carried by the record written above; the closure's own copies
    // are consumed here.
    let _ = &caller;
    // The inventory names every staged file and its digest inside the bound
    // tree, so the container can prove the whole closure itself rather than two
    // sentinels. It is written last: a crash leaves a closure without a usable
    // inventory rather than an inventory that describes absent bytes.
    let inventory = inventory_bytes(&manifest);
    replace_unowned_entry(&plan.inventory())?;
    if std::fs::read(plan.inventory()).ok().as_deref() != Some(inventory.as_slice()) {
        write_file_atomically(&plan.inventory(), &inventory, false)?;
        changed = true;
    }
    // Prove the whole staged closure from disk, not just the sentinels: every
    // recorded digest, no unaccounted file, and the recorded digest count.
    verify_staged(plan, &manifest)?;
    // Only now does the record claim the closure.
    manifest.state = StageState::Complete;
    write_manifest(plan, &manifest)?;
    Ok(StagedSource {
        manifest,
        changed,
        inventory_digest: sha256_hex(&inventory),
        bytes: closure_bytes,
    })
}

/// The inventory the container reads: one `sha256  path` line per staged file,
/// in path order, exactly as `sha256sum` prints them.
fn inventory_bytes(manifest: &StageManifest) -> Vec<u8> {
    let mut body = String::new();
    for (relative, file) in &manifest.files {
        body.push_str(&file.sha256);
        body.push_str("  ");
        body.push_str(relative);
        body.push('\n');
    }
    body.into_bytes()
}

/// The staged tree must carry exactly the bytes the record names. Every file is
/// hashed and the tree is walked for unaccounted source, so a stale, missing or
/// aliased input is refused here rather than compiled.
fn verify_staged(plan: &StagePlan, manifest: &StageManifest) -> Result<(), RunnerError> {
    for (relative, expected) in &manifest.files {
        let path = plan.source.join(relative);
        let metadata = std::fs::symlink_metadata(&path).map_err(|error| {
            refusal(format!(
                "staged source `{}` is missing or unreadable: {error}",
                path.display()
            ))
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(refusal(format!(
                "staged source `{}` is not a regular file; the closure is unproven",
                path.display()
            )));
        }
        let bytes = std::fs::read(&path).map_err(|error| {
            refusal(format!(
                "staged source `{}` is unreadable: {error}",
                path.display()
            ))
        })?;
        if sha256_hex(&bytes) != expected.sha256 {
            return Err(refusal(format!(
                "staged source `{}` does not carry the digest this build recorded; nothing was bound",
                path.display()
            )));
        }
        if is_executable(&metadata) != expected.executable {
            return Err(refusal(format!(
                "staged source `{}` does not carry the executable mode this build recorded (recorded executable: {}); nothing was bound",
                path.display(),
                expected.executable
            )));
        }
    }
    let accounted = manifest.files.len();
    let found = count_source_files(&plan.source, "")?;
    if found != accounted {
        return Err(refusal(format!(
            "the staged source `{}` carries {found} files, not the {accounted} this build recorded; nothing was bound",
            plan.source.display()
        )));
    }
    Ok(())
}

/// Every staged source file, excluding the build's own `.effigy` output and the
/// inventory itself. Links are counted as entries and never followed.
fn count_source_files(directory: &Path, prefix: &str) -> Result<usize, RunnerError> {
    let mut count = 0;
    for entry in std::fs::read_dir(directory).map_err(|error| {
        refusal(format!(
            "cannot read the staged source `{}`: {error}",
            directory.display()
        ))
    })? {
        let entry = entry.map_err(|error| {
            refusal(format!(
                "cannot read the staged source `{}`: {error}",
                directory.display()
            ))
        })?;
        let name = entry.file_name().to_string_lossy().replace('\\', "/");
        let relative = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        if relative == ".effigy" || relative == STAGE_INVENTORY_FILE {
            continue;
        }
        let file_type = entry.file_type().map_err(|error| {
            refusal(format!(
                "cannot inspect staged entry `{}`: {error}",
                entry.path().display()
            ))
        })?;
        if file_type.is_dir() {
            count += count_source_files(&entry.path(), &relative)?;
        } else {
            count += 1;
        }
    }
    Ok(count)
}

/// Create one staged directory of this environment's own. A link in its place
/// is removed as a link — never followed, so nothing outside the owned tree is
/// touched — and the result must be a real directory.
fn ensure_owned_directory(directory: &Path) -> Result<(), RunnerError> {
    if let Ok(metadata) = std::fs::symlink_metadata(directory) {
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            std::fs::remove_file(directory).map_err(|error| {
                refusal(format!(
                    "cannot replace the unowned staged directory `{}`: {error}",
                    directory.display()
                ))
            })?;
        }
    }
    std::fs::create_dir_all(directory).map_err(|error| {
        refusal(format!(
            "cannot create the staged directory `{}`: {error}",
            directory.display()
        ))
    })?;
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

/// Whether the staged path already carries exactly the closure's entry: the
/// same bytes and the same executable bit. A mode-only change in the caller is a
/// changed input, so bytes alone are not enough.
fn staged_file_matches(path: &Path, file: &StagedFile) -> bool {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return false;
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return false;
    }
    if is_executable(&metadata) != file.executable {
        return false;
    }
    std::fs::read(path)
        .map(|bytes| sha256_hex(&bytes) == file.sha256)
        .unwrap_or(false)
}

/// Whether this staged entry carries an executable bit for its owner classes.
fn is_executable(metadata: &std::fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        false
    }
}

/// Publish the binding record atomically beside the staged trees.
fn write_manifest(plan: &StagePlan, record: &StageManifest) -> Result<(), RunnerError> {
    let body = serde_json::to_vec_pretty(record)
        .map_err(|error| refusal(format!("cannot render the stage record: {error}")))?;
    write_file_atomically(&plan.manifest, &body, false)
}

/// Read this checkout's own binding record for the staged child, or prove that
/// the child holds nothing to adopt. Anything else refuses before any mutation.
fn prove_owned_child(
    repo_root: &Path,
    plan: &StagePlan,
    binding: &StageBinding,
) -> Result<Option<StageManifest>, RunnerError> {
    let entries = match std::fs::read_dir(&plan.stage_root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
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
        return Ok(None);
    }
    match std::fs::symlink_metadata(&plan.manifest) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(refusal(format!(
                "staged source record `{}` is not a regular file; the staged child `{}` is not written, cleaned or adopted",
                plan.manifest.display(),
                plan.stage_root.display()
            )))
        }
        Ok(_) => {
            let record = StageManifest::read(plan).map_err(|error| {
                refusal(format!(
                    "{error}; the staged child `{}` already holds entries this checkout did not record, so it is not written, cleaned or adopted",
                    plan.stage_root.display()
                ))
            })?;
            record
                .verify_binding(plan, binding)
                .map_err(|reason| refusal(format!("{reason}; it is not written, cleaned or adopted")))?;
            let _ = repo_root;
            Ok(Some(record))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(refusal(format!(
            "the staged child `{}` already holds entries but carries no binding record for checkout `{}`; it is not written, cleaned or adopted",
            plan.stage_root.display(),
            binding.caller.display()
        ))),
        Err(error) => Err(refusal(format!(
            "cannot read staged source record `{}`: {error}",
            plan.manifest.display()
        ))),
    }
}

/// Create the staged directories between the bound root and `target`, one
/// component at a time. An existing link is removed as a link, so creating a
/// directory can never reach through an alias into another tree.
fn ensure_owned_directory_chain(root: &Path, target: &Path) -> Result<(), RunnerError> {
    let relative = target.strip_prefix(root).map_err(|_| {
        refusal(format!(
            "staged directory `{}` is outside the bound source `{}`",
            target.display(),
            root.display()
        ))
    })?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let std::path::Component::Normal(name) = component else {
            return Err(refusal(format!(
                "staged directory `{}` is not a plain path below `{}`",
                target.display(),
                root.display()
            )));
        };
        current.push(name);
        ensure_owned_directory(&current)?;
    }
    Ok(())
}

/// Remove an entry that must not be there as a link or a directory before this
/// invocation writes its file in its place. A link is unlinked, never followed.
fn replace_unowned_entry(path: &Path) -> Result<(), RunnerError> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            std::fs::remove_file(path).map_err(|error| {
                refusal(format!(
                    "cannot remove the unowned staged link `{}`: {error}",
                    path.display()
                ))
            })?;
        }
        Ok(metadata) if metadata.is_dir() => {
            return Err(refusal(format!(
                "staged path `{}` is a directory where this build writes a file",
                path.display()
            )));
        }
        _ => {}
    }
    Ok(())
}

/// Remove staged source the closure no longer names. The tree is this
/// checkout's own build source, so a deleted or renamed file must not survive
/// as stale input; a link is removed as a link and never traversed.
fn remove_unaccounted(
    source: &Path,
    files: &BTreeMap<String, StagedFile>,
) -> Result<bool, RunnerError> {
    remove_unaccounted_tree(source, "", files)
}

fn remove_unaccounted_tree(
    directory: &Path,
    prefix: &str,
    files: &BTreeMap<String, StagedFile>,
) -> Result<bool, RunnerError> {
    let mut removed = false;
    for entry in std::fs::read_dir(directory).map_err(|error| {
        refusal(format!(
            "cannot read staged source `{}`: {error}",
            directory.display()
        ))
    })? {
        let entry = entry.map_err(|error| {
            refusal(format!(
                "cannot read staged source `{}`: {error}",
                directory.display()
            ))
        })?;
        let name = entry.file_name().to_string_lossy().replace('\\', "/");
        let relative = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        // The build's own output (cargo target and artifacts) is Effigy's, not
        // the caller's source: it stays, and it is never handed back as source.
        if prefix.is_empty() && (name == ".effigy" || name == STAGE_INVENTORY_FILE) {
            continue;
        }
        let file_type = entry.file_type().map_err(|error| {
            refusal(format!(
                "cannot inspect staged entry `{}`: {error}",
                entry.path().display()
            ))
        })?;
        // `file_type` is the entry's own kind: a symlink here is never a
        // directory, so recursion cannot follow an alias out of the tree.
        if file_type.is_dir() && !file_type.is_symlink() {
            if remove_unaccounted_tree(&entry.path(), &relative, files)? {
                removed = true;
            }
            let empty = std::fs::read_dir(entry.path())
                .map(|mut entries| entries.next().is_none())
                .unwrap_or(false);
            if empty {
                std::fs::remove_dir(entry.path()).map_err(|error| {
                    refusal(format!(
                        "cannot remove empty staged directory `{}`: {error}",
                        entry.path().display()
                    ))
                })?;
            }
            continue;
        }
        if files.contains_key(&relative) {
            continue;
        }
        std::fs::remove_file(entry.path()).map_err(|error| {
            refusal(format!(
                "cannot remove stale staged source `{}`: {error}",
                entry.path().display()
            ))
        })?;
        removed = true;
    }
    Ok(removed)
}

/// The caller's tracked working-tree closure: its file digests, its submodule
/// pins and a digest over both, in path order.
struct WorkspaceClosure {
    files: BTreeMap<String, StagedFile>,
    submodules: BTreeMap<String, String>,
    head: Option<String>,
    digest: String,
    /// Exact bytes the closure carries, read from this checkout while it is
    /// listed. Staging refuses above its declared cap before writing any of it.
    bytes: u64,
}

fn workspace_closure(caller: &Path) -> Result<WorkspaceClosure, RunnerError> {
    let output = effigy_core::git_exec::run_git_output(
        Some(caller),
        &["ls-files", "-s", "-z"],
        |error| {
            refusal(format!(
                "cannot list the tracked source of `{}`: {error}",
                caller.display()
            ))
        },
        |detail| {
            refusal(format!(
                "cannot list the tracked source of `{}`: {detail}",
                caller.display()
            ))
        },
    )?;
    let raw = String::from_utf8(output.stdout).map_err(|error| {
        refusal(format!(
            "the tracked source list of `{}` is not UTF-8: {error}",
            caller.display()
        ))
    })?;
    let mut files = BTreeMap::new();
    let mut submodules = BTreeMap::new();
    let mut bytes_total = 0u64;
    for record in raw.split('\0').filter(|record| !record.is_empty()) {
        let (meta, relative) = record.split_once('\t').ok_or_else(|| {
            refusal(format!(
                "tracked source record `{record}` of `{}` has no path",
                caller.display()
            ))
        })?;
        let mut fields = meta.split(' ');
        let mode = fields.next().unwrap_or_default();
        let object = fields.next().unwrap_or_default();
        let relative = relative.replace('\\', "/");
        if STAGE_EXCLUDED_PREFIXES
            .iter()
            .any(|prefix| relative == prefix.trim_end_matches('/') || relative.starts_with(prefix))
        {
            continue;
        }
        let path = caller.join(&relative);
        // A submodule has no working-tree bytes here: its contents are never
        // copied, and the commit the caller records is kept as evidence.
        if mode == "160000" {
            submodules.insert(relative, object.to_owned());
            continue;
        }
        let metadata = std::fs::symlink_metadata(&path).map_err(|error| {
            refusal(format!(
                "tracked source `{}` is missing or unreadable: {error}",
                path.display()
            ))
        })?;
        if metadata.is_symlink() {
            return Err(refusal(format!(
                "tracked source `{}` is a symlink; it is not staged, because a link could carry an unaccounted path into the builder",
                path.display()
            )));
        }
        if !metadata.is_file() {
            return Err(refusal(format!(
                "tracked source `{}` is not a regular file; it is not staged",
                path.display()
            )));
        }
        let bytes = std::fs::read(&path).map_err(|error| {
            refusal(format!(
                "tracked source `{}` is unreadable: {error}",
                path.display()
            ))
        })?;
        let executable = mode == "100755";
        bytes_total += bytes.len() as u64;
        files.insert(
            relative,
            StagedFile {
                sha256: sha256_hex(&bytes),
                executable,
            },
        );
    }
    let head = effigy_core::git_exec::run_git_output(
        Some(caller),
        &["rev-parse", "HEAD"],
        |error| {
            refusal(format!(
                "cannot read the current commit of `{}`: {error}",
                caller.display()
            ))
        },
        |detail| {
            refusal(format!(
                "cannot read the current commit of `{}`: {detail}",
                caller.display()
            ))
        },
    )
    .ok()
    .and_then(|output| String::from_utf8(output.stdout).ok())
    .map(|raw| raw.trim().to_owned())
    .filter(|head| !head.is_empty());
    let digest = closure_digest(&files, &submodules);
    Ok(WorkspaceClosure {
        files,
        submodules,
        head,
        digest,
        bytes: bytes_total,
    })
}

/// The ordered `path sha256` inventory (and the submodule pins) as one digest.
fn closure_digest(
    files: &BTreeMap<String, StagedFile>,
    submodules: &BTreeMap<String, String>,
) -> String {
    let mut inventory = String::new();
    for (relative, file) in files {
        inventory.push_str(relative);
        inventory.push('\0');
        inventory.push_str(&file.sha256);
        inventory.push('\n');
    }
    for (relative, commit) in submodules {
        inventory.push_str("submodule\0");
        inventory.push_str(relative);
        inventory.push('\0');
        inventory.push_str(commit);
        inventory.push('\n');
    }
    sha256_hex(inventory.as_bytes())
}

fn sha256_hex(bytes: &[u8]) -> String {
    effigy_core::git_source::sha256_hex(bytes)
}

/// Publish one staged file without ever exposing a partial one: write beside it,
/// set its mode, then rename over the target.
fn write_file_atomically(target: &Path, bytes: &[u8], executable: bool) -> Result<(), RunnerError> {
    use std::io::Write;
    let parent = target.parent().ok_or_else(|| {
        refusal(format!(
            "staged source `{}` has no parent directory",
            target.display()
        ))
    })?;
    let temporary = parent.join(format!(
        ".{}.staging-{}",
        target
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "source".to_owned()),
        std::process::id()
    ));
    let mut file = std::fs::File::create(&temporary).map_err(|error| {
        refusal(format!(
            "cannot write staged source `{}`: {error}",
            temporary.display()
        ))
    })?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| {
            refusal(format!(
                "cannot write staged source `{}`: {error}",
                temporary.display()
            ))
        })?;
    drop(file);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = if executable { 0o755 } else { 0o644 };
        std::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(mode)).map_err(
            |error| {
                refusal(format!(
                    "cannot set the mode of staged source `{}`: {error}",
                    temporary.display()
                ))
            },
        )?;
    }
    std::fs::rename(&temporary, target).map_err(|error| {
        refusal(format!(
            "cannot publish staged source `{}`: {error}",
            target.display()
        ))
    })?;
    Ok(())
}

/// Refuse to publish when the source stopped matching what the builder compiled.
///
/// The builder reads a closure this producer snapshotted. A caller edit, a
/// concurrent writer or a changed staged tree during a long build would make the
/// produced artifact describe source that is no longer the recorded one, so both
/// sides are re-proved before any byte is copied back: the caller's current
/// tracked closure must still hash to the recorded digest, and the staged tree
/// must still carry every recorded digest.
pub(in crate::runner) fn verify_source_unchanged(
    plan: &StagePlan,
    staged: &StagedSource,
) -> Result<(), RunnerError> {
    // Only a complete record describes written bytes; a pending one is
    // ownership evidence and must never authorize a publication.
    staged
        .manifest
        .clone()
        .require_complete()
        .map_err(refusal)?;
    let current = workspace_closure(&staged.manifest.caller)?;
    if current.digest != staged.manifest.closure {
        return Err(refusal(format!(
            "the checkout `{}` changed while the builder compiled the recorded source (closure `{}` now, `{}` compiled); the produced artifact is not published, run again",
            staged.manifest.caller.display(),
            &current.digest[..12],
            &staged.manifest.closure[..12],
        )));
    }
    verify_staged(plan, &staged.manifest).map_err(|error| {
        refusal(format!(
            "the staged source changed while the builder compiled it ({error}); the produced artifact is not published, run again"
        ))
    })?;
    Ok(())
}

/// The staged artifacts a successful build produced, verified against their own
/// producer receipt before any byte reaches the caller's checkout.
#[derive(Debug, Clone)]
pub(in crate::runner) struct ProducedArtifact {
    /// Exact bytes this copy-back wrote into the checkout: its own host write,
    /// reported beside the export it read.
    pub bytes: u64,
    /// Whether the staged export was released after the bytes were published.
    pub export_released: bool,
}

/// Copy one verified staged artifact back into the caller's checkout, in the
/// order the existing freshness checks read it: bytes, then the version sidecar,
/// then the receipt that blesses them. Each file lands atomically, and nothing is
/// copied when the receipt's digest or target does not match the bytes.
pub(in crate::runner) fn restore_produced_artifact(
    plan: &StagePlan,
    checkout: &Path,
    triple: &str,
    active_version: &str,
) -> Result<ProducedArtifact, RunnerError> {
    let staged_artifacts = plan.source.join(".effigy/linux-release/artifacts");
    let checkout_artifacts = checkout.join(".effigy/linux-release/artifacts");
    let name = format!("effigy-{triple}");
    let staged_binary = staged_artifacts.join(&name);
    let staged_version_path = staged_artifacts.join(format!("{name}.active-version"));
    let staged_receipt_path = staged_artifacts.join(format!("{name}.receipt"));
    let receipt_bytes = read_capped(&staged_receipt_path, ARTIFACT_RECEIPT_CAP_BYTES)?;
    let receipt = String::from_utf8(receipt_bytes.clone()).map_err(|error| {
        refusal(format!(
            "the staged build's producer receipt `{}` is not UTF-8: {error}",
            staged_receipt_path.display()
        ))
    })?;
    let field = |key: &str| {
        receipt
            .lines()
            .find_map(|line| line.trim().strip_prefix(&format!("{key}=")))
            .map(str::trim)
    };
    if field("release_triple") != Some(triple) {
        return Err(refusal(format!(
            "the staged build's producer receipt names target `{}`, not `{triple}`",
            field("release_triple").unwrap_or("none")
        )));
    }
    let bytes = read_capped(&staged_binary, ARTIFACT_BINARY_CAP_BYTES)?;
    let digest = sha256_hex(&bytes);
    if field("sha256") != Some(digest.as_str()) {
        return Err(refusal(format!(
            "the staged artifact `{}` does not carry the digest its producer receipt names",
            staged_binary.display()
        )));
    }
    let staged_version = String::from_utf8(read_capped(
        &staged_version_path,
        ARTIFACT_SIDECAR_CAP_BYTES,
    )?)
    .map_err(|error| {
        refusal(format!(
            "the staged active version `{}` is not UTF-8: {error}",
            staged_version_path.display()
        ))
    })?;
    if staged_version.trim() != active_version {
        return Err(refusal(format!(
            "the staged artifact names version `{}`, not this host's `{active_version}`",
            staged_version.trim()
        )));
    }
    std::fs::create_dir_all(&checkout_artifacts).map_err(|error| {
        refusal(format!(
            "cannot create the artifact directory `{}`: {error}",
            checkout_artifacts.display()
        ))
    })?;
    let mut copied = 0u64;
    for (name, contents, executable) in [
        (name.clone(), bytes, true),
        (
            format!("{name}.active-version"),
            staged_version.into_bytes(),
            false,
        ),
        (format!("{name}.receipt"), receipt_bytes, false),
    ] {
        copied += contents.len() as u64;
        write_file_atomically(&checkout_artifacts.join(&name), &contents, executable)?;
    }
    // Release this build's staged export now that the bytes it blessed live in
    // the checkout: the container's writable export target is this checkout's
    // own output, never retained source. Only a link-free tree below the owned
    // export path is removed, and a refusal to remove it is reported rather than
    // hidden.
    let export_released = release_staged_export(plan)?;
    Ok(ProducedArtifact {
        bytes: copied,
        export_released,
    })
}

/// Read one produced file only when it is a regular file within its declared cap.
fn read_capped(path: &Path, cap: u64) -> Result<Vec<u8>, RunnerError> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| {
        refusal(format!(
            "the staged build wrote no `{}`: {error}",
            path.display()
        ))
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(refusal(format!(
            "the staged build's `{}` is not a regular file",
            path.display()
        )));
    }
    if metadata.len() > cap {
        return Err(refusal(format!(
            "the staged build's `{}` is {} bytes, above its declared {cap}-byte cap",
            path.display(),
            metadata.len()
        )));
    }
    let bytes = std::fs::read(path).map_err(|error| {
        refusal(format!(
            "the staged build's `{}` is unreadable: {error}",
            path.display()
        ))
    })?;
    if bytes.len() as u64 > cap {
        return Err(refusal(format!(
            "the staged build's `{}` changed while it was read past its declared {cap}-byte cap",
            path.display()
        )));
    }
    Ok(bytes)
}

/// Remove the staged build's own export directory, and only that.
///
/// The export lives in the staged source the profile's shared mount carries, so
/// the container can write it without a second writable bind. It is this
/// checkout's output, not source: releasing it keeps the staged tree the source
/// closure the build compiled and returns the host bytes the export used.
fn release_staged_export(plan: &StagePlan) -> Result<bool, RunnerError> {
    let export = plan.source.join(ARTIFACT_EXPORT_DIR);
    // `artifacts`, its `linux-release` parent and the private `.effigy` root the
    // container wrote, innermost first. Nothing above them is touched, and the
    // staged closure never carries any of them.
    let (Some(service), Some(private)) = (export.parent(), export.parent().and_then(Path::parent))
    else {
        return Ok(false);
    };
    for path in [&export, service, private] {
        let metadata = match std::fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
            Err(error) => {
                return Err(refusal(format!(
                    "cannot inspect the staged export `{}`: {error}",
                    path.display()
                )))
            }
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(refusal(format!(
                "the staged export `{}` is not a directory of its own; it is not removed",
                path.display()
            )));
        }
    }
    for path in [&export, service, private] {
        std::fs::remove_dir_all(path).map_err(|error| {
            refusal(format!(
                "cannot release the staged export `{}`: {error}",
                path.display()
            ))
        })?;
    }
    Ok(true)
}
