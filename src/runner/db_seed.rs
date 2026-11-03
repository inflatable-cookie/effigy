use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};

use effigy_cli::BootstrapDbSeedInput;
use effigy_data::{
    collect_manifest_data_targets, database_seed_import_command, database_seed_reset_command,
    normalize_seed_source_path, seed_artifact_staging_plan, select_data_targets,
    select_database_service, ArtifactDataHandoff, DataSeedInput, DataSeedPlan, DataSeedSource,
    DataTargetManifestEntry, DataTargetManifestInput, DataTargetRef, DataTargetSelectionError,
    DatabaseService, DatabaseServiceKind, DatabaseServiceSelectionError, ResolvedDataTarget,
    SeedArtifactStagingPlan,
};
use effigy_execution::ExecutionSurface;
use effigy_manifest::{ManifestContainerServiceConfig, TASK_MANIFEST_FILE};
use effigy_runtime_plan::{RuntimeActivationPlan, RuntimeActivationRoute};
use effigy_state::artifact::{
    stage_local_artifact, stage_oci_artifact, ArtifactSourceRef, LocalArtifactRef,
    LocalArtifactStagingRequest, OciArtifactAdapter, OciArtifactPullRequest,
    OciArtifactStagingRequest,
};
use serde_json::json;

use crate::runner::container_command::{
    resolve_shared_data_binding, run_container_exec_capture_with_credentials,
    run_shared_data_command, verify_shared_data_target,
};
use crate::runner::container_runtime_prep::{
    activate_container_runtime_for_task, build_runtime_activation_plan, ActivationRequest,
};
use crate::runner::db_services::collect_manifest_database_services;
use crate::runner::execute::api::{
    resolve_execution_binding_resolution, run_manifest_task_with_surface_and_env,
};
use crate::runner::manifest::load_task_manifest;
use crate::runner::runtime_session_context::{with_runtime_session_context, RuntimeSessionContext};
use effigy_state::artifact::{infer_kind_from_primary_files, OrasCliArtifactAdapter};

use super::error::RunnerError;
use effigy_cli::TaskInvocation;

pub(in crate::runner) const DB_SEED_TASK: &str = "data:seed";
pub(in crate::runner) const DB_SEEDS_DIR: &str = ".effigy/local/db-seeds";
pub(in crate::runner) const DB_SEEDS_METADATA_FILE: &str = "_effigy-db-seeds.json";
pub(in crate::runner) const LEGACY_BOOTSTRAP_DB_SEEDS_METADATA_FILE: &str =
    "_effigy-bootstrap-db-seeds.json";
pub(in crate::runner) const DB_SEEDS_DIR_ENV: &str = "EFFIGY_DB_SEEDS_DIR";
pub(in crate::runner) const DB_SEED_FILE_ENV: &str = "EFFIGY_DB_SEED_FILE";
pub(in crate::runner) const DB_SEED_COUNT_ENV: &str = "EFFIGY_DB_SEED_COUNT";
pub(in crate::runner) const DB_SEED_FILES_ENV: &str = "EFFIGY_DB_SEED_FILES";
pub(in crate::runner) const DB_SEEDS_JSON_ENV: &str = "EFFIGY_DB_SEEDS_JSON";
pub(in crate::runner) const DB_SEED_TARGET_ENV: &str = "EFFIGY_DB_SEED_TARGET";
pub(in crate::runner) const LEGACY_BOOTSTRAP_DB_SEEDS_DIR_ENV: &str =
    "EFFIGY_BOOTSTRAP_DB_SEEDS_DIR";
pub(in crate::runner) const LEGACY_BOOTSTRAP_DB_SEED_FILE_ENV: &str =
    "EFFIGY_BOOTSTRAP_DB_SEED_FILE";
pub(in crate::runner) const LEGACY_BOOTSTRAP_DB_SEED_COUNT_ENV: &str =
    "EFFIGY_BOOTSTRAP_DB_SEED_COUNT";
pub(in crate::runner) const LEGACY_BOOTSTRAP_DB_SEED_FILES_ENV: &str =
    "EFFIGY_BOOTSTRAP_DB_SEED_FILES";
pub(in crate::runner) const LEGACY_BOOTSTRAP_DB_SEEDS_JSON_ENV: &str =
    "EFFIGY_BOOTSTRAP_DB_SEEDS_JSON";
pub(in crate::runner) const LEGACY_BOOTSTRAP_DB_SEED_TARGET_ENV: &str =
    "EFFIGY_BOOTSTRAP_DB_SEED_TARGET";

#[derive(Debug, serde::Deserialize)]
struct SeedMetadataEntry {
    target: Option<String>,
    staged_path: String,
}

pub(in crate::runner) fn data_seed_runtime_session_context() -> RuntimeSessionContext {
    RuntimeSessionContext {
        data_seed_session: true,
        ..RuntimeSessionContext::default()
    }
}

/// One database dump staged into the repository for the seed task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct StagedDbSeed {
    pub target: Option<String>,
    pub source_path: PathBuf,
    pub staged_path: PathBuf,
}

pub(in crate::runner) fn resolve_db_seed_input_paths(
    cwd: &Path,
    db_seeds: &[BootstrapDbSeedInput],
) -> Vec<BootstrapDbSeedInput> {
    db_seeds
        .iter()
        .map(|seed| BootstrapDbSeedInput {
            target: seed.target.clone(),
            path: normalize_seed_source_path(cwd, seed.path.clone()),
        })
        .collect()
}

pub(in crate::runner) fn maybe_prompt_db_seed_inputs(
    repo_root: &Path,
    output_json: bool,
    no_prompt: bool,
    effective_db_seeds: &mut Vec<BootstrapDbSeedInput>,
    prompt_checked: &mut bool,
) -> Result<(), RunnerError> {
    if *prompt_checked
        || !effective_db_seeds.is_empty()
        || !should_prompt_db_seeds(output_json, no_prompt)
    {
        return Ok(());
    }

    let manifest = load_task_manifest(&repo_root.join(TASK_MANIFEST_FILE))?;
    let targets = logical_database_targets(&manifest);
    if targets.is_empty() {
        *prompt_checked = true;
        return Ok(());
    }

    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    *effective_db_seeds = collect_db_seed_prompts_from_io(
        repo_root,
        &targets
            .iter()
            .map(|target| target.name.to_string())
            .collect::<Vec<_>>(),
        &mut stdin,
        &mut stdout,
    )?;
    *prompt_checked = true;
    Ok(())
}

pub(in crate::runner) fn should_prompt_db_seeds(output_json: bool, no_prompt: bool) -> bool {
    !output_json && !no_prompt && io::stdin().is_terminal() && io::stdout().is_terminal()
}

fn prompt_yes_no<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    prompt: &str,
) -> Result<bool, RunnerError> {
    prompt_yes_no_with_default(input, output, prompt, true)
}

fn prompt_yes_no_with_default<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    prompt: &str,
    default: bool,
) -> Result<bool, RunnerError> {
    output
        .write_all(prompt.as_bytes())
        .and_then(|_| output.flush())
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to render interactive bootstrap prompt: {error}"
            ))
        })?;
    let mut line = String::new();
    input.read_line(&mut line).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to read interactive bootstrap input: {error}"
        ))
    })?;
    let normalized = line.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Ok(default);
    }
    Ok(normalized == "y" || normalized == "yes")
}

pub(in crate::runner) fn collect_db_seed_prompts_from_io<R: BufRead, W: Write>(
    repo_root: &Path,
    targets: &[String],
    input: &mut R,
    output: &mut W,
) -> Result<Vec<BootstrapDbSeedInput>, RunnerError> {
    output
        .write_all(
            b"No --db-seed inputs were supplied.\nEnter a SQL dump path for each database, or leave blank to skip.\n\n",
        )
        .and_then(|_| output.flush())
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to render interactive bootstrap prompt: {error}"
            ))
        })?;

    let mut db_seeds = Vec::new();
    for target in targets {
        loop {
            write!(output, "{target}: ")
                .and_then(|_| output.flush())
                .map_err(|error| {
                    RunnerError::task_invocation(format!(
                        "failed to render interactive bootstrap prompt: {error}"
                    ))
                })?;
            let mut line = String::new();
            input.read_line(&mut line).map_err(|error| {
                RunnerError::task_invocation(format!(
                    "failed to read interactive bootstrap input: {error}"
                ))
            })?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                break;
            }
            let path = PathBuf::from(trimmed);
            let normalized = if path.is_absolute() {
                path
            } else {
                repo_root.join(path)
            };
            if !normalized.is_file() {
                writeln!(
                    output,
                    "Path does not exist or is not a readable file: {}",
                    normalized.display()
                )
                .and_then(|_| output.flush())
                .map_err(|error| {
                    RunnerError::task_invocation(format!(
                        "failed to render interactive bootstrap prompt: {error}"
                    ))
                })?;
                continue;
            }
            db_seeds.push(BootstrapDbSeedInput {
                target: Some(target.clone()),
                path: normalized,
            });
            break;
        }
    }

    if db_seeds.is_empty() {
        return Ok(db_seeds);
    }

    let prompt = if db_seeds.len() == 1 {
        "Continue with 1 database seed file? [Y/n]: ".to_owned()
    } else {
        format!(
            "Continue with {} database seed file(s)? [Y/n]: ",
            db_seeds.len()
        )
    };
    if !prompt_yes_no(input, output, &prompt)? {
        return Err(RunnerError::task_invocation(
            "bootstrap cancelled during interactive database seed prompt",
        ));
    }

    Ok(db_seeds)
}

pub(in crate::runner) fn stage_db_seed_files(
    repo_root: &Path,
    db_seeds: &[BootstrapDbSeedInput],
) -> Result<Vec<StagedDbSeed>, RunnerError> {
    let adapter = OrasCliArtifactAdapter::default();
    stage_db_seed_files_with_adapter(repo_root, db_seeds, &adapter)
}

pub(in crate::runner) fn validate_db_seed_import_inputs(
    repo_root: &Path,
    db_seeds: &[BootstrapDbSeedInput],
) -> Result<(), RunnerError> {
    let manifest = load_task_manifest(&repo_root.join(TASK_MANIFEST_FILE))?;
    resolve_db_seed_targets(repo_root, &manifest, db_seeds)?;
    Ok(())
}

pub(in crate::runner) fn run_db_seed_import_report(
    repo_root: &Path,
    source: PathBuf,
    target: Option<String>,
) -> Result<serde_json::Value, RunnerError> {
    let adapter = OrasCliArtifactAdapter::default();
    let stage_report = stage_db_seed_files_report_with_adapter(
        repo_root,
        &[BootstrapDbSeedInput {
            target: target.clone(),
            path: source.clone(),
        }],
        &adapter,
    )?;
    let env = db_seed_env(&stage_report.staged);
    run_db_seed_task(repo_root, &env)?;
    Ok(json!({
        "schema": "effigy.state-stack.sql-import.v1",
        "schema_version": 1,
        "ok": true,
        "target": target,
        "source": source.display().to_string(),
        "artifact_reports": stage_report.artifact_reports,
        "staged": stage_report.staged.iter().map(|seed| {
            json!({
                "target": seed.target,
                "source_path": seed.source_path.display().to_string(),
                "staged_path": seed.staged_path.display().to_string(),
            })
        }).collect::<Vec<_>>(),
    }))
}

fn stage_db_seed_files_with_adapter(
    repo_root: &Path,
    db_seeds: &[BootstrapDbSeedInput],
    adapter: &dyn OciArtifactAdapter,
) -> Result<Vec<StagedDbSeed>, RunnerError> {
    Ok(stage_db_seed_files_report_with_adapter(repo_root, db_seeds, adapter)?.staged)
}

/// Bytes preserved verbatim in a staged seed file name; every other byte
/// (including `%`, path separators and control bytes) is percent-encoded.
fn is_staged_seed_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')
}

/// Percent-encode one target label into a single staging file-name component.
///
/// Encoding `%` itself keeps the mapping injective, so distinct labels stay
/// distinct staged names while ordinary labels such as `app` or `legacy_mysql`
/// keep their existing `<target>--<file>` names byte for byte. No label can
/// introduce a separator or a parent component into the staged name.
fn encode_staged_seed_name(label: &str) -> String {
    let mut encoded = String::with_capacity(label.len());
    for byte in label.bytes() {
        if is_staged_seed_name_byte(byte) {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

/// The staged file name for one seed payload inside the owned staging root.
fn staged_db_seed_file_name(target: Option<&str>, base_name: &str) -> String {
    match target {
        Some(target) => format!("{}--{base_name}", encode_staged_seed_name(target)),
        None => base_name.to_owned(),
    }
}

/// The repo-relative path that metadata, the seed environment handoff and
/// cleanup all use for one staged payload.
///
/// Staging places every payload directly inside the owned `db-seeds` root, so
/// this reconstruction names exactly the copied object.
fn staged_seed_repo_path(staged_path: &Path) -> PathBuf {
    Path::new(DB_SEEDS_DIR).join(
        staged_path
            .file_name()
            .expect("staged seed file should have name"),
    )
}

#[derive(Debug)]
struct DbSeedStageReport {
    staged: Vec<StagedDbSeed>,
    artifact_reports: Vec<serde_json::Value>,
}

fn stage_db_seed_files_report_with_adapter(
    repo_root: &Path,
    db_seeds: &[BootstrapDbSeedInput],
    adapter: &dyn OciArtifactAdapter,
) -> Result<DbSeedStageReport, RunnerError> {
    let manifest = load_task_manifest(&repo_root.join(TASK_MANIFEST_FILE))?;
    let seed_plans = bootstrap_db_seed_plans(repo_root, &manifest, db_seeds)?;
    let staging_dir = repo_root.join(DB_SEEDS_DIR);
    std::fs::create_dir_all(&staging_dir).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to create db seed directory {}: {error}",
            staging_dir.display()
        ))
    })?;

    let mut seen_names = BTreeSet::new();
    let mut staged = Vec::with_capacity(seed_plans.len());
    let mut artifact_reports = Vec::with_capacity(seed_plans.len());
    for plan in &seed_plans {
        let source = data_seed_source_display(&plan.input.source);
        let artifact_report = stage_seed_artifact(repo_root, plan, adapter)?;
        let staged_artifact_file =
            artifact_report
                .metadata
                .primary_files
                .first()
                .ok_or_else(|| {
                    RunnerError::task_invocation(format!(
                        "db seed artifact produced no primary file: {source}"
                    ))
                })?;
        let Some(file_name) = staged_artifact_file.file_name() else {
            return Err(RunnerError::task_invocation(format!(
                "db seed artifact primary file has no file name: {}",
                staged_artifact_file.display()
            )));
        };
        let base_name = file_name.to_string_lossy().to_string();
        let staged_name = staged_db_seed_file_name(
            plan.input.target.as_ref().map(DataTargetRef::as_str),
            &base_name,
        );
        if !seen_names.insert(staged_name.clone()) {
            return Err(RunnerError::task_invocation(format!(
                "duplicate staged db seed file name `{staged_name}`; rename one input or use distinct seed targets"
            )));
        }
        let destination = staging_dir.join(&staged_name);
        // Target labels are opaque (DataTargetRef carries any string), so the
        // encoded staged name must be the only thing ever joined onto the
        // staging root. This guard fails the staging closed if a name ever
        // stops being a single component inside that root.
        if destination.parent() != Some(staging_dir.as_path()) {
            return Err(RunnerError::task_invocation(format!(
                "staged db seed name `{staged_name}` is not a single file name inside {}; refusing to write outside owned staging",
                staging_dir.display()
            )));
        }
        std::fs::copy(staged_artifact_file, &destination).map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to stage db seed {source} -> {}: {error}",
                destination.display(),
            ))
        })?;
        let encoded_artifact_report = serde_json::to_value(&artifact_report).map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to encode db seed artifact staging report: {error}"
            ))
        })?;
        staged.push(StagedDbSeed {
            target: plan.input.target.as_ref().map(ToString::to_string),
            source_path: PathBuf::from(artifact_report.metadata.source),
            staged_path: destination,
        });
        artifact_reports.push(encoded_artifact_report);
    }

    let metadata = serde_json::to_string(
        &staged
            .iter()
            .map(|seed| {
                json!({
                    "target": seed.target,
                    "source_path": seed.source_path.display().to_string(),
                    "staged_path": staged_seed_repo_path(&seed.staged_path)
                        .display()
                        .to_string(),
                })
            })
            .collect::<Vec<_>>(),
    )
    .expect("db seed metadata should serialize");

    for file_name in [
        DB_SEEDS_METADATA_FILE,
        LEGACY_BOOTSTRAP_DB_SEEDS_METADATA_FILE,
    ] {
        std::fs::write(staging_dir.join(file_name), &metadata).map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to write db seed metadata file {}: {error}",
                staging_dir.join(file_name).display()
            ))
        })?;
    }
    Ok(DbSeedStageReport {
        staged,
        artifact_reports,
    })
}

fn stage_seed_artifact(
    repo_root: &Path,
    plan: &DataSeedPlan,
    adapter: &dyn OciArtifactAdapter,
) -> Result<effigy_state::artifact::StagedArtifactReport, RunnerError> {
    let handoff = plan.artifact_handoff.as_ref().ok_or_else(|| {
        RunnerError::task_invocation("db seed plan is missing an artifact handoff")
    })?;
    match handoff {
        ArtifactDataHandoff::StageSource { ref source, .. } => {
            match ArtifactSourceRef::parse(source)
                .map_err(|error| RunnerError::task_invocation(error.to_string()))?
            {
                ArtifactSourceRef::Local(_) => {
                    let Some(SeedArtifactStagingPlan::Local {
                        source_path: path,
                        artifact_root,
                    }) = seed_artifact_staging_plan(repo_root, handoff)
                    else {
                        unreachable!("local seed handoff should produce local staging plan")
                    };
                    if !path.is_file() {
                        return Err(RunnerError::task_invocation(format!(
                            "db seed is not a readable file: {}",
                            path.display()
                        )));
                    }
                    stage_local_artifact(&LocalArtifactStagingRequest::new(
                        LocalArtifactRef::new(path.clone()),
                        repo_root.to_path_buf(),
                        artifact_root,
                    ))
                    .map_err(|error| {
                        RunnerError::task_invocation(format!(
                            "failed to stage db seed artifact {}: {error}",
                            path.display()
                        ))
                    })
                }
                ArtifactSourceRef::Oci(oci) => {
                    let Some(SeedArtifactStagingPlan::Oci {
                        artifact_root,
                        pull_destination_root,
                        ..
                    }) = seed_artifact_staging_plan(repo_root, handoff)
                    else {
                        unreachable!("OCI seed handoff should produce OCI staging plan")
                    };
                    let pull = adapter
                        .pull(&OciArtifactPullRequest {
                            reference: oci.clone(),
                            destination_root: pull_destination_root,
                        })
                        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
                    let kind = infer_kind_from_primary_files(&pull.primary_files);
                    let mut request = OciArtifactStagingRequest::new(
                        oci,
                        pull.pulled_root,
                        artifact_root,
                        pull.primary_files,
                        kind,
                    );
                    if let Some(digest) = pull.descriptor.digest {
                        request = request.with_digest(digest);
                    }
                    stage_oci_artifact(&request).map_err(|error| {
                        RunnerError::task_invocation(format!(
                            "failed to stage OCI db seed artifact {source}: {error}"
                        ))
                    })
                }
            }
        }
        ArtifactDataHandoff::CaptureDestination { .. } => {
            unreachable!("seed handoff cannot capture")
        }
    }
}

pub(in crate::runner) fn db_seed_env(staged_db_seeds: &[StagedDbSeed]) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    if staged_db_seeds.is_empty() {
        return env;
    }
    let seeds_dir = Path::new(DB_SEEDS_DIR);
    let metadata_json = serde_json::to_string(
        &staged_db_seeds
            .iter()
            .map(|seed| {
                json!({
                    "target": seed.target,
                    "source_path": seed.source_path.display().to_string(),
                    "staged_path": staged_seed_repo_path(&seed.staged_path)
                        .display()
                        .to_string(),
                })
            })
            .collect::<Vec<_>>(),
    )
    .expect("db seed env json should serialize");
    let files = staged_db_seeds
        .iter()
        .map(|seed| {
            staged_seed_repo_path(&seed.staged_path)
                .display()
                .to_string()
        })
        .collect::<Vec<_>>();

    for key in [DB_SEEDS_DIR_ENV, LEGACY_BOOTSTRAP_DB_SEEDS_DIR_ENV] {
        env.insert(key.to_owned(), seeds_dir.display().to_string());
    }
    for key in [DB_SEED_COUNT_ENV, LEGACY_BOOTSTRAP_DB_SEED_COUNT_ENV] {
        env.insert(key.to_owned(), staged_db_seeds.len().to_string());
    }
    for key in [DB_SEED_FILES_ENV, LEGACY_BOOTSTRAP_DB_SEED_FILES_ENV] {
        env.insert(key.to_owned(), files.join("\n"));
    }
    for key in [DB_SEEDS_JSON_ENV, LEGACY_BOOTSTRAP_DB_SEEDS_JSON_ENV] {
        env.insert(key.to_owned(), metadata_json.clone());
    }

    if staged_db_seeds.len() == 1 {
        let staged_file = staged_seed_repo_path(&staged_db_seeds[0].staged_path)
            .display()
            .to_string();
        for key in [DB_SEED_FILE_ENV, LEGACY_BOOTSTRAP_DB_SEED_FILE_ENV] {
            env.insert(key.to_owned(), staged_file.clone());
        }
        if let Some(target) = staged_db_seeds[0].target.as_deref() {
            for key in [DB_SEED_TARGET_ENV, LEGACY_BOOTSTRAP_DB_SEED_TARGET_ENV] {
                env.insert(key.to_owned(), target.to_owned());
            }
        }
    }
    env
}

pub(in crate::runner) fn run_db_seed_task(
    repo_root: &Path,
    env_overrides: &BTreeMap<String, String>,
) -> Result<(), RunnerError> {
    let manifest = load_task_manifest(&repo_root.join(TASK_MANIFEST_FILE))?;
    if manifest.tasks.contains_key(DB_SEED_TASK) {
        prepare_db_seed_runtime(repo_root, &manifest)?;
        return run_manifest_db_seed_task(repo_root, env_overrides);
    }
    let policy = effigy_containers::load_container_policy(repo_root, None)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    run_db_seed_task_for_policy(repo_root, env_overrides, &policy)
}

/// Run the database seed for one already-selected environment.
///
/// The public data command passes the policy its `--name` selected, so seed
/// reads that environment's declarations rather than reloading the manifest's
/// default. The built-in seed's declared shared targets are resolved and proved
/// here, before anything reconciles the environment: an activation apply would
/// publish, start or replace the shared service the data command must not
/// touch. The activation therefore runs only when no selected target is shared,
/// and a local target in a mixed selection needs its environment already
/// running.
pub(in crate::runner) fn run_db_seed_task_for_policy(
    repo_root: &Path,
    env_overrides: &BTreeMap<String, String>,
    policy: &effigy_containers::EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    let manifest = load_task_manifest(&repo_root.join(TASK_MANIFEST_FILE))?;
    if manifest.tasks.contains_key(DB_SEED_TASK) {
        // An app-defined seed task owns its own environment binding.
        prepare_db_seed_runtime(repo_root, &manifest)?;
        return run_manifest_db_seed_task(repo_root, env_overrides);
    }
    let entries = resolve_builtin_db_seed_entries(repo_root, &manifest, policy, env_overrides)?;
    let shared = shared_seed_bindings(&entries);
    for binding in &shared {
        verify_shared_data_target(policy, binding)?;
    }
    if shared.is_empty() {
        prepare_builtin_db_seed_runtime(repo_root, policy)?;
    }
    run_builtin_db_seed_entries_with(
        repo_root,
        policy,
        &entries,
        |policy, binding, command, stdin_file| {
            run_shared_data_command(policy, binding, command, stdin_file)
        },
    )
}

fn run_manifest_db_seed_task(
    repo_root: &Path,
    env_overrides: &BTreeMap<String, String>,
) -> Result<(), RunnerError> {
    with_runtime_session_context(data_seed_runtime_session_context(), || {
        run_manifest_task_with_surface_and_env(
            &TaskInvocation {
                name: DB_SEED_TASK.to_owned(),
                args: Vec::new(),
            },
            repo_root.to_path_buf(),
            ExecutionSurface::DataSeed,
            env_overrides,
        )
        .map(|_| ())
        .map_err(|err| {
            RunnerError::task_invocation(format!(
                "database seed task `{DB_SEED_TASK}` failed: {err}"
            ))
        })
    })
}

fn prepare_db_seed_runtime(
    repo_root: &Path,
    manifest: &effigy_manifest::TaskManifest,
) -> Result<(), RunnerError> {
    let Some(task) = manifest.tasks.get(DB_SEED_TASK) else {
        return Ok(());
    };
    let binding_resolution = resolve_execution_binding_resolution(
        manifest
            .task_defaults
            .as_ref()
            .and_then(|defaults| defaults.run_in),
        manifest.systems.as_ref(),
        manifest.containers.as_ref(),
        DB_SEED_TASK,
        task,
        "database seed",
    )?;
    let Some(policy) = binding_resolution.effective_policy(repo_root)? else {
        return Ok(());
    };
    let session_context = data_seed_runtime_session_context();
    let plan = db_seed_runtime_activation_plan(
        repo_root,
        policy.name.as_str(),
        binding_resolution
            .binding()
            .container_name()
            .map(str::to_owned),
        session_context,
    );
    activate_container_runtime_for_task(
        repo_root,
        &policy,
        ActivationRequest {
            container_name: plan.request.container_name.as_deref(),
            repo_override: plan.request.repo_override.clone(),
            route: plan.route,
            session_context,
            // An environment with no shared services keeps its ordinary
            // activation, so a stopped local stack still starts before the seed.
            reconcile_shared_services: policy.shared_services.is_empty(),
        },
    )?;
    Ok(())
}

fn prepare_builtin_db_seed_runtime(
    repo_root: &Path,
    policy: &effigy_containers::EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    let session_context = data_seed_runtime_session_context();
    activate_container_runtime_for_task(
        repo_root,
        policy,
        ActivationRequest {
            container_name: Some(policy.name.as_str()),
            repo_override: Some(repo_root.to_path_buf()),
            route: RuntimeActivationRoute::DataSeed,
            session_context,
            // Only an environment that has shared services must leave them
            // alone; one with none keeps its prior bring-up behaviour.
            reconcile_shared_services: policy.shared_services.is_empty(),
        },
    )?;
    Ok(())
}

fn db_seed_runtime_activation_plan(
    repo_root: &Path,
    policy_name: &str,
    container_name: Option<String>,
    session_context: RuntimeSessionContext,
) -> RuntimeActivationPlan {
    build_runtime_activation_plan(
        repo_root,
        policy_name,
        container_name.as_deref(),
        Some(repo_root.to_path_buf()),
        RuntimeActivationRoute::DataSeed,
        session_context,
    )
}

fn resolve_db_seed_targets(
    repo_root: &Path,
    manifest: &effigy_manifest::TaskManifest,
    db_seeds: &[BootstrapDbSeedInput],
) -> Result<Vec<BootstrapDbSeedInput>, RunnerError> {
    let declared_targets = logical_database_targets(manifest);
    let selected = select_data_targets(
        &declared_targets,
        &db_seeds
            .iter()
            .map(|seed| seed.target.clone())
            .collect::<Vec<_>>(),
    )
    .map_err(|error| db_seed_target_selection_error(repo_root, db_seeds, error))?;

    Ok(db_seeds
        .iter()
        .zip(selected)
        .map(|(seed, target)| BootstrapDbSeedInput {
            target: target.map(|target| target.to_string()),
            path: seed.path.clone(),
        })
        .collect())
}

fn bootstrap_db_seed_plans(
    repo_root: &Path,
    manifest: &effigy_manifest::TaskManifest,
    db_seeds: &[BootstrapDbSeedInput],
) -> Result<Vec<DataSeedPlan>, RunnerError> {
    let resolved_seeds = resolve_db_seed_targets(repo_root, manifest, db_seeds)?;
    let declared_targets = logical_database_targets(manifest);

    Ok(resolved_seeds
        .into_iter()
        .map(|seed| {
            let mut input = DataSeedInput::new(DataSeedSource::from_raw_path(seed.path));
            if let Some(target) = seed.target {
                input = input.target(DataTargetRef::from(target));
            }
            let mut plan = DataSeedPlan::new(input);
            if let Some(target) = plan.input.target.as_ref() {
                if let Some(declared) = declared_targets
                    .iter()
                    .find(|declared| declared.name.as_str() == target.as_str())
                {
                    plan = plan.resolved_target(declared.clone());
                }
            }
            plan
        })
        .collect())
}

fn data_seed_source_display(source: &DataSeedSource) -> String {
    match source {
        DataSeedSource::Local(path) => path.display().to_string(),
        DataSeedSource::Oci(reference) => reference.clone(),
    }
}

fn db_seed_target_selection_error(
    repo_root: &Path,
    db_seeds: &[BootstrapDbSeedInput],
    error: DataTargetSelectionError,
) -> RunnerError {
    match error {
        DataTargetSelectionError::UnknownTarget {
            target,
            valid_targets,
            ..
        } => RunnerError::task_invocation(format!(
            "db seed target `{target}` is not declared in `[bundle].databases` or `[data.targets]` for {}; valid targets: {}",
            repo_root.join(TASK_MANIFEST_FILE).display(),
            valid_targets.join(", ")
        )),
        DataTargetSelectionError::MissingTarget {
            index,
            valid_targets,
        } => RunnerError::task_invocation(format!(
            "db seed input `{}` must name a target because multiple database targets are declared in `[bundle].databases` and `[data.targets]`: {}",
            db_seeds[index].path.display(),
            valid_targets.join(", ")
        )),
        DataTargetSelectionError::DuplicateTarget { target, .. } => {
            RunnerError::task_invocation(format!("duplicate db seed target `{target}`"))
        }
    }
}

pub(in crate::runner) fn bundle_database_targets(
    bundle: &effigy_manifest::ManifestBundleConfig,
) -> Option<Vec<String>> {
    let value = bundle
        .inputs
        .get("databases")
        .or_else(|| bundle.inputs.get("database"))?;

    match value {
        toml::Value::Array(values) => {
            let targets = values
                .iter()
                .filter_map(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>();
            if targets.is_empty() {
                None
            } else {
                Some(targets)
            }
        }
        toml::Value::String(value) => {
            let value = value.trim();
            if value.is_empty() {
                None
            } else {
                Some(vec![value.to_owned()])
            }
        }
        _ => None,
    }
}

pub(in crate::runner) fn logical_database_targets(
    manifest: &effigy_manifest::TaskManifest,
) -> Vec<ResolvedDataTarget> {
    let mut input = DataTargetManifestInput::new();
    if let Some(bundle) = manifest.bundle.as_ref() {
        if let Some(bundle_targets) = bundle_database_targets(bundle) {
            input = input.bundle_databases(bundle_targets);
        }
    }
    if let Some(data) = manifest.data.as_ref() {
        input = input.data_targets(
            data.targets
                .iter()
                .map(|(name, target)| {
                    DataTargetManifestEntry::new(
                        name.clone(),
                        target.service.clone(),
                        target.database.clone(),
                    )
                })
                .collect(),
        );
    }
    collect_manifest_data_targets(&input)
}

/// One built-in seed entry: the staged file, the manifest target it names, and
/// the route the selected environment resolves for it.
#[derive(Debug, Clone)]
pub(in crate::runner) struct BuiltinDbSeedEntry {
    pub target: String,
    pub staged_path: String,
    pub declared: ResolvedDataTarget,
    pub service: String,
    pub kind: DatabaseServiceKind,
    pub password: String,
    pub shared: Option<effigy_containers::SharedServiceBinding>,
}

/// Resolve the built-in seed's staged entries against the selected
/// environment's own service declarations.
///
/// The public data command runs this for its selected `--name` policy before
/// any activation, and the built-in task runs it again to execute, so neither
/// path reloads the manifest's default environment.
pub(in crate::runner) fn resolve_builtin_db_seed_entries(
    repo_root: &Path,
    manifest: &effigy_manifest::TaskManifest,
    policy: &effigy_containers::EffectiveContainerPolicy,
    env_overrides: &BTreeMap<String, String>,
) -> Result<Vec<BuiltinDbSeedEntry>, RunnerError> {
    let metadata_json = env_overrides
        .get(DB_SEEDS_JSON_ENV)
        .or_else(|| env_overrides.get(LEGACY_BOOTSTRAP_DB_SEEDS_JSON_ENV))
        .ok_or_else(|| {
            RunnerError::task_invocation(
                "database seed metadata is missing from the seed environment",
            )
        })?;
    let seed_specs =
        serde_json::from_str::<Vec<SeedMetadataEntry>>(metadata_json).map_err(|error| {
            RunnerError::task_invocation(format!("invalid database seed metadata: {error}"))
        })?;
    let declared_targets = logical_database_targets(manifest);
    let containers = manifest.containers.as_ref().ok_or_else(|| {
        RunnerError::task_invocation("manifest does not define a `[containers]` registry")
    })?;
    let container = containers.environments.get(&policy.name).ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "container `{}` is not defined in `{}`",
            policy.name,
            repo_root.join(TASK_MANIFEST_FILE).display()
        ))
    })?;

    let mut entries = Vec::with_capacity(seed_specs.len());
    for seed in &seed_specs {
        let target = seed
            .target
            .clone()
            .or_else(|| match declared_targets.as_slice() {
                [declared] => Some(declared.name.to_string()),
                _ => None,
            });
        let target = target.ok_or_else(|| {
            RunnerError::task_invocation(format!(
                "database seed entry `{}` is missing a target and the manifest declares multiple database targets",
                seed.staged_path
            ))
        })?;
        let declared = declared_targets
            .iter()
            .find(|declared| declared.name.as_str() == target)
            .ok_or_else(|| {
                RunnerError::task_invocation(format!(
                    "database seed target `{target}` is not declared in `[bundle].databases` or `[data.targets]`"
                ))
            })?;
        let (service, catalog, password) = resolve_builtin_seed_service(container, declared)?;
        // A declared `shared = true` service belongs to its own machine-wide
        // Compose project, so the reset and import run there. An unshared
        // service keeps the checkout-local route byte for byte.
        let shared = resolve_shared_data_binding(policy, manifest, &service)?;
        entries.push(BuiltinDbSeedEntry {
            target,
            staged_path: seed.staged_path.clone(),
            declared: declared.clone(),
            service,
            kind: DatabaseServiceKind::from_catalog(catalog)
                .expect("unsupported seed catalog should be filtered before rendering"),
            password,
            shared,
        });
    }
    Ok(entries)
}

/// One shared binding per distinct shared identity among the entries.
pub(in crate::runner) fn shared_seed_bindings(
    entries: &[BuiltinDbSeedEntry],
) -> Vec<effigy_containers::SharedServiceBinding> {
    let mut bindings = Vec::new();
    for entry in entries {
        let Some(binding) = entry.shared.as_ref() else {
            continue;
        };
        if bindings
            .iter()
            .any(|seen: &effigy_containers::SharedServiceBinding| {
                seen.project_name == binding.project_name
                    && seen.compose_file == binding.compose_file
                    && seen.render_digest == binding.render_digest
            })
        {
            continue;
        }
        bindings.push(binding.clone());
    }
    bindings
}

/// The built-in seed with its shared-project exec injected, so the declared
/// shared route and the unshared checkout-local route are both provable
/// without a live runtime (effigy#477).
#[cfg(test)]
pub(in crate::runner) fn run_builtin_db_seed_task_with<SharedExec>(
    repo_root: &Path,
    manifest: &effigy_manifest::TaskManifest,
    policy: &effigy_containers::EffectiveContainerPolicy,
    env_overrides: &BTreeMap<String, String>,
    shared_exec: SharedExec,
) -> Result<(), RunnerError>
where
    SharedExec: Fn(
        &effigy_containers::EffectiveContainerPolicy,
        &effigy_containers::SharedServiceBinding,
        &effigy_data::DatabaseCommandPlan,
        Option<&Path>,
    ) -> Result<std::process::Output, RunnerError>,
{
    let entries = resolve_builtin_db_seed_entries(repo_root, manifest, policy, env_overrides)?;
    run_builtin_db_seed_entries_with(repo_root, policy, &entries, shared_exec)
}

pub(in crate::runner) fn run_builtin_db_seed_entries_with<SharedExec>(
    repo_root: &Path,
    policy: &effigy_containers::EffectiveContainerPolicy,
    entries: &[BuiltinDbSeedEntry],
    shared_exec: SharedExec,
) -> Result<(), RunnerError>
where
    SharedExec: Fn(
        &effigy_containers::EffectiveContainerPolicy,
        &effigy_containers::SharedServiceBinding,
        &effigy_data::DatabaseCommandPlan,
        Option<&Path>,
    ) -> Result<std::process::Output, RunnerError>,
{
    let container_name = policy.name.as_str();
    for entry in entries {
        let stdin_file = repo_root.join(&entry.staged_path);
        let seed_plan = DataSeedPlan::new(DataSeedInput::new(DataSeedSource::Local(
            stdin_file.clone(),
        )))
        .resolved_target(entry.declared.clone())
        .reset_command(database_seed_reset_command(
            &entry.service,
            entry.kind,
            &entry.password,
            &entry.declared.database,
        ))
        .command(
            database_seed_import_command(
                &entry.service,
                entry.kind,
                &entry.password,
                &entry.declared.database,
            )
            .stdin(stdin_file.clone()),
        );
        let reset_command = seed_plan
            .reset_command
            .as_ref()
            .expect("builtin seed plan should include reset command");
        let reset_output = match entry.shared.as_ref() {
            Some(binding) => shared_exec(policy, binding, reset_command, None)?,
            None => run_container_exec_capture_with_credentials(
                repo_root,
                Some(container_name),
                Some(&reset_command.service),
                &reset_command.argv,
                &reset_command.credential_env,
                None,
            )?,
        };
        if !reset_output.status.success() {
            let stderr = String::from_utf8_lossy(&reset_output.stderr)
                .trim()
                .to_owned();
            let stdout = String::from_utf8_lossy(&reset_output.stdout)
                .trim()
                .to_owned();
            return Err(RunnerError::task_invocation(format!(
                "[error] database reset failed\nstdout:\n{stdout}\n\nstderr:\n{stderr}"
            )));
        }
        let command = seed_plan
            .command
            .as_ref()
            .expect("builtin seed plan should include import command");
        let output = match entry.shared.as_ref() {
            Some(binding) => shared_exec(policy, binding, command, command.stdin.as_deref())?,
            None => run_container_exec_capture_with_credentials(
                repo_root,
                Some(container_name),
                Some(&command.service),
                &command.argv,
                &command.credential_env,
                command.stdin.as_deref(),
            )?,
        };
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            return Err(RunnerError::task_invocation(format!(
                "[error] SQL import failed\nstdout:\n{stdout}\n\nstderr:\n{stderr}"
            )));
        }
    }
    let staged = entries
        .iter()
        .map(|entry| SeedMetadataEntry {
            target: Some(entry.target.clone()),
            staged_path: entry.staged_path.clone(),
        })
        .collect::<Vec<_>>();
    cleanup_builtin_seed_staging(repo_root, &staged)
}

fn cleanup_builtin_seed_staging(
    repo_root: &Path,
    seed_specs: &[SeedMetadataEntry],
) -> Result<(), RunnerError> {
    let staging_dir = repo_root.join(DB_SEEDS_DIR);
    for seed in seed_specs {
        let staged_path = repo_root.join(&seed.staged_path);
        match std::fs::remove_file(&staged_path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(RunnerError::task_invocation(format!(
                    "database seed import succeeded but failed to remove staged seed {}: {error}",
                    staged_path.display()
                )));
            }
        }
    }
    for file_name in [
        DB_SEEDS_METADATA_FILE,
        LEGACY_BOOTSTRAP_DB_SEEDS_METADATA_FILE,
    ] {
        let metadata_path = staging_dir.join(file_name);
        match std::fs::remove_file(&metadata_path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(RunnerError::task_invocation(format!(
                    "database seed import succeeded but failed to remove staged seed metadata {}: {error}",
                    metadata_path.display()
                )));
            }
        }
    }
    Ok(())
}

fn resolve_builtin_seed_service(
    container: &effigy_manifest::ManifestContainerConfig,
    target: &ResolvedDataTarget,
) -> Result<(String, &'static str, String), RunnerError> {
    if let Some(service_name) = target.service.as_deref() {
        let service = container.services.get(service_name).ok_or_else(|| {
            RunnerError::task_invocation(format!(
                "database target `{}` references unknown service `{service_name}`",
                target.name.as_str()
            ))
        })?;
        if DatabaseServiceKind::from_catalog(&service.catalog).is_none() {
            return Err(RunnerError::task_invocation(format!(
                "database target `{}` uses unsupported service catalog `{}`",
                target.name.as_str(),
                service.catalog
            )));
        }
    }

    let services = collect_builtin_seed_services(&container.services);
    let service = select_database_service(&services, target.service.as_deref(), &target.database)
        .map_err(|error| db_seed_service_selection_error(target, error))?;
    Ok((
        service.name.clone(),
        service.kind.catalog(),
        service.password.clone(),
    ))
}

fn collect_builtin_seed_services(
    services: &BTreeMap<String, ManifestContainerServiceConfig>,
) -> Vec<DatabaseService> {
    collect_manifest_database_services(services)
}

fn db_seed_service_selection_error(
    target: &ResolvedDataTarget,
    error: DatabaseServiceSelectionError,
) -> RunnerError {
    match error {
        DatabaseServiceSelectionError::UnknownService { service } => {
            RunnerError::task_invocation(format!(
                "database target `{}` references unknown service `{service}`",
                target.name.as_str()
            ))
        }
        DatabaseServiceSelectionError::AmbiguousDeclaredDatabase { .. }
        | DatabaseServiceSelectionError::AmbiguousPrimaryDatabase { .. }
        | DatabaseServiceSelectionError::NoServiceForDatabase { .. } => {
            RunnerError::task_invocation(format!(
                "database target `{}` is ambiguous; expected exactly one matching database service for `{}`",
                target.name.as_str(), target.database
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        cleanup_builtin_seed_staging, db_seed_env, db_seed_runtime_activation_plan,
        encode_staged_seed_name, logical_database_targets, resolve_db_seed_input_paths,
        resolve_db_seed_targets, stage_db_seed_files, stage_db_seed_files_report_with_adapter,
        stage_db_seed_files_with_adapter, SeedMetadataEntry, DB_SEEDS_DIR, DB_SEEDS_DIR_ENV,
        DB_SEEDS_JSON_ENV, DB_SEEDS_METADATA_FILE, DB_SEED_COUNT_ENV, DB_SEED_FILES_ENV,
        DB_SEED_FILE_ENV, DB_SEED_TARGET_ENV, LEGACY_BOOTSTRAP_DB_SEEDS_METADATA_FILE,
        LEGACY_BOOTSTRAP_DB_SEED_FILE_ENV,
    };
    use effigy_cli::BootstrapDbSeedInput;
    use effigy_manifest::TaskManifest;
    use effigy_runtime_plan::{RuntimeActivationRoute, RuntimeLeasePolicy};
    use effigy_state::artifact::{
        OciArtifactAdapter, OciArtifactDescriptor, OciArtifactError, OciArtifactInspectRequest,
        OciArtifactPullReport, OciArtifactPullRequest, OciArtifactPushReport,
        OciArtifactPushRequest,
    };
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

    fn temp_repo(name: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "effigy-db-seed-{name}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).expect("create temp repo");
        fs::write(
            root.join("effigy.toml"),
            "[data.targets.app]\nservice = \"db\"\ndatabase = \"app\"\n",
        )
        .expect("write manifest");
        root
    }

    /// A fixture repo whose manifest declares no data targets, so seed inputs
    /// pass target resolution with any label (`DataTargetRef` stays opaque).
    fn temp_repo_without_targets(name: &str) -> PathBuf {
        let root = temp_repo(name);
        fs::write(root.join("effigy.toml"), "[tasks.setup]\nrun = \"true\"\n")
            .expect("write manifest");
        root
    }

    /// A sentinel file inside the fixture repo but outside the owned staging
    /// root: staging and cleanup must never touch it.
    const SENTINEL_BYTES: &[u8; 14] = b"sentinel stays";

    fn write_outside_sentinel(repo: &Path) -> PathBuf {
        let outside = repo.join("outside-staging");
        fs::create_dir_all(&outside).expect("create outside-staging");
        let sentinel = outside.join("keep.txt");
        fs::write(&sentinel, SENTINEL_BYTES).expect("write sentinel");
        sentinel
    }

    fn read_metadata_entries(repo: &Path, file_name: &str) -> Vec<serde_json::Value> {
        let raw = fs::read_to_string(repo.join(DB_SEEDS_DIR).join(file_name)).expect("metadata");
        serde_json::from_str(&raw).expect("parse metadata json")
    }

    fn staging_file_names(repo: &Path) -> Vec<String> {
        let mut names = fs::read_dir(repo.join(DB_SEEDS_DIR))
            .expect("read staging dir")
            .map(|entry| {
                entry
                    .expect("staging entry")
                    .file_name()
                    .to_string_lossy()
                    .to_string()
            })
            .collect::<Vec<_>>();
        names.sort();
        names
    }

    #[test]
    fn logical_database_targets_include_explicit_sidecar_targets() {
        let manifest: TaskManifest = toml::from_str(
            r#"
[bundle]
base = { type = "path", dir = "bundles/workspace-app" }
databases = ["acowtancy", "acowtancy_test"]

[data.targets.legacy_mysql]
service = "mysql"
database = "acowtancy"
"#,
        )
        .expect("manifest");

        let targets = logical_database_targets(&manifest);
        assert_eq!(targets.len(), 3);
        assert_eq!(targets[0].name.as_str(), "acowtancy");
        assert_eq!(targets[0].database, "acowtancy");
        assert_eq!(targets[1].name.as_str(), "acowtancy_test");
        assert_eq!(targets[2].name.as_str(), "legacy_mysql");
        assert_eq!(targets[2].service.as_deref(), Some("mysql"));
        assert_eq!(targets[2].database, "acowtancy");
    }

    #[test]
    fn db_seed_runtime_activation_plan_keeps_identity_and_lease_policy() {
        let repo_root = PathBuf::from("/tmp/repo");
        let plan = db_seed_runtime_activation_plan(
            &repo_root,
            "web",
            Some("db".to_owned()),
            super::data_seed_runtime_session_context(),
        );

        assert_eq!(plan.request.repo_root, repo_root);
        assert_eq!(plan.request.policy_name, "web");
        assert_eq!(plan.request.container_name.as_deref(), Some("db"));
        assert_eq!(plan.request.repo_override, Some(PathBuf::from("/tmp/repo")));
        assert_eq!(plan.route, RuntimeActivationRoute::DataSeed);
        assert_eq!(
            plan.request.lease_policy,
            RuntimeLeasePolicy::RefreshOnActivation
        );
    }

    #[test]
    fn resolve_db_seed_targets_accepts_explicit_sidecar_targets() {
        let manifest: TaskManifest = toml::from_str(
            r#"
[bundle]
base = { type = "path", dir = "bundles/workspace-app" }
databases = ["acowtancy", "acowtancy_test"]

[data.targets.legacy_mysql]
service = "mysql"
database = "acowtancy"
"#,
        )
        .expect("manifest");

        let resolved = resolve_db_seed_targets(
            PathBuf::from("/tmp/acowtancy").as_path(),
            &manifest,
            &[BootstrapDbSeedInput {
                target: Some("legacy_mysql".to_owned()),
                path: PathBuf::from("./legacy.sql"),
            }],
        )
        .expect("resolved");

        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].target.as_deref(), Some("legacy_mysql"));
    }

    #[test]
    fn stage_db_seed_files_writes_artifact_metadata_and_preserves_seed_path() {
        let repo = temp_repo("artifact-stage");
        let source = repo.join("latest.sql");
        fs::write(&source, b"select 1;").expect("write seed");

        let staged = stage_db_seed_files(
            &repo,
            &[BootstrapDbSeedInput {
                target: Some("app".to_owned()),
                path: source.clone(),
            }],
        )
        .expect("stage seeds");

        assert_eq!(staged.len(), 1);
        assert_eq!(staged[0].source_path, source);
        assert_eq!(
            staged[0].staged_path,
            repo.join(".effigy/local/db-seeds/app--latest.sql")
        );
        assert_eq!(
            fs::read(&staged[0].staged_path).expect("read staged seed"),
            b"select 1;"
        );

        let artifact_metadata_files = find_artifact_metadata_files(&repo);
        assert_eq!(artifact_metadata_files.len(), 1);
        let metadata =
            fs::read_to_string(&artifact_metadata_files[0]).expect("read artifact metadata");
        assert!(metadata.contains("\"schema\": \"effigy.artifact.v1\""));
        assert!(metadata.contains("\"kind\": \"sql-dump\""));
    }

    #[test]
    fn resolve_db_seed_input_paths_preserves_oci_refs() {
        let resolved = resolve_db_seed_input_paths(
            Path::new("/tmp/repo"),
            &[BootstrapDbSeedInput {
                target: Some("app".to_owned()),
                path: PathBuf::from("oci://ghcr.io/acowtancy/private:uat"),
            }],
        );

        assert_eq!(
            resolved[0].path,
            PathBuf::from("oci://ghcr.io/acowtancy/private:uat")
        );
    }

    #[test]
    fn stage_db_seed_files_accepts_oci_artifact_refs() {
        let repo = temp_repo("oci-stage");
        let adapter = FakeOciArtifactAdapter;

        let staged = stage_db_seed_files_with_adapter(
            &repo,
            &[BootstrapDbSeedInput {
                target: Some("app".to_owned()),
                path: PathBuf::from("oci://ghcr.io/acowtancy/private:uat"),
            }],
            &adapter,
        )
        .expect("stage seeds");

        assert_eq!(staged.len(), 1);
        assert_eq!(
            staged[0].source_path,
            PathBuf::from("oci://ghcr.io/acowtancy/private:uat")
        );
        assert_eq!(
            staged[0].staged_path,
            repo.join(".effigy/local/db-seeds/app--legacy.sql")
        );
        assert_eq!(
            fs::read(&staged[0].staged_path).expect("read staged seed"),
            b"select 1;"
        );

        let artifact_metadata_files = find_artifact_metadata_files(&repo);
        assert_eq!(artifact_metadata_files.len(), 1);
        let metadata =
            fs::read_to_string(&artifact_metadata_files[0]).expect("read artifact metadata");
        assert!(metadata.contains("\"source_type\": \"oci\""));
        assert!(metadata.contains("\"digest\": \"sha256:fakedigest\""));
    }

    #[test]
    fn cleanup_builtin_seed_staging_removes_staged_files_and_metadata() {
        let repo = temp_repo("cleanup-staged");
        let staging_dir = repo.join(DB_SEEDS_DIR);
        fs::create_dir_all(&staging_dir).expect("create staging dir");
        let staged_path = staging_dir.join("app--latest.sql");
        fs::write(&staged_path, b"select 1;").expect("write staged seed");
        for file_name in [
            DB_SEEDS_METADATA_FILE,
            LEGACY_BOOTSTRAP_DB_SEEDS_METADATA_FILE,
        ] {
            fs::write(staging_dir.join(file_name), "[]").expect("write metadata");
        }

        cleanup_builtin_seed_staging(
            &repo,
            &[SeedMetadataEntry {
                target: Some("app".to_owned()),
                staged_path: format!("{DB_SEEDS_DIR}/app--latest.sql"),
            }],
        )
        .expect("cleanup staged seeds");

        assert!(!staged_path.exists());
        assert!(!staging_dir.join(DB_SEEDS_METADATA_FILE).exists());
        assert!(!staging_dir
            .join(LEGACY_BOOTSTRAP_DB_SEEDS_METADATA_FILE)
            .exists());
        assert!(staging_dir.is_dir());
    }

    #[test]
    fn cleanup_builtin_seed_staging_tolerates_missing_files() {
        let repo = temp_repo("cleanup-missing");
        fs::create_dir_all(repo.join(DB_SEEDS_DIR)).expect("create staging dir");

        cleanup_builtin_seed_staging(
            &repo,
            &[SeedMetadataEntry {
                target: Some("app".to_owned()),
                staged_path: format!("{DB_SEEDS_DIR}/app--latest.sql"),
            }],
        )
        .expect("cleanup missing staged seeds");
    }

    /// An absolute target label must never redirect the copy: the payload is
    /// staged inside the owned root under the encoded label, and the probe
    /// directory outside staging keeps exactly its sentinel.
    #[test]
    fn db_seed_staging_absolute_target_stays_inside_owned_staging() {
        let repo = temp_repo_without_targets("absolute-target");
        fs::write(repo.join("dump.sql"), b"select 1;").expect("write source");
        let sentinel = write_outside_sentinel(&repo);
        let probe_dir = repo.join("outside-staging");

        let staged = stage_db_seed_files(
            &repo,
            &[BootstrapDbSeedInput {
                target: Some(probe_dir.display().to_string()),
                path: repo.join("dump.sql"),
            }],
        )
        .expect("stage seeds with an absolute target label");

        let staged_name = format!(
            "{}--dump.sql",
            encode_staged_seed_name(&probe_dir.display().to_string())
        );
        let staged_path = repo.join(DB_SEEDS_DIR).join(&staged_name);
        assert_eq!(staged[0].staged_path, staged_path);
        assert_eq!(fs::read(&staged_path).expect("staged bytes"), b"select 1;");
        assert_eq!(
            fs::read(&sentinel).expect("sentinel bytes"),
            SENTINEL_BYTES
        );
        // The probe directory holds only its sentinel; no
        // `<target>--dump.sql` stray landed beside it or in the repo root.
        assert_eq!(
            fs::read_dir(&probe_dir)
                .expect("read probe dir")
                .map(|entry| entry.expect("probe entry").path())
                .collect::<Vec<_>>(),
            vec![sentinel]
        );
        assert!(!repo.join("outside-staging--dump.sql").exists());
        // Metadata records the staged object under the owned root.
        let entries = read_metadata_entries(&repo, DB_SEEDS_METADATA_FILE);
        assert_eq!(
            entries[0]["staged_path"],
            format!("{DB_SEEDS_DIR}/{staged_name}")
        );
    }

    /// Parent-bearing labels keep exact arithmetic: every payload lands
    /// directly inside the staging root, no file appears at the raw join's
    /// destination (`.effigy/local/`, `.effigy/`, the repo root or outside
    /// the checkout), and the outside sentinel is untouched.
    #[test]
    fn db_seed_staging_parent_labels_stage_inside_staging() {
        let repo = temp_repo_without_targets("parent-labels");
        fs::write(repo.join("dump.sql"), b"select 1;").expect("write source");
        let sentinel = write_outside_sentinel(&repo);

        let labels = ["../one", "../../two", "../../../three", "../../../../four"];
        let staged = stage_db_seed_files(
            &repo,
            &labels
                .iter()
                .map(|label| BootstrapDbSeedInput {
                    target: Some((*label).to_owned()),
                    path: repo.join("dump.sql"),
                })
                .collect::<Vec<_>>(),
        )
        .expect("stage parent-labelled seeds");

        let staging = repo.join(DB_SEEDS_DIR);
        for (seed, label) in staged.iter().zip(&labels) {
            let expected = staging.join(format!("{}--dump.sql", encode_staged_seed_name(label)));
            assert_eq!(seed.staged_path, expected, "label {label}");
            assert_eq!(fs::read(&expected).expect("staged bytes"), b"select 1;");
        }
        assert!(!repo.join(".effigy/local/one--dump.sql").exists());
        assert!(!repo.join(".effigy/two--dump.sql").exists());
        assert!(!repo.join("three--dump.sql").exists());
        assert!(!repo
            .parent()
            .expect("fixture parent")
            .join("four--dump.sql")
            .exists());
        assert_eq!(
            fs::read(&sentinel).expect("sentinel bytes"),
            SENTINEL_BYTES
        );
    }

    /// Distinct labels stay distinct staged names: the encoding is injective
    /// (it encodes `%` itself), so `legacy/mysql` and `legacy%2Fmysql` cannot
    /// collide and punctuation-only labels stay single components.
    #[test]
    fn db_seed_staging_encoding_keeps_distinct_labels_distinct() {
        let repo = temp_repo("label-encoding");
        fs::write(
            repo.join("effigy.toml"),
            r#"
[data.targets."legacy/mysql"]
service = "db"
database = "app"

[data.targets."legacy%2Fmysql"]
service = "db"
database = "app"

[data.targets.".."]
service = "db"
database = "app"

[data.targets."app v2"]
service = "db"
database = "app"
"#,
        )
        .expect("write manifest");
        for name in ["one", "two", "three", "four"] {
            fs::write(repo.join(format!("{name}.sql")), format!("{name};")).expect("write source");
        }

        let inputs = [
            ("legacy/mysql", "one.sql"),
            ("legacy%2Fmysql", "two.sql"),
            ("..", "three.sql"),
            ("app v2", "four.sql"),
        ];
        let staged = stage_db_seed_files(
            &repo,
            &inputs
                .iter()
                .map(|(target, path)| BootstrapDbSeedInput {
                    target: Some((*target).to_owned()),
                    path: repo.join(path),
                })
                .collect::<Vec<_>>(),
        )
        .expect("stage distinct labels");

        let expected_names = [
            "legacy%2Fmysql--one.sql",
            "legacy%252Fmysql--two.sql",
            "..--three.sql",
            "app%20v2--four.sql",
        ];
        let staging = repo.join(DB_SEEDS_DIR);
        for (seed, expected) in staged.iter().zip(&expected_names) {
            let staged_path = staging.join(expected);
            assert_eq!(seed.staged_path, staged_path, "name {expected}");
            assert!(staged_path.is_file(), "staged payload {expected}");
        }
    }

    /// Returned paths, both metadata formats, the environment handoff and
    /// cleanup all name the same staged object; cleanup removes exactly that
    /// object and the metadata, and the sentinel outside staging survives.
    #[test]
    fn db_seed_staging_env_metadata_and_cleanup_name_one_object() {
        let repo = temp_repo("env-metadata-cleanup");
        fs::write(repo.join("dump.sql"), b"select 1;").expect("write source");
        let sentinel = write_outside_sentinel(&repo);

        let staged = stage_db_seed_files(
            &repo,
            &[BootstrapDbSeedInput {
                target: Some("app".to_owned()),
                path: repo.join("dump.sql"),
            }],
        )
        .expect("stage seeds");

        let staged_path = repo.join(DB_SEEDS_DIR).join("app--dump.sql");
        assert_eq!(staged[0].staged_path, staged_path);
        assert_eq!(fs::read(&staged_path).expect("staged bytes"), b"select 1;");

        let recorded = format!("{DB_SEEDS_DIR}/app--dump.sql");
        for file_name in [
            DB_SEEDS_METADATA_FILE,
            LEGACY_BOOTSTRAP_DB_SEEDS_METADATA_FILE,
        ] {
            let entries = read_metadata_entries(&repo, file_name);
            assert_eq!(entries[0]["target"], "app", "{file_name}");
            assert_eq!(entries[0]["staged_path"], recorded, "{file_name}");
            assert_eq!(
                fs::read(repo.join(&recorded)).expect("recorded bytes"),
                b"select 1;"
            );
        }

        let env = db_seed_env(&staged);
        assert_eq!(env.get(DB_SEEDS_DIR_ENV).unwrap(), DB_SEEDS_DIR);
        assert_eq!(env.get(DB_SEED_COUNT_ENV).unwrap(), "1");
        assert_eq!(env.get(DB_SEED_FILE_ENV).unwrap(), recorded.as_str());
        assert_eq!(env.get(DB_SEED_FILES_ENV).unwrap(), recorded.as_str());
        assert_eq!(env.get(DB_SEED_TARGET_ENV).unwrap(), "app");
        assert_eq!(
            env.get(LEGACY_BOOTSTRAP_DB_SEED_FILE_ENV).unwrap(),
            recorded.as_str()
        );
        let env_entries: Vec<serde_json::Value> =
            serde_json::from_str(env.get(DB_SEEDS_JSON_ENV).unwrap()).expect("env metadata json");
        assert_eq!(env_entries[0]["staged_path"], recorded);

        let metadata: Vec<SeedMetadataEntry> = serde_json::from_str(
            &fs::read_to_string(repo.join(DB_SEEDS_DIR).join(DB_SEEDS_METADATA_FILE))
                .expect("read metadata"),
        )
        .expect("parse metadata entries");
        cleanup_builtin_seed_staging(&repo, &metadata).expect("cleanup staged seeds");

        assert!(!staged_path.exists());
        assert!(!repo
            .join(DB_SEEDS_DIR)
            .join(DB_SEEDS_METADATA_FILE)
            .exists());
        assert!(!repo
            .join(DB_SEEDS_DIR)
            .join(LEGACY_BOOTSTRAP_DB_SEEDS_METADATA_FILE)
            .exists());
        assert_eq!(
            fs::read(&sentinel).expect("sentinel bytes"),
            SENTINEL_BYTES
        );
    }

    /// A pre-existing destination inside staging is replaced in place, and
    /// nothing else in the staging root changes.
    #[test]
    fn db_seed_staging_existing_destination_is_replaced_inside_staging() {
        let repo = temp_repo("existing-destination");
        fs::write(repo.join("dump.sql"), b"select 1;").expect("write source");
        let staging = repo.join(DB_SEEDS_DIR);
        fs::create_dir_all(&staging).expect("create staging dir");
        fs::write(staging.join("app--dump.sql"), b"OLD STAGED BYTES")
            .expect("write old staged payload");
        fs::write(staging.join("unrelated.sql"), b"UNRELATED").expect("write unrelated file");

        let staged = stage_db_seed_files(
            &repo,
            &[BootstrapDbSeedInput {
                target: Some("app".to_owned()),
                path: repo.join("dump.sql"),
            }],
        )
        .expect("stage over an existing destination");

        assert_eq!(
            fs::read(&staged[0].staged_path).expect("staged bytes"),
            b"select 1;"
        );
        assert_eq!(
            fs::read(staging.join("unrelated.sql")).expect("unrelated bytes"),
            b"UNRELATED"
        );
        assert_eq!(
            staging_file_names(&repo),
            vec![
                "_effigy-bootstrap-db-seeds.json".to_owned(),
                "_effigy-db-seeds.json".to_owned(),
                "app--dump.sql".to_owned(),
                "unrelated.sql".to_owned(),
            ]
        );
    }

    /// A colliding staged name fails before the second copy and before any
    /// metadata is written, so cleanup never claims a payload it did not
    /// stage.
    #[test]
    fn db_seed_staging_duplicate_staged_names_fail_before_copy() {
        let repo = temp_repo_without_targets("duplicate-names");
        fs::create_dir_all(repo.join("a")).expect("create a");
        fs::create_dir_all(repo.join("b")).expect("create b");
        fs::write(repo.join("a/dump.sql"), b"FIRST;").expect("write a");
        fs::write(repo.join("b/app--dump.sql"), b"SECOND;").expect("write b");

        let error = stage_db_seed_files(
            &repo,
            &[
                BootstrapDbSeedInput {
                    target: Some("app".to_owned()),
                    path: repo.join("a/dump.sql"),
                },
                BootstrapDbSeedInput {
                    target: None,
                    path: repo.join("b/app--dump.sql"),
                },
            ],
        )
        .expect_err("colliding staged names should fail");

        assert!(error
            .to_string()
            .contains("duplicate staged db seed file name `app--dump.sql`"));
        assert_eq!(
            fs::read(repo.join(DB_SEEDS_DIR).join("app--dump.sql")).expect("first payload"),
            b"FIRST;"
        );
        assert!(!repo
            .join(DB_SEEDS_DIR)
            .join(DB_SEEDS_METADATA_FILE)
            .exists());
        assert!(!repo
            .join(DB_SEEDS_DIR)
            .join(LEGACY_BOOTSTRAP_DB_SEEDS_METADATA_FILE)
            .exists());
    }

    struct FakeOciArtifactAdapter;

    impl OciArtifactAdapter for FakeOciArtifactAdapter {
        fn inspect(
            &self,
            request: &OciArtifactInspectRequest,
        ) -> Result<OciArtifactDescriptor, OciArtifactError> {
            Ok(OciArtifactDescriptor::new(&request.reference).with_digest("sha256:fakedigest"))
        }

        fn pull(
            &self,
            request: &OciArtifactPullRequest,
        ) -> Result<OciArtifactPullReport, OciArtifactError> {
            let pulled_root = request.destination_root.join("fake-pull");
            fs::create_dir_all(&pulled_root).expect("create pulled root");
            fs::write(pulled_root.join("legacy.sql"), b"select 1;").expect("write pulled file");
            Ok(OciArtifactPullReport {
                descriptor: self.inspect(&OciArtifactInspectRequest {
                    reference: request.reference.clone(),
                })?,
                pulled_root,
                primary_files: vec![PathBuf::from("legacy.sql")],
            })
        }

        fn push(
            &self,
            request: &OciArtifactPushRequest,
        ) -> Result<OciArtifactPushReport, OciArtifactError> {
            let descriptor =
                OciArtifactDescriptor::new(&request.reference).with_digest("sha256:pushdigest");
            Ok(OciArtifactPushReport {
                pushed_ref: request.reference.redacted(),
                digest: descriptor.digest.clone(),
                descriptor,
            })
        }
    }

    /// A pull adapter whose second pull refuses, to prove the failure path's
    /// metadata honesty.
    struct FailOnSecondPull {
        pulls: AtomicUsize,
    }

    impl OciArtifactAdapter for FailOnSecondPull {
        fn inspect(
            &self,
            request: &OciArtifactInspectRequest,
        ) -> Result<OciArtifactDescriptor, OciArtifactError> {
            Ok(OciArtifactDescriptor::new(&request.reference).with_digest("sha256:fakedigest"))
        }

        fn pull(
            &self,
            request: &OciArtifactPullRequest,
        ) -> Result<OciArtifactPullReport, OciArtifactError> {
            let index = self.pulls.fetch_add(1, Ordering::SeqCst);
            if index >= 1 {
                return Err(OciArtifactError::PullFailed {
                    reference: request.reference.redacted(),
                    message: "second pull refused".to_owned(),
                });
            }
            let pulled_root = request.destination_root.join("fake-pull");
            fs::create_dir_all(&pulled_root).expect("create pulled root");
            fs::write(pulled_root.join("legacy.sql"), b"select 1;").expect("write pulled file");
            Ok(OciArtifactPullReport {
                descriptor: self.inspect(&OciArtifactInspectRequest {
                    reference: request.reference.clone(),
                })?,
                pulled_root,
                primary_files: vec![PathBuf::from("legacy.sql")],
            })
        }

        fn push(
            &self,
            request: &OciArtifactPushRequest,
        ) -> Result<OciArtifactPushReport, OciArtifactError> {
            let descriptor =
                OciArtifactDescriptor::new(&request.reference).with_digest("sha256:pushdigest");
            Ok(OciArtifactPushReport {
                pushed_ref: request.reference.redacted(),
                digest: descriptor.digest.clone(),
                descriptor,
            })
        }
    }

    /// A mid-stage OCI failure leaves the first payload staged, claims it in
    /// no metadata file, and names the failing artifact in the error.
    #[test]
    fn db_seed_staging_oci_failure_leaves_no_metadata_claims() {
        let repo = temp_repo_without_targets("oci-failure");
        let adapter = FailOnSecondPull {
            pulls: AtomicUsize::new(0),
        };

        let error = stage_db_seed_files_report_with_adapter(
            &repo,
            &[
                BootstrapDbSeedInput {
                    target: Some("app".to_owned()),
                    path: PathBuf::from("oci://example/first:uat"),
                },
                BootstrapDbSeedInput {
                    target: Some("beta".to_owned()),
                    path: PathBuf::from("oci://example/second:uat"),
                },
            ],
            &adapter,
        )
        .expect_err("the second pull should fail");

        assert!(error.to_string().contains("second:uat"));
        assert_eq!(adapter.pulls.load(Ordering::SeqCst), 2);
        assert!(repo.join(DB_SEEDS_DIR).join("app--legacy.sql").is_file());
        assert!(!repo
            .join(DB_SEEDS_DIR)
            .join(DB_SEEDS_METADATA_FILE)
            .exists());
        assert!(!repo
            .join(DB_SEEDS_DIR)
            .join(LEGACY_BOOTSTRAP_DB_SEEDS_METADATA_FILE)
            .exists());
    }

    fn find_artifact_metadata_files(repo: &Path) -> Vec<PathBuf> {
        let root = repo.join(".effigy/local/artifacts");
        let mut found = Vec::new();
        let Ok(entries) = fs::read_dir(root) else {
            return found;
        };
        for entry in entries.flatten() {
            let metadata = entry.path().join("effigy-artifact.json");
            if metadata.is_file() {
                found.push(metadata);
            }
        }
        found.sort();
        found
    }
}
