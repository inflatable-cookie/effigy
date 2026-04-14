//! CLI command handler for `effigy distribution` subcommands.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::Local;
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

    match args.subcommand {
        DistributionSubcommand::ValidateMetadata { tag } => {
            run_validate_metadata(&repo_root, tag.as_deref(), args.output_json)
        }
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
            &tag,
            &resolve_repo_input(&repo_root, artifacts_dir),
            crate_version.as_deref(),
            &repo_url,
            &brew_formula,
            homebrew_executed,
            &log_files,
            args.output_json,
        ),
    }
}

fn run_preflight(
    repo_root: &Path,
    tag: Option<&str>,
    skip_docs: bool,
    skip_smoke: bool,
    output_path: Option<PathBuf>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let mut docs_status = "skipped";
    let mut smoke_status = "skipped";

    if !skip_docs {
        run_effigy_task(repo_root, "qa:docs")?;
        docs_status = "ok";
    }

    let _ = run_validate_metadata(repo_root, tag, false)?;
    let metadata_status = "ok";

    if !skip_smoke {
        run_effigy_task(repo_root, "dist:preflight:smoke")?;
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

    let result = run_first_publish_inner(
        repo_root,
        tag,
        crate_version,
        repo_url,
        brew_formula,
        skip_homebrew,
        &artifacts_dir,
        &work_dir,
        output_json,
    );

    let _ = std::fs::remove_dir_all(&work_dir);
    if let Some(path) = cleanup_artifacts_dir {
        let _ = std::fs::remove_dir_all(path);
    }
    result
}

fn run_validate_metadata(
    repo_root: &Path,
    tag: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let cargo = std::fs::read_to_string(repo_root.join("Cargo.toml")).map_err(|err| {
        RunnerError::task_invocation_failed_read(&repo_root.join("Cargo.toml"), err)
    })?;
    let cargo: TomlValue = cargo.parse().map_err(|err| {
        RunnerError::task_invocation_failed_parse(&repo_root.join("Cargo.toml"), err)
    })?;
    let package = cargo
        .get("package")
        .and_then(TomlValue::as_table)
        .ok_or_else(|| RunnerError::task_invocation("Cargo.toml is missing [package] metadata"))?;

    let name = package
        .get("name")
        .and_then(TomlValue::as_str)
        .unwrap_or_default()
        .to_owned();
    let version = package
        .get("version")
        .and_then(TomlValue::as_str)
        .unwrap_or_default()
        .to_owned();
    let license = package
        .get("license")
        .and_then(TomlValue::as_str)
        .unwrap_or_default()
        .to_owned();
    let description = package
        .get("description")
        .and_then(TomlValue::as_str)
        .unwrap_or_default()
        .to_owned();

    let semver_re =
        Regex::new(r"^[0-9]+\.[0-9]+\.[0-9]+([-.][0-9A-Za-z.]+)?$").expect("semver regex");
    let tag_re = Regex::new(r"^v[0-9]+\.[0-9]+\.[0-9]+([-.][0-9A-Za-z.]+)?$").expect("tag regex");

    let required_docs = [
        "docs/guides/010-path-installation-and-release.md",
        "docs/guides/014-release-checklist-template.md",
        "docs/guides/041-distribution-ci-pinning-and-wrapper-migration.md",
        "docs/guides/042-homebrew-tap-and-release-automation.md",
        "docs/guides/044-distribution-first-publish-execution-runbook.md",
    ];
    let required_files = [
        ".github/workflows/release-binaries.yml",
        "scripts/check-linux-glibc-floor.sh",
    ];
    let workflow_path = repo_root.join(".github/workflows/release-binaries.yml");
    let workflow = std::fs::read_to_string(&workflow_path)
        .map_err(|err| RunnerError::task_invocation_failed_read(&workflow_path, err))?;

    let mut errors = Vec::new();
    if name != "effigy" {
        errors.push(format!("expected package name `effigy`, got `{name}`"));
    }
    if !semver_re.is_match(&version) {
        errors.push(format!("package version is not semver-like: `{version}`"));
    }
    if license.is_empty() {
        errors.push("package license is empty".to_owned());
    }
    if description.is_empty() {
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
    artifacts_dir: &Path,
    expect_homebrew: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    let _ = repo_root;
    if !artifacts_dir.is_dir() {
        return Err(RunnerError::task_invocation(format!(
            "artifacts directory not found: {}",
            artifacts_dir.display()
        )));
    }
    let base_patterns = [
        ("tag install validation", "tag-install-validation"),
        ("crates.io install", "crates-io-install-validation"),
        ("crates.io binary help", "crates-io-binary-help"),
        ("crates.io binary json tasks", "crates-io-binary-json-tasks"),
    ];
    let homebrew_patterns = [
        ("homebrew install", "homebrew-install"),
        ("homebrew binary help", "homebrew-binary-help"),
        ("homebrew binary json tasks", "homebrew-binary-json-tasks"),
        ("homebrew upgrade", "homebrew-upgrade"),
    ];

    let mut found = Vec::new();
    let mut missing = Vec::new();
    for (label, pattern) in base_patterns.into_iter().chain(if expect_homebrew {
        homebrew_patterns.into_iter().collect::<Vec<_>>()
    } else {
        Vec::new()
    }) {
        match find_log_by_pattern(artifacts_dir, pattern) {
            Some(path) => found.push(json!({
                "label": label,
                "pattern": pattern,
                "file": path.file_name().and_then(|name| name.to_str()).unwrap_or_default(),
            })),
            None => missing.push(json!({
                "label": label,
                "pattern": pattern,
            })),
        }
    }

    let payload = json!({
        "schema": "effigy.distribution.artifacts.v1",
        "schema_version": 1,
        "ok": missing.is_empty(),
        "artifacts_dir": artifacts_dir.display().to_string(),
        "expect_homebrew": expect_homebrew,
        "found": found,
        "missing": missing,
    });

    if output_json {
        return if payload["ok"] == true {
            Ok(payload.to_string())
        } else {
            Err(RunnerError::task_invocation(payload.to_string()))
        };
    }
    if payload["ok"] == true {
        return Ok("[ok] distribution artifact validation passed".to_owned());
    }
    Err(RunnerError::task_invocation(
        payload["missing"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|value| {
                Some(format!(
                    "missing {} log (pattern: *{}*.log)",
                    value.get("label")?.as_str()?,
                    value.get("pattern")?.as_str()?
                ))
            })
            .collect::<Vec<_>>()
            .join("\n"),
    ))
}

fn run_generate_closeout(
    repo_root: &Path,
    tag: &str,
    artifacts_dir: &Path,
    output_path: Option<PathBuf>,
    owner: &str,
    expect_homebrew: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    let tag_re = Regex::new(r"^v[0-9]+\.[0-9]+\.[0-9]+([-.][0-9A-Za-z.]+)?$").expect("tag regex");
    if !tag_re.is_match(tag) {
        return Err(RunnerError::task_invocation(format!(
            "tag must match vX.Y.Z format: {tag}"
        )));
    }
    if !artifacts_dir.is_dir() {
        return Err(RunnerError::task_invocation(format!(
            "artifacts directory not found: {}",
            artifacts_dir.display()
        )));
    }

    let summary_path = artifacts_dir.join("distribution-summary.env");
    let mut inferred_expect_homebrew = expect_homebrew;
    if !expect_homebrew && summary_path.is_file() {
        let summary = std::fs::read_to_string(&summary_path)
            .map_err(|err| RunnerError::task_invocation_failed_read(&summary_path, err))?;
        inferred_expect_homebrew = summary
            .lines()
            .find_map(|line| line.strip_prefix("HOMEBREW_EXECUTED="))
            == Some("1");
    }

    let _ = run_validate_artifacts(repo_root, artifacts_dir, inferred_expect_homebrew, false)?;

    let mut log_files = std::fs::read_dir(artifacts_dir)
        .map_err(|err| RunnerError::task_invocation_failed_read(artifacts_dir, err))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("log"))
        .collect::<Vec<_>>();
    log_files.sort();
    if log_files.is_empty() {
        return Err(RunnerError::task_invocation(format!(
            "no .log files found in artifacts directory: {}",
            artifacts_dir.display()
        )));
    }

    let has_homebrew_logs = log_files.iter().any(|path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.contains("homebrew"))
    });

    let now = Local::now();
    let output_path = output_path.unwrap_or_else(|| {
        let sanitized_tag = tag.trim_start_matches('v').replace('.', "-");
        repo_root.join(format!(
            "docs/logs/{}/{}-{}-distribution-acceptance-closeout-{}.md",
            now.format("%Y-%m"),
            now.format("%d"),
            now.format("%H%M%S"),
            sanitized_tag
        ))
    });
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| RunnerError::task_invocation_failed_write(parent, err))?;
    }

    let today = now.format("%F").to_string();
    let evidence_lines = log_files
        .iter()
        .filter_map(|path| path.file_name().and_then(|name| name.to_str()))
        .map(|name| format!("- {name}"))
        .collect::<Vec<_>>()
        .join("\n");
    let rendered = format!(
        "# Distribution Acceptance Closeout ({tag})\n\nDate: {today}\nOwner: {owner}\nRelated roadmap: g01.backlog.distribution-channels\n\n## Scope\n\n- Capture publish-cycle distribution evidence from artifact logs.\n- Record acceptance-closeout outcomes for tag {tag}.\n\n## Inputs\n\n- release tag: {tag}\n- artifacts directory: {}\n- artifacts summary: {}\n\n## Evidence Logs\n\n{evidence_lines}\n\n## Outcomes\n\n- First-publish artifacts were captured and linked for closeout evidence.\n- Rust-native install path and CI-style install validation evidence are included in this log via artifact outputs.\n- Homebrew evidence included: {has_homebrew_logs}.\n\n## Risks / Follow-ups\n\n- If any expected channel log is missing, rerun `effigy distribution first-publish --tag {tag} --artifacts-dir <dir>` before final sign-off.\n- External channel state (crates.io availability, Homebrew tap CI, network reliability) still determines final release readiness.\n\n## Next Batch Recommendation\n\n- Reconcile acceptance checkboxes in docs/roadmaps/backlog/distribution-channels.md against this evidence and publish release sign-off notes.\n",
        artifacts_dir.display(),
        summary_path.display(),
    );
    std::fs::write(&output_path, &rendered)
        .map_err(|err| RunnerError::task_invocation_failed_write(&output_path, err))?;

    let payload = json!({
        "schema": "effigy.distribution.closeout.v1",
        "schema_version": 1,
        "ok": true,
        "tag": tag,
        "artifacts_dir": artifacts_dir.display().to_string(),
        "output": output_path.display().to_string(),
        "owner": owner,
        "has_homebrew_logs": has_homebrew_logs,
        "log_count": log_files.len(),
    });
    if output_json {
        return Ok(payload.to_string());
    }
    Ok(format!("[ok] wrote log: {}", output_path.display()))
}

fn run_write_summary(
    repo_root: &Path,
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
    let tag_re = Regex::new(r"^v[0-9]+\.[0-9]+\.[0-9]+([-.][0-9A-Za-z.]+)?$").expect("tag regex");
    if !tag_re.is_match(tag) {
        return Err(RunnerError::task_invocation(format!(
            "tag must match vX.Y.Z format: {tag}"
        )));
    }
    std::fs::create_dir_all(artifacts_dir)
        .map_err(|err| RunnerError::task_invocation_failed_write(artifacts_dir, err))?;

    let crate_version = crate_version.unwrap_or_else(|| tag.trim_start_matches('v'));
    let summary_path = artifacts_dir.join("distribution-summary.env");
    let rendered = format!(
        "TAG={tag}\nCRATE_VERSION={crate_version}\nREPO_URL={repo_url}\nBREW_FORMULA={brew_formula}\nHOMEBREW_EXECUTED={}\nLOG_FILES={}\n",
        if homebrew_executed { 1 } else { 0 },
        log_files.join(","),
    );
    std::fs::write(&summary_path, rendered)
        .map_err(|err| RunnerError::task_invocation_failed_write(&summary_path, err))?;

    let payload = json!({
        "schema": "effigy.distribution.summary.v1",
        "schema_version": 1,
        "ok": true,
        "tag": tag,
        "crate_version": crate_version,
        "artifacts_dir": artifacts_dir.display().to_string(),
        "summary": summary_path.display().to_string(),
        "repo_url": repo_url,
        "brew_formula": brew_formula,
        "homebrew_executed": homebrew_executed,
        "log_files": log_files,
    });
    if output_json {
        return Ok(payload.to_string());
    }
    Ok(format!("[ok] wrote summary: {}", summary_path.display()))
}

fn run_first_publish_inner(
    repo_root: &Path,
    tag: &str,
    crate_version: &str,
    repo_url: &str,
    brew_formula: &str,
    skip_homebrew: bool,
    artifacts_dir: &Path,
    work_dir: &Path,
    output_json: bool,
) -> Result<String, RunnerError> {
    let mut step_index = 0usize;
    let mut log_files = Vec::new();
    let mut homebrew_executed = false;
    let homebrew_status: String;

    let effigy_bin = std::env::current_exe().map_err(RunnerError::Cwd)?;
    run_logged_step(
        artifacts_dir,
        &mut step_index,
        &mut log_files,
        "tag install validation",
        {
            let mut command = Command::new(&effigy_bin);
            command.args([
                "release",
                "verify-install",
                "--repo",
                &repo_root.display().to_string(),
                "--tag",
                tag,
                "--repo-url",
                repo_url,
            ]);
            command
        },
    )?;

    let crate_install_root = work_dir.join("crates-install-root");
    run_logged_step(
        artifacts_dir,
        &mut step_index,
        &mut log_files,
        &format!("crates.io install validation ({crate_version})"),
        {
            let mut command = Command::new("cargo");
            command.args([
                "install",
                "effigy",
                "--version",
                crate_version,
                "--locked",
                "--root",
                &crate_install_root.display().to_string(),
                "--force",
            ]);
            command
        },
    )?;

    let crate_bin = crate_install_root.join("bin/effigy");
    if !crate_bin.is_file() {
        return Err(RunnerError::task_invocation(format!(
            "expected crates.io-installed binary at {}",
            crate_bin.display()
        )));
    }

    run_logged_step(
        artifacts_dir,
        &mut step_index,
        &mut log_files,
        "crates.io binary help",
        {
            let mut command = Command::new(&crate_bin);
            command.arg("--help");
            command
        },
    )?;
    run_logged_step(
        artifacts_dir,
        &mut step_index,
        &mut log_files,
        "crates.io binary json tasks",
        {
            let mut command = Command::new(&crate_bin);
            command.args(["--json", "tasks"]);
            command
        },
    )?;

    if skip_homebrew {
        homebrew_status = "skipped (--skip-homebrew)".to_owned();
    } else if !command_exists("brew") {
        homebrew_status = "skipped (brew not available)".to_owned();
    } else {
        homebrew_executed = true;
        homebrew_status = "executed".to_owned();
        run_logged_step(
            artifacts_dir,
            &mut step_index,
            &mut log_files,
            "homebrew install",
            {
                let mut command = Command::new("brew");
                command.args(["install", brew_formula]);
                command
            },
        )?;
        run_logged_step(
            artifacts_dir,
            &mut step_index,
            &mut log_files,
            "homebrew binary help",
            {
                let mut command = Command::new("effigy");
                command.arg("--help");
                command
            },
        )?;
        run_logged_step(
            artifacts_dir,
            &mut step_index,
            &mut log_files,
            "homebrew binary json tasks",
            {
                let mut command = Command::new("effigy");
                command.args(["--json", "tasks"]);
                command
            },
        )?;
        run_logged_step(
            artifacts_dir,
            &mut step_index,
            &mut log_files,
            "homebrew upgrade",
            {
                let mut command = Command::new("brew");
                command.args(["upgrade", "effigy"]);
                command
            },
        )?;
    }

    let _ = run_write_summary(
        repo_root,
        tag,
        artifacts_dir,
        Some(crate_version),
        repo_url,
        brew_formula,
        homebrew_executed,
        &log_files,
        false,
    )?;
    let _ = run_validate_artifacts(repo_root, artifacts_dir, homebrew_executed, false)?;
    let summary_path = artifacts_dir.join("distribution-summary.env");

    let payload = json!({
        "schema": "effigy.distribution.first-publish.v1",
        "schema_version": 1,
        "ok": true,
        "tag": tag,
        "crate_version": crate_version,
        "repo_url": repo_url,
        "brew_formula": brew_formula,
        "homebrew_executed": homebrew_executed,
        "homebrew_status": homebrew_status,
        "artifacts_dir": artifacts_dir.display().to_string(),
        "summary_path": summary_path.display().to_string(),
        "log_files": log_files,
    });
    if output_json {
        return Ok(payload.to_string());
    }

    Ok(format!(
        "[ok] distribution first-publish matrix passed\n[ok] artifacts directory: {}\n[ok] artifacts summary: {}",
        artifacts_dir.display(),
        summary_path.display()
    ))
}

fn run_logged_step(
    artifacts_dir: &Path,
    step_index: &mut usize,
    log_files: &mut Vec<String>,
    label: &str,
    mut command: Command,
) -> Result<(), RunnerError> {
    *step_index += 1;
    let slug = slugify(label);
    let log_file = format!("{:02}-{slug}.log", *step_index);
    let log_path = artifacts_dir.join(&log_file);
    let output = command
        .output()
        .map_err(|err| RunnerError::task_invocation(err.to_string()))?;
    let mut rendered = String::new();
    rendered.push_str(&String::from_utf8_lossy(&output.stdout));
    rendered.push_str(&String::from_utf8_lossy(&output.stderr));
    std::fs::write(&log_path, rendered)
        .map_err(|err| RunnerError::task_invocation_failed_write(&log_path, err))?;
    log_files.push(log_file);

    if output.status.success() {
        Ok(())
    } else {
        let tail = read_log_tail(&log_path, 40);
        Err(RunnerError::task_invocation(format!(
            "[error] {label} failed (log: {})\n[error] tail of log:\n{}",
            log_path.display(),
            tail
        )))
    }
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

fn command_exists(program: &str) -> bool {
    let candidate = Path::new(program);
    if candidate.components().count() > 1 {
        return is_executable_file(candidate);
    }

    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .map(|entry| entry.join(program))
        .any(|path| is_executable_file(&path))
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn slugify(value: &str) -> String {
    let lower = value.to_ascii_lowercase();
    let mut out = String::new();
    let mut last_dash = false;
    for ch in lower.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    out.trim_matches('-').to_owned()
}

fn read_log_tail(path: &Path, line_count: usize) -> String {
    std::fs::read_to_string(path)
        .ok()
        .map(|contents| {
            let lines = contents.lines().collect::<Vec<_>>();
            let start = lines.len().saturating_sub(line_count);
            lines[start..].join("\n")
        })
        .unwrap_or_else(|| "(unable to read log tail)".to_owned())
}

fn allocate_distribution_temp_dir(prefix: &str) -> Result<PathBuf, RunnerError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| RunnerError::task_invocation(err.to_string()))?
        .as_nanos();
    Ok(std::env::temp_dir().join(format!("{prefix}-{now}")))
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

fn find_log_by_pattern(artifacts_dir: &Path, pattern: &str) -> Option<PathBuf> {
    let mut matches = std::fs::read_dir(artifacts_dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().and_then(|ext| ext.to_str()) == Some("log")
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.contains(pattern))
        })
        .collect::<Vec<_>>();
    matches.sort();
    matches.into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::{
        command_exists, find_log_by_pattern, run_preflight, run_validate_artifacts,
        run_validate_metadata, run_write_summary,
    };
    use std::fs;

    #[test]
    fn find_log_by_pattern_returns_matching_log() {
        let root = std::env::temp_dir().join(format!(
            "effigy-distribution-log-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("mkdir");
        fs::write(root.join("01-tag-install-validation.log"), "ok\n").expect("write log");
        let found = find_log_by_pattern(&root, "tag-install-validation").expect("match");
        assert_eq!(
            found.file_name().and_then(|name| name.to_str()),
            Some("01-tag-install-validation.log")
        );
    }

    #[test]
    fn validate_artifacts_rejects_missing_required_logs() {
        let root = std::env::temp_dir().join(format!(
            "effigy-distribution-artifacts-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("mkdir");
        fs::write(root.join("01-tag-install-validation.log"), "ok\n").expect("write log");
        let err = run_validate_artifacts(std::path::Path::new("."), &root, false, false)
            .expect_err("should fail");
        assert!(err.to_string().contains("crates.io install"));
    }

    #[test]
    fn write_summary_defaults_crate_version_from_tag() {
        let root = std::env::temp_dir().join(format!(
            "effigy-distribution-summary-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("mkdir");
        run_write_summary(
            std::path::Path::new("."),
            "v0.2.5",
            &root,
            None,
            "https://github.com/inflatable-cookie/effigy.git",
            "inflatable-cookie/effigy/effigy",
            true,
            &["01-tag-install-validation.log".to_owned()],
            false,
        )
        .expect("write summary");
        let rendered =
            fs::read_to_string(root.join("distribution-summary.env")).expect("read summary");
        assert!(rendered.contains("CRATE_VERSION=0.2.5"));
        assert!(rendered.contains("HOMEBREW_EXECUTED=1"));
    }

    #[test]
    fn current_repo_distribution_metadata_requires_only_workflow_bound_glibc_script() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        run_validate_metadata(root, Some("v0.2.13"), false).expect("metadata should pass");
        assert!(
            !root
                .join("scripts/check-distribution-first-publish.sh")
                .exists(),
            "first-publish wrapper should be retired"
        );
        assert!(
            root.join("scripts/check-linux-glibc-floor.sh").exists(),
            "glibc floor guard should remain until workflow cutover"
        );
    }

    #[test]
    fn preflight_recommends_native_first_publish_command() {
        let output = run_preflight(
            std::path::Path::new("."),
            Some("v0.2.13"),
            true,
            true,
            None,
            false,
        )
        .expect("preflight should render");

        assert!(output.contains("effigy distribution first-publish --tag v0.2.13"));
        assert!(!output.contains("check-distribution-first-publish.sh"));
    }

    #[test]
    fn command_exists_checks_path_without_shell() {
        let temp_dir = std::env::temp_dir().join(format!(
            "effigy-command-exists-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&temp_dir).expect("mkdir");
        let fake_bin = temp_dir.join("fake-tool");
        fs::write(&fake_bin, "#!/bin/sh\nexit 0\n").expect("write fake tool");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let mut perms = fs::metadata(&fake_bin).expect("metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&fake_bin, perms).expect("chmod");
        }

        assert!(command_exists(fake_bin.to_str().expect("utf8 path")));
    }
}
