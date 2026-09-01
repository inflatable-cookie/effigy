//! `effigy service pack` — the catalog-pack acquisition surface.
//!
//! This module is the runner edge of the pack domain. It owns three things the
//! domain deliberately does not:
//!
//! - the OCI transport implementation, built on the existing artifact adapter
//!   in `effigy-artifacts` rather than a second transport client;
//! - text and JSON rendering for the four public shapes;
//! - the doctor finding for unhealthy installed state.
//!
//! There is no `update` shape here. The official channel is modelled in
//! `effigy_catalog::pack::channel` and adapter-tested, but it has no published
//! artifact, so exposing a public command would ship one that cannot succeed.

use std::path::Path;

use effigy_artifacts::{
    ArtifactSourceRef, OciArtifactAdapter, OciArtifactPullRequest, OrasCliArtifactAdapter,
};
use effigy_catalog::pack::{
    install_pack, select_pack, InstalledPackRecord, LocalPackAcquirer, PackAcquireRequest,
    PackAcquisition, PackCandidateAcquirer, PackCandidateSource, PackError, PackSelection,
    PackSelectionReason, PackStore, PackStoreState, StoredContentOutcome,
};
use effigy_cli::{ServicePackInstallSource, ServicePackSubcommand};
use effigy_doctor::{check_id, DoctorFinding, DoctorSeverity};
use serde_json::{json, Value};

use crate::runner::error::RunnerError;

/// The running Effigy version, used for pack compatibility checks.
pub(in crate::runner) fn effigy_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Acquires candidates through the existing OCI artifact adapter.
///
/// The adapter is injected so focused tests can drive the whole install
/// transaction without `oras`, a registry, or a network.
pub struct OciPackAcquirer<'a> {
    adapter: &'a dyn OciArtifactAdapter,
}

impl<'a> OciPackAcquirer<'a> {
    /// Wrap an adapter for pack acquisition.
    pub fn new(adapter: &'a dyn OciArtifactAdapter) -> Self {
        Self { adapter }
    }
}

impl PackCandidateAcquirer for OciPackAcquirer<'_> {
    fn acquire(&self, request: &PackAcquireRequest) -> Result<PackAcquisition, PackError> {
        let PackCandidateSource::Oci { reference } = &request.source else {
            return Err(PackError::AcquireFailed {
                origin: request.source.display(),
                reason: "OCI acquirer received a non-OCI candidate".to_owned(),
            });
        };
        let parsed = ArtifactSourceRef::parse(format!("oci://{reference}")).map_err(|error| {
            PackError::AcquireFailed {
                origin: format!("oci://{reference}"),
                reason: error.to_string(),
            }
        })?;
        let ArtifactSourceRef::Oci(oci) = parsed else {
            return Err(PackError::AcquireFailed {
                origin: format!("oci://{reference}"),
                reason: "reference did not resolve to an OCI artifact".to_owned(),
            });
        };
        let report = self
            .adapter
            .pull(&OciArtifactPullRequest {
                reference: oci,
                destination_root: request.destination.clone(),
            })
            .map_err(|error| PackError::AcquireFailed {
                origin: format!("oci://{reference}"),
                reason: error.to_string(),
            })?;
        Ok(PackAcquisition {
            payload_root: report.pulled_root,
            resolved_digest: report.descriptor.digest,
        })
    }
}

/// Dispatch one `service pack` shape.
pub(super) fn run_service_pack(
    subcommand: ServicePackSubcommand,
    output_json: bool,
) -> Result<String, RunnerError> {
    match subcommand {
        ServicePackSubcommand::Status => run_status(output_json),
        ServicePackSubcommand::Install { source } => {
            let adapter = OrasCliArtifactAdapter::default();
            run_install(&source, &OciPackAcquirer::new(&adapter), output_json)
        }
        ServicePackSubcommand::Rollback => run_rollback(output_json),
        ServicePackSubcommand::Reset => run_reset(output_json),
    }
}

fn run_status(output_json: bool) -> Result<String, RunnerError> {
    let selection = select_pack(effigy_version());
    let store = PackStore::user();
    let state = match store.as_ref().map(PackStore::load) {
        Some(Ok(state)) => Some(state),
        // A store this command cannot read is exactly what `status` exists to
        // report, so the failure becomes content instead of an error exit.
        Some(Err(_)) | None => None,
    };

    if output_json {
        return Ok(json!({
            "schema": "effigy.service.pack.status.v1",
            "schema_version": 1,
            "ok": true,
            "effigy_version": effigy_version(),
            "store_root": selection.store_root.as_deref().map(display_path),
            "selection": selection_payload(&selection),
            "active": state
                .as_ref()
                .and_then(PackStoreState::active_record)
                .map(record_payload),
            "previous": state
                .as_ref()
                .and_then(PackStoreState::previous_record)
                .map(record_payload),
            "installs": state
                .as_ref()
                .map(|state| state.installs.iter().map(record_payload).collect::<Vec<_>>())
                .unwrap_or_default(),
        })
        .to_string());
    }

    let mut lines = Vec::new();
    if let Some(warning) = selection.fallback_warning() {
        lines.push(warning);
    }
    lines.push(format!(
        "[service pack] selection: {} (effigy {})",
        selection_label(&selection),
        effigy_version()
    ));
    lines.push(format!(
        "store: {}",
        selection
            .store_root
            .as_deref()
            .map(display_path)
            .unwrap_or_else(|| "<unavailable>".to_owned())
    ));
    lines.push(format!("reason: {}", selection.reason.as_str()));
    if let Some(detail) = &selection.detail {
        lines.push(format!("detail: {detail}"));
    }
    match state.as_ref().and_then(PackStoreState::active_record) {
        Some(record) => lines.push(format!("active: {}", record_line(record))),
        None => lines.push("active: compiled baseline".to_owned()),
    }
    match state.as_ref().and_then(PackStoreState::previous_record) {
        Some(record) => lines.push(format!("rollback target: {}", record_line(record))),
        None => lines.push("rollback target: none".to_owned()),
    }
    if let Some(state) = state.as_ref() {
        for record in &state.installs {
            lines.push(format!("installed: {}", record_line(record)));
        }
    }
    Ok(lines.join("\n"))
}

fn run_install(
    source: &ServicePackInstallSource,
    oci: &dyn PackCandidateAcquirer,
    output_json: bool,
) -> Result<String, RunnerError> {
    let store = require_store()?;
    let candidate = match source {
        ServicePackInstallSource::Oci { reference } => {
            PackCandidateSource::parse_oci(reference).map_err(pack_error)?
        }
        ServicePackInstallSource::Path { path } => {
            PackCandidateSource::local(path).map_err(pack_error)?
        }
    };
    let acquirer: &dyn PackCandidateAcquirer = match candidate {
        PackCandidateSource::Oci { .. } => oci,
        PackCandidateSource::Local { .. } => &LocalPackAcquirer,
    };

    let report =
        install_pack(&store, acquirer, &candidate, effigy_version()).map_err(pack_error)?;

    if output_json {
        return Ok(json!({
            "schema": "effigy.service.pack.install.v1",
            "schema_version": 1,
            "ok": true,
            "installed": record_payload(&report.installed),
            "replaced": report.replaced,
            "stored_content": report.stored_content.as_str(),
            "previous": report.state.previous,
            "store_root": display_path(store.root()),
        })
        .to_string());
    }

    let mut lines = vec![format!(
        "[ok] installed and activated {}",
        record_line(&report.installed)
    )];
    lines.push(format!("source: {}", report.installed.source.display()));
    if let Some(digest) = report.installed.source.digest() {
        lines.push(format!("digest: {digest}"));
    }
    lines.push(format!("content: {}", report.installed.content_id));
    if report.stored_content == StoredContentOutcome::RepairedCorrupt {
        lines.push(
            "note: existing stored content failed identity verification and was replaced"
                .to_owned(),
        );
    }
    match &report.replaced {
        Some(replaced) => lines.push(format!("rollback target: {replaced}")),
        None => lines.push("rollback target: compiled baseline".to_owned()),
    }
    Ok(lines.join("\n"))
}

fn run_rollback(output_json: bool) -> Result<String, RunnerError> {
    let store = require_store()?;
    let state = store.rollback().map_err(pack_error)?;
    let active = state.active_record().cloned();

    if output_json {
        return Ok(json!({
            "schema": "effigy.service.pack.rollback.v1",
            "schema_version": 1,
            "ok": true,
            "active": active.as_ref().map(record_payload),
            "previous": state.previous,
        })
        .to_string());
    }

    let target = active
        .as_ref()
        .map(record_line)
        .unwrap_or_else(|| "compiled baseline".to_owned());
    Ok(format!("[ok] rolled back to {target}"))
}

fn run_reset(output_json: bool) -> Result<String, RunnerError> {
    let store = require_store()?;
    let state = store.reset().map_err(pack_error)?;

    if output_json {
        return Ok(json!({
            "schema": "effigy.service.pack.reset.v1",
            "schema_version": 1,
            "ok": true,
            "active": Value::Null,
            "previous": state.previous,
            "retained_installs": state.installs.len(),
        })
        .to_string());
    }

    let mut lines = vec!["[ok] selected the compiled baseline".to_owned()];
    // Reset is recoverable on purpose: content stays, overrides are untouched.
    lines.push(format!(
        "retained installs: {} (project and user overrides unchanged)",
        state.installs.len()
    ));
    match &state.previous {
        Some(previous) => lines.push(format!("rollback target: {previous}")),
        None => lines.push("rollback target: none".to_owned()),
    }
    Ok(lines.join("\n"))
}

/// Structured selection facts shared by `service list` and `service pack status`.
pub(super) fn selection_payload(selection: &PackSelection) -> Value {
    json!({
        "layer": if selection.uses_baseline() { "compiled-baseline" } else { "installed-pack" },
        "reason": selection.reason.as_str(),
        "fallback": selection.reason.is_fallback(),
        "detail": selection.detail,
        "pack_id": selection.active.as_ref().map(|record| record.pack_id.clone()),
        "pack_version": selection.active.as_ref().map(|record| record.pack_version.clone()),
        "install_id": selection.active.as_ref().map(|record| record.install_id.clone()),
    })
}

/// Doctor finding for an installed pack that is no longer usable.
///
/// Returns `None` on a healthy machine, including one that has never installed
/// a pack. The remediation names exactly one command.
pub(in crate::runner) fn pack_health_finding(selection: &PackSelection) -> Option<DoctorFinding> {
    if !selection.reason.is_fallback() {
        return None;
    }
    let repair = match rollback_target() {
        Some(record) => format!(
            "Run `effigy service pack rollback` to select the previous validated pack ({} {}).",
            record.pack_id, record.pack_version
        ),
        None => "Run `effigy service pack reset` to select the compiled baseline.".to_owned(),
    };
    Some(DoctorFinding {
        check_id: check_id::CATALOG_PACK_HEALTH.to_owned(),
        severity: DoctorSeverity::Warning,
        evidence: format!(
            "active catalog pack is unhealthy ({}); catalog resolution fell back to the compiled baseline{}",
            selection.reason.as_str(),
            selection
                .detail
                .as_ref()
                .map(|detail| format!(": {detail}"))
                .unwrap_or_default()
        ),
        remediation: repair,
        fixable: false,
    })
}

fn rollback_target() -> Option<InstalledPackRecord> {
    PackStore::user()?.load().ok()?.previous_record().cloned()
}

fn require_store() -> Result<PackStore, RunnerError> {
    PackStore::user().ok_or_else(|| {
        RunnerError::task_invocation(
            "cannot locate the Effigy user-state home; set `HOME` before managing catalog packs"
                .to_owned(),
        )
    })
}

fn pack_error(error: PackError) -> RunnerError {
    RunnerError::task_invocation(error.to_string())
}

fn selection_label(selection: &PackSelection) -> String {
    match (&selection.reason, selection.active.as_ref()) {
        (PackSelectionReason::ActivePack, Some(record)) => {
            format!("installed pack {} {}", record.pack_id, record.pack_version)
        }
        _ => "compiled baseline".to_owned(),
    }
}

fn record_line(record: &InstalledPackRecord) -> String {
    format!(
        "{} {} [{}] {} ({})",
        record.pack_id,
        record.pack_version,
        record.source.kind(),
        record.install_id,
        record.requires_effigy
    )
}

fn record_payload(record: &InstalledPackRecord) -> Value {
    json!({
        "install_id": record.install_id,
        "pack_id": record.pack_id,
        "pack_version": record.pack_version,
        "manifest_schema_version": record.manifest_schema_version,
        "requires_effigy": record.requires_effigy,
        "compatible": accepts_running_effigy(&record.requires_effigy),
        "source_type": record.source.kind(),
        "source": record.source.display(),
        "digest": record.source.digest(),
        "content_id": record.content_id,
        "installed_at_unix": record.installed_at_unix,
    })
}

/// Whether a recorded compatibility requirement still accepts this build.
///
/// Reported per install so `status` can show that a stored pack has aged out
/// of compatibility before anyone tries to activate it.
fn accepts_running_effigy(requirement: &str) -> bool {
    let (Ok(requirement), Ok(mut version)) = (
        semver::VersionReq::parse(requirement),
        semver::Version::parse(effigy_version()),
    ) else {
        return false;
    };
    version.pre = semver::Prerelease::EMPTY;
    requirement.matches(&version)
}

fn display_path(path: &Path) -> String {
    path.display().to_string()
}

#[cfg(test)]
#[path = "pack/tests.rs"]
mod tests;
