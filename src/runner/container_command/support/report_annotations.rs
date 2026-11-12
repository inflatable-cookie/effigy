//! What a container command's report says about the runtime work it did.
//!
//! Each section is one fact a person reads as text and a machine reads in
//! `json`: gateway routes, shared services, data handover, source refreshes,
//! TCP aliases and warnings.

use serde_json::json;

pub(in crate::runner::container_command) fn annotate_shared_service_notes(
    report: &mut effigy_containers::ContainerCommandReport,
    notes: &[String],
) {
    if notes.is_empty() {
        return;
    }
    if let Some(json_object) = report.json.as_object_mut() {
        json_object.insert(
            "shared_service_actions".to_owned(),
            json!({
                "action": "ensured",
                "services": notes,
            }),
        );
    }
    for note in notes {
        report.success_text.push('\n');
        match note.starts_with("replaced shared ") {
            true => report.success_text.push_str(&format!("[shared] {note}")),
            false => report
                .success_text
                .push_str(&format!("[shared] ensured {note}")),
        }
    }
}

/// Surface `data_from` handover lines in the detached up report: one text
/// line per handover plus a machine-readable `data_handover` list.
pub(in crate::runner::container_command) fn annotate_data_handover_notes(
    report: &mut effigy_containers::ContainerCommandReport,
    notes: &[String],
) {
    if notes.is_empty() {
        return;
    }
    if let Some(json_object) = report.json.as_object_mut() {
        json_object.insert("data_handover".to_owned(), json!(notes));
    }
    for note in notes {
        report.success_text.push('\n');
        report.success_text.push_str(note);
    }
}

/// Bundle or catalog sources this Prepare refreshed, in the JSON envelope's
/// `notes` (contract 020). The one line per source is already on stderr.
pub(in crate::runner::container_command) fn annotate_source_update_notes(
    report: &mut effigy_containers::ContainerCommandReport,
) {
    let notes = effigy_manifest::bundle_update_notes();
    if notes.is_empty() {
        return;
    }
    if let Some(json_object) = report.json.as_object_mut() {
        json_object.insert("notes".to_owned(), json!(notes));
    }
}

pub(in crate::runner::container_command) fn annotate_tcp_alias_host_notes(
    report: &mut effigy_containers::ContainerCommandReport,
    notes: &[String],
) {
    if notes.is_empty() {
        return;
    }
    if let Some(json_object) = report.json.as_object_mut() {
        json_object.insert(
            "tcp_alias_hosts".to_owned(),
            json!({
                "action": "installed",
                "aliases": notes,
            }),
        );
    }
    for note in notes {
        report.success_text.push('\n');
        report
            .success_text
            .push_str(&format!("[gateway] installed container TCP alias {note}"));
    }
}

pub(in crate::runner::container_command) fn annotate_warning_lines(
    report: &mut effigy_containers::ContainerCommandReport,
    warnings: &[String],
) {
    if warnings.is_empty() {
        return;
    }
    if let Some(json_object) = report.json.as_object_mut() {
        json_object.insert("warnings".to_owned(), json!(warnings));
    }
    for warning in warnings {
        report.success_text.push('\n');
        report.success_text.push_str(&format!("[warn] {warning}"));
    }
}

#[cfg(test)]
pub(in crate::runner::container_command) fn annotate_left_running_shared_services(
    report: &mut effigy_containers::ContainerCommandReport,
    policy: &effigy_containers::EffectiveContainerPolicy,
) {
    if policy.shared_services.is_empty() {
        return;
    }
    let services = policy
        .shared_services
        .iter()
        .map(|service| {
            format!(
                "{} [{}] -> {}:{}",
                service.service_name, service.catalog, service.host, service.host_port
            )
        })
        .collect::<Vec<_>>();
    if let Some(json_object) = report.json.as_object_mut() {
        json_object.insert(
            "shared_service_actions".to_owned(),
            json!({
                "action": "left-running",
                "services": services,
            }),
        );
    }
    for service in services {
        report.success_text.push('\n');
        report
            .success_text
            .push_str(&format!("[shared] left running {service}"));
    }
}
