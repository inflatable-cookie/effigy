//! The v0.14.1 marker importer (contract 052 §11, contract 053 §6).
//!
//! An existing stack's lifecycle markers (`.persistent`, the lease, `.complete`,
//! `.partial.json`, `.handover.json`) and its workloads' ownership labels
//! become one environment record, automatically and idempotently:
//!
//! - legacy files are only read, never changed or removed (M3);
//! - running workloads are adopted only when their labels or a bind mount of
//!   this checkout prove ownership; nothing is started, stopped or recreated
//!   (M7); unprovable ownership imports as `Unknown` and blocks destructive
//!   action (M4);
//! - an environment that already has an engine record is never re-imported.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use effigy_kernel::gate::Wait;
use effigy_kernel::id::Digest;
use effigy_kernel::observe::Observed;
use serde_json::Value;

use super::backend::{BackendSelection, Workload};
use super::reconcile::{EnvContext, EnvError, EnvRef, EnvRun, Operation};
use super::record::{
    EngineRecord, EnvRecord, ImportedMarker, LabelEvidence, LegacyImport, Retention,
};

/// Largest marker file the importer reads (a partial admission is ~2 KiB).
const MARKER_MAX: u64 = 256 << 10;

/// The legacy lifecycle markers of one environment, read without effects.
#[derive(Debug, Clone, Default)]
pub struct LegacyMarkers {
    pub persistent: Option<Marker>,
    pub lease: Option<(Marker, Option<u128>)>,
    pub complete: Option<Marker>,
    pub partial: Option<(Marker, Option<Value>)>,
    pub handover: Option<Marker>,
}

#[derive(Debug, Clone)]
pub struct Marker {
    pub path: PathBuf,
    pub sha256: String,
}

impl LegacyMarkers {
    /// Reads `<dir>/<name>.{persistent,json,complete,partial.json,handover.json}`.
    /// A missing directory or file is no marker; an unreadable one is recorded
    /// with an empty digest so the import says so.
    pub fn read(dir: &Path, name: &str) -> Self {
        let stem = sanitize(name);
        let marker = |suffix: &str| read_marker(&dir.join(format!("{stem}{suffix}")));
        let lease = marker(".json").map(|(marker, bytes)| {
            let expires = serde_json::from_slice::<Value>(&bytes)
                .ok()
                .and_then(|value| value.get("expires_at_epoch_ms").and_then(Value::as_u64))
                .map(u128::from);
            (marker, expires)
        });
        let partial = marker(".partial.json")
            .map(|(marker, bytes)| (marker, serde_json::from_slice::<Value>(&bytes).ok()));
        Self {
            persistent: marker(".persistent").map(|(marker, _)| marker),
            lease,
            complete: marker(".complete").map(|(marker, _)| marker),
            partial,
            handover: marker(".handover.json").map(|(marker, _)| marker),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.persistent.is_none()
            && self.lease.is_none()
            && self.complete.is_none()
            && self.partial.is_none()
            && self.handover.is_none()
    }

    /// An unexpired v0.14.1 lease whose reaper may still be sleeping.
    pub fn lease_pending(&self, now_ms: u128) -> Option<Duration> {
        let expires = self.lease.as_ref()?.1?;
        (expires > now_ms).then(|| Duration::from_millis((expires - now_ms) as u64))
    }

    /// The retention the markers imply. v0.14.1 idle-stopped only a stack
    /// with a lease and no persistent owner; every other stack stayed up
    /// until an explicit `down`, so it imports as persistent.
    pub fn retention(&self) -> Retention {
        if self.lease.is_some() && self.persistent.is_none() {
            Retention::Held
        } else {
            Retention::Persistent
        }
    }

    fn list(&self) -> Vec<ImportedMarker> {
        [
            self.persistent.as_ref(),
            self.lease.as_ref().map(|(marker, _)| marker),
            self.complete.as_ref(),
            self.partial.as_ref().map(|(marker, _)| marker),
            self.handover.as_ref(),
        ]
        .into_iter()
        .flatten()
        .map(|marker| ImportedMarker {
            path: marker.path.display().to_string(),
            sha256: marker.sha256.clone(),
        })
        .collect()
    }
}

fn read_marker(path: &Path) -> Option<(Marker, Vec<u8>)> {
    use std::io::Read;
    let file = std::fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    let read = file.take(MARKER_MAX).read_to_end(&mut bytes);
    let sha256 = match read {
        Ok(_) => Digest::of(&bytes)
            .0
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
        Err(_) => String::new(),
    };
    Some((
        Marker {
            path: path.to_path_buf(),
            sha256,
        },
        bytes,
    ))
}

/// Same component rule the v0.14.1 writer used.
pub fn sanitize(value: &str) -> String {
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

/// The checkout identity a workload must carry to be adopted.
#[derive(Debug, Clone)]
pub struct Ownership {
    /// Canonical checkout path.
    pub checkout: PathBuf,
    /// The worktree scope token, when the checkout is scoped.
    pub scope: Option<String>,
}

impl Ownership {
    /// Positive ownership: matching checkout and scope labels, or, for a
    /// pre-label container of an unscoped checkout, a bind mount of exactly
    /// this checkout. Anything else is not evidence.
    pub fn owns(&self, workload: &Workload) -> bool {
        let label = |key: &str| workload.labels.get(key).map(String::as_str);
        let checkout = label(crate::runtime::scope::CHECKOUT_LABEL)
            .or_else(|| label("com.docker.compose.project.working_dir"));
        let scope = label(crate::runtime::scope::SCOPE_LABEL);
        let scope_matches = match (self.scope.as_deref().map(str::trim), scope) {
            (None, None) => true,
            (Some(token), Some(value)) => !token.is_empty() && token == value,
            _ => false,
        };
        if let Some(checkout) = checkout {
            return scope_matches
                && Path::new(checkout).canonicalize().ok().as_deref() == Some(&self.checkout);
        }
        self.scope.is_none()
            && scope.is_none()
            && workload.binds.iter().any(|source| {
                Path::new(source).canonicalize().ok().as_deref() == Some(&self.checkout)
            })
    }
}

/// The evidence the labels give for a project's workloads.
pub fn label_evidence(workloads: &Observed<Vec<Workload>>, ownership: &Ownership) -> LabelEvidence {
    match workloads {
        Observed::Verified(rows, _) if rows.is_empty() => LabelEvidence::Absent,
        Observed::Verified(rows, _) => match rows.iter().find(|row| !ownership.owns(row)) {
            None => LabelEvidence::Verified {
                workloads: rows.len(),
            },
            Some(row) => LabelEvidence::Unknown(format!(
                "workload `{}` does not prove it belongs to this checkout",
                row.name
            )),
        },
        Observed::Absent(_) => LabelEvidence::Absent,
        Observed::Unknown(reason) => LabelEvidence::Unknown(format!("{reason:?}")),
    }
}

/// What one import did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Imported {
    /// The environment already had a record; nothing changed.
    AlreadyRecorded,
    /// Neither markers nor workloads exist: nothing to carry over.
    Nothing,
    /// A record was created from the markers and labels.
    Recorded {
        retention: Retention,
        active: bool,
        labels: LabelEvidence,
    },
}

/// One import of a legacy environment into its kernel record.
#[derive(Debug)]
pub struct ImportRequest<'a> {
    pub env: &'a EnvRef,
    pub project: &'a str,
    pub markers: &'a LegacyMarkers,
    pub workloads: Observed<Vec<Workload>>,
    pub ownership: &'a Ownership,
    /// The primary service whose running workload makes the stack active.
    pub primary_service: &'a str,
    pub backend: &'a BackendSelection,
}

/// Imports idempotently. A pending v0.14.1 lease refuses: its reaper may
/// still stop the stack, so adoption waits for it (contract 052 §11).
pub fn import(ctx: &EnvContext<'_>, request: ImportRequest<'_>) -> Result<Imported, EnvError> {
    if let Observed::Verified(record, _) =
        EnvRecord::read(&ctx.observe(), ctx.root, &request.env.resource)
    {
        if record.engine.is_some() && !resumable(&record) {
            return Ok(Imported::AlreadyRecorded);
        }
    }
    let labels = label_evidence(&request.workloads, request.ownership);
    if request.markers.is_empty() && labels == LabelEvidence::Absent {
        return Ok(Imported::Nothing);
    }
    if let Some(remaining) = request.markers.lease_pending(now_ms()) {
        return Err(EnvError::Invalid(format!(
            "environment `{}` still has a v0.14.1 idle-stop lease for {}s; its reaper may stop the stack. Nothing was adopted; retry after it expires",
            request.env.name,
            remaining.as_secs().max(1)
        )));
    }
    let mut run = EnvRun::begin(ctx, request.env, Operation::Import, Wait::FailFast)?;
    if run.record().engine.is_some() && !resumable(run.record()) {
        run.finish(true)?;
        return Ok(Imported::AlreadyRecorded);
    }
    let retention = request.markers.retention();
    let mut record = EngineRecord::new(retention, request.project);
    record.imported = Some(LegacyImport {
        markers: request.markers.list(),
        labels: labels.clone(),
        pending: true,
    });
    let primary_running = matches!(&request.workloads, Observed::Verified(rows, _) if rows.iter().any(|row| {
        row.running
            && row.labels.get("com.docker.compose.service").map(String::as_str)
                == Some(request.primary_service)
    }));
    let active = matches!(labels, LabelEvidence::Verified { .. }) && primary_running;
    let result = (|| {
        run.write_record(&record)?;
        if active {
            // Adoption publishes the running stack as applied without any
            // backend call: nothing restarts because of an import (M7).
            run.activate(&[])?;
        }
        if let Some(imported) = record.imported.as_mut() {
            imported.pending = false;
        }
        run.write_record(&record)?;
        Ok::<_, EnvError>(())
    })();
    let ok = result.is_ok();
    run.finish(ok)?;
    result.map(|()| Imported::Recorded {
        retention,
        active,
        labels,
    })
}

/// An import a killed process left between its record and adoption steps.
fn resumable(record: &EnvRecord) -> bool {
    record
        .engine()
        .and_then(|engine| engine.imported.as_ref())
        .is_some_and(|imported| imported.pending)
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
