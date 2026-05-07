use effigy_containers::ContainerCommandReport;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredGatewayRoute {
    pub domain: String,
    pub target: Option<String>,
    pub dns_ip: Option<std::net::Ipv4Addr>,
    pub tls: bool,
}

pub(super) fn render_container_report(report: ContainerCommandReport, output_json: bool) -> String {
    if output_json {
        report.json.to_string()
    } else {
        report.success_text
    }
}

pub(super) fn annotate_registered_gateway_routes(
    report: &mut ContainerCommandReport,
    routes: &[RegisteredGatewayRoute],
) {
    if routes.is_empty() {
        return;
    }
    if let Some(json_object) = report.json.as_object_mut() {
        json_object.insert(
            "gateway_routes".to_owned(),
            serde_json::json!(routes
                .iter()
                .map(|route| serde_json::json!({
                    "action": "registered",
                    "domain": route.domain,
                    "target": route.target,
                    "dns_ip": route.dns_ip.map(|value| value.to_string()),
                    "tls": route.tls,
                }))
                .collect::<Vec<_>>()),
        );
    }
    for route in routes {
        report.success_text.push('\n');
        match route.target.as_deref() {
            Some(target) => report.success_text.push_str(&format!(
                "[gateway] registered {} -> {}",
                route.domain, target
            )),
            None => report.success_text.push_str(&format!(
                "[gateway] registered {} -> dns {}",
                route.domain,
                route
                    .dns_ip
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "default".to_owned())
            )),
        }
    }
}

pub(super) fn annotate_shared_service_notes(report: &mut ContainerCommandReport, notes: &[String]) {
    if notes.is_empty() {
        return;
    }
    if let Some(json_object) = report.json.as_object_mut() {
        json_object.insert(
            "shared_service_actions".to_owned(),
            serde_json::json!({
                "action": "ensured",
                "services": notes,
            }),
        );
    }
    for note in notes {
        report.success_text.push('\n');
        report.success_text.push_str(&format!("[shared] {note}"));
    }
}

pub(super) fn annotate_warning_lines(report: &mut ContainerCommandReport, warnings: &[String]) {
    if warnings.is_empty() {
        return;
    }
    if let Some(json_object) = report.json.as_object_mut() {
        json_object.insert("warnings".to_owned(), serde_json::json!(warnings));
    }
    for warning in warnings {
        report.success_text.push('\n');
        report.success_text.push_str(&format!("[warn] {warning}"));
    }
}
