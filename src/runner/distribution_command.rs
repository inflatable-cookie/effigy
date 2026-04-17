//! CLI command handler for `effigy distribution` subcommands.

use std::path::{Path, PathBuf};

use effigy_distribution::{
    allocate_distribution_temp_dir, check_glibc_floor_command, effective_brew_formula,
    effective_repo_url, first_publish_command, generate_closeout_command, load_distribution_policy,
    preflight_command, validate_artifacts_command, validate_metadata_command,
    write_summary_command, EffectiveDistributionPolicy,
};

use crate::runner::command_context::{current_working_dir, resolve_repo_root};
use crate::{DistributionArgs, DistributionSubcommand};

use super::error::RunnerError;

pub(super) fn run_distribution(args: DistributionArgs) -> Result<String, RunnerError> {
    let cwd = current_working_dir()?;
    let resolved = resolve_repo_root(cwd, args.repo_override.clone())?;
    let repo_root = resolved.resolved_root;
    let distribution_policy = load_distribution_policy(&repo_root)?;

    match args.subcommand {
        DistributionSubcommand::ValidateMetadata { tag } => run_validate_metadata(
            &repo_root,
            &distribution_policy,
            tag.as_deref(),
            args.output_json,
        ),
        DistributionSubcommand::CheckGlibcFloor {
            binary_path,
            max_glibc,
        } => run_check_glibc_floor(
            &resolve_repo_input(&repo_root, binary_path),
            &max_glibc,
            args.output_json,
        ),
        DistributionSubcommand::Preflight {
            tag,
            skip_docs,
            skip_smoke,
            output_path,
        } => run_preflight(
            &repo_root,
            &distribution_policy,
            tag.as_deref(),
            skip_docs,
            skip_smoke,
            output_path
                .as_ref()
                .map(|path| resolve_repo_input(&repo_root, path.clone())),
            args.output_json,
        ),
        DistributionSubcommand::FirstPublish {
            tag,
            crate_version,
            repo_url,
            brew_formula,
            skip_homebrew,
            artifacts_dir,
        } => run_first_publish(
            &repo_root,
            &distribution_policy,
            &tag,
            crate_version.as_deref(),
            &repo_url,
            &brew_formula,
            skip_homebrew,
            artifacts_dir
                .as_ref()
                .map(|path| resolve_repo_input(&repo_root, path.clone())),
            args.output_json,
        ),
        DistributionSubcommand::ValidateArtifacts {
            artifacts_dir,
            expect_homebrew,
        } => run_validate_artifacts(
            &repo_root,
            &distribution_policy,
            &resolve_repo_input(&repo_root, artifacts_dir),
            expect_homebrew,
            args.output_json,
        ),
        DistributionSubcommand::GenerateCloseout {
            tag,
            artifacts_dir,
            output_path,
            owner,
            expect_homebrew,
        } => run_generate_closeout(
            &repo_root,
            &distribution_policy,
            &tag,
            &resolve_repo_input(&repo_root, artifacts_dir),
            output_path
                .as_ref()
                .map(|path| resolve_repo_input(&repo_root, path.clone())),
            &owner,
            expect_homebrew,
            args.output_json,
        ),
        DistributionSubcommand::WriteSummary {
            tag,
            artifacts_dir,
            crate_version,
            repo_url,
            brew_formula,
            homebrew_executed,
            log_files,
        } => run_write_summary(
            &repo_root,
            &distribution_policy,
            &tag,
            &resolve_repo_input(&repo_root, artifacts_dir),
            crate_version.as_deref(),
            &effective_repo_url(&distribution_policy, &repo_url),
            &effective_brew_formula(&distribution_policy, &brew_formula),
            homebrew_executed,
            &log_files,
            args.output_json,
        ),
    }
}

fn run_preflight(
    repo_root: &Path,
    distribution_policy: &EffectiveDistributionPolicy,
    tag: Option<&str>,
    skip_docs: bool,
    skip_smoke: bool,
    output_path: Option<PathBuf>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let effigy_bin = std::env::current_exe().map_err(RunnerError::Cwd)?;
    preflight_command(
        repo_root,
        distribution_policy,
        tag,
        skip_docs,
        skip_smoke,
        output_path.as_deref(),
        &effigy_bin,
        output_json,
    )
    .map_err(Into::into)
}

fn run_check_glibc_floor(
    binary_path: &Path,
    max_glibc: &str,
    output_json: bool,
) -> Result<String, RunnerError> {
    check_glibc_floor_command(binary_path, max_glibc, output_json).map_err(Into::into)
}

fn run_first_publish(
    repo_root: &Path,
    distribution_policy: &EffectiveDistributionPolicy,
    tag: &str,
    crate_version: Option<&str>,
    repo_url: &str,
    brew_formula: &str,
    skip_homebrew: bool,
    artifacts_dir: Option<PathBuf>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let tag_re =
        regex::Regex::new(r"^v[0-9]+\.[0-9]+\.[0-9]+([-.][0-9A-Za-z.]+)?$").expect("tag regex");
    if !tag_re.is_match(tag) {
        return Err(RunnerError::task_invocation(format!(
            "tag must match vX.Y.Z format: {tag}"
        )));
    }

    let crate_version = crate_version.unwrap_or_else(|| tag.trim_start_matches('v'));
    let repo_url = effective_repo_url(distribution_policy, repo_url);
    let brew_formula = effective_brew_formula(distribution_policy, brew_formula);
    let (artifacts_dir, cleanup_artifacts_dir) = if let Some(path) = artifacts_dir {
        std::fs::create_dir_all(&path)
            .map_err(|err| RunnerError::task_invocation_failed_write(&path, err))?;
        (path, None)
    } else {
        let path = allocate_distribution_temp_dir("effigy-distribution-first-publish")?;
        std::fs::create_dir_all(&path)
            .map_err(|err| RunnerError::task_invocation_failed_write(&path, err))?;
        (path.clone(), Some(path))
    };
    let work_dir = allocate_distribution_temp_dir("effigy-distribution-first-publish-work")?;
    std::fs::create_dir_all(&work_dir)
        .map_err(|err| RunnerError::task_invocation_failed_write(&work_dir, err))?;
    let effigy_bin = std::env::current_exe().map_err(RunnerError::Cwd)?;

    let result = first_publish_command(
        repo_root,
        distribution_policy,
        tag,
        crate_version,
        &repo_url,
        &brew_formula,
        skip_homebrew,
        &artifacts_dir,
        &work_dir,
        &effigy_bin,
        output_json,
    )
    .map_err(Into::into);

    let _ = std::fs::remove_dir_all(&work_dir);
    if let Some(path) = cleanup_artifacts_dir {
        let _ = std::fs::remove_dir_all(path);
    }
    result
}

fn run_validate_metadata(
    repo_root: &Path,
    distribution_policy: &EffectiveDistributionPolicy,
    tag: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    validate_metadata_command(repo_root, distribution_policy, tag, output_json).map_err(Into::into)
}

fn run_validate_artifacts(
    repo_root: &Path,
    distribution_policy: &EffectiveDistributionPolicy,
    artifacts_dir: &Path,
    expect_homebrew: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    let _ = repo_root;
    validate_artifacts_command(
        distribution_policy,
        artifacts_dir,
        expect_homebrew,
        output_json,
    )
    .map_err(Into::into)
}

fn run_generate_closeout(
    repo_root: &Path,
    distribution_policy: &EffectiveDistributionPolicy,
    tag: &str,
    artifacts_dir: &Path,
    output_path: Option<PathBuf>,
    owner: &str,
    expect_homebrew: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    let output_path = output_path.map(|path| {
        path.strip_prefix(repo_root)
            .map(Path::to_path_buf)
            .unwrap_or(path)
    });
    let rendered = generate_closeout_command(
        distribution_policy,
        tag,
        artifacts_dir,
        output_path,
        owner,
        expect_homebrew,
        output_json,
    )?;

    if output_json {
        Ok(rendered)
    } else if let Some(path) = rendered.strip_prefix("[ok] wrote log: ") {
        Ok(format!(
            "[ok] wrote log: {}",
            repo_root.join(path).display()
        ))
    } else {
        Ok(rendered)
    }
}

fn run_write_summary(
    repo_root: &Path,
    distribution_policy: &EffectiveDistributionPolicy,
    tag: &str,
    artifacts_dir: &Path,
    crate_version: Option<&str>,
    repo_url: &str,
    brew_formula: &str,
    homebrew_executed: bool,
    log_files: &[String],
    output_json: bool,
) -> Result<String, RunnerError> {
    let _ = repo_root;
    write_summary_command(
        distribution_policy,
        tag,
        artifacts_dir,
        crate_version,
        repo_url,
        brew_formula,
        homebrew_executed,
        log_files,
        output_json,
    )
    .map_err(Into::into)
}

fn resolve_repo_input(repo_root: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        repo_root.join(path)
    }
}

#[cfg(test)]
mod tests;
