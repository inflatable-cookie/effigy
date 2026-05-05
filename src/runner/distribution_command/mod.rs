//! CLI command handler for `effigy distribution` subcommands.

use std::path::{Path, PathBuf};

use effigy_distribution::{effective_brew_formula, effective_repo_url, load_distribution_policy};

use crate::runner::command_context::resolve_active_repo_root;
use effigy_cli::{DistributionArgs, DistributionSubcommand};

use super::error::RunnerError;

mod ops;

pub(super) fn run_distribution(args: DistributionArgs) -> Result<String, RunnerError> {
    let resolved = resolve_active_repo_root(args.repo_override.clone())?;
    let repo_root = resolved.resolved_root;
    let distribution_policy = load_distribution_policy(&repo_root)?;

    match args.subcommand {
        DistributionSubcommand::ValidateMetadata { tag } => ops::run_validate_metadata(
            &repo_root,
            &distribution_policy,
            tag.as_deref(),
            args.output_json,
        ),
        DistributionSubcommand::CheckGlibcFloor {
            binary_path,
            max_glibc,
        } => ops::run_check_glibc_floor(
            &resolve_repo_input(&repo_root, binary_path),
            &max_glibc,
            args.output_json,
        ),
        DistributionSubcommand::Preflight {
            tag,
            skip_docs,
            skip_smoke,
            output_path,
        } => ops::run_preflight(
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
        } => ops::run_first_publish(
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
        } => ops::run_validate_artifacts(
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
        } => ops::run_generate_closeout(
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
        } => ops::run_write_summary(
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

fn resolve_repo_input(repo_root: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        repo_root.join(path)
    }
}

#[cfg(test)]
mod tests;
