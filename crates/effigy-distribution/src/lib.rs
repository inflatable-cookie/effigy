use anstyle::Style;
use std::io::IsTerminal;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::Local;
use effigy_manifest::{
    config_sections::{ManifestDistributionCloseoutConfig, ManifestDistributionPublishConfig},
    load_task_manifest, ManifestDistributionConfig, ManifestDistributionMetadataConfig,
    ManifestDistributionPackageConfig, ManifestDistributionPreflightConfig, ManifestError,
    TASK_MANIFEST_FILE,
};
use effigy_ui::theme::{resolve_color_enabled, Theme};
use effigy_ui::OutputMode;
use regex::Regex;
use serde_json::{json, Value};

pub const DEFAULT_PACKAGE_NAME: &str = "effigy";
pub const DEFAULT_REPO_URL: &str = "https://github.com/inflatable-cookie/effigy.git";
pub const DEFAULT_BREW_FORMULA: &str = "inflatable-cookie/effigy/effigy";
pub const DEFAULT_BINARY_NAME: &str = "effigy";
pub const DEFAULT_REGISTRY_LABEL: &str = "crates.io";
pub const DEFAULT_DOCS_TASK: &str = "qa:docs";
pub const DEFAULT_SMOKE_TASK: &str = "dist:preflight:smoke";
pub const DEFAULT_CLOSEOUT_OWNER: &str = "release";
pub const DEFAULT_CLOSEOUT_NEXT_STEP: &str =
    "Review the captured evidence and publish release sign-off notes in your repo's chosen workflow.";
pub const DEFAULT_REQUIRED_DOCS: [&str; 5] = [
    "docs/guides/010-path-installation-and-release.md",
    "docs/guides/014-release-checklist-template.md",
    "docs/guides/041-distribution-ci-pinning-and-wrapper-migration.md",
    "docs/guides/042-homebrew-tap-and-release-automation.md",
    "docs/guides/044-distribution-first-publish-execution-runbook.md",
];
pub const DEFAULT_REQUIRED_FILES: [&str; 1] = [".github/workflows/release-binaries.yml"];

#[derive(Debug, Clone)]
pub struct EffectiveDistributionPolicy {
    pub manifest_adopted: bool,
    pub package_name: String,
    pub binary_name: String,
    pub registry_label: String,
    pub verify_tag_install: bool,
    pub verify_binary_json_tasks: bool,
    pub repo_url: String,
    pub brew_formula: String,
    pub docs_task: String,
    pub smoke_task: String,
    pub required_docs: Vec<String>,
    pub required_files: Vec<String>,
    pub closeout_owner: String,
    pub closeout_related: Option<String>,
    pub closeout_next_step: String,
}

#[derive(Debug)]
pub enum DistributionPolicyError {
    Manifest(ManifestError),
}

#[derive(Debug)]
pub enum DistributionExecutionError {
    Io {
        path: PathBuf,
        error: std::io::Error,
    },
    Message(String),
}

impl std::fmt::Display for DistributionExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, error } => write!(f, "failed to access {}: {error}", path.display()),
            Self::Message(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for DistributionExecutionError {}

impl std::fmt::Display for DistributionPolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Manifest(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for DistributionPolicyError {}

impl From<ManifestError> for DistributionPolicyError {
    fn from(value: ManifestError) -> Self {
        Self::Manifest(value)
    }
}

impl EffectiveDistributionPolicy {
    pub fn from_manifest(config: Option<ManifestDistributionConfig>) -> Self {
        let manifest_adopted = config.is_some();
        let package = config.as_ref().and_then(|config| config.package.as_ref());
        let publish = config.as_ref().and_then(|config| config.publish.as_ref());
        let preflight = config.as_ref().and_then(|config| config.preflight.as_ref());
        let metadata = config.as_ref().and_then(|config| config.metadata.as_ref());
        let closeout = config.as_ref().and_then(|config| config.closeout.as_ref());
        let package_name = package_name_from_config(package);
        Self {
            manifest_adopted,
            package_name: package_name.clone(),
            binary_name: binary_name_from_config(publish, &package_name),
            registry_label: registry_label_from_config(publish),
            verify_tag_install: verify_tag_install_from_config(publish),
            verify_binary_json_tasks: verify_binary_json_tasks_from_config(publish),
            repo_url: repo_url_from_config(package),
            brew_formula: brew_formula_from_config(package),
            docs_task: docs_task_from_config(preflight),
            smoke_task: smoke_task_from_config(preflight),
            required_docs: required_docs_from_config(metadata, manifest_adopted),
            required_files: required_files_from_config(metadata, manifest_adopted),
            closeout_owner: closeout_owner_from_config(closeout),
            closeout_related: closeout_related_from_config(closeout),
            closeout_next_step: closeout_next_step_from_config(closeout),
        }
    }
}

pub fn load_distribution_policy(
    repo_root: &Path,
) -> Result<EffectiveDistributionPolicy, DistributionPolicyError> {
    let manifest_path = repo_root.join(TASK_MANIFEST_FILE);
    let distribution = if manifest_path.is_file() {
        load_task_manifest(&manifest_path)?.distribution
    } else {
        None
    };
    Ok(EffectiveDistributionPolicy::from_manifest(distribution))
}

pub fn effective_repo_url(
    distribution_policy: &EffectiveDistributionPolicy,
    repo_url: &str,
) -> String {
    if repo_url == DEFAULT_REPO_URL {
        distribution_policy.repo_url.clone()
    } else {
        repo_url.to_owned()
    }
}

pub fn effective_brew_formula(
    distribution_policy: &EffectiveDistributionPolicy,
    brew_formula: &str,
) -> String {
    if brew_formula == DEFAULT_BREW_FORMULA {
        distribution_policy.brew_formula.clone()
    } else {
        brew_formula.to_owned()
    }
}

pub fn effective_closeout_owner(
    distribution_policy: &EffectiveDistributionPolicy,
    owner: &str,
) -> String {
    if owner == DEFAULT_CLOSEOUT_OWNER {
        distribution_policy.closeout_owner.clone()
    } else {
        owner.to_owned()
    }
}

pub fn base_artifact_patterns(
    distribution_policy: &EffectiveDistributionPolicy,
) -> Vec<(String, String)> {
    let registry_slug = slugify(&distribution_policy.registry_label);
    let mut patterns = Vec::new();
    if distribution_policy.verify_tag_install {
        patterns.push((
            "tag install validation".to_owned(),
            "tag-install-validation".to_owned(),
        ));
    }
    patterns.extend([
        (
            format!("{} install", distribution_policy.registry_label),
            format!("{registry_slug}-install-validation"),
        ),
        (
            format!("{} binary help", distribution_policy.registry_label),
            format!("{registry_slug}-binary-help"),
        ),
    ]);
    if distribution_policy.verify_binary_json_tasks {
        patterns.push((
            format!("{} binary json tasks", distribution_policy.registry_label),
            format!("{registry_slug}-binary-json-tasks"),
        ));
    }
    patterns
}

pub fn homebrew_artifact_patterns(
    distribution_policy: &EffectiveDistributionPolicy,
) -> Vec<(String, String)> {
    let mut patterns = vec![
        ("homebrew install".to_owned(), "homebrew-install".to_owned()),
        (
            "homebrew binary help".to_owned(),
            "homebrew-binary-help".to_owned(),
        ),
        ("homebrew upgrade".to_owned(), "homebrew-upgrade".to_owned()),
    ];
    if distribution_policy.verify_binary_json_tasks {
        patterns.push((
            "homebrew binary json tasks".to_owned(),
            "homebrew-binary-json-tasks".to_owned(),
        ));
    }
    patterns
}

fn schema_v1_payload(schema: &str, payload: Value) -> Value {
    match payload {
        Value::Object(mut map) => {
            map.insert("schema".to_owned(), Value::String(schema.to_owned()));
            map.insert("schema_version".to_owned(), Value::from(1));
            Value::Object(map)
        }
        _ => panic!("distribution schema payloads must be JSON objects"),
    }
}

pub fn validate_artifacts_command(
    distribution_policy: &EffectiveDistributionPolicy,
    artifacts_dir: &Path,
    expect_homebrew: bool,
    output_json: bool,
) -> Result<String, DistributionExecutionError> {
    if !artifacts_dir.is_dir() {
        return Err(DistributionExecutionError::Message(format!(
            "artifacts directory not found: {}",
            artifacts_dir.display()
        )));
    }
    let base_patterns = base_artifact_patterns(distribution_policy);
    let homebrew_patterns = homebrew_artifact_patterns(distribution_policy);

    let mut found = Vec::new();
    let mut missing = Vec::new();
    for (label, pattern) in base_patterns.into_iter().chain(if expect_homebrew {
        homebrew_patterns
    } else {
        Vec::new()
    }) {
        match find_log_by_pattern(artifacts_dir, &pattern) {
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

    let payload = schema_v1_payload(
        "effigy.distribution.artifacts.v1",
        json!({
            "ok": missing.is_empty(),
            "artifacts_dir": artifacts_dir.display().to_string(),
            "expect_homebrew": expect_homebrew,
            "found": found,
            "missing": missing,
        }),
    );

    if output_json {
        return if payload["ok"] == true {
            Ok(payload.to_string())
        } else {
            Err(DistributionExecutionError::Message(payload.to_string()))
        };
    }
    if payload["ok"] == true {
        return Ok("[ok] distribution artifact validation passed".to_owned());
    }
    Err(DistributionExecutionError::Message(
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

pub fn generate_closeout_command(
    distribution_policy: &EffectiveDistributionPolicy,
    tag: &str,
    artifacts_dir: &Path,
    output_path: Option<PathBuf>,
    owner: &str,
    expect_homebrew: bool,
    output_json: bool,
) -> Result<String, DistributionExecutionError> {
    let tag_re = Regex::new(r"^v[0-9]+\.[0-9]+\.[0-9]+([-.][0-9A-Za-z.]+)?$").expect("tag regex");
    if !tag_re.is_match(tag) {
        return Err(DistributionExecutionError::Message(format!(
            "tag must match vX.Y.Z format: {tag}"
        )));
    }
    if !artifacts_dir.is_dir() {
        return Err(DistributionExecutionError::Message(format!(
            "artifacts directory not found: {}",
            artifacts_dir.display()
        )));
    }

    let summary_path = artifacts_dir.join("distribution-summary.env");
    let mut inferred_expect_homebrew = expect_homebrew;
    if !expect_homebrew && summary_path.is_file() {
        let summary = std::fs::read_to_string(&summary_path).map_err(|error| {
            DistributionExecutionError::Io {
                path: summary_path.clone(),
                error,
            }
        })?;
        inferred_expect_homebrew = summary
            .lines()
            .find_map(|line| line.strip_prefix("HOMEBREW_EXECUTED="))
            == Some("1");
    }

    let _ = validate_artifacts_command(
        distribution_policy,
        artifacts_dir,
        inferred_expect_homebrew,
        false,
    )?;

    let mut log_files = std::fs::read_dir(artifacts_dir)
        .map_err(|error| DistributionExecutionError::Io {
            path: artifacts_dir.to_path_buf(),
            error,
        })?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("log"))
        .collect::<Vec<_>>();
    log_files.sort();
    if log_files.is_empty() {
        return Err(DistributionExecutionError::Message(format!(
            "no .log files found in artifacts directory: {}",
            artifacts_dir.display()
        )));
    }

    let homebrew_patterns = homebrew_artifact_patterns(distribution_policy);
    let has_homebrew_logs = log_files.iter().any(|path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| {
                homebrew_patterns
                    .iter()
                    .any(|(_, pattern)| name.contains(pattern))
            })
    });

    let now = Local::now();
    let output_path = output_path.unwrap_or_else(|| {
        let sanitized_tag = tag.trim_start_matches('v').replace('.', "-");
        PathBuf::from(format!(
            "docs/logs/{}/{}-{}-distribution-acceptance-closeout-{}.md",
            now.format("%Y-%m"),
            now.format("%d"),
            now.format("%H%M%S"),
            sanitized_tag
        ))
    });
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| DistributionExecutionError::Io {
            path: parent.to_path_buf(),
            error,
        })?;
    }

    let owner = effective_closeout_owner(distribution_policy, owner);
    let today = now.format("%F").to_string();
    let related_line = distribution_policy
        .closeout_related
        .as_ref()
        .map(|related| format!("Related: {related}\n"))
        .unwrap_or_default();
    let evidence_lines = log_files
        .iter()
        .filter_map(|path| path.file_name().and_then(|name| name.to_str()))
        .map(|name| format!("- {name}"))
        .collect::<Vec<_>>()
        .join("\n");
    let rendered = format!(
        "# Distribution Acceptance Closeout ({tag})\n\nDate: {today}\nOwner: {owner}\n{related_line}\n## Scope\n\n- Capture publish-cycle distribution evidence from artifact logs.\n- Record acceptance-closeout outcomes for tag {tag}.\n\n## Inputs\n\n- release tag: {tag}\n- artifacts directory: {}\n- artifacts summary: {}\n\n## Evidence Logs\n\n{evidence_lines}\n\n## Outcomes\n\n- First-publish artifacts were captured and linked for closeout evidence.\n- Install validation evidence for `{}` is included in this closeout via artifact outputs.\n- Homebrew evidence included: {has_homebrew_logs}.\n\n## Risks / Follow-ups\n\n- If any expected channel log is missing, rerun `effigy release proof --tag {tag} --artifacts-dir <dir>` before final sign-off.\n- External distribution channel state still determines final release readiness.\n\n## Next Step\n\n- {}\n",
        artifacts_dir.display(),
        summary_path.display(),
        distribution_policy.package_name,
        distribution_policy.closeout_next_step,
    );
    std::fs::write(&output_path, &rendered).map_err(|error| DistributionExecutionError::Io {
        path: output_path.clone(),
        error,
    })?;

    let payload = schema_v1_payload(
        "effigy.distribution.closeout.v1",
        json!({
            "ok": true,
            "tag": tag,
            "artifacts_dir": artifacts_dir.display().to_string(),
            "output": output_path.display().to_string(),
            "owner": owner,
            "related": distribution_policy.closeout_related,
            "has_homebrew_logs": has_homebrew_logs,
            "log_count": log_files.len(),
        }),
    );
    if output_json {
        return Ok(payload.to_string());
    }
    Ok(format!("[ok] wrote log: {}", output_path.display()))
}

pub fn write_summary_command(
    distribution_policy: &EffectiveDistributionPolicy,
    tag: &str,
    artifacts_dir: &Path,
    crate_version: Option<&str>,
    repo_url: &str,
    brew_formula: &str,
    homebrew_executed: bool,
    log_files: &[String],
    output_json: bool,
) -> Result<String, DistributionExecutionError> {
    let tag_re = Regex::new(r"^v[0-9]+\.[0-9]+\.[0-9]+([-.][0-9A-Za-z.]+)?$").expect("tag regex");
    if !tag_re.is_match(tag) {
        return Err(DistributionExecutionError::Message(format!(
            "tag must match vX.Y.Z format: {tag}"
        )));
    }
    std::fs::create_dir_all(artifacts_dir).map_err(|error| DistributionExecutionError::Io {
        path: artifacts_dir.to_path_buf(),
        error,
    })?;

    let crate_version = crate_version.unwrap_or_else(|| tag.trim_start_matches('v'));
    let summary_path = artifacts_dir.join("distribution-summary.env");
    let rendered = format!(
        "TAG={tag}\nPACKAGE_NAME={}\nBINARY_NAME={}\nREGISTRY_LABEL={}\nCRATE_VERSION={crate_version}\nREPO_URL={repo_url}\nBREW_FORMULA={brew_formula}\nHOMEBREW_EXECUTED={}\nLOG_FILES={}\n",
        distribution_policy.package_name,
        distribution_policy.binary_name,
        distribution_policy.registry_label,
        if homebrew_executed { 1 } else { 0 },
        log_files.join(","),
    );
    std::fs::write(&summary_path, rendered).map_err(|error| DistributionExecutionError::Io {
        path: summary_path.clone(),
        error,
    })?;

    let payload = schema_v1_payload(
        "effigy.distribution.summary.v1",
        json!({
            "ok": true,
            "tag": tag,
            "package_name": distribution_policy.package_name,
            "binary_name": distribution_policy.binary_name,
            "registry_label": distribution_policy.registry_label,
            "crate_version": crate_version,
            "artifacts_dir": artifacts_dir.display().to_string(),
            "summary": summary_path.display().to_string(),
            "repo_url": repo_url,
            "brew_formula": brew_formula,
            "homebrew_executed": homebrew_executed,
            "log_files": log_files,
        }),
    );
    if output_json {
        return Ok(payload.to_string());
    }
    Ok(format!("[ok] wrote summary: {}", summary_path.display()))
}

pub fn first_publish_command(
    repo_root: &Path,
    distribution_policy: &EffectiveDistributionPolicy,
    tag: &str,
    crate_version: &str,
    repo_url: &str,
    brew_formula: &str,
    skip_homebrew: bool,
    artifacts_dir: &Path,
    work_dir: &Path,
    effigy_bin: &Path,
    output_json: bool,
) -> Result<String, DistributionExecutionError> {
    let brew_available = command_exists("brew");
    let plan = build_first_publish_plan(
        repo_root,
        distribution_policy,
        tag,
        crate_version,
        repo_url,
        brew_formula,
        skip_homebrew,
        work_dir,
        effigy_bin,
        brew_available,
    );

    let mut step_index = 0usize;
    let mut log_files = Vec::new();
    for step in plan.pre_install_steps {
        let label = step.label.clone();
        run_logged_step(
            artifacts_dir,
            &mut step_index,
            &mut log_files,
            &label,
            step.into_command(),
        )?;
    }

    let install_label = plan.install_step.label.clone();
    run_logged_step(
        artifacts_dir,
        &mut step_index,
        &mut log_files,
        &install_label,
        plan.install_step.into_command(),
    )?;

    let crate_bin = plan.crate_bin.clone();
    if !crate_bin.is_file() {
        return Err(DistributionExecutionError::Message(format!(
            "expected installed binary at {}",
            crate_bin.display()
        )));
    }

    for step in plan.post_install_steps {
        let label = step.label.clone();
        run_logged_step(
            artifacts_dir,
            &mut step_index,
            &mut log_files,
            &label,
            step.into_command(),
        )?;
    }

    for step in plan.homebrew_steps {
        let label = step.label.clone();
        run_logged_step(
            artifacts_dir,
            &mut step_index,
            &mut log_files,
            &label,
            step.into_command(),
        )?;
    }

    let _ = write_summary_command(
        distribution_policy,
        tag,
        artifacts_dir,
        Some(crate_version),
        repo_url,
        brew_formula,
        plan.homebrew_executed,
        &log_files,
        false,
    )?;
    let _ = validate_artifacts_command(
        distribution_policy,
        artifacts_dir,
        plan.homebrew_executed,
        false,
    )?;
    let summary_path = artifacts_dir.join("distribution-summary.env");

    let payload = schema_v1_payload(
        "effigy.distribution.first-publish.v1",
        json!({
            "ok": true,
            "tag": tag,
            "package_name": distribution_policy.package_name,
            "binary_name": distribution_policy.binary_name,
            "registry_label": distribution_policy.registry_label,
            "crate_version": crate_version,
            "repo_url": repo_url,
            "brew_formula": brew_formula,
            "homebrew_executed": plan.homebrew_executed,
            "homebrew_status": plan.homebrew_status,
            "artifacts_dir": artifacts_dir.display().to_string(),
            "summary_path": summary_path.display().to_string(),
            "log_files": log_files,
        }),
    );
    if output_json {
        return Ok(payload.to_string());
    }

    Ok(format!(
        "[ok] release proof matrix passed\n[ok] artifacts directory: {}\n[ok] artifacts summary: {}",
        artifacts_dir.display(),
        summary_path.display()
    ))
}

struct PlannedCommand {
    label: String,
    program: String,
    args: Vec<String>,
}

impl PlannedCommand {
    fn new(label: impl Into<String>, program: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            label: label.into(),
            program: program.into(),
            args,
        }
    }

    fn into_command(self) -> Command {
        let mut command = Command::new(self.program);
        command.args(self.args);
        command
    }
}

struct FirstPublishPlan {
    crate_bin: PathBuf,
    homebrew_executed: bool,
    homebrew_status: String,
    pre_install_steps: Vec<PlannedCommand>,
    install_step: PlannedCommand,
    post_install_steps: Vec<PlannedCommand>,
    homebrew_steps: Vec<PlannedCommand>,
}

fn build_first_publish_plan(
    repo_root: &Path,
    distribution_policy: &EffectiveDistributionPolicy,
    tag: &str,
    crate_version: &str,
    repo_url: &str,
    brew_formula: &str,
    skip_homebrew: bool,
    work_dir: &Path,
    effigy_bin: &Path,
    brew_available: bool,
) -> FirstPublishPlan {
    let mut pre_install_steps = Vec::new();
    if distribution_policy.verify_tag_install {
        pre_install_steps.push(PlannedCommand::new(
            "tag install validation",
            effigy_bin.display().to_string(),
            vec![
                "release".to_owned(),
                "verify-install".to_owned(),
                "--repo".to_owned(),
                repo_root.display().to_string(),
                "--tag".to_owned(),
                tag.to_owned(),
                "--repo-url".to_owned(),
                repo_url.to_owned(),
            ],
        ));
    }

    let crate_install_root = work_dir.join("crates-install-root");
    let install_step = PlannedCommand::new(
        format!(
            "{} install validation ({crate_version})",
            distribution_policy.registry_label
        ),
        "cargo",
        vec![
            "install".to_owned(),
            distribution_policy.package_name.clone(),
            "--version".to_owned(),
            crate_version.to_owned(),
            "--locked".to_owned(),
            "--root".to_owned(),
            crate_install_root.display().to_string(),
            "--force".to_owned(),
        ],
    );

    let crate_bin = crate_install_root
        .join("bin")
        .join(&distribution_policy.binary_name);
    let mut post_install_steps = vec![PlannedCommand::new(
        format!("{} binary help", distribution_policy.registry_label),
        crate_bin.display().to_string(),
        vec!["--help".to_owned()],
    )];
    if distribution_policy.verify_binary_json_tasks {
        post_install_steps.push(PlannedCommand::new(
            format!("{} binary json tasks", distribution_policy.registry_label),
            crate_bin.display().to_string(),
            vec!["--json".to_owned(), "tasks".to_owned()],
        ));
    }

    let (homebrew_executed, homebrew_status, homebrew_steps) = if skip_homebrew {
        (false, "skipped (--skip-homebrew)".to_owned(), Vec::new())
    } else if !brew_available {
        (false, "skipped (brew not available)".to_owned(), Vec::new())
    } else {
        let mut homebrew_steps = vec![
            PlannedCommand::new(
                "homebrew install",
                "brew",
                vec!["install".to_owned(), brew_formula.to_owned()],
            ),
            PlannedCommand::new(
                "homebrew binary help",
                distribution_policy.binary_name.clone(),
                vec!["--help".to_owned()],
            ),
        ];
        if distribution_policy.verify_binary_json_tasks {
            homebrew_steps.push(PlannedCommand::new(
                "homebrew binary json tasks",
                distribution_policy.binary_name.clone(),
                vec!["--json".to_owned(), "tasks".to_owned()],
            ));
        }
        homebrew_steps.push(PlannedCommand::new(
            "homebrew upgrade",
            "brew",
            vec!["upgrade".to_owned(), "effigy".to_owned()],
        ));
        (true, "executed".to_owned(), homebrew_steps)
    };

    FirstPublishPlan {
        crate_bin,
        homebrew_executed,
        homebrew_status,
        pre_install_steps,
        install_step,
        post_install_steps,
        homebrew_steps,
    }
}

pub fn allocate_distribution_temp_dir(prefix: &str) -> Result<PathBuf, DistributionExecutionError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| DistributionExecutionError::Message(error.to_string()))?
        .as_nanos();
    Ok(std::env::temp_dir().join(format!("{prefix}-{now}")))
}

fn package_name_from_config(config: Option<&ManifestDistributionPackageConfig>) -> String {
    config
        .and_then(|config| config.name.as_ref())
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| DEFAULT_PACKAGE_NAME.to_owned())
}

fn repo_url_from_config(config: Option<&ManifestDistributionPackageConfig>) -> String {
    config
        .and_then(|config| config.repo_url.as_ref())
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| DEFAULT_REPO_URL.to_owned())
}

fn brew_formula_from_config(config: Option<&ManifestDistributionPackageConfig>) -> String {
    config
        .and_then(|config| config.brew_formula.as_ref())
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| DEFAULT_BREW_FORMULA.to_owned())
}

fn binary_name_from_config(
    config: Option<&ManifestDistributionPublishConfig>,
    package_name: &str,
) -> String {
    config
        .and_then(|config| config.binary_name.as_ref())
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| {
            if package_name.trim().is_empty() {
                DEFAULT_BINARY_NAME.to_owned()
            } else {
                package_name.to_owned()
            }
        })
}

fn registry_label_from_config(config: Option<&ManifestDistributionPublishConfig>) -> String {
    config
        .and_then(|config| config.registry_label.as_ref())
        .filter(|value: &&String| !value.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| DEFAULT_REGISTRY_LABEL.to_owned())
}

fn verify_tag_install_from_config(config: Option<&ManifestDistributionPublishConfig>) -> bool {
    config
        .and_then(|config| config.verify_tag_install)
        .unwrap_or(true)
}

fn verify_binary_json_tasks_from_config(
    config: Option<&ManifestDistributionPublishConfig>,
) -> bool {
    config
        .and_then(|config| config.verify_binary_json_tasks)
        .unwrap_or(true)
}

fn docs_task_from_config(config: Option<&ManifestDistributionPreflightConfig>) -> String {
    config
        .and_then(|config| config.docs_task.as_ref())
        .filter(|value: &&String| !value.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| DEFAULT_DOCS_TASK.to_owned())
}

fn smoke_task_from_config(config: Option<&ManifestDistributionPreflightConfig>) -> String {
    config
        .and_then(|config| config.smoke_task.as_ref())
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| DEFAULT_SMOKE_TASK.to_owned())
}

fn required_docs_from_config(
    config: Option<&ManifestDistributionMetadataConfig>,
    manifest_adopted: bool,
) -> Vec<String> {
    config
        .and_then(|config| config.required_docs.clone())
        .unwrap_or_else(|| {
            if manifest_adopted {
                return Vec::new();
            }
            DEFAULT_REQUIRED_DOCS
                .iter()
                .map(|value| (*value).to_owned())
                .collect()
        })
}

fn required_files_from_config(
    config: Option<&ManifestDistributionMetadataConfig>,
    manifest_adopted: bool,
) -> Vec<String> {
    config
        .and_then(|config| config.required_files.clone())
        .unwrap_or_else(|| {
            if manifest_adopted {
                return Vec::new();
            }
            DEFAULT_REQUIRED_FILES
                .iter()
                .map(|value| (*value).to_owned())
                .collect()
        })
}

fn closeout_owner_from_config(config: Option<&ManifestDistributionCloseoutConfig>) -> String {
    config
        .and_then(|config| config.owner.as_ref())
        .filter(|value: &&String| !value.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| DEFAULT_CLOSEOUT_OWNER.to_owned())
}

fn closeout_related_from_config(
    config: Option<&ManifestDistributionCloseoutConfig>,
) -> Option<String> {
    config
        .and_then(|config| config.related.as_ref())
        .map(|value: &String| value.trim())
        .filter(|value: &&str| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn closeout_next_step_from_config(config: Option<&ManifestDistributionCloseoutConfig>) -> String {
    config
        .and_then(|config| config.next_step.as_ref())
        .filter(|value: &&String| !value.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| DEFAULT_CLOSEOUT_NEXT_STEP.to_owned())
}

fn slugify(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_owned()
}

fn run_logged_step(
    artifacts_dir: &Path,
    step_index: &mut usize,
    log_files: &mut Vec<String>,
    label: &str,
    mut command: Command,
) -> Result<(), DistributionExecutionError> {
    *step_index += 1;
    let slug = slugify(label);
    let log_file = format!("{:02}-{slug}.log", *step_index);
    let log_path = artifacts_dir.join(&log_file);
    let output = command
        .output()
        .map_err(|error| DistributionExecutionError::Message(error.to_string()))?;
    let mut rendered = String::new();
    rendered.push_str(&String::from_utf8_lossy(&output.stdout));
    rendered.push_str(&String::from_utf8_lossy(&output.stderr));
    std::fs::write(&log_path, rendered).map_err(|error| DistributionExecutionError::Io {
        path: log_path.clone(),
        error,
    })?;
    log_files.push(log_file);

    if output.status.success() {
        Ok(())
    } else {
        let tail = read_log_tail(&log_path, 40);
        Err(DistributionExecutionError::Message(format!(
            "[error] {label} failed (log: {})\n[error] tail of log:\n{}",
            log_path.display(),
            tail
        )))
    }
}

pub fn find_log_by_pattern(artifacts_dir: &Path, pattern: &str) -> Option<PathBuf> {
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

pub fn command_exists(program: &str) -> bool {
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

/// Collect the GLIBC minor versions referenced by a binary.
///
/// Tries `readelf`, `objdump`, and `strings` in order; stops at the first
/// candidate that returns any `GLIBC_x.y` references. Returned versions
/// are de-duplicated and sorted ascending by numeric comparison.
pub fn collect_glibc_versions(
    binary_path: &Path,
) -> Result<Vec<String>, DistributionExecutionError> {
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
            .map_err(|err| DistributionExecutionError::Message(err.to_string()))?;
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

/// Compare two `x.y` GLIBC version strings numerically.
pub fn compare_glibc_versions(left: &str, right: &str) -> Option<std::cmp::Ordering> {
    let parse = |value: &str| -> Option<(u32, u32)> {
        let mut parts = value.split('.');
        Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
    };
    let left = parse(left)?;
    let right = parse(right)?;
    Some(left.cmp(&right))
}

/// Run the GLIBC floor compatibility check for a distribution binary.
///
/// Reads the dynamic GLIBC symbol requirements of `binary_path`, compares
/// the highest required version against `max_glibc`, and shapes the
/// distribution-check payload and text response. Returns an `Ok` rendered
/// payload/text on compatibility, or an `Err` carrying the same payload /
/// diagnostic text when the floor is violated.
pub fn check_glibc_floor_command(
    binary_path: &Path,
    max_glibc: &str,
    output_json: bool,
) -> Result<String, DistributionExecutionError> {
    if !binary_path.is_file() {
        return Err(DistributionExecutionError::Message(format!(
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

    let payload = schema_v1_payload(
        "effigy.distribution.glibc-floor.v1",
        json!({
            "ok": ok,
            "binary": binary_path.display().to_string(),
            "required_glibc": required_glibc,
            "max_glibc": max_glibc,
            "dynamic_symbols_found": required_glibc.is_some(),
        }),
    );
    if output_json {
        return if ok {
            Ok(payload.to_string())
        } else {
            Err(DistributionExecutionError::Message(payload.to_string()))
        };
    }

    if let Some(required) = required_glibc {
        if ok {
            Ok(format!(
                "{} GLIBC floor is compatible (requires GLIBC_{required}, max GLIBC_{max_glibc})",
                styled_status_prefix("[ok]", Theme::default().success),
            ))
        } else {
            Err(DistributionExecutionError::Message(format!(
                "{} requires GLIBC_{required} but the release floor is GLIBC_{max_glibc}",
                binary_path.display()
            )))
        }
    } else {
        Ok(format!(
            "{} no dynamic GLIBC symbol requirements found: {}",
            styled_status_prefix("[ok]", Theme::default().success),
            binary_path.display()
        ))
    }
}

fn styled_status_prefix(prefix: &str, style: Style) -> String {
    if !resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal()) {
        return prefix.to_owned();
    }
    format!("{}{}{}", style.render(), prefix, style.render_reset())
}

/// Validate distribution-related metadata in `Cargo.toml` against a
/// distribution policy.
///
/// Reads `Cargo.toml`, checks package name / semver version / license /
/// description / required docs / required files / release workflow
/// wiring, and optionally cross-checks the supplied `tag` against the
/// Cargo version. Returns an `Ok` rendered payload/text when all checks
/// pass, or an `Err` with the same shape describing the failures.
pub fn validate_metadata_command(
    repo_root: &Path,
    distribution_policy: &EffectiveDistributionPolicy,
    tag: Option<&str>,
    output_json: bool,
) -> Result<String, DistributionExecutionError> {
    let cargo_path = repo_root.join("Cargo.toml");
    let cargo =
        std::fs::read_to_string(&cargo_path).map_err(|error| DistributionExecutionError::Io {
            path: cargo_path.clone(),
            error,
        })?;
    let cargo: toml::Value = toml::from_str(&cargo).map_err(|error: toml::de::Error| {
        DistributionExecutionError::Message(format!(
            "failed to parse {}: {error}",
            cargo_path.display()
        ))
    })?;
    let package = cargo.get("package").and_then(toml::Value::as_table);
    let workspace_package = cargo
        .get("workspace")
        .and_then(toml::Value::as_table)
        .and_then(|workspace| workspace.get("package"))
        .and_then(toml::Value::as_table);
    if package.is_none() && workspace_package.is_none() {
        return Err(DistributionExecutionError::Message(
            "Cargo.toml is missing [package] or [workspace.package] metadata".to_owned(),
        ));
    }

    // Resolve a string field against `[package]` first, falling back to
    // `[workspace.package]` whenever the package-level entry is missing
    // or is the workspace-inheritance marker `{ workspace = true }`. This
    // matches Cargo's own resolution rule for `version.workspace = true`
    // and friends, so the validator sees the effective value rather than
    // the literal inheritance table.
    let resolve_str = |key: &str| -> Option<String> {
        let from_package = package.and_then(|tbl| tbl.get(key));
        let inherited = from_package
            .and_then(toml::Value::as_table)
            .and_then(|tbl| tbl.get("workspace"))
            .and_then(toml::Value::as_bool)
            == Some(true);
        if !inherited {
            if let Some(value) = from_package.and_then(toml::Value::as_str) {
                return Some(value.to_owned());
            }
        }
        workspace_package
            .and_then(|tbl| tbl.get(key))
            .and_then(toml::Value::as_str)
            .map(str::to_owned)
    };

    let name = resolve_str("name").unwrap_or_else(|| distribution_policy.package_name.clone());
    let version = resolve_str("version").unwrap_or_default();
    let license = resolve_str("license").unwrap_or_default();
    let description = resolve_str("description").unwrap_or_default();

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
        let workflow = std::fs::read_to_string(&workflow_path).map_err(|error| {
            DistributionExecutionError::Io {
                path: workflow_path.clone(),
                error,
            }
        })?;
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
        ] {
            if !workflow.contains(needle) {
                errors.push(format!(
                    "expected {description} in .github/workflows/release-binaries.yml"
                ));
            }
        }
        let linux_glibc_guards = [
            "./effigy-${{ matrix.target }} release check-binary ./effigy-${{ matrix.target }} --glibc-floor 2.35",
            "./effigy-${{ matrix.target }} distribution check-glibc-floor --binary ./effigy-${{ matrix.target }} --max-glibc 2.35",
        ];
        if !linux_glibc_guards
            .iter()
            .any(|needle| workflow.contains(needle))
        {
            errors.push(
                "expected Linux glibc compatibility guard in .github/workflows/release-binaries.yml"
                    .to_owned(),
            );
        }
    }

    let payload = schema_v1_payload(
        "effigy.distribution.metadata.v1",
        json!({
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
        }),
    );

    if output_json {
        return if payload["ok"] == true {
            Ok(payload.to_string())
        } else {
            Err(DistributionExecutionError::Message(payload.to_string()))
        };
    }
    if payload["ok"] == true {
        return Ok("[ok] distribution metadata checks passed".to_owned());
    }
    Err(DistributionExecutionError::Message(errors.join("\n")))
}

/// Run the release preflight sequence: docs task, metadata check,
/// smoke task — each configurable and skippable.
///
/// Invokes `effigy_bin <task> --repo <repo_root>` for docs/smoke tasks and
/// calls [`validate_metadata_command`] for the metadata slice. Optionally
/// writes a key=value status file to `output_path` summarising each slice.
/// Returns the shaped preflight payload (json or text).
pub fn preflight_command(
    repo_root: &Path,
    distribution_policy: &EffectiveDistributionPolicy,
    tag: Option<&str>,
    skip_docs: bool,
    skip_smoke: bool,
    output_path: Option<&Path>,
    effigy_bin: &Path,
    output_json: bool,
) -> Result<String, DistributionExecutionError> {
    let mut docs_status = "skipped";
    let mut smoke_status = "skipped";

    if !skip_docs {
        run_effigy_task(effigy_bin, repo_root, &distribution_policy.docs_task)?;
        docs_status = "ok";
    }

    let _ = validate_metadata_command(repo_root, distribution_policy, tag, false)?;
    let metadata_status = "ok";

    if !skip_smoke {
        run_effigy_task(effigy_bin, repo_root, &distribution_policy.smoke_task)?;
        smoke_status = "ok";
    }

    if let Some(output_path) = output_path {
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| DistributionExecutionError::Io {
                path: parent.to_path_buf(),
                error,
            })?;
        }
        let rendered = format!(
            "TAG={}\nDOCS_STATUS={docs_status}\nMETADATA_STATUS={metadata_status}\nSMOKE_STATUS={smoke_status}\n",
            tag.unwrap_or("")
        );
        std::fs::write(output_path, rendered).map_err(|error| DistributionExecutionError::Io {
            path: output_path.to_path_buf(),
            error,
        })?;
    }

    let next_command = if let Some(tag) = tag {
        format!("effigy release proof --tag {tag} --artifacts-dir ./artifacts/distribution-{tag}")
    } else {
        "effigy release proof --tag vX.Y.Z --artifacts-dir ./artifacts/distribution-vX.Y.Z"
            .to_owned()
    };

    let payload = schema_v1_payload(
        "effigy.distribution.preflight.v1",
        json!({
            "ok": true,
            "tag": tag,
            "docs_status": docs_status,
            "metadata_status": metadata_status,
            "smoke_status": smoke_status,
            "output": output_path.map(|path| path.display().to_string()),
            "next_command": next_command,
        }),
    );
    if output_json {
        return Ok(payload.to_string());
    }

    let mut lines = Vec::new();
    if let Some(output_path) = output_path {
        lines.push(format!(
            "[ok] wrote preflight summary: {}",
            output_path.display()
        ));
    }
    lines.push("[ok] release preflight checks passed".to_owned());
    lines.push(format!("[next] real publish-cycle command: {next_command}"));
    Ok(lines.join("\n"))
}

fn run_effigy_task(
    effigy_bin: &Path,
    repo_root: &Path,
    task: &str,
) -> Result<(), DistributionExecutionError> {
    let output = Command::new(effigy_bin)
        .arg(task)
        .arg("--repo")
        .arg(repo_root)
        .env("NO_COLOR", "1")
        .output()
        .map_err(|err| {
            DistributionExecutionError::Message(format!("failed to run `{task}`: {err}"))
        })?;
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
    Err(DistributionExecutionError::Message(format!(
        "`{task}` failed\n{combined}"
    )))
}

#[cfg(test)]
mod tests;
