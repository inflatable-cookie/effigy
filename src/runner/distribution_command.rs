//! CLI command handler for `effigy distribution` subcommands.

use std::path::{Path, PathBuf};
use std::process::Command;

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
            "./scripts/check-distribution-first-publish.sh --tag {tag} --artifacts-dir ./artifacts/distribution-{tag}"
        )
    } else {
        "./scripts/check-distribution-first-publish.sh --tag vX.Y.Z --artifacts-dir ./artifacts/distribution-vX.Y.Z".to_owned()
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
        "scripts/check-distribution-first-publish.sh",
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
        "# Distribution Acceptance Closeout ({tag})\n\nDate: {today}\nOwner: {owner}\nRelated roadmap: g01.backlog.distribution-channels\n\n## Scope\n\n- Capture publish-cycle distribution evidence from artifact logs.\n- Record acceptance-closeout outcomes for tag {tag}.\n\n## Inputs\n\n- release tag: {tag}\n- artifacts directory: {}\n- artifacts summary: {}\n\n## Evidence Logs\n\n{evidence_lines}\n\n## Outcomes\n\n- First-publish script artifacts were captured and linked for closeout evidence.\n- Rust-native install path and CI-style install validation evidence are included in this log via artifact outputs.\n- Homebrew evidence included: {has_homebrew_logs}.\n\n## Risks / Follow-ups\n\n- If any expected channel log is missing, rerun ./scripts/check-distribution-first-publish.sh --tag {tag} --artifacts-dir <dir> before final sign-off.\n- External channel state (crates.io availability, Homebrew tap CI, network reliability) still determines final release readiness.\n\n## Next Batch Recommendation\n\n- Reconcile acceptance checkboxes in docs/roadmaps/backlog/distribution-channels.md against this evidence and publish release sign-off notes.\n",
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
    use super::{find_log_by_pattern, run_validate_artifacts, run_write_summary};
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
    fn current_repo_first_publish_wrapper_delegates_to_native_builtins() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let script = fs::read_to_string(root.join("scripts/check-distribution-first-publish.sh"))
            .expect("read first-publish script");

        assert!(script.contains("release verify-install --repo"));
        assert!(script.contains("distribution write-summary"));
        assert!(script.contains("distribution validate-artifacts"));
        assert!(!script.contains("check-release-install-from-tag.sh"));
        assert!(!script.contains("validate-distribution-artifacts.sh"));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let mode = fs::metadata(root.join("scripts/check-distribution-first-publish.sh"))
                .expect("wrapper script metadata")
                .permissions()
                .mode();
            assert_ne!(mode & 0o111, 0, "wrapper script should stay executable");
        }
    }
}
