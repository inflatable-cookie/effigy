use std::cell::RefCell;
use std::ffi::OsString;
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

use effigy_bootstrap::{
    execute_bootstrap_request_with_progress as crate_execute_bootstrap,
    render_bootstrap_plan as crate_render_bootstrap_plan,
    render_bootstrap_result as crate_render_bootstrap_result,
    resolve_bootstrap_request as crate_resolve_bootstrap, BootstrapDbSeedInput, BootstrapError,
    BootstrapExecutionResult, BootstrapProgressEvent, BootstrapResolution, BootstrapStagedDbSeed,
};
use effigy_cli::{BootstrapArgs, BootstrapSubcommand, TaskInvocation};
use effigy_manifest::{ManifestManagedRun, TASK_MANIFEST_FILE};
use effigy_ui::theme::{is_ci_environment, resolve_color_enabled, Theme};
use effigy_ui::{style_text, OutputMode, PlainRenderer, Renderer, SpinnerHandle};

use crate::runner::container_runtime_prep::{
    activate_container_runtime_for_task, ActivationRequest, ExecutionSurfaceKind,
};
use crate::runner::embedded_runner::run_embedded_task;
use crate::runner::execute::api::{
    resolve_execution_binding_resolution, run_managed_run_with_cwd,
    run_manifest_task_with_cwd_and_env,
};
use crate::runner::manifest::load_task_manifest;
use crate::runner::runtime_session_context::{
    with_runtime_session_context, LeaseRefreshPolicy, PublicWorkspaceCleanupOverride,
    RuntimeSessionContext,
};
use effigy_builtin::{PromptDecision, PromptPolicy};

use super::error::RunnerError;

mod deps;

const BOOTSTRAP_DB_SEED_TASK: &str = "bootstrap:db-seed";
const BOOTSTRAP_DB_SEEDS_DIR: &str = ".effigy/local/db-seeds";
const BOOTSTRAP_DB_SEEDS_METADATA_FILE: &str = "_effigy-bootstrap-db-seeds.json";
const BOOTSTRAP_DB_SEEDS_DIR_ENV: &str = "EFFIGY_BOOTSTRAP_DB_SEEDS_DIR";
const BOOTSTRAP_DB_SEED_FILE_ENV: &str = "EFFIGY_BOOTSTRAP_DB_SEED_FILE";
const BOOTSTRAP_DB_SEED_COUNT_ENV: &str = "EFFIGY_BOOTSTRAP_DB_SEED_COUNT";
const BOOTSTRAP_DB_SEED_FILES_ENV: &str = "EFFIGY_BOOTSTRAP_DB_SEED_FILES";
const BOOTSTRAP_DB_SEEDS_JSON_ENV: &str = "EFFIGY_BOOTSTRAP_DB_SEEDS_JSON";
const BOOTSTRAP_DB_SEED_TARGET_ENV: &str = "EFFIGY_BOOTSTRAP_DB_SEED_TARGET";

pub(in crate::runner) fn run_bootstrap_with_cwd(
    args: BootstrapArgs,
    cwd: PathBuf,
) -> Result<String, RunnerError> {
    match &args.subcommand {
        BootstrapSubcommand::Clone {
            plan, no_prompt, ..
        } => {
            let request = resolve_bootstrap_request(&cwd, &args)?;
            if *plan {
                return Ok(crate_render_bootstrap_plan(&request, args.output_json));
            }

            maybe_confirm_bootstrap_path_reuse(&request.destination, args.output_json, *no_prompt)?;
            let result = execute_bootstrap_request(&request, args.output_json, *no_prompt)?;
            Ok(crate_render_bootstrap_result(&result, args.output_json))
        }
        BootstrapSubcommand::DepsSync { mode, paths } => {
            deps::run_bootstrap_deps_sync(&cwd, *mode, paths, args.output_json)
        }
    }
}

fn resolve_bootstrap_request(
    cwd: &Path,
    args: &BootstrapArgs,
) -> Result<BootstrapResolution, RunnerError> {
    let BootstrapSubcommand::Clone {
        repo_url,
        path,
        branch,
        db_seeds,
        no_prompt: _,
        start,
        ..
    } = &args.subcommand
    else {
        return Err(RunnerError::task_invocation(
            "bootstrap repo resolution requires the clone subcommand".to_owned(),
        ));
    };

    crate_resolve_bootstrap(
        cwd,
        repo_url,
        path.as_deref(),
        branch.as_deref(),
        &db_seeds
            .iter()
            .map(|seed| BootstrapDbSeedInput {
                target: seed.target.clone(),
                path: seed.path.clone(),
            })
            .collect::<Vec<_>>(),
        *start,
    )
    .map_err(map_bootstrap_error)
}

fn maybe_confirm_bootstrap_path_reuse(
    destination: &Path,
    output_json: bool,
    no_prompt: bool,
) -> Result<(), RunnerError> {
    if !is_existing_non_empty_dir(destination)? {
        return Ok(());
    }

    let policy = PromptPolicy {
        output_json,
        plan: false,
        explicit_non_interactive: no_prompt,
        stdin_is_tty: io::stdin().is_terminal(),
        stdout_is_tty: io::stdout().is_terminal(),
    };
    match policy.decide() {
        PromptDecision::Prompt => {
            let mut stdin = io::stdin().lock();
            let mut stdout = io::stdout().lock();
            confirm_bootstrap_path_reuse_from_io(destination, &mut stdin, &mut stdout)
        }
        PromptDecision::SuppressedByExplicitNonInteractive => Ok(()),
        PromptDecision::SuppressedByJson
        | PromptDecision::SuppressedByPlan
        | PromptDecision::SuppressedByNonTty => Err(RunnerError::task_invocation(format!(
            "bootstrap destination already exists and is non-empty: {}. Rerun from an interactive terminal to confirm reuse, pass --no-prompt to use the existing path non-interactively, or choose a different --path.",
            destination.display()
        ))),
    }
}

fn is_existing_non_empty_dir(path: &Path) -> Result<bool, RunnerError> {
    if !path.exists() || !path.is_dir() {
        return Ok(false);
    }
    let mut entries = std::fs::read_dir(path).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to inspect bootstrap destination {}: {error}",
            path.display()
        ))
    })?;
    Ok(entries
        .next()
        .transpose()
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to inspect bootstrap destination {}: {error}",
                path.display()
            ))
        })?
        .is_some())
}

fn confirm_bootstrap_path_reuse_from_io<R: BufRead, W: Write>(
    destination: &Path,
    input: &mut R,
    output: &mut W,
) -> Result<(), RunnerError> {
    writeln!(
        output,
        "Bootstrap destination already exists and is non-empty:\n{}\n",
        destination.display()
    )
    .and_then(|_| output.flush())
    .map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to render interactive bootstrap prompt: {error}"
        ))
    })?;
    if prompt_yes_no_with_default(
        input,
        output,
        "Reuse this destination and continue? [y/N]: ",
        false,
    )? {
        return Ok(());
    }
    Err(RunnerError::task_invocation(
        "bootstrap cancelled during destination reuse confirmation",
    ))
}

fn execute_bootstrap_request(
    request: &BootstrapResolution,
    output_json: bool,
    no_prompt: bool,
) -> Result<BootstrapExecutionResult, RunnerError> {
    let progress = RefCell::new(BootstrapProgressReporter::new(output_json));
    let mut staged_db_seeds = None::<Vec<BootstrapStagedDbSeed>>;
    let mut db_seed_env = None::<ScopedEnvOverride>;
    let mut effective_db_seeds = request.db_seeds.clone();
    let mut db_seed_prompt_checked = false;
    let mut crate_request = request.clone();
    // The runner owns start ordering so bootstrap-owned DB seed work,
    // whether explicit or collected interactively, always runs before
    // `[bootstrap].start`.
    crate_request.start_requested = false;

    let mut result = crate_execute_bootstrap(
        &crate_request,
        |manifest_path| {
            let manifest = load_task_manifest(manifest_path)
                .map_err(|e| BootstrapError::task_invocation(e.to_string()))?;
            Ok(manifest.bootstrap)
        },
        |repo_root, run, phase| {
            maybe_prompt_bootstrap_db_seed_inputs(
                repo_root,
                output_json,
                no_prompt,
                &mut effective_db_seeds,
                &mut db_seed_prompt_checked,
            )
            .map_err(|e| BootstrapError::task_invocation(e.to_string()))?;
            maybe_stage_bootstrap_db_seed_inputs(
                &effective_db_seeds,
                &crate_request.destination,
                repo_root,
                &mut staged_db_seeds,
                &mut db_seed_env,
                &mut progress.borrow_mut(),
            )
            .map_err(|e| BootstrapError::task_invocation(e.to_string()))?;
            run_bootstrap_run(repo_root, run, phase)
                .map_err(|e| BootstrapError::task_invocation(e.to_string()))
        },
        |repo_root, selector, phase| {
            run_bootstrap_task(repo_root, selector, phase)
                .map_err(|e| BootstrapError::task_invocation(e.to_string()))
        },
        |event| progress.borrow_mut().handle(event),
    )
    .map_err(map_bootstrap_error)?;

    let effective_destination = result.request.destination.clone();
    maybe_prompt_bootstrap_db_seed_inputs(
        &effective_destination,
        output_json,
        no_prompt,
        &mut effective_db_seeds,
        &mut db_seed_prompt_checked,
    )?;

    result.request.start_requested = request.start_requested;
    result.request.db_seeds = effective_db_seeds.clone();

    if !effective_db_seeds.is_empty() {
        maybe_stage_bootstrap_db_seed_inputs(
            &effective_db_seeds,
            &effective_destination,
            &effective_destination,
            &mut staged_db_seeds,
            &mut db_seed_env,
            &mut progress.borrow_mut(),
        )?;
        result.staged_db_seeds = staged_db_seeds.clone().unwrap_or_default();

        progress
            .borrow_mut()
            .start_command_phase("[bootstrap] running database seed task");
        let db_seed_env_entries = bootstrap_db_seed_env(&result.staged_db_seeds);
        run_bootstrap_seed_task(&effective_destination, &db_seed_env_entries)?;
        progress.borrow_mut().finish_success(&format!(
            "[ok] database seed task complete ({BOOTSTRAP_DB_SEED_TASK})"
        ));
        result.db_seed_task = Some(BOOTSTRAP_DB_SEED_TASK.to_owned());
    }

    if request.start_requested && !result.start_ran {
        if result.start_tasks.is_empty() {
            return Err(RunnerError::task_invocation(
                "bootstrap start was requested but `[bootstrap].start` is not configured",
            ));
        }
        for selector in &result.start_tasks {
            progress
                .borrow_mut()
                .handle(BootstrapProgressEvent::StartTaskStarted {
                    destination: effective_destination.clone(),
                    selector: selector.clone(),
                });
            run_bootstrap_task(&effective_destination, selector, "bootstrap start")?;
            progress
                .borrow_mut()
                .handle(BootstrapProgressEvent::StartTaskFinished {
                    destination: effective_destination.clone(),
                    selector: selector.clone(),
                });
        }
        result.start_ran = true;
    }

    Ok(result)
}

fn maybe_stage_bootstrap_db_seed_inputs(
    db_seeds: &[BootstrapDbSeedInput],
    destination_root: &Path,
    repo_root: &Path,
    staged_db_seeds: &mut Option<Vec<BootstrapStagedDbSeed>>,
    db_seed_env: &mut Option<ScopedEnvOverride>,
    progress: &mut BootstrapProgressReporter,
) -> Result<(), RunnerError> {
    if db_seeds.is_empty() || repo_root != destination_root {
        return Ok(());
    }
    if staged_db_seeds.is_some() {
        return Ok(());
    }

    progress.start_command_phase("[bootstrap] staging database seed files");
    let manifest = load_task_manifest(&repo_root.join(TASK_MANIFEST_FILE))?;
    let resolved_seeds = resolve_bootstrap_db_seed_targets(repo_root, &manifest, db_seeds)?;
    let staged = stage_bootstrap_db_seed_files(repo_root, &resolved_seeds)?;
    *db_seed_env = Some(ScopedEnvOverride::set(&bootstrap_db_seed_env(&staged)));
    progress.finish_success(&format!(
        "[ok] staged database seed files ({})",
        staged
            .iter()
            .map(|seed| match seed.target.as_deref() {
                Some(target) => format!("{target}={}", seed.staged_path.display()),
                None => seed.staged_path.display().to_string(),
            })
            .collect::<Vec<_>>()
            .join(", ")
    ));
    *staged_db_seeds = Some(staged);
    Ok(())
}

fn maybe_prompt_bootstrap_db_seed_inputs(
    repo_root: &Path,
    output_json: bool,
    no_prompt: bool,
    effective_db_seeds: &mut Vec<BootstrapDbSeedInput>,
    prompt_checked: &mut bool,
) -> Result<(), RunnerError> {
    if *prompt_checked
        || !effective_db_seeds.is_empty()
        || !should_prompt_bootstrap_db_seeds(output_json, no_prompt)
    {
        return Ok(());
    }

    let manifest = load_task_manifest(&repo_root.join(TASK_MANIFEST_FILE))?;
    let Some(targets) = manifest.bundle.as_ref().and_then(bundle_database_targets) else {
        *prompt_checked = true;
        return Ok(());
    };
    if targets.is_empty() {
        *prompt_checked = true;
        return Ok(());
    }

    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    *effective_db_seeds =
        collect_bootstrap_db_seed_prompts_from_io(repo_root, &targets, &mut stdin, &mut stdout)?;
    *prompt_checked = true;
    Ok(())
}

fn should_prompt_bootstrap_db_seeds(output_json: bool, no_prompt: bool) -> bool {
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

pub(super) fn collect_bootstrap_db_seed_prompts_from_io<R: BufRead, W: Write>(
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

fn stage_bootstrap_db_seed_files(
    repo_root: &Path,
    db_seeds: &[BootstrapDbSeedInput],
) -> Result<Vec<BootstrapStagedDbSeed>, RunnerError> {
    let staging_dir = repo_root.join(BOOTSTRAP_DB_SEEDS_DIR);
    std::fs::create_dir_all(&staging_dir).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to create bootstrap db seed directory {}: {error}",
            staging_dir.display()
        ))
    })?;

    let mut seen_names = std::collections::BTreeSet::new();
    let mut staged = Vec::with_capacity(db_seeds.len());
    for seed in db_seeds {
        let source = &seed.path;
        if !source.is_file() {
            return Err(RunnerError::task_invocation(format!(
                "bootstrap db seed is not a readable file: {}",
                source.display()
            )));
        }
        let Some(file_name) = source.file_name() else {
            return Err(RunnerError::task_invocation(format!(
                "bootstrap db seed path has no file name: {}",
                source.display()
            )));
        };
        let base_name = file_name.to_string_lossy().to_string();
        let staged_name = match seed.target.as_deref() {
            Some(target) => format!("{target}--{base_name}"),
            None => base_name.clone(),
        };
        if !seen_names.insert(staged_name.clone()) {
            return Err(RunnerError::task_invocation(format!(
                "duplicate staged bootstrap db seed file name `{staged_name}`; rename one input or use distinct seed targets"
            )));
        }
        let destination = staging_dir.join(&staged_name);
        std::fs::copy(source, &destination).map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to stage bootstrap db seed {} -> {}: {error}",
                source.display(),
                destination.display()
            ))
        })?;
        staged.push(BootstrapStagedDbSeed {
            target: seed.target.clone(),
            source_path: source.clone(),
            staged_path: destination,
        });
    }
    std::fs::write(
        staging_dir.join(BOOTSTRAP_DB_SEEDS_METADATA_FILE),
        serde_json::to_string(
            &staged
                .iter()
                .map(|seed| {
                    serde_json::json!({
                        "target": seed.target,
                        "source_path": seed.source_path.display().to_string(),
                        "staged_path": Path::new(BOOTSTRAP_DB_SEEDS_DIR)
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
        .expect("bootstrap db seed metadata should serialize"),
    )
    .map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to write bootstrap db seed metadata file {}: {error}",
            staging_dir.join(BOOTSTRAP_DB_SEEDS_METADATA_FILE).display()
        ))
    })?;
    Ok(staged)
}

fn bootstrap_db_seed_env(
    staged_db_seeds: &[BootstrapStagedDbSeed],
) -> std::collections::BTreeMap<String, String> {
    let mut env = std::collections::BTreeMap::new();
    if staged_db_seeds.is_empty() {
        return env;
    }
    let seeds_dir = Path::new(BOOTSTRAP_DB_SEEDS_DIR);
    env.insert(
        BOOTSTRAP_DB_SEEDS_DIR_ENV.to_owned(),
        seeds_dir.display().to_string(),
    );
    env.insert(
        BOOTSTRAP_DB_SEED_COUNT_ENV.to_owned(),
        staged_db_seeds.len().to_string(),
    );
    if staged_db_seeds.len() == 1 {
        env.insert(
            BOOTSTRAP_DB_SEED_FILE_ENV.to_owned(),
            seeds_dir
                .join(
                    staged_db_seeds[0]
                        .staged_path
                        .file_name()
                        .expect("staged seed file should have name"),
                )
                .display()
                .to_string(),
        );
        if let Some(target) = staged_db_seeds[0].target.as_deref() {
            env.insert(BOOTSTRAP_DB_SEED_TARGET_ENV.to_owned(), target.to_owned());
        }
    }
    env.insert(
        BOOTSTRAP_DB_SEED_FILES_ENV.to_owned(),
        staged_db_seeds
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
            .collect::<Vec<_>>()
            .join("\n"),
    );
    env.insert(
        BOOTSTRAP_DB_SEEDS_JSON_ENV.to_owned(),
        serde_json::to_string(
            &staged_db_seeds
                .iter()
                .map(|seed| {
                    serde_json::json!({
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
        .expect("bootstrap db seed env json should serialize"),
    );
    env
}

fn resolve_bootstrap_db_seed_targets(
    repo_root: &Path,
    manifest: &effigy_manifest::TaskManifest,
    db_seeds: &[BootstrapDbSeedInput],
) -> Result<Vec<BootstrapDbSeedInput>, RunnerError> {
    let declared_targets = manifest.bundle.as_ref().and_then(bundle_database_targets);

    let mut seen_targets = std::collections::BTreeSet::new();
    let mut resolved = Vec::with_capacity(db_seeds.len());
    for seed in db_seeds {
        let effective_target = match seed.target.as_deref() {
            Some(target) => {
                if let Some(declared_targets) = declared_targets.as_ref() {
                    if !declared_targets.iter().any(|declared| declared == target) {
                        return Err(RunnerError::task_invocation(format!(
                            "bootstrap db seed target `{target}` is not declared in `[bundle].databases` for {}; valid targets: {}",
                            repo_root.join(TASK_MANIFEST_FILE).display(),
                            declared_targets.join(", ")
                        )));
                    }
                }
                Some(target.to_owned())
            }
            None => match declared_targets.as_ref() {
                Some(declared_targets) if declared_targets.len() == 1 => {
                    Some(declared_targets[0].clone())
                }
                Some(declared_targets) if declared_targets.len() > 1 => {
                    return Err(RunnerError::task_invocation(format!(
                        "bootstrap db seed input `{}` must name a target because `[bundle].databases` declares multiple databases: {}",
                        seed.path.display(),
                        declared_targets.join(", ")
                    )));
                }
                _ => None,
            },
        };
        if let Some(target) = effective_target.as_deref() {
            if !seen_targets.insert(target.to_owned()) {
                return Err(RunnerError::task_invocation(format!(
                    "duplicate bootstrap db seed target `{target}`"
                )));
            }
        }
        resolved.push(BootstrapDbSeedInput {
            target: effective_target,
            path: seed.path.clone(),
        });
    }
    Ok(resolved)
}

fn bundle_database_targets(bundle: &effigy_manifest::ManifestBundleConfig) -> Option<Vec<String>> {
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

fn run_bootstrap_seed_task(
    repo_root: &Path,
    env_overrides: &std::collections::BTreeMap<String, String>,
) -> Result<(), RunnerError> {
    let manifest = load_task_manifest(&repo_root.join(TASK_MANIFEST_FILE))?;
    if !manifest.tasks.contains_key(BOOTSTRAP_DB_SEED_TASK) {
        return Err(RunnerError::task_invocation(format!(
            "bootstrap received database seed input but {} does not define task `{BOOTSTRAP_DB_SEED_TASK}`",
            repo_root.join(TASK_MANIFEST_FILE).display()
        )));
    }
    prepare_bootstrap_seed_runtime(repo_root, &manifest)?;
    with_runtime_session_context(
        bootstrap_runtime_session_context("bootstrap db seed"),
        || {
            run_manifest_task_with_cwd_and_env(
                &TaskInvocation {
                    name: BOOTSTRAP_DB_SEED_TASK.to_owned(),
                    args: Vec::new(),
                },
                repo_root.to_path_buf(),
                env_overrides,
            )
            .map(|_| ())
            .map_err(|err| {
                RunnerError::task_invocation(format!(
                    "bootstrap db seed task `{BOOTSTRAP_DB_SEED_TASK}` failed: {err}"
                ))
            })
        },
    )
}

fn prepare_bootstrap_seed_runtime(
    repo_root: &Path,
    manifest: &effigy_manifest::TaskManifest,
) -> Result<(), RunnerError> {
    let Some(task) = manifest.tasks.get(BOOTSTRAP_DB_SEED_TASK) else {
        return Ok(());
    };
    let binding_resolution = resolve_execution_binding_resolution(
        manifest
            .task_defaults
            .as_ref()
            .and_then(|defaults| defaults.run_in),
        manifest.systems.as_ref(),
        manifest.containers.as_ref(),
        BOOTSTRAP_DB_SEED_TASK,
        task,
        "bootstrap db seed",
    )?;
    let Some(policy) = binding_resolution.effective_policy(repo_root)? else {
        return Ok(());
    };
    activate_container_runtime_for_task(
        repo_root,
        &policy,
        ActivationRequest {
            surface: ExecutionSurfaceKind::StandardTask,
            container_name: binding_resolution.binding().container_name(),
            repo_override: Some(repo_root.to_path_buf()),
            session_context: bootstrap_runtime_session_context("bootstrap db seed"),
        },
    )?;
    Ok(())
}

struct ScopedEnvOverride {
    _guard: MutexGuard<'static, ()>,
    original: Vec<(String, Option<OsString>)>,
}

impl ScopedEnvOverride {
    fn set(entries: &std::collections::BTreeMap<String, String>) -> Self {
        let guard = bootstrap_env_override_lock()
            .lock()
            .expect("bootstrap env override mutex should not be poisoned");
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

impl Drop for ScopedEnvOverride {
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

fn bootstrap_env_override_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

struct BootstrapProgressReporter {
    spinner: Option<Box<dyn SpinnerHandle>>,
    enabled: bool,
    color_enabled: bool,
    emitted_output: bool,
}

impl BootstrapProgressReporter {
    fn new(output_json: bool) -> Self {
        let stderr_is_tty = std::io::stderr().is_terminal();
        let enabled = !output_json && stderr_is_tty && !is_ci_environment();
        let color_enabled =
            !output_json && resolve_color_enabled(OutputMode::from_env(), stderr_is_tty);
        Self {
            spinner: None,
            enabled,
            color_enabled,
            emitted_output: false,
        }
    }

    fn handle(&mut self, event: BootstrapProgressEvent) {
        match event {
            BootstrapProgressEvent::RootCheckoutStarted {
                repo_url,
                destination,
            } => {
                self.start(&format!(
                    "[bootstrap] pulling {} -> {}",
                    repo_url,
                    destination.display()
                ));
            }
            BootstrapProgressEvent::RootCheckoutFinished {
                repo_state,
                destination,
            } => {
                self.finish_success(&format!(
                    "[ok] root repo {repo_state}: {}",
                    destination.display()
                ));
            }
            BootstrapProgressEvent::SubmodulesStarted {
                destination,
                policy,
            } => {
                self.start(&format!(
                    "[bootstrap] submodules {} ({})",
                    destination.display(),
                    effigy_bootstrap::submodule_policy_label(policy)
                ));
            }
            BootstrapProgressEvent::SubmodulesFinished {
                destination,
                policy,
                applied,
            } => {
                let suffix = if applied { "applied" } else { "skipped" };
                self.finish_success(&format!(
                    "[ok] submodules {} {} ({})",
                    suffix,
                    destination.display(),
                    effigy_bootstrap::submodule_policy_label(policy)
                ));
            }
            BootstrapProgressEvent::ChildCheckoutStarted {
                path, destination, ..
            } => {
                self.start(&format!(
                    "[bootstrap] pulling child {} -> {}",
                    path,
                    destination.display()
                ));
            }
            BootstrapProgressEvent::ChildCheckoutFinished {
                path, repo_state, ..
            } => {
                self.finish_success(&format!("[ok] child {path} {repo_state}"));
            }
            BootstrapProgressEvent::ChildCheckoutWarning { path, warning, .. } => {
                self.finish_error(&format!("[warn] child {path} skipped: {warning}"));
            }
            BootstrapProgressEvent::ChildRunStarted { path, .. } => {
                self.start_command_phase(&format!("[bootstrap] running child setup for {path}"));
            }
            BootstrapProgressEvent::ChildRunFinished { path, run, .. } => {
                self.finish_success(&format!("[ok] child {path} setup complete ({run})"));
            }
            BootstrapProgressEvent::RootRunStarted { .. } => {
                self.start_command_phase("[bootstrap] running root setup");
            }
            BootstrapProgressEvent::RootRunFinished { run, .. } => {
                self.finish_success(&format!("[ok] root setup complete ({run})"));
            }
            BootstrapProgressEvent::StartTaskStarted { selector, .. } => {
                self.start_command_phase(&format!("[bootstrap] starting {selector}"));
            }
            BootstrapProgressEvent::StartTaskFinished { selector, .. } => {
                self.finish_success(&format!("[ok] start task complete ({selector})"));
            }
        }
    }

    fn start(&mut self, label: &str) {
        self.finish_clear();
        if self.enabled {
            let mut renderer = PlainRenderer::stderr(OutputMode::from_env());
            self.spinner = renderer.spinner(label).ok();
        } else {
            self.print_line(label);
        }
    }

    fn start_command_phase(&mut self, label: &str) {
        self.finish_clear();
        self.print_group_break();
        self.print_line(label);
    }

    fn finish_success(&mut self, message: &str) {
        self.finish_clear();
        self.print_line(message);
    }

    fn finish_error(&mut self, message: &str) {
        self.finish_clear();
        self.print_line(message);
    }

    fn finish_clear(&mut self) {
        if let Some(spinner) = self.spinner.take() {
            spinner.finish_clear();
        }
    }

    fn print_group_break(&mut self) {
        if self.emitted_output {
            eprintln!();
        }
    }

    fn print_line(&mut self, message: &str) {
        eprintln!(
            "{}",
            render_bootstrap_progress_message(message, self.color_enabled)
        );
        self.emitted_output = true;
    }
}

fn render_bootstrap_progress_message(message: &str, color_enabled: bool) -> String {
    message
        .split_inclusive('\n')
        .map(|line| render_bootstrap_progress_line(line, color_enabled))
        .collect()
}

fn render_bootstrap_progress_line(line: &str, color_enabled: bool) -> String {
    const STATUS_PREFIXES: [(&str, fn(&Theme) -> anstyle::Style); 6] = [
        ("[ok]", |theme| theme.success),
        ("[warn]", |theme| theme.warning),
        ("[info]", |theme| theme.label),
        ("[next]", |theme| theme.accent),
        ("[gateway]", |theme| theme.label),
        ("[bootstrap]", |theme| theme.label),
    ];

    for (prefix, style) in STATUS_PREFIXES {
        if let Some(rest) = line.strip_prefix(prefix) {
            return format!(
                "{}{}",
                style_text(color_enabled, style(&Theme::default()), prefix),
                rest
            );
        }
    }

    line.to_owned()
}

fn run_bootstrap_run(
    repo_root: &Path,
    run: &ManifestManagedRun,
    phase: &str,
) -> Result<(), RunnerError> {
    with_runtime_session_context(bootstrap_runtime_session_context(phase), || {
        run_managed_run_with_cwd(run, repo_root.to_path_buf(), "bootstrap")
            .map(|_| ())
            .map_err(|err| RunnerError::task_invocation(format!("{phase} failed: {err}")))
    })
}

fn run_bootstrap_task(repo_root: &Path, selector: &str, phase: &str) -> Result<(), RunnerError> {
    with_runtime_session_context(bootstrap_runtime_session_context(phase), || {
        run_embedded_task(
            &TaskInvocation {
                name: selector.to_owned(),
                args: Vec::new(),
            },
            repo_root,
        )
        .map(|_| ())
        .map_err(|err| {
            RunnerError::task_invocation(format!("{phase} task `{selector}` failed: {err}"))
        })
    })
}

pub(in crate::runner) fn bootstrap_runtime_session_context(phase: &str) -> RuntimeSessionContext {
    RuntimeSessionContext {
        lease_refresh_policy: LeaseRefreshPolicy::SkipRefresh,
        public_workspace_cleanup: if phase == "bootstrap start" {
            PublicWorkspaceCleanupOverride::ForceStopOnExit
        } else {
            PublicWorkspaceCleanupOverride::Default
        },
    }
}

fn map_bootstrap_error(error: BootstrapError) -> RunnerError {
    match error {
        BootstrapError::TaskInvocation(message) => RunnerError::task_invocation(message),
        BootstrapError::Read { path, error } => {
            RunnerError::task_invocation_failed_read(&path, error)
        }
        BootstrapError::Write { path, error } => {
            RunnerError::task_invocation_failed_write(&path, error)
        }
    }
}

#[cfg(test)]
mod tests;
