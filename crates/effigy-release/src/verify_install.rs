use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::time::Instant;

use super::{ReleaseError, ReleaseVerifyInstall, VerificationStepResult};
use serde_json::Value;

pub fn resolve_verify_install_tag(
    tag: Option<String>,
    github_ref_name: Option<String>,
) -> Result<String, ReleaseError> {
    tag.or(github_ref_name)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ReleaseError::TaskInvocation(
                "release verify-install requires `--tag <TAG>` or `GITHUB_REF_NAME`".to_owned(),
            )
        })
}

pub fn normalize_verify_install_repo_url(repo_url: &str) -> String {
    let trimmed = repo_url.trim();
    if trimmed.is_empty()
        || trimmed.contains("://")
        || trimmed.starts_with('/')
        || trimmed.starts_with("./")
        || trimmed.starts_with("../")
        || trimmed.starts_with("~/")
    {
        return trimmed.to_owned();
    }

    if let Some((host_part, path_part)) = trimmed.split_once(':') {
        if !path_part.is_empty()
            && path_part.contains('/')
            && !path_part.starts_with('/')
            && (host_part.contains('@') || host_part.contains('.'))
        {
            return format!("ssh://{host_part}/{}", path_part.trim_start_matches('/'));
        }
    }

    trimmed.to_owned()
}

fn make_release_temp_dir(purpose: &str) -> Result<PathBuf, ReleaseError> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| {
            ReleaseError::TaskInvocation(format!("failed to read system time: {error}"))
        })?
        .as_nanos();
    let root = std::env::temp_dir().join(format!("effigy-release-{purpose}-{ts}"));
    std::fs::create_dir_all(&root).map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "failed to create release temp directory `{}`: {error}",
            root.display()
        ))
    })?;
    Ok(root)
}

fn write_release_install_fixture(path: &Path) -> Result<(), ReleaseError> {
    let manifest_path = path.join("effigy.toml");
    std::fs::write(
        &manifest_path,
        "[catalog]\nalias = \"catalog_a\"\n\n[tasks]\nnoop = \"echo noop\"\n",
    )
    .map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "failed to write verify-install fixture `{}`: {error}",
            manifest_path.display()
        ))
    })
}

fn run_verification_step(
    name: &str,
    program: &str,
    args: &[String],
    cwd: Option<&Path>,
) -> VerificationStepResult {
    let mut command = ProcessCommand::new(program);
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let started = Instant::now();
    match command.output() {
        Ok(output) => VerificationStepResult {
            name: name.to_owned(),
            command: format_command(program, args),
            passed: output.status.success(),
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            launch_error: None,
            duration_ms: started.elapsed().as_millis(),
        },
        Err(error) => VerificationStepResult {
            name: name.to_owned(),
            command: format_command(program, args),
            passed: false,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            launch_error: Some(error.to_string()),
            duration_ms: started.elapsed().as_millis(),
        },
    }
}

#[derive(Clone, Copy)]
enum VerificationExpectation<'a> {
    ContainsStdout(&'a str),
    JsonHelpEnvelope,
    VersionMatchesTag(&'a str),
}

fn normalize_release_tag_version(tag: &str) -> &str {
    tag.strip_prefix('v').unwrap_or(tag)
}

fn validate_verification_step(
    mut result: VerificationStepResult,
    expectation: VerificationExpectation<'_>,
) -> VerificationStepResult {
    if !result.passed {
        return result;
    }

    let validation_error = match expectation {
        VerificationExpectation::ContainsStdout(expected) => {
            if result.stdout.contains(expected) {
                None
            } else if result.stdout.is_empty() {
                Some(format!(
                    "expected stdout to contain `{expected}`, but it was empty"
                ))
            } else {
                Some(format!(
                    "expected stdout to contain `{expected}`, got `{}`",
                    result.stdout
                ))
            }
        }
        VerificationExpectation::JsonHelpEnvelope => {
            match serde_json::from_str::<Value>(&result.stdout) {
                Ok(parsed) => {
                    let schema = parsed.get("schema").and_then(Value::as_str);
                    let result_schema = parsed
                        .get("result")
                        .and_then(|value| value.get("schema"))
                        .and_then(Value::as_str);
                    if schema == Some("effigy.command.v1")
                        && result_schema == Some("effigy.help.v1")
                    {
                        None
                    } else {
                        Some(format!(
                        "expected effigy help JSON envelope, got schema={schema:?} result.schema={result_schema:?}"
                    ))
                    }
                }
                Err(error) => Some(format!(
                    "expected JSON help output, got parse error: {error}"
                )),
            }
        }
        VerificationExpectation::VersionMatchesTag(tag) => {
            let expected = format!("v{}", normalize_release_tag_version(tag));
            if result.stdout.contains(&expected) {
                None
            } else if result.stdout.is_empty() {
                Some(format!(
                    "expected version output to contain `{expected}`, but it was empty"
                ))
            } else {
                Some(format!(
                    "expected version output to contain `{expected}`, got `{}`",
                    result.stdout
                ))
            }
        }
    };

    if let Some(error) = validation_error {
        result.passed = false;
        if result.stderr.is_empty() {
            result.stderr = error;
        } else {
            result.stderr = format!("{}\n{error}", result.stderr);
        }
    }

    result
}

fn format_command(program: &str, args: &[String]) -> String {
    if args.is_empty() {
        return program.to_owned();
    }
    format!("{program} {}", args.join(" "))
}

pub fn run_release_verify_install(
    repo_root: PathBuf,
    tag: String,
    repo_url: String,
) -> Result<ReleaseVerifyInstall, ReleaseError> {
    let temp_root = make_release_temp_dir("verify-install")?;
    let install_root = temp_root.join("install-root");
    let fixture_dir = temp_root.join("fixture");
    std::fs::create_dir_all(&fixture_dir).map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "failed to create verify-install fixture directory `{}`: {error}",
            fixture_dir.display()
        ))
    })?;
    write_release_install_fixture(&fixture_dir)?;

    let install_command = vec![
        "install".to_owned(),
        "--locked".to_owned(),
        "--git".to_owned(),
        repo_url.clone(),
        "--tag".to_owned(),
        tag.clone(),
        "--root".to_owned(),
        install_root.display().to_string(),
        "--force".to_owned(),
        "effigy".to_owned(),
    ];
    let mut results = vec![run_verification_step(
        "cargo install from git tag",
        "cargo",
        &install_command,
        None,
    )];

    let mut blockers = Vec::new();
    if !results[0].passed {
        blockers.push(format!(
            "install verification step `{}` failed",
            results[0].name
        ));
        return Ok(ReleaseVerifyInstall {
            repo_root,
            tag,
            repo_url,
            installed_bin: None,
            configured_check_count: 7,
            executed_check_count: results.len(),
            stopped_early: true,
            results,
            blockers,
            verified: false,
        });
    }

    let installed_bin = install_root.join("bin/effigy");
    if !installed_bin.is_file() {
        blockers.push(format!(
            "installed binary is missing or not executable: {}",
            installed_bin.display()
        ));
        return Ok(ReleaseVerifyInstall {
            repo_root,
            tag,
            repo_url,
            installed_bin: Some(installed_bin),
            configured_check_count: 7,
            executed_check_count: results.len(),
            stopped_early: true,
            results,
            blockers,
            verified: false,
        });
    }

    let verification_checks = vec![
        (
            "installed binary version check",
            installed_bin.clone(),
            vec!["version".to_owned()],
            VerificationExpectation::VersionMatchesTag(&tag),
        ),
        (
            "installed binary tasks fixture check",
            installed_bin.clone(),
            vec![
                "tasks".to_owned(),
                "--repo".to_owned(),
                fixture_dir.display().to_string(),
            ],
            VerificationExpectation::ContainsStdout("noop"),
        ),
        (
            "installed binary prefixed builtin tasks check",
            installed_bin.clone(),
            vec![
                "catalog_a/tasks".to_owned(),
                "--repo".to_owned(),
                fixture_dir.display().to_string(),
            ],
            VerificationExpectation::ContainsStdout("noop"),
        ),
        (
            "installed binary json help check",
            installed_bin.clone(),
            vec!["--json".to_owned(), "help".to_owned()],
            VerificationExpectation::JsonHelpEnvelope,
        ),
        (
            "installed binary completion check",
            installed_bin.clone(),
            vec!["completion".to_owned(), "bash".to_owned()],
            VerificationExpectation::ContainsStdout("complete"),
        ),
        (
            "installed binary completion candidates check",
            installed_bin.clone(),
            vec![
                "completion".to_owned(),
                "candidates".to_owned(),
                "--repo".to_owned(),
                fixture_dir.display().to_string(),
            ],
            VerificationExpectation::ContainsStdout("noop"),
        ),
    ];

    let mut stopped_early = false;
    for (name, program, args, expectation) in verification_checks {
        let result = validate_verification_step(
            run_verification_step(name, &program.display().to_string(), &args, None),
            expectation,
        );
        let passed = result.passed;
        results.push(result);
        if !passed {
            blockers.push(format!(
                "install verification step `{}` failed",
                results
                    .last()
                    .map(|step| step.name.as_str())
                    .unwrap_or(name)
            ));
            stopped_early = true;
            break;
        }
    }

    Ok(ReleaseVerifyInstall {
        repo_root,
        tag,
        repo_url,
        installed_bin: Some(installed_bin),
        configured_check_count: 7,
        executed_check_count: results.len(),
        stopped_early,
        blockers: blockers.clone(),
        verified: blockers.is_empty(),
        results,
    })
}
