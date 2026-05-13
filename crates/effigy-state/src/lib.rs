use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use effigy_artifacts::{ArtifactKind, ArtifactRefError, ArtifactSourceRef};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const STATE_STACK_SCHEMA: &str = "effigy.state-stack.v1";
pub const STATE_STACK_LINEAGE_SCHEMA: &str = "effigy.state-stack.lineage.v1";
pub const STATE_STACK_APPLY_SCHEMA: &str = "effigy.state-stack.apply.v1";
pub const STATE_STACK_CAPTURE_SCHEMA: &str = "effigy.state-stack.capture.v1";
pub const STATE_STACK_CAPTURE_SET_SCHEMA: &str = "effigy.state-stack.capture-set.v1";
pub const STATE_STACK_HISTORY_SCHEMA: &str = "effigy.state-stack.history.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateStackManifest {
    pub schema: String,
    pub name: String,
    pub environment: StateEnvironment,
    pub layers: Vec<StateStackLayer>,
}

impl StateStackManifest {
    pub fn parse_toml(input: &str) -> Result<Self, StateStackParseError> {
        toml::from_str(input).map_err(StateStackParseError::Toml)
    }

    pub fn plan_lineage(&self) -> Result<StateStackLineagePlan, StateStackValidationError> {
        validate_state_stack(self)?;
        Ok(StateStackLineagePlan::from_manifest(self))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateStackLayer {
    pub key: String,
    pub role: StateLayerRole,
    pub source: String,
    pub apply_mode: StateLayerApplyMode,
    pub environment_policy: StateLayerEnvironmentPolicy,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub artifact_kind: Option<ArtifactKind>,
    #[serde(default)]
    pub snapshot_identity: Option<String>,
    #[serde(default)]
    pub hook: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default, alias = "target")]
    pub sql_target: Option<String>,
}

impl StateStackLayer {
    pub fn artifact_source(&self) -> Result<Option<ArtifactSourceRef>, ArtifactRefError> {
        match self.apply_mode {
            StateLayerApplyMode::Artifact | StateLayerApplyMode::Sql => {
                ArtifactSourceRef::parse(&self.source).map(Some)
            }
            StateLayerApplyMode::Task
            | StateLayerApplyMode::Manual
            | StateLayerApplyMode::Checkpoint => Ok(None),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StateEnvironment {
    Dev,
    Uat,
    Production,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StateLayerRole {
    Structure,
    BaselineSeed,
    LegacyImport,
    MediaLibrary,
    BaseApply,
    DevOverlay,
    WorkingBaseline,
    UatCapture,
    LegacyRefresh,
    Rebase,
    SchemaEvolution,
    FullCapture,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StateLayerApplyMode {
    Task,
    Artifact,
    Sql,
    Manual,
    Checkpoint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StateLayerEnvironmentPolicy {
    All,
    DevOnly,
    NonProduction,
    Production,
    CaptureOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateStackLineagePlan {
    pub schema: String,
    pub lineage_id: String,
    pub stack_name: String,
    pub environment: StateEnvironment,
    pub layers: Vec<StateStackLineageLayer>,
    pub artifact_reports: Vec<StateStackArtifactReportRef>,
    pub warnings: Vec<String>,
}

impl StateStackLineagePlan {
    pub fn report(self, created_at: impl Into<String>) -> StateStackLineageReport {
        StateStackLineageReport {
            schema: self.schema,
            lineage_id: self.lineage_id,
            stack_name: self.stack_name,
            environment: self.environment,
            created_at: created_at.into(),
            layers: self.layers,
            artifact_reports: self.artifact_reports,
            warnings: self.warnings,
            written_report_path: None,
            written_history_path: None,
        }
    }

    fn from_manifest(manifest: &StateStackManifest) -> Self {
        let mut artifact_reports = Vec::new();
        let mut layers = Vec::with_capacity(manifest.layers.len());
        for (index, layer) in manifest.layers.iter().enumerate() {
            let artifact_source = layer
                .artifact_source()
                .ok()
                .flatten()
                .map(|source| source.display_ref());
            if let Some(source_ref) = artifact_source.as_ref() {
                artifact_reports.push(StateStackArtifactReportRef {
                    layer_key: layer.key.clone(),
                    source_ref: source_ref.clone(),
                    artifact_kind: layer.artifact_kind,
                    operation: StateStackArtifactOperation::PlannedResolve,
                });
            }
            layers.push(StateStackLineageLayer {
                index,
                key: layer.key.clone(),
                role: layer.role,
                apply_mode: layer.apply_mode,
                environment_policy: layer.environment_policy,
                source: layer.source.clone(),
                artifact_source,
                hook: layer.hook.clone(),
                snapshot_identity: layer.snapshot_identity.clone(),
                sql_target: layer.sql_target.clone(),
            });
        }

        Self {
            schema: STATE_STACK_LINEAGE_SCHEMA.to_owned(),
            lineage_id: lineage_id(manifest),
            stack_name: manifest.name.clone(),
            environment: manifest.environment,
            layers,
            artifact_reports,
            warnings: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateStackLineageReport {
    pub schema: String,
    pub lineage_id: String,
    pub stack_name: String,
    pub environment: StateEnvironment,
    pub created_at: String,
    pub layers: Vec<StateStackLineageLayer>,
    pub artifact_reports: Vec<StateStackArtifactReportRef>,
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub written_report_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub written_history_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateStackLineageLayer {
    pub index: usize,
    pub key: String,
    pub role: StateLayerRole,
    pub apply_mode: StateLayerApplyMode,
    pub environment_policy: StateLayerEnvironmentPolicy,
    pub source: String,
    pub artifact_source: Option<String>,
    pub hook: Option<String>,
    pub snapshot_identity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sql_target: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateStackArtifactReportRef {
    pub layer_key: String,
    pub source_ref: String,
    pub artifact_kind: Option<ArtifactKind>,
    pub operation: StateStackArtifactOperation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StateStackArtifactOperation {
    PlannedResolve,
}

pub fn state_report_write_paths(
    repo_root: &Path,
    stack_name: &str,
    kind: StateHistoryKind,
    lineage: Option<&str>,
    compatibility_file: Option<&str>,
) -> StateReportWritePaths {
    let stack_dir = repo_root
        .join(".effigy")
        .join("reports")
        .join("state")
        .join(safe_path_component(stack_name));
    let latest_path = stack_dir.join(format!("latest-{kind}.json"));
    let history_path = state_history_report_path(
        &stack_dir,
        kind,
        &utc_basic_timestamp(SystemTime::now()),
        &short_safe_lineage(lineage.unwrap_or("lineage-unknown")),
    );
    let compatibility_path = compatibility_file.map(|file| stack_dir.join(file));
    StateReportWritePaths {
        compatibility_path,
        latest_path,
        history_path,
    }
}

fn state_history_report_path(
    stack_dir: &Path,
    kind: StateHistoryKind,
    timestamp: &str,
    lineage: &str,
) -> PathBuf {
    let history_dir = stack_dir.join("history");
    let base_name = format!("{timestamp}-{kind}-{lineage}");
    let mut path = history_dir.join(format!("{base_name}.json"));
    let mut suffix = 2;
    while path.exists() {
        path = history_dir.join(format!("{base_name}-{suffix}.json"));
        suffix += 1;
    }
    path
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateReportWritePaths {
    pub compatibility_path: Option<PathBuf>,
    pub latest_path: PathBuf,
    pub history_path: PathBuf,
}

impl StateReportWritePaths {
    pub fn all_paths(&self) -> impl Iterator<Item = &Path> {
        self.compatibility_path
            .iter()
            .map(PathBuf::as_path)
            .chain(std::iter::once(self.latest_path.as_path()))
            .chain(std::iter::once(self.history_path.as_path()))
    }
}

fn short_safe_lineage(lineage: &str) -> String {
    let safe = safe_path_component(lineage);
    safe.chars().take(48).collect()
}

fn utc_basic_timestamp(time: SystemTime) -> String {
    let seconds = time
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let days = seconds.div_euclid(86_400);
    let day_seconds = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_unix_days(days);
    let hour = day_seconds / 3_600;
    let minute = (day_seconds % 3_600) / 60;
    let second = day_seconds % 60;
    format!("{year:04}{month:02}{day:02}T{hour:02}{minute:02}{second:02}Z")
}

fn civil_from_unix_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 }.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = year + if month <= 2 { 1 } else { 0 };
    (year, month, day)
}

fn safe_path_component(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StateStackHistoryReport {
    pub schema: String,
    pub schema_version: u8,
    pub stack_name: String,
    pub reports: Vec<StateStackHistoryItem>,
    pub warnings: Vec<String>,
}

impl StateStackHistoryReport {
    pub fn scan(
        repo_root: &Path,
        stack: &str,
        kind: Option<StateHistoryKind>,
        limit: usize,
        lineage: Option<&str>,
    ) -> Self {
        let mut warnings = Vec::new();
        let stack_dir = repo_root
            .join(".effigy")
            .join("reports")
            .join("state")
            .join(safe_path_component(stack));
        let mut candidates = Vec::new();
        collect_state_history_candidates(&stack_dir, &mut candidates, &mut warnings);
        collect_state_history_candidates(
            &stack_dir.join("history"),
            &mut candidates,
            &mut warnings,
        );

        let mut reports = Vec::new();
        for path in candidates {
            match read_state_history_item(repo_root, &path) {
                Ok(Some(item)) => {
                    if kind.is_some_and(|expected| item.kind != expected) {
                        continue;
                    }
                    if let Some(lineage) = lineage {
                        let matches_lineage = item.lineage_id.as_deref() == Some(lineage)
                            || item.parent_lineage_id.as_deref() == Some(lineage);
                        if !matches_lineage {
                            continue;
                        }
                    }
                    reports.push(item);
                }
                Ok(None) => {}
                Err(error) => warnings.push(error),
            }
        }
        reports.sort_by(|left, right| {
            right
                .created_at
                .cmp(&left.created_at)
                .then_with(|| right.path.cmp(&left.path))
        });
        reports.truncate(limit);

        Self {
            schema: STATE_STACK_HISTORY_SCHEMA.to_owned(),
            schema_version: 1,
            stack_name: stack.to_owned(),
            reports,
            warnings,
        }
    }
}

fn collect_state_history_candidates(
    dir: &Path,
    candidates: &mut Vec<PathBuf>,
    warnings: &mut Vec<String>,
) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        match entry {
            Ok(entry) => {
                let path = entry.path();
                if path.is_file()
                    && path
                        .extension()
                        .and_then(|extension| extension.to_str())
                        .is_some_and(|extension| extension == "json")
                {
                    candidates.push(path);
                }
            }
            Err(error) => warnings.push(format!(
                "failed to read state history entry in {}: {error}",
                dir.display()
            )),
        }
    }
}

fn read_state_history_item(
    repo_root: &Path,
    path: &Path,
) -> Result<Option<StateStackHistoryItem>, String> {
    let raw = fs::read_to_string(path)
        .map_err(|error| format!("failed to read state report {}: {error}", path.display()))?;
    let value: Value = serde_json::from_str(&raw)
        .map_err(|error| format!("ignored malformed state report {}: {error}", path.display()))?;
    let schema = value
        .get("schema")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let Some(kind) =
        StateHistoryKind::from_schema(&schema).or_else(|| StateHistoryKind::from_path(path))
    else {
        return Ok(None);
    };
    let lineage_id = value
        .get("lineage_id")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let parent_lineage_id = value
        .get("parent_lineage_id")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let created_at = value
        .get("created_at")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| path_created_at_fallback(path));
    let ok = value.get("ok").and_then(Value::as_bool);
    let executed = value.get("executed").and_then(Value::as_bool);
    let path_display = path_display(path, repo_root);
    Ok(Some(StateStackHistoryItem {
        kind,
        schema,
        path: path_display,
        created_at,
        lineage_id,
        parent_lineage_id,
        ok,
        executed,
        summary: state_history_summary(kind, &value),
    }))
}

fn path_created_at_fallback(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("unknown")
        .to_owned()
}

fn state_history_summary(kind: StateHistoryKind, value: &Value) -> String {
    match kind {
        StateHistoryKind::Plan => value
            .get("layers")
            .and_then(Value::as_array)
            .map(|layers| format!("{} planned layer(s)", layers.len()))
            .unwrap_or_else(|| "plan report".to_owned()),
        StateHistoryKind::Apply => value
            .get("layers")
            .and_then(Value::as_array)
            .map(|layers| format!("{} apply layer(s)", layers.len()))
            .unwrap_or_else(|| "apply report".to_owned()),
        StateHistoryKind::Capture => value
            .get("captures")
            .and_then(Value::as_array)
            .map(|captures| format!("{} capture set item(s)", captures.len()))
            .or_else(|| {
                value
                    .get("produced_layers")
                    .and_then(Value::as_array)
                    .map(|layers| format!("{} produced layer(s)", layers.len()))
            })
            .unwrap_or_else(|| "capture report".to_owned()),
    }
}

fn path_display(path: &Path, repo_root: &Path) -> String {
    path.strip_prefix(repo_root)
        .unwrap_or(path)
        .display()
        .to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StateHistoryKind {
    Plan,
    Apply,
    Capture,
}

impl StateHistoryKind {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "plan" => Some(Self::Plan),
            "apply" => Some(Self::Apply),
            "capture" => Some(Self::Capture),
            _ => None,
        }
    }

    fn from_schema(schema: &str) -> Option<Self> {
        match schema {
            STATE_STACK_LINEAGE_SCHEMA => Some(Self::Plan),
            STATE_STACK_APPLY_SCHEMA => Some(Self::Apply),
            STATE_STACK_CAPTURE_SCHEMA | STATE_STACK_CAPTURE_SET_SCHEMA => Some(Self::Capture),
            _ => None,
        }
    }

    fn from_path(path: &Path) -> Option<Self> {
        let file_name = path.file_name()?.to_str()?;
        if file_name.contains("plan") {
            Some(Self::Plan)
        } else if file_name.contains("apply") {
            Some(Self::Apply)
        } else if file_name.contains("capture") {
            Some(Self::Capture)
        } else {
            None
        }
    }
}

impl fmt::Display for StateHistoryKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Plan => "plan",
            Self::Apply => "apply",
            Self::Capture => "capture",
        };
        formatter.write_str(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StateStackHistoryItem {
    pub kind: StateHistoryKind,
    pub schema: String,
    pub path: String,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lineage_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_lineage_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ok: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executed: Option<bool>,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StateStackApplyReport {
    pub schema: String,
    pub schema_version: u8,
    pub ok: bool,
    pub executed: bool,
    pub stack_name: String,
    pub environment: StateEnvironment,
    pub lineage_id: String,
    pub layers: Vec<StateStackApplyLayerReport>,
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub written_report_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub written_history_path: Option<String>,
}

impl StateStackApplyReport {
    pub fn from_lineage(lineage: &StateStackLineageReport, execute: bool) -> Self {
        let layers = lineage
            .layers
            .iter()
            .map(|layer| {
                let status = match layer.apply_mode {
                    StateLayerApplyMode::Task if execute => StateStackApplyLayerStatus::PlannedTask,
                    StateLayerApplyMode::Artifact if execute => {
                        StateStackApplyLayerStatus::PlannedArtifactStage
                    }
                    StateLayerApplyMode::Sql if execute => {
                        StateStackApplyLayerStatus::PlannedSqlImport
                    }
                    StateLayerApplyMode::Task => StateStackApplyLayerStatus::WouldExecute,
                    StateLayerApplyMode::Artifact => StateStackApplyLayerStatus::WouldStage,
                    StateLayerApplyMode::Sql => StateStackApplyLayerStatus::WouldImport,
                    _ => StateStackApplyLayerStatus::Unsupported,
                };
                StateStackApplyLayerReport {
                    index: layer.index,
                    key: layer.key.clone(),
                    role: layer.role,
                    apply_mode: layer.apply_mode,
                    source: layer.source.clone(),
                    target: layer.sql_target.clone(),
                    hook: layer.hook.clone(),
                    status,
                    output: None,
                    artifact_report: None,
                    sql_report: None,
                    hook_status: None,
                    hook_context_path: None,
                    hook_output: None,
                    hook_error: None,
                    error: None,
                }
            })
            .collect();
        Self {
            schema: STATE_STACK_APPLY_SCHEMA.to_owned(),
            schema_version: 1,
            ok: true,
            executed: execute,
            stack_name: lineage.stack_name.clone(),
            environment: lineage.environment,
            lineage_id: lineage.lineage_id.clone(),
            layers,
            warnings: lineage.warnings.clone(),
            written_report_path: None,
            written_history_path: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StateStackApplyLayerReport {
    pub index: usize,
    pub key: String,
    pub role: StateLayerRole,
    pub apply_mode: StateLayerApplyMode,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hook: Option<String>,
    pub status: StateStackApplyLayerStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_report: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sql_report: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hook_status: Option<StateStackApplyHookStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hook_context_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hook_output: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hook_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateCapturePlanRequest {
    pub source_environment: String,
    pub key: String,
    pub source: Option<String>,
    pub destination_ref: Option<String>,
    pub hook: Option<String>,
}

impl StateCapturePlanRequest {
    pub fn new(source_environment: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            source_environment: source_environment.into(),
            key: key.into(),
            source: None,
            destination_ref: None,
            hook: None,
        }
    }

    pub fn source(mut self, source: Option<String>) -> Self {
        self.source = source;
        self
    }

    pub fn destination_ref(mut self, destination_ref: Option<String>) -> Self {
        self.destination_ref = destination_ref;
        self
    }

    pub fn hook(mut self, hook: Option<String>) -> Self {
        self.hook = hook;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StateStackCaptureProducedLayer {
    pub key: String,
    pub role: StateLayerRole,
    pub apply_mode: StateLayerApplyMode,
    pub environment_policy: StateLayerEnvironmentPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_kind: Option<ArtifactKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_identity: Option<String>,
    pub depends_on: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hook: Option<String>,
}

pub fn capture_produced_layer(
    lineage: &StateStackLineageReport,
    capture_role: StateLayerRole,
    request: &StateCapturePlanRequest,
) -> Result<StateStackCaptureProducedLayer, StateCapturePlanningError> {
    let capture_mode = StateCaptureMode::from_role(capture_role)
        .ok_or(StateCapturePlanningError::UnsupportedCaptureRole { role: capture_role })?;
    Ok(StateStackCaptureProducedLayer {
        key: request.key.clone(),
        role: capture_role,
        apply_mode: StateLayerApplyMode::Artifact,
        environment_policy: capture_mode.environment_policy(),
        artifact_kind: Some(ArtifactKind::AppSpecific),
        source_ref: request.destination_ref.clone(),
        snapshot_identity: Some(format!(
            "{}@planned",
            capture_mode.snapshot_identity_prefix()
        )),
        depends_on: lineage
            .layers
            .last()
            .map(|layer| vec![layer.key.clone()])
            .unwrap_or_default(),
        hook: request.hook.clone(),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StateCaptureMode {
    UatOverlay,
    FullSnapshot,
}

impl StateCaptureMode {
    pub fn from_role(role: StateLayerRole) -> Option<Self> {
        match role {
            StateLayerRole::UatCapture => Some(Self::UatOverlay),
            StateLayerRole::FullCapture => Some(Self::FullSnapshot),
            _ => None,
        }
    }

    pub fn environment_policy(self) -> StateLayerEnvironmentPolicy {
        match self {
            Self::UatOverlay => StateLayerEnvironmentPolicy::NonProduction,
            Self::FullSnapshot => StateLayerEnvironmentPolicy::CaptureOnly,
        }
    }

    pub fn snapshot_identity_prefix(self) -> &'static str {
        match self {
            Self::UatOverlay => "uat-authored-content",
            Self::FullSnapshot => "full-system-capture",
        }
    }
}

impl fmt::Display for StateCaptureMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::UatOverlay => "uat-overlay",
            Self::FullSnapshot => "full-snapshot",
        };
        formatter.write_str(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateCapturePlanningError {
    UnsupportedCaptureRole { role: StateLayerRole },
}

impl fmt::Display for StateCapturePlanningError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedCaptureRole { role } => {
                write!(formatter, "unsupported state capture role `{role:?}`")
            }
        }
    }
}

impl std::error::Error for StateCapturePlanningError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StateStackApplyHookStatus {
    Executed,
    Failed,
}

impl fmt::Display for StateStackApplyHookStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Executed => "executed",
            Self::Failed => "failed",
        };
        formatter.write_str(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StateStackApplyLayerStatus {
    WouldExecute,
    WouldStage,
    WouldImport,
    PlannedTask,
    PlannedArtifactStage,
    PlannedSqlImport,
    Executed,
    Staged,
    Imported,
    Unsupported,
    Failed,
}

impl fmt::Display for StateStackApplyLayerStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::WouldExecute => "would-execute",
            Self::WouldStage => "would-stage",
            Self::WouldImport => "would-import",
            Self::PlannedTask => "planned-task",
            Self::PlannedArtifactStage => "planned-artifact-stage",
            Self::PlannedSqlImport => "planned-sql-import",
            Self::Executed => "executed",
            Self::Staged => "staged",
            Self::Imported => "imported",
            Self::Unsupported => "unsupported",
            Self::Failed => "failed",
        };
        formatter.write_str(value)
    }
}

pub fn validate_state_stack(
    manifest: &StateStackManifest,
) -> Result<(), StateStackValidationError> {
    if manifest.schema != STATE_STACK_SCHEMA {
        return Err(StateStackValidationError::UnsupportedSchema {
            schema: manifest.schema.clone(),
        });
    }
    if manifest.name.trim().is_empty() {
        return Err(StateStackValidationError::EmptyStackName);
    }
    if manifest.layers.is_empty() {
        return Err(StateStackValidationError::NoLayers);
    }

    let mut seen_keys = BTreeSet::new();
    let mut previous_order = 0;
    for (index, layer) in manifest.layers.iter().enumerate() {
        let key = layer.key.trim();
        if key.is_empty() {
            return Err(StateStackValidationError::EmptyLayerKey { index });
        }
        if !seen_keys.insert(key.to_owned()) {
            return Err(StateStackValidationError::DuplicateLayerKey {
                index,
                key: key.to_owned(),
            });
        }
        let order = layer_role_order(layer.role);
        if order < previous_order {
            return Err(StateStackValidationError::LayerOrder {
                index,
                key: layer.key.clone(),
                role: layer.role,
            });
        }
        previous_order = order;
        validate_environment_policy(manifest.environment, layer, index)?;
        if !matches!(
            layer.role,
            StateLayerRole::Structure
                | StateLayerRole::BaselineSeed
                | StateLayerRole::LegacyImport
                | StateLayerRole::MediaLibrary
                | StateLayerRole::DevOverlay
                | StateLayerRole::UatCapture
                | StateLayerRole::FullCapture
        ) && !matches!(
            layer.apply_mode,
            StateLayerApplyMode::Manual | StateLayerApplyMode::Checkpoint
        ) {
            return Err(StateStackValidationError::DeferredExecutableRole {
                index,
                key: layer.key.clone(),
                role: layer.role,
            });
        }
        layer.artifact_source().map_err(|source| {
            StateStackValidationError::InvalidArtifactSource {
                index,
                key: layer.key.clone(),
                source,
            }
        })?;
    }

    for (index, layer) in manifest.layers.iter().enumerate() {
        for dependency in &layer.depends_on {
            if !seen_keys.contains(dependency) {
                return Err(StateStackValidationError::UnknownDependency {
                    index,
                    key: layer.key.clone(),
                    dependency: dependency.clone(),
                });
            }
        }
    }

    Ok(())
}

fn validate_environment_policy(
    environment: StateEnvironment,
    layer: &StateStackLayer,
    index: usize,
) -> Result<(), StateStackValidationError> {
    let allowed = match layer.environment_policy {
        StateLayerEnvironmentPolicy::All => true,
        StateLayerEnvironmentPolicy::DevOnly => environment == StateEnvironment::Dev,
        StateLayerEnvironmentPolicy::NonProduction => environment != StateEnvironment::Production,
        StateLayerEnvironmentPolicy::Production => environment == StateEnvironment::Production,
        StateLayerEnvironmentPolicy::CaptureOnly => matches!(
            layer.role,
            StateLayerRole::UatCapture | StateLayerRole::FullCapture
        ),
    };

    if allowed {
        Ok(())
    } else {
        Err(StateStackValidationError::EnvironmentPolicy {
            index,
            key: layer.key.clone(),
            environment,
            policy: layer.environment_policy,
        })
    }
}

fn layer_role_order(role: StateLayerRole) -> usize {
    match role {
        StateLayerRole::Structure => 10,
        StateLayerRole::BaselineSeed => 20,
        StateLayerRole::LegacyImport => 30,
        StateLayerRole::MediaLibrary => 40,
        StateLayerRole::BaseApply => 50,
        StateLayerRole::DevOverlay => 60,
        StateLayerRole::WorkingBaseline => 70,
        StateLayerRole::UatCapture => 80,
        StateLayerRole::LegacyRefresh => 90,
        StateLayerRole::Rebase => 100,
        StateLayerRole::SchemaEvolution => 110,
        StateLayerRole::FullCapture => 120,
    }
}

fn lineage_id(manifest: &StateStackManifest) -> String {
    let keys = manifest
        .layers
        .iter()
        .map(|layer| layer.key.as_str())
        .collect::<Vec<_>>()
        .join("+");
    format!("{}:{:?}:{}", manifest.name, manifest.environment, keys)
}

#[derive(Debug)]
pub enum StateStackParseError {
    Toml(toml::de::Error),
}

impl fmt::Display for StateStackParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Toml(error) => write!(formatter, "failed to parse state stack manifest: {error}"),
        }
    }
}

impl std::error::Error for StateStackParseError {}

#[derive(Debug)]
pub enum StateStackValidationError {
    UnsupportedSchema {
        schema: String,
    },
    EmptyStackName,
    NoLayers,
    EmptyLayerKey {
        index: usize,
    },
    DuplicateLayerKey {
        index: usize,
        key: String,
    },
    LayerOrder {
        index: usize,
        key: String,
        role: StateLayerRole,
    },
    DeferredExecutableRole {
        index: usize,
        key: String,
        role: StateLayerRole,
    },
    EnvironmentPolicy {
        index: usize,
        key: String,
        environment: StateEnvironment,
        policy: StateLayerEnvironmentPolicy,
    },
    InvalidArtifactSource {
        index: usize,
        key: String,
        source: ArtifactRefError,
    },
    UnknownDependency {
        index: usize,
        key: String,
        dependency: String,
    },
}

impl fmt::Display for StateStackValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedSchema { schema } => {
                write!(formatter, "unsupported state stack schema `{schema}`")
            }
            Self::EmptyStackName => formatter.write_str("state stack name is empty"),
            Self::NoLayers => formatter.write_str("state stack has no layers"),
            Self::EmptyLayerKey { index } => {
                write!(formatter, "state stack layer {index} has an empty key")
            }
            Self::DuplicateLayerKey { index, key } => {
                write!(formatter, "state stack layer {index} duplicates key `{key}`")
            }
            Self::LayerOrder { index, key, role } => write!(
                formatter,
                "state stack layer {index} `{key}` has out-of-order role `{role:?}`"
            ),
            Self::DeferredExecutableRole { index, key, role } => write!(
                formatter,
                "state stack layer {index} `{key}` uses deferred executable role `{role:?}`"
            ),
            Self::EnvironmentPolicy {
                index,
                key,
                environment,
                policy,
            } => write!(
                formatter,
                "state stack layer {index} `{key}` policy `{policy:?}` does not apply to `{environment:?}`"
            ),
            Self::InvalidArtifactSource { index, key, source } => write!(
                formatter,
                "state stack layer {index} `{key}` has invalid artifact source: {source}"
            ),
            Self::UnknownDependency {
                index,
                key,
                dependency,
            } => write!(
                formatter,
                "state stack layer {index} `{key}` depends on unknown layer `{dependency}`"
            ),
        }
    }
}

impl std::error::Error for StateStackValidationError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_acowtancy_shaped_stack_and_plans_lineage() {
        let manifest = StateStackManifest::parse_toml(acowtancy_fixture()).expect("parse manifest");
        let plan = manifest.plan_lineage().expect("plan lineage");

        assert_eq!(plan.schema, STATE_STACK_LINEAGE_SCHEMA);
        assert_eq!(plan.stack_name, "acowtancy-uat");
        assert_eq!(plan.environment, StateEnvironment::Uat);
        assert_eq!(
            plan.layers
                .iter()
                .map(|layer| layer.key.as_str())
                .collect::<Vec<_>>(),
            vec![
                "structure",
                "baseline-seed",
                "legacy-content",
                "dev-users",
                "uat-content-capture",
                "full-system-capture",
            ]
        );
        assert_eq!(plan.artifact_reports.len(), 3);
        assert_eq!(
            plan.artifact_reports
                .iter()
                .map(|report| report.layer_key.as_str())
                .collect::<Vec<_>>(),
            vec!["baseline-seed", "legacy-content", "uat-content-capture"]
        );
    }

    #[test]
    fn lineage_report_is_deterministic() {
        let manifest = StateStackManifest::parse_toml(acowtancy_fixture()).expect("parse manifest");
        let report = manifest
            .plan_lineage()
            .expect("plan lineage")
            .report("2026-05-08T12:00:00Z");

        assert_eq!(report.schema, STATE_STACK_LINEAGE_SCHEMA);
        assert_eq!(
            report.lineage_id,
            "acowtancy-uat:Uat:structure+baseline-seed+legacy-content+dev-users+uat-content-capture+full-system-capture"
        );
        assert_eq!(
            report.layers[2].artifact_source.as_deref(),
            Some("oci://ghcr.io/acowtancy/legacy-content:2026-05-08")
        );
        assert_eq!(report.created_at, "2026-05-08T12:00:00Z");
    }

    #[test]
    fn rejects_out_of_order_roles() {
        let mut manifest = StateStackManifest::parse_toml(acowtancy_fixture()).expect("parse");
        manifest.layers.swap(0, 1);

        let error = manifest.plan_lineage().expect_err("reject order");
        assert!(matches!(
            error,
            StateStackValidationError::LayerOrder { .. }
        ));
    }

    #[test]
    fn rejects_dev_only_layer_in_production_stack() {
        let mut manifest = StateStackManifest::parse_toml(acowtancy_fixture()).expect("parse");
        manifest.environment = StateEnvironment::Production;

        let error = manifest.plan_lineage().expect_err("reject environment");
        assert!(matches!(
            error,
            StateStackValidationError::EnvironmentPolicy { .. }
        ));
    }

    #[test]
    fn rejects_ambiguous_oci_source_for_artifact_layer() {
        let mut manifest = StateStackManifest::parse_toml(acowtancy_fixture()).expect("parse");
        manifest.layers[2].source = "ghcr.io/acowtancy/legacy-content:latest".to_owned();

        let error = manifest.plan_lineage().expect_err("reject source");
        assert!(matches!(
            error,
            StateStackValidationError::InvalidArtifactSource { .. }
        ));
    }

    #[test]
    fn accepts_media_library_object_store_layers() {
        let manifest = StateStackManifest::parse_toml(media_fixture()).expect("parse");

        let plan = manifest.plan_lineage().expect("plan");

        assert_eq!(plan.layers[1].key, "legacy-media");
        assert_eq!(plan.layers[1].role, StateLayerRole::MediaLibrary);
        assert_eq!(plan.layers[1].sql_target.as_deref(), Some("media"));
        assert_eq!(
            plan.artifact_reports[0].artifact_kind,
            Some(effigy_artifacts::ArtifactKind::ObjectStore)
        );
    }

    #[test]
    fn report_write_paths_preserve_state_report_layout() {
        let repo = temp_state_repo("report-paths");
        let paths = state_report_write_paths(
            &repo,
            "acowtancy uat",
            StateHistoryKind::Apply,
            Some("lineage/with spaces"),
            Some("apply.json"),
        );

        assert_eq!(
            paths.latest_path,
            repo.join(".effigy/reports/state/acowtancy-uat/latest-apply.json")
        );
        assert_eq!(
            paths.compatibility_path,
            Some(repo.join(".effigy/reports/state/acowtancy-uat/apply.json"))
        );
        let history = paths.history_path.display().to_string();
        assert!(history.contains(".effigy/reports/state/acowtancy-uat/history/"));
        assert!(history.contains("-apply-lineage-with-spaces.json"));
    }

    #[test]
    fn history_scan_filters_sorts_and_summarizes_reports() {
        let repo = temp_state_repo("history-scan");
        let history_dir = repo.join(".effigy/reports/state/acowtancy-uat/history");
        std::fs::create_dir_all(&history_dir).expect("create history dir");
        std::fs::write(
            history_dir.join("20260512T010000Z-apply-lineage-a.json"),
            r#"{
  "schema": "effigy.state-stack.apply.v1",
  "lineage_id": "lineage-a",
  "created_at": "2026-05-12T01:00:00Z",
  "ok": true,
  "executed": true,
  "layers": [{ "key": "legacy" }]
}
"#,
        )
        .expect("write apply report");
        std::fs::write(
            history_dir.join("20260512T020000Z-capture-lineage-a.json"),
            r#"{
  "schema": "effigy.state-stack.capture.v1",
  "parent_lineage_id": "lineage-a",
  "created_at": "2026-05-12T02:00:00Z",
  "ok": true,
  "executed": true,
  "produced_layers": [{ "key": "uat" }]
}
"#,
        )
        .expect("write capture report");

        let report = StateStackHistoryReport::scan(
            &repo,
            "acowtancy-uat",
            Some(StateHistoryKind::Apply),
            10,
            Some("lineage-a"),
        );

        assert_eq!(report.schema, STATE_STACK_HISTORY_SCHEMA);
        assert_eq!(report.reports.len(), 1);
        assert_eq!(report.reports[0].kind, StateHistoryKind::Apply);
        assert_eq!(report.reports[0].summary, "1 apply layer(s)");
        assert_eq!(report.reports[0].lineage_id.as_deref(), Some("lineage-a"));
    }

    #[test]
    fn apply_report_plans_execution_and_dry_run_statuses() {
        let lineage = StateStackManifest::parse_toml(acowtancy_fixture())
            .expect("parse")
            .plan_lineage()
            .expect("lineage")
            .report("planned");

        let execute = StateStackApplyReport::from_lineage(&lineage, true);
        assert_eq!(execute.schema, STATE_STACK_APPLY_SCHEMA);
        assert_eq!(
            execute.layers[0].status,
            StateStackApplyLayerStatus::PlannedTask
        );
        assert_eq!(
            execute.layers[1].status,
            StateStackApplyLayerStatus::PlannedSqlImport
        );
        assert_eq!(
            execute.layers[2].status,
            StateStackApplyLayerStatus::PlannedArtifactStage
        );
        assert_eq!(
            execute.layers[2].hook.as_deref(),
            Some("farmyard:seed-bundle:apply")
        );

        let plan_only = StateStackApplyReport::from_lineage(&lineage, false);
        assert_eq!(
            plan_only.layers[0].status,
            StateStackApplyLayerStatus::WouldExecute
        );
        assert_eq!(
            plan_only.layers[1].status,
            StateStackApplyLayerStatus::WouldImport
        );
    }

    #[test]
    fn capture_produced_layer_uses_role_policy_and_lineage_parent() {
        let lineage = StateStackManifest::parse_toml(acowtancy_fixture())
            .expect("parse")
            .plan_lineage()
            .expect("lineage")
            .report("planned");

        let layer = capture_produced_layer(
            &lineage,
            StateLayerRole::UatCapture,
            &StateCapturePlanRequest::new("uat", "uat-content-2026-05-12")
                .destination_ref(Some(
                    "oci://ghcr.io/acowtancy/content:uat-2026-05-12".to_owned(),
                ))
                .hook(Some("farmyard:state:capture".to_owned())),
        )
        .expect("capture layer");

        assert_eq!(layer.key, "uat-content-2026-05-12");
        assert_eq!(layer.role, StateLayerRole::UatCapture);
        assert_eq!(layer.apply_mode, StateLayerApplyMode::Artifact);
        assert_eq!(
            layer.environment_policy,
            StateLayerEnvironmentPolicy::NonProduction
        );
        assert_eq!(
            layer.source_ref.as_deref(),
            Some("oci://ghcr.io/acowtancy/content:uat-2026-05-12")
        );
        assert_eq!(
            layer.snapshot_identity.as_deref(),
            Some("uat-authored-content@planned")
        );
        assert_eq!(layer.depends_on, vec!["full-system-capture".to_owned()]);
        assert_eq!(layer.hook.as_deref(), Some("farmyard:state:capture"));
    }

    #[test]
    fn requires_deferred_roles_to_be_manual_or_checkpoint() {
        let mut manifest = StateStackManifest::parse_toml(acowtancy_fixture()).expect("parse");
        manifest.layers.insert(
            5,
            StateStackLayer {
                key: "legacy-refresh".to_owned(),
                role: StateLayerRole::LegacyRefresh,
                source: "farmyard:legacy-refresh".to_owned(),
                apply_mode: StateLayerApplyMode::Task,
                environment_policy: StateLayerEnvironmentPolicy::NonProduction,
                depends_on: Vec::new(),
                artifact_kind: None,
                snapshot_identity: None,
                hook: None,
                notes: None,
                sql_target: None,
            },
        );

        let error = manifest.plan_lineage().expect_err("reject deferred role");
        assert!(matches!(
            error,
            StateStackValidationError::DeferredExecutableRole { .. }
        ));
    }

    fn temp_state_repo(label: &str) -> std::path::PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("effigy-state-{label}-{unique}"));
        std::fs::create_dir_all(&dir).expect("create temp state repo");
        dir
    }

    fn acowtancy_fixture() -> &'static str {
        r#"
schema = "effigy.state-stack.v1"
name = "acowtancy-uat"
environment = "uat"

[[layers]]
key = "structure"
role = "structure"
source = "farmyard:db:migrate"
apply_mode = "task"
environment_policy = "all"
hook = "farmyard:db:migrate"

[[layers]]
key = "baseline-seed"
role = "baseline-seed"
source = "./seed/static.sql"
apply_mode = "sql"
environment_policy = "all"
artifact_kind = "sql-dump"

[[layers]]
key = "legacy-content"
role = "legacy-import"
source = "oci://ghcr.io/acowtancy/legacy-content:2026-05-08"
apply_mode = "artifact"
environment_policy = "all"
artifact_kind = "migrated-base-snapshot"
snapshot_identity = "legacy-db-2026-05-08"
hook = "farmyard:seed-bundle:apply"

[[layers]]
key = "dev-users"
role = "dev-overlay"
source = "farmyard:dev-seed-users"
apply_mode = "task"
environment_policy = "non-production"
hook = "farmyard:dev-seed-users"

[[layers]]
key = "uat-content-capture"
role = "uat-capture"
source = "oci://ghcr.io/acowtancy/uat-content:2026-05-08"
apply_mode = "artifact"
environment_policy = "capture-only"
artifact_kind = "uat-content-snapshot"

[[layers]]
key = "full-system-capture"
role = "full-capture"
source = "farmyard:full-capture"
apply_mode = "checkpoint"
environment_policy = "capture-only"
"#
    }

    fn media_fixture() -> &'static str {
        r#"
schema = "effigy.state-stack.v1"
name = "acowtancy-uat"
environment = "uat"

[[layers]]
key = "structure"
role = "structure"
source = "farmyard:db:migrate"
apply_mode = "task"
environment_policy = "all"

[[layers]]
key = "legacy-media"
role = "media-library"
source = "farmyard/state/legacy/dist/oci/media.oci"
apply_mode = "artifact"
environment_policy = "all"
artifact_kind = "object-store"
snapshot_identity = "legacy-media@local"
target = "media"
"#
    }
}
