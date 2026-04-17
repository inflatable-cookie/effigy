//! CLI command handler for `effigy distribution` subcommands.

use std::path::{Path, PathBuf};
use std::process::Command;

use effigy_distribution::{
    allocate_distribution_temp_dir, command_exists, effective_brew_formula, effective_repo_url,
    first_publish_command, generate_closeout_command, load_distribution_policy,
    validate_artifacts_command, write_summary_command, DistributionExecutionError,
    DistributionPolicyError, EffectiveDistributionPolicy,
};
use regex::Regex;
use serde_json::json;
use toml::Value as TomlValue;

use crate::runner::command_context::{current_working_dir, resolve_repo_root};
use crate::{DistributionArgs, DistributionSubcommand};

use super::error::RunnerError;

pub(super) fn run_distribution(args: DistributionArgs) -> Result<String, RunnerError> {
    let cwd = current_working_dir()?;
    let resolved = resolve_repo_root(cwd, args.repo_override.clone())?;
    let repo_root = resolved.resolved_root;
    let distribution_policy =
        load_distribution_policy(&repo_root).map_err(map_distribution_policy_error)?;

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
    let mut docs_status = "skipped";
    let mut smoke_status = "skipped";

    if !skip_docs {
        run_effigy_task(repo_root, &distribution_policy.docs_task)?;
        docs_status = "ok";
    }

    let _ = run_validate_metadata(repo_root, distribution_policy, tag, false)?;
    let metadata_status = "ok";

    if !skip_smoke {
        run_effigy_task(repo_root, &distribution_policy.smoke_task)?;
        smoke_status = "ok";
    }

    if let Some(output_path) = output_path.as_ref() {
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| RunnerError::task_invocation_failed_write(parent, err))?;
        }
        let rendered = format!(
            "TAG={}\nDOCS_STATUS={docs_status}\nMETADATA_STATUS={metadata_status}\nSMOKE_STATUS={smoke_status}\n",
            tag.unwrap_or("")
        );
        std::fs::write(output_path, rendered)
            .map_err(|err| RunnerError::task_invocation_failed_write(output_path, err))?;
    }

    let next_command = if let Some(tag) = tag {
        format!(
            "effigy distribution first-publish --tag {tag} --artifacts-dir ./artifacts/distribution-{tag}"
        )
    } else {
        "effigy distribution first-publish --tag vX.Y.Z --artifacts-dir ./artifacts/distribution-vX.Y.Z".to_owned()
    };

    let payload = json!({
        "schema": "effigy.distribution.preflight.v1",
        "schema_version": 1,
        "ok": true,
        "tag": tag,
        "docs_status": docs_status,
        "metadata_status": metadata_status,
        "smoke_status": smoke_status,
        "output": output_path.as_ref().map(|path| path.display().to_string()),
        "next_command": next_command,
    });
    if output_json {
        return Ok(payload.to_string());
    }

    let mut lines = Vec::new();
    if let Some(output_path) = output_path.as_ref() {
        lines.push(format!(
            "[ok] wrote preflight summary: {}",
            output_path.display()
        ));
    }
    lines.push("[ok] distribution preflight checks passed".to_owned());
    lines.push(format!("[next] real publish-cycle command: {next_command}"));
    Ok(lines.join("\n"))
}

fn run_check_glibc_floor(
    binary_path: &Path,
    max_glibc: &str,
    output_json: bool,
) -> Result<String, RunnerError> {
    if !binary_path.is_file() {
        return Err(RunnerError::task_invocation(format!(
            "binary not found: {}",
            binary_path.display()
        )));
    }

    let versions = collect_glibc_versions(binary_path)?;
    let (ok, required_glibc) = if let Some(required) = versions.last() {
        let compatible = compare_glibc_versions(required, max_glibc)
            .is_some_and(|ordering| ordering != std::cmp::Ordering::Greater);
        (compatible, Some(required.clone()))
    } else {
        (true, None)
    };

    let payload = json!({
        "schema": "effigy.distribution.glibc-floor.v1",
        "schema_version": 1,
        "ok": ok,
        "binary": binary_path.display().to_string(),
        "required_glibc": required_glibc,
        "max_glibc": max_glibc,
        "dynamic_symbols_found": required_glibc.is_some(),
    });
    if output_json {
        return if ok {
            Ok(payload.to_string())
        } else {
            Err(RunnerError::task_invocation(payload.to_string()))
        };
    }

    if let Some(required) = required_glibc {
        if ok {
            Ok(format!(
                "[ok] {} GLIBC floor is compatible (requires GLIBC_{required}, max GLIBC_{max_glibc})",
                binary_path.display()
            ))
        } else {
            Err(RunnerError::task_invocation(format!(
                "{} requires GLIBC_{required} but the release floor is GLIBC_{max_glibc}",
                binary_path.display()
            )))
        }
    } else {
        Ok(format!(
            "[ok] no dynamic GLIBC symbol requirements found: {}",
            binary_path.display()
        ))
    }
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
    let tag_re = Regex::new(r"^v[0-9]+\.[0-9]+\.[0-9]+([-.][0-9A-Za-z.]+)?$").expect("tag regex");
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
        let path = allocate_distribution_temp_dir("effigy-distribution-first-publish")
            .map_err(map_distribution_execution_error)?;
        std::fs::create_dir_all(&path)
            .map_err(|err| RunnerError::task_invocation_failed_write(&path, err))?;
        (path.clone(), Some(path))
    };
    let work_dir = allocate_distribution_temp_dir("effigy-distribution-first-publish-work")
        .map_err(map_distribution_execution_error)?;
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
    .map_err(map_distribution_execution_error);

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
    let cargo = std::fs::read_to_string(repo_root.join("Cargo.toml")).map_err(|err| {
        RunnerError::task_invocation_failed_read(&repo_root.join("Cargo.toml"), err)
    })?;
    let cargo: TomlValue = cargo.parse().map_err(|err| {
        RunnerError::task_invocation_failed_parse(&repo_root.join("Cargo.toml"), err)
    })?;
    let package = cargo.get("package").and_then(TomlValue::as_table);
    let workspace_package = cargo
        .get("workspace")
        .and_then(TomlValue::as_table)
        .and_then(|workspace| workspace.get("package"))
        .and_then(TomlValue::as_table);
    let package_metadata = package.or(workspace_package).ok_or_else(|| {
        RunnerError::task_invocation(
            "Cargo.toml is missing [package] or [workspace.package] metadata",
        )
    })?;

    let name = package_metadata
        .get("name")
        .and_then(TomlValue::as_str)
        .unwrap_or(&distribution_policy.package_name)
        .to_owned();
    let version = package_metadata
        .get("version")
        .and_then(TomlValue::as_str)
        .unwrap_or_default()
        .to_owned();
    let license = package_metadata
        .get("license")
        .and_then(TomlValue::as_str)
        .unwrap_or_default()
        .to_owned();
    let description = package_metadata
        .get("description")
        .and_then(TomlValue::as_str)
        .unwrap_or_default()
        .to_owned();

    let semver_re =
        Regex::new(r"^[0-9]+\.[0-9]+\.[0-9]+([-.][0-9A-Za-z.]+)?$").expect("semver regex");
    let tag_re = Regex::new(r"^v[0-9]+\.[0-9]+\.[0-9]+([-.][0-9A-Za-z.]+)?$").expect("tag regex");

    let required_docs = &distribution_policy.required_docs;
    let required_files = &distribution_policy.required_files;
    let mut errors = Vec::new();
    if name != distribution_policy.package_name {
        errors.push(format!(
            "expected package name `{}`, got `{name}`",
            distribution_policy.package_name
        ));
    }
    if !semver_re.is_match(&version) {
        errors.push(format!("package version is not semver-like: `{version}`"));
    }
    if !distribution_policy.manifest_adopted && license.is_empty() {
        errors.push("package license is empty".to_owned());
    }
    if !distribution_policy.manifest_adopted && package.is_some() && description.is_empty() {
        errors.push("package description is empty".to_owned());
    }
    if let Some(tag) = tag {
        if !tag_re.is_match(tag) {
            errors.push(format!("tag must match vX.Y.Z format: `{tag}`"));
        } else if tag.trim_start_matches('v') != version {
            errors.push(format!(
                "tag version `{}` does not match Cargo version `{version}`",
                tag.trim_start_matches('v')
            ));
        }
    }
    for path in required_docs.iter().chain(required_files.iter()) {
        if !repo_root.join(path).is_file() {
            errors.push(format!("required file is missing: {path}"));
        }
    }
    if !distribution_policy.manifest_adopted {
        let workflow_path = repo_root.join(".github/workflows/release-binaries.yml");
        let workflow = std::fs::read_to_string(&workflow_path)
            .map_err(|err| RunnerError::task_invocation_failed_read(&workflow_path, err))?;
        for (needle, description) in [
            ("name: Release Binaries", "release workflow name"),
            ("Create GitHub Release", "GitHub Release job wiring"),
            ("Update Homebrew tap", "Homebrew automation job wiring"),
            ("      - \"v*\"", "tag trigger wiring"),
            (
                "          - target: x86_64-unknown-linux-gnu\n            os: ubuntu-22.04",
                "x86_64 Linux release baseline pinning",
            ),
            (
                "          - target: aarch64-unknown-linux-gnu\n            os: ubuntu-22.04",
                "aarch64 Linux release baseline pinning",
            ),
            (
                "./scripts/check-linux-glibc-floor.sh ./effigy-${{ matrix.target }} 2.35",
                "Linux glibc compatibility guard",
            ),
        ] {
            if !workflow.contains(needle) {
                errors.push(format!(
                    "expected {description} in .github/workflows/release-binaries.yml"
                ));
            }
        }
    }

    let payload = json!({
        "schema": "effigy.distribution.metadata.v1",
        "schema_version": 1,
        "ok": errors.is_empty(),
        "package": {
            "name": name,
            "version": version,
            "license": license,
            "description": description,
        },
        "tag": tag,
        "required_docs": required_docs,
        "required_files": required_files,
        "errors": errors,
    });

    if output_json {
        return if payload["ok"] == true {
            Ok(payload.to_string())
        } else {
            Err(RunnerError::task_invocation(payload.to_string()))
        };
    }
    if payload["ok"] == true {
        return Ok("[ok] distribution metadata checks passed".to_owned());
    }
    Err(RunnerError::task_invocation(
        payload["errors"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|value| value.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
    ))
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
    .map_err(map_distribution_execution_error)
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
    )
    .map_err(map_distribution_execution_error)?;

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
    .map_err(map_distribution_execution_error)
}

fn collect_glibc_versions(binary_path: &Path) -> Result<Vec<String>, RunnerError> {
    let candidates = [
        (
            "readelf",
            vec![
                "--version-info".to_owned(),
                binary_path.display().to_string(),
            ],
        ),
        (
            "objdump",
            vec!["-T".to_owned(), binary_path.display().to_string()],
        ),
        ("strings", vec![binary_path.display().to_string()]),
    ];
    let glibc_re = Regex::new(r"GLIBC_([0-9]+\.[0-9]+)").expect("glibc regex");

    let mut captured = Vec::new();
    for (program, args) in candidates {
        if !command_exists(program) {
            continue;
        }
        let output = Command::new(program)
            .args(&args)
            .output()
            .map_err(|err| RunnerError::task_invocation(err.to_string()))?;
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        for capture in glibc_re.captures_iter(&combined) {
            captured.push(capture[1].to_owned());
        }
        if !captured.is_empty() {
            break;
        }
    }
    captured.sort_by(|left, right| {
        compare_glibc_versions(left, right).unwrap_or(std::cmp::Ordering::Equal)
    });
    captured.dedup();
    Ok(captured)
}

fn compare_glibc_versions(left: &str, right: &str) -> Option<std::cmp::Ordering> {
    let parse = |value: &str| -> Option<(u32, u32)> {
        let mut parts = value.split('.');
        Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
    };
    let left = parse(left)?;
    let right = parse(right)?;
    Some(left.cmp(&right))
}

fn run_effigy_task(repo_root: &Path, task: &str) -> Result<(), RunnerError> {
    let output = Command::new(std::env::current_exe().map_err(|err| {
        RunnerError::task_invocation(format!("failed to resolve current effigy binary: {err}"))
    })?)
    .arg(task)
    .arg("--repo")
    .arg(repo_root)
    .env("NO_COLOR", "1")
    .output()
    .map_err(|err| RunnerError::task_invocation(format!("failed to run `{task}`: {err}")))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let combined = if stderr.is_empty() {
        stdout
    } else if stdout.is_empty() {
        stderr
    } else {
        format!("{stdout}\n{stderr}")
    };
    Err(RunnerError::task_invocation(format!(
        "`{task}` failed\n{combined}"
    )))
}

fn resolve_repo_input(repo_root: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        repo_root.join(path)
    }
}

fn map_distribution_policy_error(error: DistributionPolicyError) -> RunnerError {
    match error {
        DistributionPolicyError::Manifest(error) => RunnerError::task_invocation(error.to_string()),
    }
}

fn map_distribution_execution_error(error: DistributionExecutionError) -> RunnerError {
    match error {
        DistributionExecutionError::Io { path, error } => {
            RunnerError::task_invocation_failed_write(&path, error)
        }
        DistributionExecutionError::Message(message) => RunnerError::task_invocation(message),
    }
}

#[cfg(test)]
mod tests;
