//! Workspace exceptions to strict resource fencing and verified resume evidence.
//! No record here is persisted; the existing reservation and journal own the facts.
use super::*;
use crate::run::{EffectSpec, EffectState};
use rusqlite::Connection;
use serde_json::Value;

/// Workspace overlap authenticates a *run*, never a generic resource target.
/// Its immutable admission record survives mapping replacement, so an older
/// session can still receive controls and finish its own journal.
pub(crate) fn check_run_fence(conn: &Connection, id: RunId, fence: &Fence) -> Result<(), RunError> {
    match run::check_workspace_fence(conn, fence) {
        Ok(()) => Ok(()),
        Err(error) => {
            let authorized: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM workspace_session_runs s \
                 JOIN runs r ON r.run_id=s.run_id \
                 LEFT JOIN workspace_activation_reservations a ON a.reservation_id=s.reservation_id \
                 WHERE s.run_id=?1 AND s.owner=?2 AND s.generation=?3 \
                 AND r.owner=s.owner AND r.generation=s.generation AND r.phase<>'closed' \
                 AND (s.reservation_id IS NULL OR a.state IN ('reserved','committed')))",
                params![id.to_string(), fence.resource.name().to_string(), fence.generation.get() as i64],
                |row| row.get(0),
            )?;
            if authorized {
                Ok(())
            } else {
                Err(error)
            }
        }
    }
}

/// New remote work may use the still-published predecessor's exact Borrow
/// attachment. Generic targets and environment mutations keep strict fencing.
pub(crate) fn check_intent_target(
    conn: &Connection,
    id: RunId,
    spec: &EffectSpec,
) -> Result<(), RunError> {
    match run::check_workspace_fence(conn, &spec.target) {
        Ok(()) => Ok(()),
        Err(error) => {
            let borrowing: bool = if spec.kind == crate::remote::KIND {
                conn.query_row(
                    "SELECT EXISTS(SELECT 1 FROM attachments WHERE run_id=?1 AND environment=?2 \
                     AND env_generation=?3 AND mode='\"Borrow\"' AND state='attached')",
                    params![
                        id.to_string(),
                        spec.target.resource.name().to_string(),
                        spec.target.generation.get() as i64
                    ],
                    |row| row.get(0),
                )?
            } else {
                false
            };
            if borrowing && crate::workspace_session::preserved_environment(conn, &spec.target)? {
                Ok(())
            } else {
                Err(error)
            }
        }
    }
}

/// Build resume evidence only from the reservation and effects read under the
/// same transaction. The caller retains the Env gate and revalidates after waits.
pub(crate) fn resume_facts(
    record: &ActivationReservationRecord,
    id: RunId,
    owner: &Fence,
    effects: &[run::Effect],
) -> Result<run::WorkspaceResumeFacts, RunError> {
    let session = record
        .session
        .clone()
        .ok_or_else(|| RunError::unavailable("workspace session binding unavailable"))?;
    let inputs = record
        .inputs
        .as_ref()
        .ok_or_else(|| RunError::unavailable("workspace frozen inputs unavailable"))?;
    let (target, platform) = run::workspace_bindings(effects)?;
    if platform.target != target {
        return Err(RunError::refused("workspace target and platform differ"));
    }
    let mut creations = effects
        .iter()
        .filter(|effect| effect.spec.kind == "environment:gate-scoped-id-difference");
    let creation = creations
        .next()
        .ok_or_else(|| RunError::unavailable("workspace creator receipt unavailable"))?;
    if creations.next().is_some()
        || creation.state != EffectState::Applied
        || creation.spec.target != *owner
    {
        return Err(RunError::unavailable(
            "workspace creator receipt is ambiguous or incomplete",
        ));
    }
    let receipt = creation
        .receipt
        .clone()
        .ok_or_else(|| RunError::unavailable("workspace creator receipt unavailable"))?;
    let record_value = receipt
        .get("record")
        .ok_or_else(|| RunError::unavailable("workspace creator receipt is malformed"))?;
    if receipt.get("schema").and_then(Value::as_str)
        != Some("effigy.environment.gate-scoped-creation.v5")
        || record_value.get("state").and_then(Value::as_str) != Some("attributed")
        || record_value.get("generation").and_then(Value::as_u64) != Some(owner.generation.get())
    {
        return Err(RunError::unavailable(
            "workspace creator receipt is malformed or stale",
        ));
    }
    // The kernel checks the complete v5 record's shape and target
    // correspondence. Backend-specific creator qualification remains the
    // container owner's responsibility before calling `resume`.
    let has_text = |value: &Value, keys: &[&str]| {
        keys.iter()
            .all(|key| value.get(key).and_then(Value::as_str).is_some())
    };
    let text = |key: &str| record_value.get(key).and_then(Value::as_str);
    let project = text("project")
        .filter(|s| !s.is_empty())
        .ok_or_else(|| RunError::unavailable("workspace creator project unavailable"))?;
    let primary = text("primary_service")
        .filter(|s| !s.is_empty())
        .ok_or_else(|| RunError::unavailable("workspace creator primary service unavailable"))?;
    if text("engine").is_none_or(str::is_empty)
        || text("context").is_none()
        || text("workdir").is_none_or(str::is_empty)
        || record_value.get("workspace_predecessor").is_none()
        || [
            "expected_created",
            "image_identities",
            "reference_resolutions",
        ]
        .iter()
        .any(|key| record_value.get(key).and_then(Value::as_object).is_none())
        || ["observed", "compose_files", "created", "attached"]
            .iter()
            .any(|key| record_value.get(key).and_then(Value::as_array).is_none())
    {
        return Err(RunError::unavailable(
            "workspace creator receipt is incomplete",
        ));
    }
    if let TargetEvidence::Absent {
        project: expected, ..
    } = &inputs.target
    {
        if expected != project {
            return Err(RunError::refused(
                "workspace creator project differs from frozen decision",
            ));
        }
    }
    let observed = record_value
        .get("observed")
        .and_then(Value::as_array)
        .ok_or_else(|| RunError::unavailable("workspace creator inventory unavailable"))?;
    let mut observed_ids = std::collections::BTreeSet::new();
    let mut target_rows = 0;
    for row in observed {
        let id = row
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| id.len() == 64 && id.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .ok_or_else(|| RunError::unavailable("workspace creator inventory is malformed"))?;
        let labels = row
            .get("labels")
            .and_then(Value::as_object)
            .ok_or_else(|| {
                RunError::unavailable("workspace creator inventory labels unavailable")
            })?;
        if !has_text(row, &["name", "image", "image_reference"])
            || row.get("running").and_then(Value::as_bool).is_none()
            || row.get("binds").and_then(Value::as_array).is_none()
            || !observed_ids.insert(id.to_owned())
            || labels
                .get("com.docker.compose.project")
                .and_then(Value::as_str)
                != Some(project)
        {
            return Err(RunError::unavailable(
                "workspace creator inventory is incomplete or ambiguous",
            ));
        }
        if id == target.container_id {
            if labels
                .get("com.docker.compose.service")
                .and_then(Value::as_str)
                != Some(primary)
            {
                return Err(RunError::refused(
                    "workspace creator target is not the primary service",
                ));
            }
            target_rows += 1;
        }
    }
    let images = record_value
        .get("image_identities")
        .and_then(Value::as_object)
        .ok_or_else(|| RunError::unavailable("workspace creator images unavailable"))?;
    if target_rows != 1 || !images.contains_key(primary) {
        return Err(RunError::unavailable(
            "workspace creator target or image identity unavailable",
        ));
    }
    for image in images.values() {
        let capability = image.get("capability").and_then(Value::as_str);
        if !has_text(image, &["rendered_reference", "observed_reference"])
            || image.get("resolved_id").is_none()
            || image.get("config_hash").is_none()
            || !matches!(
                capability,
                Some(
                    "container_image_id"
                        | "reference_resolved_id"
                        | "digest_reference"
                        | "attached_reference_config_hash"
                )
            )
        {
            return Err(RunError::unavailable(
                "workspace creator image identity is incomplete",
            ));
        }
    }
    if ["compose_files", "attached"].iter().any(|key| {
        record_value
            .get(key)
            .and_then(Value::as_array)
            .is_none_or(|rows| rows.iter().any(|row| row.as_str().is_none()))
    }) {
        return Err(RunError::unavailable(
            "workspace creator attachment or compose list is malformed",
        ));
    }
    let created = record_value
        .get("created")
        .and_then(Value::as_array)
        .ok_or_else(|| RunError::unavailable("workspace creator ID set unavailable"))?;
    let mut created_ids = Vec::with_capacity(created.len());
    for entry in created {
        let id = entry
            .get("container_id")
            .and_then(Value::as_str)
            .filter(|id| id.len() == 64 && id.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .ok_or_else(|| RunError::unavailable("workspace creator ID set is malformed"))?;
        if !has_text(
            entry,
            &[
                "name",
                "service",
                "image_id",
                "image_reference",
                "image_capability",
            ],
        ) || !observed_ids.contains(id)
        {
            return Err(RunError::unavailable(
                "workspace creator ID record is incomplete or absent from inventory",
            ));
        }
        created_ids.push(id.to_owned());
    }
    created_ids.sort();
    if created_ids.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(RunError::unavailable(
            "workspace creator ID set is ambiguous",
        ));
    }
    match &inputs.target {
        TargetEvidence::Observed(expected) if expected == &target => {}
        TargetEvidence::Absent { .. } if created_ids.contains(&target.container_id) => {}
        _ => {
            return Err(RunError::refused(
                "workspace target differs from frozen creation evidence",
            ))
        }
    }
    Ok(run::WorkspaceResumeFacts {
        reservation: record.id,
        environment: owner.resource.clone(),
        environment_run: id,
        environment_generation: owner.generation,
        session,
        selection: inputs.selection.clone(),
        target,
        platform,
        creator_receipt: receipt,
        created_ids,
    })
}
