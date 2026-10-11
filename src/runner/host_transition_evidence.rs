//! Bounded receipt of managed listener transitions.
//!
//! A listener supervisor appends one JSON object per line to
//! `<name>.transitions.jsonl`, beside `<name>.listener.json`. Each record names
//! the cause the supervisor selected at its decision boundary, the reserved
//! dependency assignments supplied to a launched generation, and bounded
//! identity correlations. The receipt observes only. It never authorizes
//! signals, publication, cleanup or adoption, and a missing record never proves
//! that no transition happened.
//!
//! New records use `effigy.managed.host-transition-evidence.v2`. Readers also
//! accept the historical v1 shape, which has no tuple-availability marker.
//! Sequences start at 1; a missing first sequence, later gap, duplicate or
//! regression is visible. Readers skip blank lines, count a newline-terminated
//! line they cannot parse or whose schema is unknown as a gap, and report an
//! unterminated final line as an incomplete tail. A failed append consumes a
//! sequence; if no complete record is readable, the gap is visible. Retention
//! is bounded by record count and bytes; the first
//! record that would exceed a bound is replaced by one `retention_limit`
//! marker, after which the supervisor writes nothing more.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use effigy_gateway::identity::GatewayStartIdentity;
use serde::{Deserialize, Serialize};

use super::error::RunnerError;
use super::host_process::append_log_line;

pub(in crate::runner) const TRANSITION_EVIDENCE_SCHEMA: &str =
    "effigy.managed.host-transition-evidence.v2";
const TRANSITION_EVIDENCE_SCHEMA_V1: &str = "effigy.managed.host-transition-evidence.v1";
pub(in crate::runner) const TRANSITION_EVIDENCE_FILE_SUFFIX: &str = ".transitions.jsonl";
const TRANSITION_EVIDENCE_MAX_RECORDS: usize = 512;
const TRANSITION_EVIDENCE_MAX_BYTES: usize = 256 * 1024;
/// Bytes kept free so the single retention marker always fits.
const RETENTION_RESERVE_BYTES: usize = 4096;
/// Effigy reserves this prefix for dependency assignments; see the managed listener contract.
const RESERVED_ASSIGNMENT_PREFIX: &str = "EFFIGY_MANAGED_HOST_";
/// Per-dependency suffixes. Anything else in the environment is never recorded.
const RESERVED_DEPENDENCY_SUFFIXES: [&str; 4] =
    ["_INTERNAL_URL", "_PUBLIC_URL", "_STATE_FILE", "_GENERATION"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::runner) struct TransitionEvidenceLimits {
    pub(in crate::runner) max_records: usize,
    pub(in crate::runner) max_bytes: usize,
}

impl TransitionEvidenceLimits {
    pub(in crate::runner) const DEFAULT: Self = Self {
        max_records: TRANSITION_EVIDENCE_MAX_RECORDS,
        max_bytes: TRANSITION_EVIDENCE_MAX_BYTES,
    };
}

/// One process identity as observed by the supervisor. Identity is correlation
/// evidence only; it never authorizes an effect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::runner) struct TransitionProcess {
    pub(in crate::runner) pid: u32,
    pub(in crate::runner) boot_identity: String,
    pub(in crate::runner) start_identity: GatewayStartIdentity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::runner) enum TransitionEvent {
    SuccessorLaunched,
    ChildExited,
    Shutdown,
    DependencyUnavailable,
    DependencyTupleChanged,
    InspectionError,
    RetentionLimit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::runner) enum TransitionPhase {
    /// Before the generation's route was published and before it became ready.
    PrePublication,
    /// After the generation was published and was ready.
    Published,
}

/// The first readiness predicate a dependency failed, in the order the
/// supervisor evaluates them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::runner) enum DependencyPredicate {
    StateFileMissing,
    StateFileUnreadable,
    StateFileUnsafe,
    StateSchemaInvalid,
    StateIdentityMismatch,
    StateNotReady,
    RouteOwnerMissing,
    OwnerUnverified,
    RouteNotMatching,
    RouteTableUnreadable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::runner) enum ObservedStatus {
    Observed,
    Unavailable,
}

/// Whether a dependency's generation tuple was available at the selected
/// inspection boundary. `Absent` and `Unreadable` are explicit unknowns, not
/// empty or inferred tuples.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::runner) enum DependencyTupleStatus {
    Observed,
    Unavailable,
    Absent,
    Unreadable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::runner) struct ObservedDependency {
    pub(in crate::runner) name: String,
    pub(in crate::runner) status: ObservedStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(in crate::runner) tuple_status: Option<DependencyTupleStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(in crate::runner) predicate: Option<DependencyPredicate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(in crate::runner) runtime_generation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(in crate::runner) generation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(in crate::runner) address: Option<String>,
}

impl ObservedDependency {
    pub(in crate::runner) fn unavailable(name: &str, predicate: DependencyPredicate) -> Self {
        Self {
            name: name.to_owned(),
            status: ObservedStatus::Unavailable,
            tuple_status: Some(match predicate {
                DependencyPredicate::StateFileMissing => DependencyTupleStatus::Absent,
                DependencyPredicate::StateFileUnreadable
                | DependencyPredicate::StateFileUnsafe
                | DependencyPredicate::StateSchemaInvalid
                | DependencyPredicate::StateIdentityMismatch
                | DependencyPredicate::RouteTableUnreadable => DependencyTupleStatus::Unreadable,
                _ => DependencyTupleStatus::Unavailable,
            }),
            predicate: Some(predicate),
            runtime_generation: None,
            generation: None,
            address: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::runner) enum InspectionCategory {
    /// The child's completion could not be inspected.
    ChildCompletion,
    /// A dependency's state or route could not be read or failed its invariants.
    DependencyState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::runner) struct TransitionExit {
    pub(in crate::runner) code: Option<i32>,
    pub(in crate::runner) signal: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::runner) struct TransitionRecord {
    pub(in crate::runner) schema: String,
    pub(in crate::runner) sequence: u64,
    pub(in crate::runner) event: TransitionEvent,
    pub(in crate::runner) owner: String,
    pub(in crate::runner) runtime_generation: String,
    pub(in crate::runner) supervisor: TransitionProcess,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(in crate::runner) phase: Option<TransitionPhase>,
    /// The listener generation this record concerns. For `successor_launched`
    /// it is the new generation; for every other event it is the generation
    /// that was running when the decision was taken.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(in crate::runner) generation: Option<String>,
    /// The most recently launched earlier generation of this supervisor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(in crate::runner) predecessor_generation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(in crate::runner) child: Option<TransitionProcess>,
    /// The child's process group. Children are placed in a group whose id is their PID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(in crate::runner) child_group: Option<u32>,
    /// The listener that answered readiness, only when it was actually observed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(in crate::runner) listener: Option<TransitionProcess>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(in crate::runner) exit: Option<TransitionExit>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(in crate::runner) observed_dependencies: Vec<ObservedDependency>,
    /// Reserved dependency assignments supplied to `generation` at launch.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(in crate::runner) assignments: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(in crate::runner) inspection_error: Option<InspectionCategory>,
}

impl TransitionRecord {
    pub(in crate::runner) fn new(event: TransitionEvent, context: &TransitionContext) -> Self {
        Self {
            schema: TRANSITION_EVIDENCE_SCHEMA.to_owned(),
            sequence: 0,
            event,
            owner: context.owner.clone(),
            runtime_generation: context.runtime_generation.clone(),
            supervisor: context.supervisor.clone(),
            phase: None,
            generation: None,
            predecessor_generation: None,
            child: None,
            child_group: None,
            listener: None,
            exit: None,
            observed_dependencies: Vec::new(),
            assignments: BTreeMap::new(),
            inspection_error: None,
        }
    }
}

/// Fixed identity of one supervisor run, stamped on every record it writes.
#[derive(Debug, Clone)]
pub(in crate::runner) struct TransitionContext {
    pub(in crate::runner) owner: String,
    pub(in crate::runner) runtime_generation: String,
    pub(in crate::runner) supervisor: TransitionProcess,
}

/// Keeps only reserved dependency assignments from a launch environment. The
/// caller passes the dependency-only map the child received; every other key,
/// including spec environment and secrets, is dropped.
pub(in crate::runner) fn reserved_assignments(
    launch_env: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    launch_env
        .iter()
        .filter(|(key, _)| {
            key.starts_with(RESERVED_ASSIGNMENT_PREFIX)
                && RESERVED_DEPENDENCY_SUFFIXES
                    .iter()
                    .any(|suffix| key.ends_with(suffix))
        })
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

/// Appends records to one receipt. Failures never change supervisor lifecycle:
/// they are written to the process log and leave a sequence gap for readers.
pub(in crate::runner) struct TransitionEvidenceWriter {
    path: PathBuf,
    log_path: PathBuf,
    context: TransitionContext,
    limits: TransitionEvidenceLimits,
    next_sequence: u64,
    records: usize,
    bytes: usize,
    needs_separator: bool,
    retention_limited: bool,
    disabled: bool,
    directory_sync_pending: bool,
}

impl TransitionEvidenceWriter {
    pub(in crate::runner) fn open(
        path: PathBuf,
        log_path: PathBuf,
        context: TransitionContext,
    ) -> Self {
        Self::open_with_limits(path, log_path, context, TransitionEvidenceLimits::DEFAULT)
    }

    /// Resumes an existing receipt so sequence numbers and bounds continue. A
    /// receipt that cannot be read disables writes for this run instead of
    /// starting a new, unrelated history in its place.
    pub(in crate::runner) fn open_with_limits(
        path: PathBuf,
        log_path: PathBuf,
        context: TransitionContext,
        limits: TransitionEvidenceLimits,
    ) -> Self {
        let mut writer = Self {
            path,
            log_path,
            context,
            limits,
            next_sequence: 1,
            records: 0,
            bytes: 0,
            needs_separator: false,
            retention_limited: false,
            disabled: false,
            directory_sync_pending: false,
        };
        match read_transition_evidence(&writer.path) {
            Ok(None) => {}
            Ok(Some(receipt)) => {
                let Some(next_sequence) = receipt.max_sequence.checked_add(1) else {
                    writer.disabled = true;
                    append_log_line(
                        &writer.log_path,
                        "[effigy host-process] transition evidence sequence is exhausted; writes disabled for this run",
                    );
                    return writer;
                };
                writer.next_sequence = next_sequence;
                writer.records = receipt.lines;
                writer.bytes = receipt.bytes;
                writer.needs_separator = receipt.incomplete_tail;
                writer.retention_limited = receipt.truncated;
                // A file created by an older writer may never have had its
                // containing directory synced. Reconfirm it on the next append.
                writer.directory_sync_pending = true;
            }
            Err(error) => {
                writer.disabled = true;
                append_log_line(
                    &writer.log_path,
                    &format!(
                        "[effigy host-process] transition evidence is unreadable; writes disabled for this run: {error}"
                    ),
                );
            }
        }
        writer
    }

    pub(in crate::runner) fn draft(&self, event: TransitionEvent) -> TransitionRecord {
        TransitionRecord::new(event, &self.context)
    }

    pub(in crate::runner) fn record(&mut self, mut record: TransitionRecord) {
        if self.disabled || self.retention_limited {
            return;
        }
        record.sequence = self.next_sequence;
        let Ok(line) = serde_json::to_string(&record) else {
            append_log_line(
                &self.log_path,
                "[effigy host-process] transition evidence record could not be encoded",
            );
            return;
        };
        let separator = usize::from(self.needs_separator);
        let fits = self.records + 1 < self.limits.max_records
            && self.bytes + separator + line.len() + 1 + RETENTION_RESERVE_BYTES
                <= self.limits.max_bytes;
        if fits {
            self.append(&line);
            return;
        }
        self.retention_limited = true;
        let mut marker = self.draft(TransitionEvent::RetentionLimit);
        marker.sequence = self.next_sequence;
        if let Ok(marker_line) = serde_json::to_string(&marker) {
            self.append(&marker_line);
        }
    }

    fn append(&mut self, line: &str) {
        let mut payload = String::with_capacity(line.len() + 2);
        if self.needs_separator {
            payload.push('\n');
        }
        payload.push_str(line);
        payload.push('\n');
        let sequence = self.next_sequence;
        let Some(next_sequence) = sequence.checked_add(1) else {
            self.disabled = true;
            append_log_line(
                &self.log_path,
                "[effigy host-process] transition evidence sequence is exhausted; writes disabled for this run",
            );
            return;
        };
        self.next_sequence = next_sequence;
        match write_appended(
            &self.path,
            payload.as_bytes(),
            &mut self.directory_sync_pending,
        ) {
            Ok(()) => {
                self.records += 1;
                self.bytes += payload.len();
                self.needs_separator = false;
            }
            Err(error) => {
                // A partial write may precede the next record; the separator
                // isolates it, and the skipped sequence number keeps the gap visible.
                // Count the full attempted payload toward bounds because an
                // unsuccessful append may still have written any prefix.
                self.records += 1;
                self.bytes += payload.len();
                self.needs_separator = true;
                append_log_line(
                    &self.log_path,
                    &format!(
                        "[effigy host-process] transition evidence record {sequence} was not confirmed durable: {error}"
                    ),
                );
            }
        }
    }
}

fn write_appended(
    path: &Path,
    payload: &[u8],
    directory_sync_pending: &mut bool,
) -> std::io::Result<()> {
    write_appended_with_parent_sync(path, payload, directory_sync_pending, sync_parent_directory)
}

fn write_appended_with_parent_sync(
    path: &Path,
    payload: &[u8],
    directory_sync_pending: &mut bool,
    mut sync_parent: impl FnMut(&Path) -> std::io::Result<()>,
) -> std::io::Result<()> {
    let (mut file, created) = match fs::OpenOptions::new()
        .create_new(true)
        .append(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
    {
        Ok(file) => (file, true),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (
            fs::OpenOptions::new()
                .append(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
                .open(path)?,
            false,
        ),
        Err(error) => return Err(error),
    };
    if created {
        *directory_sync_pending = true;
    }
    let data_result = file.write_all(payload).and_then(|()| file.sync_all());
    let directory_result = if *directory_sync_pending {
        sync_parent(path)
    } else {
        Ok(())
    };
    if directory_result.is_ok() {
        *directory_sync_pending = false;
    }
    data_result.and(directory_result)
}

fn sync_parent_directory(path: &Path) -> std::io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "transition receipt has no containing directory",
        )
    })?;
    fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
        .open(parent)?
        .sync_all()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct TransitionEvidenceReceipt {
    pub(in crate::runner) records: Vec<TransitionRecord>,
    pub(in crate::runner) gaps: Vec<TransitionGap>,
    /// True when a `retention_limit` marker is present; later transitions were not kept.
    pub(in crate::runner) truncated: bool,
    /// True when the final line is unterminated, as after an interrupted append.
    pub(in crate::runner) incomplete_tail: bool,
    /// Newline-terminated lines, including blank and rejected lines.
    pub(in crate::runner) lines: usize,
    pub(in crate::runner) bytes: usize,
    pub(in crate::runner) max_sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) enum TransitionGap {
    /// A complete line that is not valid v1 or v2 JSON for this record.
    Malformed {
        line: usize,
    },
    /// A complete line whose `schema` is unsupported by this reader.
    UnsupportedSchema {
        line: usize,
    },
    SequenceGap {
        expected: u64,
        found: u64,
    },
}

/// Reads a receipt snapshot. An absent file is `None`, which never proves that
/// no transition happened. Symlinks, non-regular files and oversized receipts
/// are refused.
pub(in crate::runner) fn read_transition_evidence(
    path: &Path,
) -> Result<Option<TransitionEvidenceReceipt>, RunnerError> {
    let bound = (TRANSITION_EVIDENCE_MAX_BYTES + RETENTION_RESERVE_BYTES) as u64;
    let file = match fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(RunnerError::task_invocation_failed_read(path, error)),
    };
    let metadata = file
        .metadata()
        .map_err(|error| RunnerError::task_invocation_failed_read(path, error))?;
    if !metadata.is_file() || metadata.len() > bound {
        return Err(RunnerError::task_invocation(format!(
            "managed transition evidence {} is not a bounded regular file",
            path.display()
        )));
    }
    let mut raw = Vec::with_capacity(metadata.len() as usize);
    file.take(bound + 1)
        .read_to_end(&mut raw)
        .map_err(|error| RunnerError::task_invocation_failed_read(path, error))?;
    if raw.len() as u64 > bound {
        return Err(RunnerError::task_invocation(format!(
            "managed transition evidence {} is not a bounded regular file",
            path.display()
        )));
    }
    Ok(Some(parse_transition_evidence(&raw)))
}

pub(in crate::runner) fn parse_transition_evidence(raw: &[u8]) -> TransitionEvidenceReceipt {
    let mut segments: Vec<&[u8]> = raw.split(|byte| *byte == b'\n').collect();
    let tail = segments.pop().unwrap_or_default();
    let mut receipt = TransitionEvidenceReceipt {
        records: Vec::new(),
        gaps: Vec::new(),
        truncated: false,
        incomplete_tail: !tail.is_empty(),
        lines: segments.len(),
        bytes: raw.len(),
        max_sequence: 0,
    };
    let mut expected = 1u64;
    let mut sequence_exhausted = false;
    for (index, line) in segments.iter().enumerate() {
        let number = index + 1;
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(line) else {
            receipt.gaps.push(TransitionGap::Malformed { line: number });
            continue;
        };
        let schema = value.get("schema").and_then(serde_json::Value::as_str);
        let is_v1 = schema == Some(TRANSITION_EVIDENCE_SCHEMA_V1);
        let is_v2 = schema == Some(TRANSITION_EVIDENCE_SCHEMA);
        if !is_v1 && !is_v2 {
            receipt
                .gaps
                .push(TransitionGap::UnsupportedSchema { line: number });
            continue;
        }
        if is_v1 && has_v2_observation_shape(&value) {
            receipt.gaps.push(TransitionGap::Malformed { line: number });
            continue;
        }
        let Ok(record) = serde_json::from_value::<TransitionRecord>(value) else {
            receipt.gaps.push(TransitionGap::Malformed { line: number });
            continue;
        };
        if is_v2 && !valid_v2_observations(&record) {
            receipt.gaps.push(TransitionGap::Malformed { line: number });
            continue;
        }
        if sequence_exhausted {
            receipt.gaps.push(TransitionGap::SequenceGap {
                expected,
                found: record.sequence,
            });
            continue;
        }
        if record.sequence != expected {
            receipt.gaps.push(TransitionGap::SequenceGap {
                expected,
                found: record.sequence,
            });
            if record.sequence == 0 || record.sequence < expected {
                continue;
            }
        }
        match record.sequence.checked_add(1) {
            Some(next) => expected = next,
            None => sequence_exhausted = true,
        }
        receipt.max_sequence = receipt.max_sequence.max(record.sequence);
        if record.event == TransitionEvent::RetentionLimit {
            receipt.truncated = true;
        }
        receipt.records.push(record);
    }
    receipt
}

fn has_v2_observation_shape(value: &serde_json::Value) -> bool {
    value
        .get("observed_dependencies")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|dependencies| {
            dependencies.iter().any(|dependency| {
                let Some(fields) = dependency.as_object() else {
                    return false;
                };
                fields.contains_key("tuple_status")
                    || matches!(
                        fields.get("predicate").and_then(serde_json::Value::as_str),
                        Some(
                            "state_file_unreadable"
                                | "state_file_unsafe"
                                | "state_schema_invalid"
                                | "state_identity_mismatch"
                                | "route_table_unreadable"
                        )
                    )
            })
        })
}

fn valid_v2_observations(record: &TransitionRecord) -> bool {
    let observations_valid = record.observed_dependencies.iter().all(|dependency| {
        match (dependency.status, dependency.tuple_status) {
            (ObservedStatus::Observed, Some(DependencyTupleStatus::Observed)) => {
                dependency.predicate.is_none()
                    && !dependency.name.trim().is_empty()
                    && dependency
                        .runtime_generation
                        .as_deref()
                        .is_some_and(|value| !value.trim().is_empty())
                    && dependency
                        .generation
                        .as_deref()
                        .is_some_and(|value| !value.trim().is_empty())
                    && dependency
                        .address
                        .as_deref()
                        .is_some_and(|value| !value.trim().is_empty())
            }
            (
                ObservedStatus::Unavailable,
                Some(
                    DependencyTupleStatus::Unavailable
                    | DependencyTupleStatus::Absent
                    | DependencyTupleStatus::Unreadable,
                ),
            ) => {
                let marker_matches = matches!(
                    (dependency.tuple_status, dependency.predicate),
                    (
                        Some(DependencyTupleStatus::Absent),
                        Some(DependencyPredicate::StateFileMissing),
                    ) | (
                        Some(DependencyTupleStatus::Unreadable),
                        Some(
                            DependencyPredicate::StateFileUnreadable
                                | DependencyPredicate::StateFileUnsafe
                                | DependencyPredicate::StateSchemaInvalid
                                | DependencyPredicate::StateIdentityMismatch
                                | DependencyPredicate::RouteTableUnreadable,
                        ),
                    ) | (
                        Some(DependencyTupleStatus::Unavailable),
                        Some(
                            DependencyPredicate::StateNotReady
                                | DependencyPredicate::RouteOwnerMissing
                                | DependencyPredicate::OwnerUnverified
                                | DependencyPredicate::RouteNotMatching,
                        ),
                    )
                );
                marker_matches
                    && !dependency.name.trim().is_empty()
                    && dependency.runtime_generation.is_none()
                    && dependency.generation.is_none()
                    && dependency.address.is_none()
            }
            _ => false,
        }
    });
    if !observations_valid {
        return false;
    }
    match record.event {
        TransitionEvent::ChildExited => {
            record.observed_dependencies.is_empty()
                && record.inspection_error.is_none()
                && record
                    .exit
                    .is_some_and(|exit| exit.code.is_some() != exit.signal.is_some())
        }
        TransitionEvent::DependencyUnavailable => {
            record.exit.is_none()
                && record.inspection_error.is_none()
                && record.observed_dependencies.len() == 1
                && record.observed_dependencies[0].status == ObservedStatus::Unavailable
        }
        TransitionEvent::DependencyTupleChanged => {
            record.exit.is_none()
                && record.inspection_error.is_none()
                && !record.observed_dependencies.is_empty()
                && record
                    .observed_dependencies
                    .iter()
                    .all(|dependency| dependency.status == ObservedStatus::Observed)
        }
        TransitionEvent::InspectionError => {
            record.exit.is_none()
                && match record.inspection_error {
                    Some(InspectionCategory::ChildCompletion) => {
                        record.observed_dependencies.is_empty()
                    }
                    Some(InspectionCategory::DependencyState) => {
                        record.observed_dependencies.len() == 1
                            && record.observed_dependencies[0].status == ObservedStatus::Unavailable
                            && matches!(
                                record.observed_dependencies[0].tuple_status,
                                Some(
                                    DependencyTupleStatus::Unreadable
                                        | DependencyTupleStatus::Absent
                                )
                            )
                    }
                    None => false,
                }
        }
        _ => {
            record.exit.is_none()
                && record.observed_dependencies.is_empty()
                && record.inspection_error.is_none()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_context() -> TransitionContext {
        TransitionContext {
            owner: "owner-a".to_owned(),
            runtime_generation: "runtime-a".to_owned(),
            supervisor: TransitionProcess {
                pid: 41,
                boot_identity: "boot-a".to_owned(),
                start_identity: GatewayStartIdentity::Linux { start_ticks: 7 },
            },
        }
    }

    fn fresh_receipt_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("fresh receipt directory")
    }

    fn exit_record(writer: &TransitionEvidenceWriter, generation: &str) -> TransitionRecord {
        let mut record = writer.draft(TransitionEvent::ChildExited);
        record.generation = Some(generation.to_owned());
        record.exit = Some(TransitionExit {
            code: Some(17),
            signal: None,
        });
        record.phase = Some(TransitionPhase::Published);
        record
    }

    #[test]
    fn host_process_transition_evidence_appends_ordered_private_records() {
        let dir = fresh_receipt_dir();
        let path = dir.path().join("app.transitions.jsonl");
        let log = dir.path().join("app.log");
        let mut writer = TransitionEvidenceWriter::open(path.clone(), log, test_context());
        writer.record(exit_record(&writer, "gen-1"));
        writer.record(exit_record(&writer, "gen-2"));

        let receipt = read_transition_evidence(&path)
            .expect("readable receipt")
            .expect("receipt present");
        assert_eq!(receipt.records.len(), 2);
        assert_eq!(receipt.records[0].sequence, 1);
        assert_eq!(receipt.records[1].sequence, 2);
        assert_eq!(receipt.records[1].generation.as_deref(), Some("gen-2"));
        assert!(receipt.gaps.is_empty());
        assert!(!receipt.truncated && !receipt.incomplete_tail);
        let mode = fs::metadata(&path).expect("receipt metadata").permissions();
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(mode.mode() & 0o777, 0o600);
    }

    #[test]
    fn host_process_transition_evidence_resumes_sequence_after_reopen() {
        let dir = fresh_receipt_dir();
        let path = dir.path().join("app.transitions.jsonl");
        let log = dir.path().join("app.log");
        let mut first = TransitionEvidenceWriter::open(path.clone(), log.clone(), test_context());
        first.record(exit_record(&first, "gen-1"));
        drop(first);
        let mut second = TransitionEvidenceWriter::open(path.clone(), log, test_context());
        second.record(exit_record(&second, "gen-2"));

        let receipt = read_transition_evidence(&path)
            .expect("readable receipt")
            .expect("receipt present");
        let sequences = receipt
            .records
            .iter()
            .map(|record| record.sequence)
            .collect::<Vec<_>>();
        assert_eq!(sequences, vec![1, 2]);
        assert!(receipt.gaps.is_empty());
    }

    #[test]
    fn host_process_transition_evidence_retention_is_bounded_and_visible() {
        let dir = fresh_receipt_dir();
        let path = dir.path().join("app.transitions.jsonl");
        let log = dir.path().join("app.log");
        let limits = TransitionEvidenceLimits {
            max_records: 8,
            max_bytes: RETENTION_RESERVE_BYTES + 2048,
        };
        let mut writer =
            TransitionEvidenceWriter::open_with_limits(path.clone(), log, test_context(), limits);
        for index in 0..20 {
            writer.record(exit_record(&writer, &format!("gen-{index}")));
        }
        let raw = fs::read(&path).expect("bounded receipt");
        assert!(
            raw.len() <= limits.max_bytes,
            "receipt exceeded its byte bound"
        );
        let receipt = read_transition_evidence(&path)
            .expect("readable receipt")
            .expect("receipt present");
        assert!(receipt.truncated, "truncation must be visible to readers");
        assert_eq!(
            receipt.records.last().map(|record| record.event),
            Some(TransitionEvent::RetentionLimit)
        );
        assert!(receipt.records.len() < limits.max_records);
        assert!(receipt.gaps.is_empty());
    }

    #[test]
    fn host_process_transition_evidence_reader_reports_partial_and_foreign_lines() {
        let mut writer_context = test_context();
        writer_context.owner = "owner-b".to_owned();
        let dir = fresh_receipt_dir();
        let path = dir.path().join("app.transitions.jsonl");
        let log = dir.path().join("app.log");
        let mut writer = TransitionEvidenceWriter::open(path.clone(), log, writer_context);
        writer.record(exit_record(&writer, "gen-1"));
        let mut raw = fs::read(&path).expect("receipt bytes");
        raw.extend_from_slice(b"{\"schema\":\"effigy.managed.host-transition-evidence.v9\"}\n");
        raw.extend_from_slice(b"{not json\n");
        raw.extend_from_slice(
            b"{\"schema\":\"effigy.managed.host-transition-evidence.v1\",\"sequence\":5",
        );
        fs::write(&path, &raw).expect("write damaged receipt");

        let receipt = read_transition_evidence(&path)
            .expect("readable receipt")
            .expect("receipt present");
        assert_eq!(receipt.records.len(), 1);
        assert!(receipt.incomplete_tail, "unterminated tail must be visible");
        assert_eq!(
            receipt.gaps,
            vec![
                TransitionGap::UnsupportedSchema { line: 2 },
                TransitionGap::Malformed { line: 3 },
            ]
        );
    }

    #[test]
    fn host_process_transition_evidence_writer_isolates_interrupted_tail() {
        let dir = fresh_receipt_dir();
        let path = dir.path().join("app.transitions.jsonl");
        let log = dir.path().join("app.log");
        fs::write(
            &path,
            b"{\"schema\":\"effigy.managed.host-transition-evidence.v1\"",
        )
        .expect("torn write");
        let mut writer = TransitionEvidenceWriter::open(path.clone(), log, test_context());
        writer.record(exit_record(&writer, "gen-after-tear"));

        let receipt = read_transition_evidence(&path)
            .expect("readable receipt")
            .expect("receipt present");
        assert_eq!(receipt.records.len(), 1);
        assert_eq!(
            receipt.records[0].generation.as_deref(),
            Some("gen-after-tear")
        );
        assert_eq!(receipt.gaps, vec![TransitionGap::Malformed { line: 1 }]);
        assert!(!receipt.incomplete_tail);
    }

    #[test]
    fn host_process_transition_evidence_sequence_gap_is_visible() {
        let dir = fresh_receipt_dir();
        let path = dir.path().join("app.transitions.jsonl");
        let log = dir.path().join("app.log");
        let mut writer = TransitionEvidenceWriter::open(path.clone(), log, test_context());
        writer.record(exit_record(&writer, "gen-1"));
        // A failed append consumes its sequence number; simulate it here.
        writer.next_sequence += 1;
        writer.record(exit_record(&writer, "gen-3"));

        let receipt = read_transition_evidence(&path)
            .expect("readable receipt")
            .expect("receipt present");
        assert_eq!(
            receipt.gaps,
            vec![TransitionGap::SequenceGap {
                expected: 2,
                found: 3
            }]
        );
    }

    #[test]
    fn host_process_transition_evidence_failed_first_append_then_recovery_shows_initial_gap() {
        let dir = fresh_receipt_dir();
        let receipt_dir = dir.path().join("created-after-failure");
        let path = receipt_dir.join("app.transitions.jsonl");
        let log = dir.path().join("app.log");
        let mut writer = TransitionEvidenceWriter::open(path.clone(), log.clone(), test_context());

        writer.record(exit_record(&writer, "gen-lost"));
        assert!(
            !path.exists(),
            "the first append must fail at the real writer"
        );
        fs::create_dir(&receipt_dir).expect("recover receipt directory");
        writer.record(exit_record(&writer, "gen-recovered"));

        let receipt = read_transition_evidence(&path)
            .expect("read recovered receipt")
            .expect("recovered receipt exists");
        assert_eq!(receipt.records.len(), 1);
        assert_eq!(receipt.records[0].sequence, 2);
        assert_eq!(
            receipt.records[0].generation.as_deref(),
            Some("gen-recovered")
        );
        assert_eq!(
            receipt.gaps,
            vec![TransitionGap::SequenceGap {
                expected: 1,
                found: 2,
            }]
        );
        let logged = fs::read_to_string(log).expect("failed append is logged");
        assert!(
            logged.contains("record 1 was not confirmed durable"),
            "{logged}"
        );
    }

    #[test]
    fn host_process_transition_evidence_accepts_historical_v1_and_mixed_resume() {
        let dir = fresh_receipt_dir();
        let path = dir.path().join("app.transitions.jsonl");
        let log = dir.path().join("app.log");
        let mut v1 = exit_record(
            &TransitionEvidenceWriter::open(path.clone(), log.clone(), test_context()),
            "gen-v1",
        );
        v1.schema = TRANSITION_EVIDENCE_SCHEMA_V1.to_owned();
        v1.sequence = 1;
        let v1_line = serde_json::to_vec(&v1).expect("v1 record encoding");
        fs::write(&path, [v1_line.as_slice(), b"\n"].concat()).expect("write historical v1");

        let old = read_transition_evidence(&path)
            .expect("read historical v1")
            .expect("v1 receipt exists");
        assert_eq!(old.records.len(), 1);
        assert_eq!(old.records[0].schema, TRANSITION_EVIDENCE_SCHEMA_V1);
        assert!(old.gaps.is_empty());

        let mut writer = TransitionEvidenceWriter::open(path.clone(), log, test_context());
        writer.record(exit_record(&writer, "gen-v2"));
        let mixed = read_transition_evidence(&path)
            .expect("read mixed receipt")
            .expect("mixed receipt exists");
        assert_eq!(mixed.records.len(), 2);
        assert_eq!(mixed.records[0].schema, TRANSITION_EVIDENCE_SCHEMA_V1);
        assert_eq!(mixed.records[1].schema, TRANSITION_EVIDENCE_SCHEMA);
        assert_eq!(mixed.records[1].sequence, 2);
        assert!(mixed.gaps.is_empty());
    }

    #[test]
    fn host_process_transition_evidence_marks_invalid_sequence_values_and_order() {
        let dir = fresh_receipt_dir();
        let context = test_context();
        let writer = TransitionEvidenceWriter::open(
            dir.path().join("unused.transitions.jsonl"),
            dir.path().join("unused.log"),
            context,
        );
        let mut records = Vec::new();
        for sequence in [0, 1, 1, 3, 2, 4] {
            let mut record = exit_record(&writer, &format!("gen-{sequence}"));
            record.sequence = sequence;
            records.push(serde_json::to_vec(&record).unwrap());
        }
        let mut raw = Vec::new();
        for record in &records {
            raw.extend_from_slice(record);
            raw.push(b'\n');
        }
        let receipt = parse_transition_evidence(&raw);
        assert_eq!(
            receipt.gaps,
            vec![
                TransitionGap::SequenceGap {
                    expected: 1,
                    found: 0,
                },
                TransitionGap::SequenceGap {
                    expected: 2,
                    found: 1,
                },
                TransitionGap::SequenceGap {
                    expected: 2,
                    found: 3,
                },
                TransitionGap::SequenceGap {
                    expected: 4,
                    found: 2,
                },
            ]
        );
        assert_eq!(
            receipt
                .records
                .iter()
                .map(|record| record.sequence)
                .collect::<Vec<_>>(),
            vec![1, 3, 4]
        );
        assert_eq!(receipt.max_sequence, 4);
    }

    #[test]
    fn host_process_transition_evidence_rejects_partial_v2_and_mislabeled_v1_observations() {
        let dir = fresh_receipt_dir();
        let writer = TransitionEvidenceWriter::open(
            dir.path().join("unused.transitions.jsonl"),
            dir.path().join("unused.log"),
            test_context(),
        );
        let mut record = writer.draft(TransitionEvent::DependencyTupleChanged);
        record.sequence = 1;
        record.observed_dependencies = vec![ObservedDependency {
            name: "app".to_owned(),
            status: ObservedStatus::Observed,
            tuple_status: Some(DependencyTupleStatus::Observed),
            predicate: None,
            runtime_generation: Some("runtime-app".to_owned()),
            generation: Some("app-gen-2".to_owned()),
            address: Some("127.0.0.1:4000".to_owned()),
        }];
        let valid = serde_json::to_value(record).expect("valid v2 observation");

        let mut partial = valid.clone();
        partial["observed_dependencies"][0]
            .as_object_mut()
            .unwrap()
            .remove("tuple_status");
        let receipt = parse_transition_evidence(
            &[serde_json::to_vec(&partial).unwrap(), b"\n".to_vec()].concat(),
        );
        assert_eq!(receipt.gaps, vec![TransitionGap::Malformed { line: 1 }]);
        assert!(receipt.records.is_empty());

        let mut mislabeled = valid;
        mislabeled["schema"] = serde_json::Value::String(TRANSITION_EVIDENCE_SCHEMA_V1.to_owned());
        let receipt = parse_transition_evidence(
            &[serde_json::to_vec(&mislabeled).unwrap(), b"\n".to_vec()].concat(),
        );
        assert_eq!(receipt.gaps, vec![TransitionGap::Malformed { line: 1 }]);
        assert!(receipt.records.is_empty());
    }

    #[test]
    fn host_process_transition_evidence_creation_sync_retries_after_directory_error() {
        let dir = fresh_receipt_dir();
        let path = dir.path().join("app.transitions.jsonl");
        let mut pending = false;
        let mut sync_attempted = false;
        let first = write_appended_with_parent_sync(&path, b"first\n", &mut pending, |_| {
            sync_attempted = true;
            Err(std::io::Error::other("controlled directory sync failure"))
        });
        assert!(first.is_err());
        assert!(
            sync_attempted,
            "creation must sync its containing directory"
        );
        assert!(pending, "failed directory sync must remain pending");
        assert_eq!(fs::read(&path).unwrap(), b"first\n");

        write_appended(&path, b"second\n", &mut pending)
            .expect("next append retries the pending directory sync");
        assert!(!pending);
        assert_eq!(fs::read(&path).unwrap(), b"first\nsecond\n");
    }

    #[test]
    fn host_process_transition_evidence_absent_and_symlink_receipts_fail_closed() {
        let dir = fresh_receipt_dir();
        let absent = dir.path().join("absent.transitions.jsonl");
        assert!(read_transition_evidence(&absent)
            .expect("absent receipt is not an error")
            .is_none());

        let target = dir.path().join("target.jsonl");
        fs::write(&target, b"").expect("symlink target");
        let link = dir.path().join("link.transitions.jsonl");
        std::os::unix::fs::symlink(&target, &link).expect("symlink receipt");
        assert!(read_transition_evidence(&link).is_err());
    }

    #[test]
    fn host_process_transition_evidence_unwritable_receipt_keeps_running_and_logs() {
        let dir = fresh_receipt_dir();
        let blocked = dir.path().join("blocked");
        fs::create_dir(&blocked).expect("blocked directory");
        let path = blocked.join("app.transitions.jsonl");
        let log = dir.path().join("app.log");
        let mut writer = TransitionEvidenceWriter::open(path.clone(), log.clone(), test_context());
        fs::remove_dir(&blocked).expect("remove directory to make appends fail");
        writer.record(exit_record(&writer, "gen-lost"));

        assert!(!path.exists());
        let logged = fs::read_to_string(&log).expect("failure is logged");
        assert!(logged.contains("not confirmed durable"), "{logged}");
    }

    #[test]
    fn host_process_transition_evidence_assignments_exclude_unreserved_environment() {
        let mut env = BTreeMap::new();
        env.insert(
            "EFFIGY_MANAGED_HOST_APP_INTERNAL_URL".to_owned(),
            "http://127.0.0.1:4000".to_owned(),
        );
        env.insert(
            "EFFIGY_MANAGED_HOST_APP_PUBLIC_URL".to_owned(),
            "https://app.test".to_owned(),
        );
        env.insert(
            "EFFIGY_MANAGED_HOST_APP_STATE_FILE".to_owned(),
            "/state/app.listener.json".to_owned(),
        );
        env.insert(
            "EFFIGY_MANAGED_HOST_APP_GENERATION".to_owned(),
            "gen-app".to_owned(),
        );
        env.insert(
            "FIXTURE_SECRET_TOKEN".to_owned(),
            "secret-sentinel-value".to_owned(),
        );
        env.insert(
            "EFFIGY_MANAGED_HOST_APP_SECRET".to_owned(),
            "secret-sentinel-value".to_owned(),
        );

        env.insert(
            "EFFIGY_MANAGED_HOST_APP_EXTRA".to_owned(),
            "extra-value".to_owned(),
        );
        let assignments = reserved_assignments(&env);
        assert_eq!(assignments.len(), 4);
        assert!(assignments
            .values()
            .all(|value| value != "secret-sentinel-value"));
        assert!(!assignments.contains_key("FIXTURE_SECRET_TOKEN"));
        assert!(!assignments.contains_key("EFFIGY_MANAGED_HOST_APP_SECRET"));
        assert!(!assignments.contains_key("EFFIGY_MANAGED_HOST_APP_EXTRA"));
    }

    #[test]
    fn host_process_transition_evidence_names_unavailable_predicate_and_missing_state() {
        let mut record =
            TransitionRecord::new(TransitionEvent::DependencyUnavailable, &test_context());
        record.observed_dependencies = vec![ObservedDependency::unavailable(
            "app",
            DependencyPredicate::StateFileMissing,
        )];
        let rendered = serde_json::to_string(&record).expect("encode record");
        assert!(
            rendered.contains("\"predicate\":\"state_file_missing\""),
            "{rendered}"
        );
        assert!(
            rendered.contains("\"status\":\"unavailable\""),
            "{rendered}"
        );
        assert!(!rendered.contains("\"generation\""), "{rendered}");
    }
}
