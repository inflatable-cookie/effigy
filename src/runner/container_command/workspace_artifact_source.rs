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
use std::path::{Path, PathBuf};

use effigy_containers::artifact_source::{
    StageBinding, StageManifest, StagePlan, StagedFile, STAGE_EXCLUDED_PREFIXES, STAGE_SCHEMA,
};

use crate::runner::error::RunnerError;

/// The staged closure this build compiles.
#[derive(Debug, Clone)]
pub(in crate::runner) struct StagedSource {
    pub manifest: StageManifest,
    /// Whether this staging wrote or removed anything. An unchanged closure
    /// keeps the builder's warm cargo target and needs no rewrite.
    pub changed: bool,
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
    std::fs::create_dir_all(&plan.stage_root).map_err(|error| {
        refusal(format!(
            "cannot create the owned staged source directory `{}`: {error}",
            plan.stage_root.display()
        ))
    })?;
    let previous = match std::fs::symlink_metadata(&plan.manifest) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return Err(refusal(format!(
                "staged source record `{}` is not a regular file; this checkout's staged source is unproven",
                plan.manifest.display()
            )))
        }
        Ok(_) => {
            let record = StageManifest::read(plan).map_err(refusal)?;
            record
                .verify_binding(plan, &binding)
                .map_err(|reason| refusal(format!("{reason}; it is not rewritten or reused")))?;
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

    let closure = workspace_closure(&caller)?;
    let mut changed = false;
    prepare_source_directory(plan)?;
    // Write the closure, then remove what it no longer names. The staged tree is
    // this checkout's own build source, so a deleted or renamed file must not
    // survive as stale input.
    for (relative, file) in &closure.files {
        let target = plan.source.join(relative);
        let unchanged = previous
            .as_ref()
            .and_then(|record| record.files.get(relative))
            .is_some_and(|staged| staged == file)
            && target.is_file();
        if unchanged {
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
            std::fs::create_dir_all(parent).map_err(|error| {
                refusal(format!(
                    "cannot create staged source directory `{}`: {error}",
                    parent.display()
                ))
            })?;
        }
        write_file_atomically(&target, &bytes, file.executable)?;
        changed = true;
    }
    changed |= remove_unaccounted(&plan.source, &closure.files)?;

    let manifest = StageManifest {
        schema: STAGE_SCHEMA.to_owned(),
        caller,
        scope: scope.clone(),
        project: policy.project_name.clone(),
        profile: policy.profile.clone(),
        source: plan.source.clone(),
        closure: closure.digest.clone(),
        head: closure.head.clone(),
        files: closure.files,
        submodules: closure.submodules,
        excluded: STAGE_EXCLUDED_PREFIXES
            .iter()
            .map(|prefix| (*prefix).to_owned())
            .collect(),
    };
    // A closure digest that did not change still rewrites the record when the
    // record itself moved (a scope or project spelling), so the binding on disk
    // always describes this environment.
    if previous.as_ref() != Some(&manifest) {
        write_file_atomically(
            &plan.manifest,
            &serde_json::to_vec_pretty(&manifest)
                .map_err(|error| refusal(format!("cannot render the stage record: {error}")))?,
            false,
        )?;
        changed = true;
    }
    verify_staged(plan, &manifest)?;
    Ok(StagedSource { manifest, changed })
}

/// The staged tree must still carry the bytes the record names. The whole tree
/// is not rehashed: the sentinels a build always compiles decide, and the guest
/// re-proves them through the real bind before the build starts.
fn verify_staged(plan: &StagePlan, manifest: &StageManifest) -> Result<(), RunnerError> {
    for sentinel in ["Cargo.toml", "Cargo.lock"] {
        let Some(expected) = manifest.files.get(sentinel) else {
            continue;
        };
        let path = plan.source.join(sentinel);
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
    }
    Ok(())
}

/// A staged source root is written where nothing but the bound tree has lived:
/// the plan always names a dedicated directory, so an unexpected entry means
/// this environment's ownership is unproven.
fn prepare_source_directory(plan: &StagePlan) -> Result<(), RunnerError> {
    std::fs::create_dir_all(&plan.source).map_err(|error| {
        refusal(format!(
            "cannot create the staged source `{}`: {error}",
            plan.source.display()
        ))
    })?;
    Ok(())
}

fn remove_unaccounted(
    source: &Path,
    files: &BTreeMap<String, StagedFile>,
) -> Result<bool, RunnerError> {
    let mut removed = false;
    for entry in std::fs::read_dir(source).map_err(|error| {
        refusal(format!(
            "cannot read the staged source `{}`: {error}",
            source.display()
        ))
    })? {
        let entry = entry.map_err(|error| {
            refusal(format!(
                "cannot read the staged source `{}`: {error}",
                source.display()
            ))
        })?;
        let name = entry.file_name().to_string_lossy().replace('\\', "/");
        // The build's own output (cargo target and artifacts) is Effigy's, not
        // the caller's source: it stays, and it is never handed back as source.
        if name == ".effigy" {
            continue;
        }
        let metadata = entry.metadata().map_err(|error| {
            refusal(format!(
                "cannot inspect staged entry `{}`: {error}",
                entry.path().display()
            ))
        })?;
        if metadata.is_dir() {
            removed |= remove_unaccounted_tree(&entry.path(), &name, files)?;
            continue;
        }
        if !files.contains_key(&name) {
            std::fs::remove_file(entry.path()).map_err(|error| {
                refusal(format!(
                    "cannot remove stale staged source `{}`: {error}",
                    entry.path().display()
                ))
            })?;
            removed = true;
        }
    }
    Ok(removed)
}

fn remove_unaccounted_tree(
    directory: &Path,
    prefix: &str,
    files: &BTreeMap<String, StagedFile>,
) -> Result<bool, RunnerError> {
    let mut removed = false;
    let mut kept = false;
    for entry in std::fs::read_dir(directory).map_err(|error| {
        refusal(format!(
            "cannot read staged directory `{}`: {error}",
            directory.display()
        ))
    })? {
        let entry = entry.map_err(|error| {
            refusal(format!(
                "cannot read staged directory `{}`: {error}",
                directory.display()
            ))
        })?;
        let name = format!(
            "{prefix}/{}",
            entry.file_name().to_string_lossy().replace('\\', "/")
        );
        let metadata = entry.metadata().map_err(|error| {
            refusal(format!(
                "cannot inspect staged entry `{}`: {error}",
                entry.path().display()
            ))
        })?;
        if metadata.is_dir() {
            if remove_unaccounted_tree(&entry.path(), &name, files)? {
                removed = true;
            }
            if std::fs::read_dir(entry.path())
                .map(|mut entries| entries.next().is_none())
                .unwrap_or(false)
            {
                let _ = std::fs::remove_dir(entry.path());
                continue;
            }
            kept = true;
            continue;
        }
        if files.contains_key(&name) {
            kept = true;
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
    if !kept {
        std::fs::remove_dir(directory).map_err(|error| {
            refusal(format!(
                "cannot remove empty staged directory `{}`: {error}",
                directory.display()
            ))
        })?;
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

/// The staged artifacts a successful build produced, verified against their own
/// producer receipt before any byte reaches the caller's checkout.
#[derive(Debug, Clone)]
pub(in crate::runner) struct ProducedArtifact {
    pub binary: PathBuf,
    pub active_version: PathBuf,
    pub receipt: PathBuf,
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
    let staged = ProducedArtifact {
        binary: staged_artifacts.join(&name),
        active_version: staged_artifacts.join(format!("{name}.active-version")),
        receipt: staged_artifacts.join(format!("{name}.receipt")),
    };
    let receipt = std::fs::read_to_string(&staged.receipt).map_err(|error| {
        refusal(format!(
            "the staged build wrote no producer receipt `{}`: {error}",
            staged.receipt.display()
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
    let bytes = std::fs::read(&staged.binary).map_err(|error| {
        refusal(format!(
            "the staged build wrote no artifact `{}`: {error}",
            staged.binary.display()
        ))
    })?;
    let digest = sha256_hex(&bytes);
    if field("sha256") != Some(digest.as_str()) {
        return Err(refusal(format!(
            "the staged artifact `{}` does not carry the digest its producer receipt names",
            staged.binary.display()
        )));
    }
    let staged_version = std::fs::read_to_string(&staged.active_version).map_err(|error| {
        refusal(format!(
            "the staged build wrote no active version `{}`: {error}",
            staged.active_version.display()
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
    for (name, contents, executable) in [
        (name.clone(), bytes, true),
        (
            format!("{name}.active-version"),
            staged_version.into_bytes(),
            false,
        ),
        (format!("{name}.receipt"), receipt.into_bytes(), false),
    ] {
        write_file_atomically(&checkout_artifacts.join(&name), &contents, executable)?;
    }
    Ok(ProducedArtifact {
        binary: checkout_artifacts.join(&name),
        active_version: checkout_artifacts.join(format!("{name}.active-version")),
        receipt: checkout_artifacts.join(format!("{name}.receipt")),
    })
}
