use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use effigy_core::git_exec::run_git_output;
use effigy_core::git_source::{canonical_git_cache_identity, sanitize_cache_segment, sha256_hex};
use effigy_rhai::RhaiSecretPolicy;
use effigy_rhai::RhaiSecretTarget;
use effigy_ui::steps::ProgressSource;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::runner::error::RunnerError;
use crate::runner::script_command::execute_repo_rhai_script_with_secret_policy;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestDeployProviderConfig {
    pub source: DeployProviderSource,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub(super) enum DeployProviderSource {
    Path {
        dir: String,
    },
    Git {
        url: String,
        #[serde(default)]
        r#ref: Option<String>,
        /// Full commit SHA. A pinned provider is reused from the cache without
        /// any network access; a floating `ref` is only refreshed online.
        #[serde(default)]
        rev: Option<String>,
    },
    Oci {
        url: String,
    },
}

/// Whether resolving a provider package may use the network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DeployProviderFetch {
    /// Use the cached checkout only: never clone or fetch. A working tree can
    /// still be materialized from objects the cache already holds.
    Offline,
    /// Refresh the cached checkout from its origin when it is not already at
    /// the requested commit.
    Online,
}

/// Where a resolved provider package came from, and exactly which commit ran.
#[derive(Debug, Clone, Serialize)]
pub(super) struct DeployProviderProvenance {
    /// `path` or `git`.
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_rev: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// `path`, `cached` or `fetched`.
    pub resolution: String,
}

impl DeployProviderProvenance {
    pub(super) fn path() -> Self {
        Self {
            kind: "path".to_owned(),
            url: None,
            requested_ref: None,
            requested_rev: None,
            commit: None,
            resolution: "path".to_owned(),
        }
    }

    /// Short human label for the resolved commit and how it was obtained.
    pub(super) fn describe(&self) -> String {
        match (&self.commit, self.resolution.as_str()) {
            (Some(commit), "cached") => format!("cached checkout at commit {commit}"),
            (Some(commit), "fetched") => format!("fetched commit {commit}"),
            (Some(commit), _) => format!("commit {commit}"),
            (None, "path") => "path source".to_owned(),
            (None, resolution) => format!("unresolved {resolution} source"),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct DeployProviderPackage {
    pub root: PathBuf,
    pub descriptor: DeployProviderDescriptor,
    pub provenance: DeployProviderProvenance,
    /// Non-fatal source warnings, for example a floating provider ref.
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DeployProviderDescriptor {
    pub provider: DeployProviderMetadata,
    #[serde(default)]
    pub capabilities: DeployProviderCapabilities,
    #[serde(default)]
    pub policy: DeployProviderPolicy,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DeployProviderMetadata {
    pub schema: String,
    pub name: String,
    pub display_name: String,
    pub version: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(super) struct DeployProviderCapabilities {
    #[serde(default)]
    pub checklist: Option<String>,
    #[serde(default)]
    pub export: Option<String>,
    #[serde(default)]
    pub validate: Option<String>,
    #[serde(default)]
    pub preflight: Option<String>,
    #[serde(default)]
    pub apply: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(super) struct DeployProviderPolicy {
    #[serde(default)]
    pub creates_projects: bool,
    #[serde(default)]
    pub creates_services: bool,
    #[serde(default)]
    pub creates_resources: bool,
    #[serde(default)]
    pub creates_variables: bool,
    #[serde(default)]
    pub creates_domains: bool,
    #[serde(default)]
    pub prints_secret_values: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(super) struct DeployProviderPhaseReport {
    pub schema: String,
    pub phase: String,
    pub provider: String,
    pub status: DeployProviderReportStatus,
    #[serde(default)]
    pub checks: Vec<DeployProviderPhaseCheck>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub blockers: Vec<String>,
    #[serde(default)]
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(super) struct DeployProviderPhaseCheck {
    pub name: String,
    pub status: DeployProviderCheckStatus,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(super) enum DeployProviderReportStatus {
    Planned,
    Ok,
    Warning,
    Blocked,
    Succeeded,
    Failed,
    Skipped,
}

impl DeployProviderReportStatus {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Ok => "ok",
            Self::Warning => "warning",
            Self::Blocked => "blocked",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }

    pub(super) fn allows_preflight_progress(self) -> bool {
        matches!(self, Self::Planned | Self::Ok)
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(super) enum DeployProviderCheckStatus {
    Planned,
    Ok,
    Warning,
    Blocked,
    Succeeded,
    Failed,
    Skipped,
}

impl DeployProviderCheckStatus {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Ok => "ok",
            Self::Warning => "warning",
            Self::Blocked => "blocked",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }
}

/// Resolve a provider package. A git source can fetch for seconds on a cold
/// cache, so it draws a live step: `resolving deploy provider acme (offline)`,
/// waiting on the exact source and pin. A path source resolves at once and
/// ends in one line. Its failure is what a deploy refuses on.
pub(super) fn resolve_provider_package(
    repo_root: &Path,
    provider_name: &str,
    providers: &BTreeMap<String, ManifestDeployProviderConfig>,
    fetch: DeployProviderFetch,
) -> Result<Option<DeployProviderPackage>, RunnerError> {
    validate_provider_name(provider_name)?;
    let Some(config) = providers.get(provider_name) else {
        return Ok(None);
    };
    let step = match &config.source {
        DeployProviderSource::Git { url, r#ref, rev } => {
            let progress = provider_fetch_progress(
                provider_name,
                url,
                r#ref.as_deref(),
                rev.as_deref(),
                fetch,
            );
            Some(effigy_ui::steps::live_step(progress))
        }
        DeployProviderSource::Path { .. } | DeployProviderSource::Oci { .. } => None,
    };
    let result = resolve_provider_package_inner(repo_root, provider_name, config, fetch);
    let (ok, summary) = match &result {
        Ok(package) => (
            true,
            provider_resolved_summary(provider_name, &package.provenance),
        ),
        Err(error) => (
            false,
            format!(
                "could not resolve deploy provider `{provider_name}`: {}",
                error.human_error().what_failed()
            ),
        ),
    };
    match step {
        Some(step) if ok => step.succeed(&summary),
        Some(step) => step.fail(&summary),
        None => effigy_ui::steps::finish_line(ok, &summary),
    }
    result.map(Some)
}

/// The live state of a git provider fetch: the current action names the
/// policy in force, and the wait names the exact source and pin.
fn provider_fetch_progress(
    provider_name: &str,
    url: &str,
    reference: Option<&str>,
    rev: Option<&str>,
    fetch: DeployProviderFetch,
) -> ProgressSource {
    let progress = ProgressSource::waiting_for(provider_source_awaiting(url, reference, rev));
    progress.update(resolving_provider_label(provider_name, fetch), None, None);
    progress
}

/// What a git provider fetch waits on: `git source <url> at rev <sha>`, or the
/// ref, or the default branch when the source names neither.
fn provider_source_awaiting(url: &str, reference: Option<&str>, rev: Option<&str>) -> String {
    let pin = match (
        rev,
        reference.map(str::trim).filter(|value| !value.is_empty()),
    ) {
        (Some(rev), _) => format!("rev {rev}"),
        (None, Some(reference)) => format!("ref {reference}"),
        (None, None) => "the default branch".to_owned(),
    };
    format!("git source {url} at {pin}")
}

/// `resolving deploy provider acme (offline)`: the label says which source
/// policy is in force, because "offline" explains a cache miss.
pub(super) fn resolving_provider_label(provider_name: &str, fetch: DeployProviderFetch) -> String {
    let mode = match fetch {
        DeployProviderFetch::Offline => "offline",
        DeployProviderFetch::Online => "online",
    };
    format!("resolving deploy provider {provider_name} ({mode})")
}

/// One line naming where the provider came from and exactly which commit ran.
pub(super) fn provider_resolved_summary(
    provider_name: &str,
    provenance: &DeployProviderProvenance,
) -> String {
    format!(
        "resolved deploy provider `{provider_name}` from {}",
        provenance.describe()
    )
}

fn resolve_provider_package_inner(
    repo_root: &Path,
    provider_name: &str,
    config: &ManifestDeployProviderConfig,
    fetch: DeployProviderFetch,
) -> Result<DeployProviderPackage, RunnerError> {
    let mut warnings = Vec::new();
    let (root, provenance) = match &config.source {
        DeployProviderSource::Path { dir } => {
            (repo_root.join(dir), DeployProviderProvenance::path())
        }
        DeployProviderSource::Git { url, r#ref, rev } => resolve_git_provider_source(
            repo_root,
            provider_name,
            url,
            r#ref.as_deref(),
            rev.as_deref(),
            fetch,
            &mut warnings,
        )?,
        DeployProviderSource::Oci { url } => {
            return Err(RunnerError::task_invocation(format!(
                "deploy provider `{provider_name}` uses OCI source `{url}`, which Effigy cannot materialize yet\nnext: declare a `path` or `git` source for `{provider_name}` in `[deploy.providers]`"
            )));
        }
    };
    let descriptor = read_provider_descriptor(&root)?;
    if descriptor.provider.schema != "effigy.deploy-provider.v1" {
        return Err(RunnerError::task_invocation(format!(
            "deploy provider `{provider_name}` declares descriptor schema `{}`, not `effigy.deploy-provider.v1`\nnext: update the provider package at {} to the v1 descriptor",
            descriptor.provider.schema,
            root.display()
        )));
    }
    if descriptor.provider.name != provider_name {
        return Err(RunnerError::task_invocation(format!(
            "deploy provider `{provider_name}` resolved a descriptor that names itself `{}`\nnext: make `[deploy.providers.{provider_name}]` point at that provider's package, or fix the descriptor's `provider.name`",
            descriptor.provider.name
        )));
    }
    validate_capability_paths(&root, provider_name, &descriptor.capabilities)?;
    Ok(DeployProviderPackage {
        root,
        descriptor,
        provenance,
        warnings,
    })
}

pub(super) fn run_provider_preflight(
    repo_root: &Path,
    provider_name: &str,
    package: &DeployProviderPackage,
    context: Value,
) -> Result<Option<DeployProviderPhaseReport>, RunnerError> {
    let Some(script) = &package.descriptor.capabilities.preflight else {
        return Ok(None);
    };
    run_provider_phase(
        repo_root,
        provider_name,
        package,
        "preflight",
        script,
        context,
        true,
    )
}

pub(super) fn run_provider_status(
    repo_root: &Path,
    provider_name: &str,
    package: &DeployProviderPackage,
    context: Value,
) -> Result<Option<DeployProviderPhaseReport>, RunnerError> {
    let Some(script) = &package.descriptor.capabilities.status else {
        return Ok(None);
    };
    run_provider_phase(
        repo_root,
        provider_name,
        package,
        "status",
        script,
        context,
        false,
    )
}

pub(super) fn run_provider_export(
    repo_root: &Path,
    provider_name: &str,
    package: &DeployProviderPackage,
    context: Value,
    plan: bool,
) -> Result<DeployProviderPhaseReport, RunnerError> {
    let Some(script) = &package.descriptor.capabilities.export else {
        return Err(RunnerError::task_invocation(format!(
            "deploy provider `{provider_name}` package does not declare an export capability"
        )));
    };
    run_provider_phase(
        repo_root,
        provider_name,
        package,
        "export",
        script,
        context,
        plan,
    )?
    .ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "deploy provider `{provider_name}` export capability did not produce a report"
        ))
    })
}

/// `plan` phases are previews: they run with no secret access at all, so a
/// provider script that asks for a secret fails instead of reaching the vault.
fn run_provider_phase(
    repo_root: &Path,
    provider_name: &str,
    package: &DeployProviderPackage,
    phase: &str,
    script: &str,
    context: Value,
    plan: bool,
) -> Result<Option<DeployProviderPhaseReport>, RunnerError> {
    let _guard = provider_script_env_lock().lock().map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to lock deploy provider script environment: {error}"
        ))
    })?;
    let workspace = provider_phase_workspace(repo_root, provider_name, phase)?;
    let refusal = plan_mode_secret_refusal(provider_name, phase);
    let secret_policy = if plan {
        RhaiSecretPolicy::Refuse(&refusal)
    } else {
        RhaiSecretPolicy::Targets(&[RhaiSecretTarget::Deploy])
    };
    let context_path = workspace.path.join("context.json");
    let report_path = workspace.path.join("report.json");
    write_provider_json(&context_path, &context)?;
    let _env_guard = ScopedProviderEnvOverride::set(&[
        (
            "EFFIGY_DEPLOY_PROVIDER_CONTEXT",
            context_path.display().to_string(),
        ),
        (
            "EFFIGY_DEPLOY_PROVIDER_REPORT",
            report_path.display().to_string(),
        ),
    ]);
    execute_repo_rhai_script_with_secret_policy(
        repo_root,
        &format!("deploy-provider:{provider_name}:{phase}"),
        &package.root.join(script),
        &[],
        secret_policy,
    )?;
    if !report_path.is_file() {
        return Err(RunnerError::task_invocation(format!(
            "deploy provider `{provider_name}` {phase} script did not write a provider report"
        )));
    }
    let raw = std::fs::read_to_string(&report_path).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to read deploy provider `{provider_name}` {phase} report {}: {error}",
            report_path.display()
        ))
    })?;
    let report = serde_json::from_str::<DeployProviderPhaseReport>(&raw).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to parse deploy provider `{provider_name}` {phase} report: {error}"
        ))
    })?;
    validate_provider_phase_report(provider_name, phase, &report)?;
    Ok(Some(report))
}

fn provider_script_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// A provider phase's context/report directory. It lives under the checkout's
/// v0.15 state root, not the retired `.effigy/runtime/` tree, and the guard
/// removes it when the phase ends, whatever the outcome.
struct ProviderPhaseWorkspace {
    path: PathBuf,
}

impl Drop for ProviderPhaseWorkspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Leftovers exist only when a previous process died mid-phase. The sweep is
/// bounded like other per-checkout cleanup and only removes Effigy's own
/// `<millis>-<phase>` directories, never anything else under the provider root.
const PROVIDER_PHASE_RETENTION: std::time::Duration = std::time::Duration::from_secs(3600);
const PROVIDER_PHASE_SWEEP_LIMIT: usize = 32;

fn provider_phase_workspace(
    repo_root: &Path,
    provider_name: &str,
    phase: &str,
) -> Result<ProviderPhaseWorkspace, RunnerError> {
    let provider_dir = repo_root
        .join(".effigy/state/deploy/provider")
        .join(provider_name);
    sweep_stale_provider_phases(&provider_dir);
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    let path = provider_dir.join(format!("{millis}-{phase}"));
    std::fs::create_dir_all(&path).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to create deploy provider `{provider_name}` {phase} workspace {}: {error}",
            path.display()
        ))
    })?;
    Ok(ProviderPhaseWorkspace { path })
}

fn sweep_stale_provider_phases(provider_dir: &Path) {
    let Ok(entries) = std::fs::read_dir(provider_dir) else {
        return;
    };
    let now = SystemTime::now();
    let mut removed = 0;
    for entry in entries.flatten() {
        if removed >= PROVIDER_PHASE_SWEEP_LIMIT {
            return;
        }
        if !is_provider_phase_workspace_name(&entry.file_name().to_string_lossy()) {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if !metadata.is_dir() {
            continue;
        }
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        let stale = now
            .duration_since(modified)
            .map(|age| age >= PROVIDER_PHASE_RETENTION)
            .unwrap_or(false);
        if stale && std::fs::remove_dir_all(entry.path()).is_ok() {
            removed += 1;
        }
    }
}

fn is_provider_phase_workspace_name(name: &str) -> bool {
    match name.split_once('-') {
        Some((millis, phase)) => {
            !millis.is_empty()
                && millis.bytes().all(|byte| byte.is_ascii_digit())
                && matches!(
                    phase,
                    "preflight" | "export" | "validate" | "status" | "apply"
                )
        }
        None => false,
    }
}

/// Plan-mode phases receive no secrets. The refusal names the phase and the
/// requested secret; the vault and manifest are never read.
fn plan_mode_secret_refusal(provider_name: &str, phase: &str) -> String {
    format!(
        "deploy provider `{provider_name}` {phase} phase runs in plan mode and receives no secret values"
    )
}

fn write_provider_json(path: &Path, value: &Value) -> Result<(), RunnerError> {
    let raw = serde_json::to_string_pretty(value).map_err(|error| {
        RunnerError::task_invocation(format!("failed to encode deploy provider context: {error}"))
    })?;
    std::fs::write(path, raw).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to write deploy provider context {}: {error}",
            path.display()
        ))
    })
}

fn validate_provider_phase_report(
    provider_name: &str,
    phase: &str,
    report: &DeployProviderPhaseReport,
) -> Result<(), RunnerError> {
    if report.schema != "effigy.deploy-provider.report.v1" {
        return Err(RunnerError::task_invocation(format!(
            "deploy provider `{provider_name}` {phase} report has unsupported schema `{}`",
            report.schema
        )));
    }
    if report.provider != provider_name {
        return Err(RunnerError::task_invocation(format!(
            "deploy provider `{provider_name}` {phase} report declares provider `{}`",
            report.provider
        )));
    }
    if report.phase != phase {
        return Err(RunnerError::task_invocation(format!(
            "deploy provider `{provider_name}` {phase} report declares phase `{}`",
            report.phase
        )));
    }
    validate_provider_report_status(provider_name, phase, report)?;
    Ok(())
}

fn validate_provider_report_status(
    provider_name: &str,
    phase: &str,
    report: &DeployProviderPhaseReport,
) -> Result<(), RunnerError> {
    let valid = match phase {
        "preflight" => matches!(
            report.status,
            DeployProviderReportStatus::Planned
                | DeployProviderReportStatus::Ok
                | DeployProviderReportStatus::Warning
                | DeployProviderReportStatus::Blocked
                | DeployProviderReportStatus::Failed
                | DeployProviderReportStatus::Skipped
        ),
        "apply" => matches!(
            report.status,
            DeployProviderReportStatus::Succeeded
                | DeployProviderReportStatus::Failed
                | DeployProviderReportStatus::Skipped
        ),
        "status" => matches!(
            report.status,
            DeployProviderReportStatus::Planned
                | DeployProviderReportStatus::Ok
                | DeployProviderReportStatus::Warning
                | DeployProviderReportStatus::Blocked
                | DeployProviderReportStatus::Failed
                | DeployProviderReportStatus::Skipped
        ),
        "export" => matches!(
            report.status,
            DeployProviderReportStatus::Planned
                | DeployProviderReportStatus::Succeeded
                | DeployProviderReportStatus::Warning
                | DeployProviderReportStatus::Failed
                | DeployProviderReportStatus::Skipped
        ),
        _ => true,
    };
    if valid {
        return Ok(());
    }
    Err(RunnerError::task_invocation(format!(
        "deploy provider `{provider_name}` {phase} report returned unsupported status `{}`",
        report.status.as_str()
    )))
}

fn read_provider_descriptor(root: &Path) -> Result<DeployProviderDescriptor, RunnerError> {
    let path = root.join("provider.toml");
    let raw = std::fs::read_to_string(&path).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to read deploy provider descriptor {}: {error}",
            path.display()
        ))
    })?;
    toml::from_str(&raw).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to parse deploy provider descriptor {}: {error}",
            path.display()
        ))
    })
}

fn validate_capability_paths(
    root: &Path,
    provider_name: &str,
    capabilities: &DeployProviderCapabilities,
) -> Result<(), RunnerError> {
    for (phase, path) in [
        ("checklist", &capabilities.checklist),
        ("export", &capabilities.export),
        ("validate", &capabilities.validate),
        ("preflight", &capabilities.preflight),
        ("apply", &capabilities.apply),
        ("status", &capabilities.status),
    ] {
        let Some(path) = path else {
            continue;
        };
        validate_provider_relative_path(provider_name, phase, path)?;
        let resolved = root.join(path);
        if !resolved.is_file() {
            return Err(RunnerError::task_invocation(format!(
                "deploy provider `{provider_name}` capability `{phase}` points to missing script {}",
                resolved.display()
            )));
        }
    }
    Ok(())
}

fn validate_provider_name(provider_name: &str) -> Result<(), RunnerError> {
    let valid = !provider_name.is_empty()
        && provider_name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
    if valid {
        return Ok(());
    }
    Err(RunnerError::task_invocation(format!(
        "deploy provider `{provider_name}` has an invalid id; use ASCII letters, numbers, `.`, `_`, or `-`"
    )))
}

fn validate_provider_relative_path(
    provider_name: &str,
    phase: &str,
    path: &str,
) -> Result<(), RunnerError> {
    let candidate = Path::new(path);
    let valid = !candidate.is_absolute()
        && candidate
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)));
    if valid {
        return Ok(());
    }
    Err(RunnerError::task_invocation(format!(
        "deploy provider `{provider_name}` capability `{phase}` must be a package-relative file path"
    )))
}

fn resolve_git_provider_source(
    repo_root: &Path,
    provider_name: &str,
    url: &str,
    reference: Option<&str>,
    rev: Option<&str>,
    fetch: DeployProviderFetch,
    warnings: &mut Vec<String>,
) -> Result<(PathBuf, DeployProviderProvenance), RunnerError> {
    let reference = reference.map(str::trim).filter(|value| !value.is_empty());
    let pinned = rev.map(|rev| pinned_rev(provider_name, rev)).transpose()?;
    let cache_key = match (&pinned, reference) {
        (Some(rev), _) => rev.clone(),
        (None, Some(reference)) => reference.to_owned(),
        (None, None) => "main".to_owned(),
    };
    if pinned.is_none() {
        let floating = reference.unwrap_or("main");
        warnings.push(format!(
            "deploy provider `{provider_name}` source uses floating ref `{floating}`; pin `rev` to a full commit SHA so plans and exports are reproducible"
        ));
    }
    let local_path = repo_root
        .join(".effigy")
        .join("cache/providers/git")
        .join(sha256_hex(canonical_git_cache_identity(url).as_bytes()))
        .join(sanitize_cache_segment(&cache_key));
    let resolution = ensure_git_checkout(GitCheckoutRequest {
        repo_root,
        provider_name,
        url,
        reference,
        pinned: pinned.as_deref(),
        fetch,
        local_path: &local_path,
    })?;
    let commit = git_head(&local_path)?;
    if let Some(pinned) = &pinned {
        if commit.as_deref() != Some(pinned.as_str()) {
            return Err(RunnerError::task_invocation(format!(
                "deploy provider `{provider_name}` checkout at {} resolved commit {} instead of pinned rev {pinned}",
                local_path.display(),
                commit.as_deref().unwrap_or("<none>")
            )));
        }
    }
    Ok((
        local_path,
        DeployProviderProvenance {
            kind: "git".to_owned(),
            url: Some(url.to_owned()),
            requested_ref: reference.map(str::to_owned),
            requested_rev: pinned,
            commit,
            resolution: resolution.as_str().to_owned(),
        },
    ))
}

fn pinned_rev(provider_name: &str, rev: &str) -> Result<String, RunnerError> {
    let rev = rev.trim().to_ascii_lowercase();
    let full_sha = matches!(rev.len(), 40 | 64) && rev.bytes().all(|byte| byte.is_ascii_hexdigit());
    if full_sha {
        return Ok(rev);
    }
    Err(RunnerError::task_invocation(format!(
        "deploy provider `{provider_name}` source `rev` must be a full commit SHA (40 hex characters), not `{rev}`; use `ref` for a moving branch or tag"
    )))
}

/// How a cached checkout was obtained for this resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GitCheckoutResolution {
    Cached,
    Fetched,
}

impl GitCheckoutResolution {
    fn as_str(self) -> &'static str {
        match self {
            Self::Cached => "cached",
            Self::Fetched => "fetched",
        }
    }
}

struct GitCheckoutRequest<'a> {
    repo_root: &'a Path,
    provider_name: &'a str,
    url: &'a str,
    reference: Option<&'a str>,
    pinned: Option<&'a str>,
    fetch: DeployProviderFetch,
    local_path: &'a Path,
}

fn ensure_git_checkout(
    request: GitCheckoutRequest<'_>,
) -> Result<GitCheckoutResolution, RunnerError> {
    let GitCheckoutRequest {
        repo_root,
        provider_name,
        url,
        reference,
        pinned,
        fetch,
        local_path,
    } = request;
    if local_path.exists() && !local_path.is_dir() {
        return Err(RunnerError::task_invocation(format!(
            "deploy provider `{provider_name}` cache path exists but is not a directory: {}",
            local_path.display()
        )));
    }
    let fresh_clone = !local_path.join(".git").is_dir();
    if fetch == DeployProviderFetch::Offline {
        if fresh_clone {
            return Err(RunnerError::task_invocation(offline_provider_message(
                provider_name,
                local_path,
                "there is no cached checkout for it",
            )));
        }
        let Some(pinned) = pinned else {
            // A floating preview uses whatever the cache holds. Materializing the
            // working tree from local objects needs no network.
            materialize_checkout(local_path, None)?;
            return Ok(GitCheckoutResolution::Cached);
        };
        let head = git_head(local_path)?;
        if head.as_deref() == Some(pinned) {
            materialize_checkout(local_path, None)?;
            return Ok(GitCheckoutResolution::Cached);
        }
        if git_rev_exists(local_path, pinned) {
            materialize_checkout(local_path, Some(pinned))?;
            return Ok(GitCheckoutResolution::Cached);
        }
        return Err(RunnerError::task_invocation(offline_provider_message(
            provider_name,
            local_path,
            &format!(
                "the cached checkout is at commit {}, not the pinned rev {pinned}",
                head.as_deref().unwrap_or("<none>")
            ),
        )));
    }
    if fresh_clone {
        if let Some(parent) = local_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                RunnerError::task_invocation(format!(
                    "failed to create deploy provider cache directory {}: {error}",
                    parent.display()
                ))
            })?;
        }
        run_git(
            repo_root,
            None,
            &["clone", "--no-checkout", url, &local_path.to_string_lossy()],
        )?;
    } else {
        run_git(
            local_path,
            Some(local_path),
            &["remote", "set-url", "origin", url],
        )?;
    }
    let Some(pinned) = pinned else {
        let reference = reference.unwrap_or("main");
        run_git(
            local_path,
            Some(local_path),
            &["fetch", "--depth", "1", "origin", reference],
        )?;
        run_git(
            local_path,
            Some(local_path),
            &["checkout", "--detach", "FETCH_HEAD"],
        )?;
        return Ok(GitCheckoutResolution::Fetched);
    };
    if !fresh_clone && git_head(local_path)?.as_deref() == Some(pinned) {
        materialize_checkout(local_path, None)?;
        return Ok(GitCheckoutResolution::Cached);
    }
    // A fresh clone carries every object, so the pin needs no fetch. Otherwise
    // fetch the configured ref when it can carry the pin, then the pin itself;
    // a server that refuses an unadvertised SHA still succeeds after the ref
    // fetch brought the commit with it.
    if !git_rev_exists(local_path, pinned) {
        let fetch_source = reference.unwrap_or(pinned);
        let fetched_ref = run_git(
            local_path,
            Some(local_path),
            &["fetch", "--depth", "1", "origin", fetch_source],
        )
        .is_ok();
        if !fetched_ref || !git_rev_exists(local_path, pinned) {
            run_git(
                local_path,
                Some(local_path),
                &["fetch", "--depth", "1", "origin", pinned],
            )?;
        }
    }
    materialize_checkout(local_path, Some(pinned))?;
    Ok(GitCheckoutResolution::Fetched)
}

/// Materialize the working tree for `commit` (or the cached HEAD) when a
/// previous run left the checkout empty. `provider.toml` is the package root's
/// required file, so for an already-correct HEAD its absence proves the working
/// tree is not materialized. An explicit commit is always checked out because
/// the cache may hold another commit's files.
fn materialize_checkout(local_path: &Path, commit: Option<&str>) -> Result<(), RunnerError> {
    if commit.is_none() && local_path.join("provider.toml").is_file() {
        return Ok(());
    }
    let target = commit.unwrap_or("HEAD");
    run_git(
        local_path,
        Some(local_path),
        &["checkout", "--detach", target],
    )
}

fn offline_provider_message(provider_name: &str, local_path: &Path, detail: &str) -> String {
    format!(
        "deploy provider `{provider_name}` cannot be resolved offline: {detail} at {}. Plan previews never fetch. Populate the cache with a networked `effigy deploy export {provider_name} --path <DIR>` (without `--plan`) or `effigy deploy status <ENV>` run, or configure a `path` source",
        local_path.display()
    )
}

fn git_head(local_path: &Path) -> Result<Option<String>, RunnerError> {
    let output = run_git_output(
        Some(local_path),
        &["rev-parse", "--verify", "--quiet", "HEAD"],
        |error| {
            RunnerError::task_invocation(format!(
                "failed to run git for deploy provider package in {}: {error}",
                local_path.display()
            ))
        },
        |detail| RunnerError::task_invocation(format!("{detail} for deploy provider package")),
    );
    match output {
        Ok(output) => {
            let head = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            Ok((!head.is_empty()).then_some(head))
        }
        Err(_) => Ok(None),
    }
}

fn git_rev_exists(local_path: &Path, reference: &str) -> bool {
    run_git_output(
        Some(local_path),
        &["rev-parse", "--verify", "--quiet", reference],
        |_| (),
        |_| (),
    )
    .is_ok()
}

fn run_git(manifest_root: &Path, cwd: Option<&Path>, args: &[&str]) -> Result<(), RunnerError> {
    run_git_output(
        cwd,
        args,
        |error| {
            RunnerError::task_invocation(format!(
                "failed to run git for deploy provider package in {}: {error}",
                manifest_root.display()
            ))
        },
        |detail| RunnerError::task_invocation(format!("{detail} for deploy provider package")),
    )
    .map(|_| ())
}

struct ScopedProviderEnvOverride {
    previous: Vec<(&'static str, Option<String>)>,
}

impl ScopedProviderEnvOverride {
    fn set(values: &[(&'static str, String)]) -> Self {
        let previous = values
            .iter()
            .map(|(key, _)| (*key, std::env::var(key).ok()))
            .collect::<Vec<_>>();
        for (key, value) in values {
            std::env::set_var(key, value);
        }
        Self { previous }
    }
}

impl Drop for ScopedProviderEnvOverride {
    fn drop(&mut self) {
        for (key, value) in self.previous.iter().rev() {
            if let Some(value) = value {
                std::env::set_var(key, value);
            } else {
                std::env::remove_var(key);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        provider_fetch_progress, provider_resolved_summary, resolving_provider_label,
        DeployProviderFetch, DeployProviderProvenance,
    };
    use std::time::Duration;

    #[test]
    fn provider_resolution_names_the_provider_and_the_source_policy() {
        assert_eq!(
            resolving_provider_label("acme", DeployProviderFetch::Offline),
            "resolving deploy provider acme (offline)"
        );
        assert_eq!(
            resolving_provider_label("acme", DeployProviderFetch::Online),
            "resolving deploy provider acme (online)"
        );
    }

    #[test]
    fn the_resolution_line_names_the_exact_commit() {
        let provenance = DeployProviderProvenance {
            kind: "git".to_owned(),
            url: Some("https://example.test/acme.git".to_owned()),
            requested_ref: Some("main".to_owned()),
            requested_rev: None,
            commit: Some("1a2b3c4d".to_owned()),
            resolution: "cached".to_owned(),
        };
        assert_eq!(
            provider_resolved_summary("acme", &provenance),
            "resolved deploy provider `acme` from cached checkout at commit 1a2b3c4d"
        );
    }

    #[test]
    fn a_fetch_frame_names_the_exact_source_and_pin_as_time_passes() {
        let url = "https://example.test/acme.git";
        let pinned = provider_fetch_progress(
            "acme",
            url,
            Some("main"),
            Some("1a2b3c4d5e6f70819a2b3c4d5e6f70819a2b3c4d"),
            DeployProviderFetch::Offline,
        );
        // One fetch, read twice: the elapsed wait moves for the same source.
        assert_eq!(
            pinned.frame_at(Duration::from_secs(5)),
            "resolving deploy provider acme (offline); waiting for git source https://example.test/acme.git at rev 1a2b3c4d5e6f70819a2b3c4d5e6f70819a2b3c4d; 5s elapsed"
        );
        assert_eq!(
            pinned.frame_at(Duration::from_secs(45)),
            "resolving deploy provider acme (offline); waiting for git source https://example.test/acme.git at rev 1a2b3c4d5e6f70819a2b3c4d5e6f70819a2b3c4d; 45s elapsed"
        );
        let floating = provider_fetch_progress(
            "acme",
            url,
            Some(" main "),
            None,
            DeployProviderFetch::Online,
        );
        assert_eq!(
            floating.frame_at(Duration::from_secs(2)),
            "resolving deploy provider acme (online); waiting for git source https://example.test/acme.git at ref main; 2s elapsed"
        );
        assert_eq!(
            floating.frame_at(Duration::from_secs(60)),
            "resolving deploy provider acme (online); waiting for git source https://example.test/acme.git at ref main; 60s elapsed"
        );
    }
}
