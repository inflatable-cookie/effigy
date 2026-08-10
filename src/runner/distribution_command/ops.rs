use std::path::{Path, PathBuf};

use effigy_distribution::{
    allocate_distribution_temp_dir, check_glibc_floor_command, first_publish_command,
    generate_closeout_command, preflight_command, validate_artifacts_command,
    validate_metadata_command, write_summary_command, DistributionPreflightRequest,
    DistributionSummaryRequest, EffectiveDistributionPolicy, FirstPublishRequest,
};

use crate::runner::RunnerError;

pub(crate) fn run_preflight(
    repo_root: &Path,
    distribution_policy: &EffectiveDistributionPolicy,
    tag: Option<&str>,
    skip_docs: bool,
    skip_smoke: bool,
    output_path: Option<PathBuf>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let effigy_bin = std::env::current_exe().map_err(RunnerError::Cwd)?;
    preflight_command(DistributionPreflightRequest {
        repo_root,
        distribution_policy,
        tag,
        skip_docs,
        skip_smoke,
        output_path: output_path.as_deref(),
        effigy_bin: &effigy_bin,
        output_json,
    })
    .map_err(Into::into)
}

pub(crate) fn run_check_glibc_floor(
    binary_path: &Path,
    max_glibc: &str,
    output_json: bool,
) -> Result<String, RunnerError> {
    check_glibc_floor_command(binary_path, max_glibc, output_json).map_err(Into::into)
}

pub(crate) struct RunFirstPublishRequest<'a> {
    pub(crate) repo_root: &'a Path,
    pub(crate) distribution_policy: &'a EffectiveDistributionPolicy,
    pub(crate) tag: &'a str,
    pub(crate) crate_version: Option<&'a str>,
    pub(crate) repo_url: &'a str,
    pub(crate) brew_formula: &'a str,
    pub(crate) skip_homebrew: bool,
    pub(crate) artifacts_dir: Option<PathBuf>,
    pub(crate) output_json: bool,
}

pub(crate) fn run_first_publish(
    request: RunFirstPublishRequest<'_>,
) -> Result<String, RunnerError> {
    let RunFirstPublishRequest {
        repo_root,
        distribution_policy,
        tag,
        crate_version,
        repo_url,
        brew_formula,
        skip_homebrew,
        artifacts_dir,
        output_json,
    } = request;
    let tag_re =
        regex::Regex::new(r"^v[0-9]+\.[0-9]+\.[0-9]+([-.][0-9A-Za-z.]+)?$").expect("tag regex");
    if !tag_re.is_match(tag) {
        return Err(RunnerError::task_invocation(format!(
            "tag must match vX.Y.Z format: {tag}"
        )));
    }

    let crate_version = crate_version.unwrap_or_else(|| tag.trim_start_matches('v'));
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

    let result = first_publish_command(FirstPublishRequest {
        repo_root,
        distribution_policy,
        tag,
        crate_version,
        repo_url,
        brew_formula,
        skip_homebrew,
        artifacts_dir: &artifacts_dir,
        work_dir: &work_dir,
        effigy_bin: &effigy_bin,
        output_json,
    })
    .map_err(Into::into);

    let _ = std::fs::remove_dir_all(&work_dir);
    if let Some(path) = cleanup_artifacts_dir {
        let _ = std::fs::remove_dir_all(path);
    }
    result
}

pub(crate) fn run_validate_metadata(
    repo_root: &Path,
    distribution_policy: &EffectiveDistributionPolicy,
    tag: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    validate_metadata_command(repo_root, distribution_policy, tag, output_json).map_err(Into::into)
}

pub(crate) fn run_validate_artifacts(
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

pub(crate) struct GenerateCloseoutRequest<'a> {
    pub(crate) repo_root: &'a Path,
    pub(crate) distribution_policy: &'a EffectiveDistributionPolicy,
    pub(crate) tag: &'a str,
    pub(crate) artifacts_dir: &'a Path,
    pub(crate) output_path: Option<PathBuf>,
    pub(crate) owner: &'a str,
    pub(crate) expect_homebrew: bool,
    pub(crate) output_json: bool,
}

pub(crate) fn run_generate_closeout(
    request: GenerateCloseoutRequest<'_>,
) -> Result<String, RunnerError> {
    let GenerateCloseoutRequest {
        repo_root,
        distribution_policy,
        tag,
        artifacts_dir,
        output_path,
        owner,
        expect_homebrew,
        output_json,
    } = request;
    let output_path = output_path.map(|path| {
        if path.is_absolute() {
            path
        } else {
            repo_root.join(path)
        }
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
        let rendered_path = PathBuf::from(path);
        let display_path = if rendered_path.is_absolute() {
            rendered_path
        } else {
            repo_root.join(rendered_path)
        };
        Ok(format!("[ok] wrote log: {}", display_path.display()))
    } else {
        Ok(rendered)
    }
}

pub(crate) fn run_write_summary(
    request: DistributionSummaryRequest<'_>,
) -> Result<String, RunnerError> {
    write_summary_command(request).map_err(Into::into)
}
