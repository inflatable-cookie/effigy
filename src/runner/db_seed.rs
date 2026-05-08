use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

use effigy_artifacts::{
    stage_local_artifact, stage_oci_artifact, ArtifactSourceRef, LocalArtifactRef,
    LocalArtifactStagingRequest, OciArtifactAdapter, OciArtifactPullRequest,
    OciArtifactStagingRequest,
};
use effigy_bootstrap::BootstrapStagedDbSeed;
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
use serde_json::json;

use crate::runner::artifact_transport::{infer_kind_from_primary_files, OrasCliArtifactAdapter};
use crate::runner::container_command::run_container_exec_capture_with_options;
use crate::runner::container_runtime_prep::{
    activate_container_runtime_for_task, build_runtime_activation_plan, ActivationRequest,
};
use crate::runner::execute::api::{
    resolve_execution_binding_resolution, run_manifest_task_with_surface_and_env,
};
use crate::runner::manifest::load_task_manifest;
use crate::runner::runtime_session_context::{with_runtime_session_context, RuntimeSessionContext};

use super::error::RunnerError;
use effigy_cli::TaskInvocation;

pub(in crate::runner) const DB_SEED_TASK: &str = "bootstrap:db-seed";
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
    RuntimeSessionContext::default()
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
) -> Result<Vec<BootstrapStagedDbSeed>, RunnerError> {
    let adapter = OrasCliArtifactAdapter::default();
    stage_db_seed_files_with_adapter(repo_root, db_seeds, &adapter)
}

fn stage_db_seed_files_with_adapter(
    repo_root: &Path,
    db_seeds: &[BootstrapDbSeedInput],
    adapter: &dyn OciArtifactAdapter,
) -> Result<Vec<BootstrapStagedDbSeed>, RunnerError> {
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
        let staged_name = match plan.input.target.as_ref() {
            Some(target) => format!("{target}--{base_name}"),
            None => base_name.clone(),
        };
        if !seen_names.insert(staged_name.clone()) {
            return Err(RunnerError::task_invocation(format!(
                "duplicate staged db seed file name `{staged_name}`; rename one input or use distinct seed targets"
            )));
        }
        let destination = staging_dir.join(&staged_name);
        std::fs::copy(staged_artifact_file, &destination).map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to stage db seed {source} -> {}: {error}",
                destination.display(),
            ))
        })?;
        staged.push(BootstrapStagedDbSeed {
            target: plan.input.target.as_ref().map(ToString::to_string),
            source_path: PathBuf::from(artifact_report.metadata.source),
            staged_path: destination,
        });
    }

    let metadata = serde_json::to_string(
        &staged
            .iter()
            .map(|seed| {
                json!({
                    "target": seed.target,
                    "source_path": seed.source_path.display().to_string(),
                    "staged_path": Path::new(DB_SEEDS_DIR)
                        .join(
                            seed.staged_path
                                .file_name()
                                .expect("staged seed file should have name"),
                        )
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
    Ok(staged)
}

fn stage_seed_artifact(
    repo_root: &Path,
    plan: &DataSeedPlan,
    adapter: &dyn OciArtifactAdapter,
) -> Result<effigy_artifacts::StagedArtifactReport, RunnerError> {
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

pub(in crate::runner) fn db_seed_env(
    staged_db_seeds: &[BootstrapStagedDbSeed],
) -> BTreeMap<String, String> {
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
                    "staged_path": seeds_dir
                        .join(
                            seed.staged_path
                                .file_name()
                                .expect("staged seed file should have name"),
                        )
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
            seeds_dir
                .join(
                    seed.staged_path
                        .file_name()
                        .expect("staged seed file should have name"),
                )
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
        let staged_file = seeds_dir
            .join(
                staged_db_seeds[0]
                    .staged_path
                    .file_name()
                    .expect("staged seed file should have name"),
            )
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
        return with_runtime_session_context(data_seed_runtime_session_context(), || {
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
        });
    }
    run_builtin_db_seed_task(repo_root, &manifest, env_overrides)
}

pub(in crate::runner) fn db_seed_task_requires_container_runtime(
    repo_root: &Path,
) -> Result<bool, RunnerError> {
    let manifest = load_task_manifest(&repo_root.join(TASK_MANIFEST_FILE))?;
    if let Some(task) = manifest.tasks.get(DB_SEED_TASK) {
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
        return Ok(binding_resolution.is_inline_container()
            || binding_resolution.effective_policy(repo_root)?.is_some());
    }
    Ok(manifest.containers.is_some())
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

fn run_builtin_db_seed_task(
    repo_root: &Path,
    manifest: &effigy_manifest::TaskManifest,
    env_overrides: &BTreeMap<String, String>,
) -> Result<(), RunnerError> {
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
    let container_name = containers.default.as_deref().ok_or_else(|| {
        RunnerError::task_invocation("manifest does not declare a default container")
    })?;
    let container = containers.environments.get(container_name).ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "default container `{container_name}` is not defined in `{}`",
            repo_root.join(TASK_MANIFEST_FILE).display()
        ))
    })?;

    for seed in seed_specs {
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
        let (service_name, catalog, password) = resolve_builtin_seed_service(container, declared)?;
        let stdin_file = repo_root.join(&seed.staged_path);
        let kind = DatabaseServiceKind::from_catalog(catalog)
            .expect("unsupported seed catalog should be filtered before rendering");
        let seed_plan = DataSeedPlan::new(DataSeedInput::new(DataSeedSource::Local(
            stdin_file.clone(),
        )))
        .resolved_target(declared.clone())
        .reset_command(database_seed_reset_command(
            &service_name,
            kind,
            &password,
            &declared.database,
        ))
        .command(
            database_seed_import_command(&service_name, kind, &password, &declared.database)
                .stdin(stdin_file.clone()),
        );
        let reset_command = seed_plan
            .reset_command
            .as_ref()
            .expect("builtin seed plan should include reset command");
        let reset_output = run_container_exec_capture_with_options(
            repo_root,
            Some(container_name),
            Some(&reset_command.service),
            &reset_command.argv,
            None,
        )?;
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
        let output = run_container_exec_capture_with_options(
            repo_root,
            Some(container_name),
            Some(&command.service),
            &command.argv,
            command.stdin.as_deref(),
        )?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            return Err(RunnerError::task_invocation(format!(
                "[error] SQL import failed\nstdout:\n{stdout}\n\nstderr:\n{stderr}"
            )));
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
        if manifest_database_service_kind(&service.catalog).is_none() {
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
    services
        .iter()
        .filter_map(|(service_name, service)| {
            let kind = manifest_database_service_kind(&service.catalog)?;
            Some(
                DatabaseService::new(service_name.clone(), kind)
                    .password(service_password(service))
                    .declared_databases(service_declared_databases(service))
                    .primary_database_opt(service_primary_database(service)),
            )
        })
        .collect()
}

fn manifest_database_service_kind(catalog: &str) -> Option<DatabaseServiceKind> {
    match catalog {
        "postgres" => Some(DatabaseServiceKind::Postgres),
        "mariadb" => Some(DatabaseServiceKind::MariaDb),
        _ => None,
    }
}

fn service_password(service: &ManifestContainerServiceConfig) -> String {
    service
        .params
        .get("password")
        .and_then(toml::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("secret")
        .to_owned()
}

fn service_declared_databases(service: &ManifestContainerServiceConfig) -> Vec<String> {
    service
        .params
        .get("databases")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(toml::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

fn service_primary_database(service: &ManifestContainerServiceConfig) -> Option<String> {
    service
        .params
        .get("database")
        .and_then(toml::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
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

fn bootstrap_env_override_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub(in crate::runner) struct ScopedDbSeedEnvOverride {
    _guard: MutexGuard<'static, ()>,
    original: Vec<(String, Option<OsString>)>,
}

impl ScopedDbSeedEnvOverride {
    pub(in crate::runner) fn set(entries: &BTreeMap<String, String>) -> Self {
        let guard = bootstrap_env_override_lock()
            .lock()
            .expect("db seed env override mutex should not be poisoned");
        let mut original = Vec::with_capacity(entries.len());
        for (key, value) in entries {
            original.push((key.clone(), std::env::var_os(key)));
            unsafe {
                std::env::set_var(key, value);
            }
        }
        Self {
            _guard: guard,
            original,
        }
    }
}

impl Drop for ScopedDbSeedEnvOverride {
    fn drop(&mut self) {
        for (key, value) in self.original.drain(..) {
            unsafe {
                match value {
                    Some(value) => std::env::set_var(&key, value),
                    None => std::env::remove_var(&key),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        db_seed_runtime_activation_plan, logical_database_targets, resolve_db_seed_input_paths,
        resolve_db_seed_targets, stage_db_seed_files, stage_db_seed_files_with_adapter,
    };
    use effigy_artifacts::{
        OciArtifactAdapter, OciArtifactDescriptor, OciArtifactError, OciArtifactInspectRequest,
        OciArtifactPullReport, OciArtifactPullRequest, OciArtifactPushReport,
        OciArtifactPushRequest,
    };
    use effigy_cli::BootstrapDbSeedInput;
    use effigy_manifest::TaskManifest;
    use effigy_runtime_plan::{RuntimeActivationRoute, RuntimeLeasePolicy};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

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

    #[test]
    fn logical_database_targets_include_explicit_sidecar_targets() {
        let manifest: TaskManifest = toml::from_str(
            r#"
[bundle]
base = "underlay"
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
base = "underlay"
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
