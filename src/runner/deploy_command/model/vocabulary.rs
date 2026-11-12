use super::manifest::ManifestDeployModelConfig;
use super::DeployWarning;
use crate::runner::error::RunnerError;

/// provider packages use it instead of approximating a static site as `node`.
const SERVICE_RUNTIMES: [&str; 10] = [
    "static", "node", "bun", "deno", "rust", "python", "go", "php", "ruby", "java",
];

/// Backing-service modes the deployment model understands.
///
/// `managed` means the provider platform owns the resource. `external` means
/// the operator supplies an endpoint Effigy does not provision.
const BACKING_SERVICE_MODES: [&str; 2] = ["managed", "external"];

pub(super) fn validate_vocabularies(
    model: &mut ManifestDeployModelConfig,
    environment_scoped: bool,
) -> Result<(), RunnerError> {
    for service in &mut model.services {
        if !SERVICE_RUNTIMES.contains(&service.runtime.as_str()) {
            return Err(RunnerError::task_invocation(format!(
                "service `{}` declares unsupported runtime `{}` in `[deploy.model.services]`; allowed runtimes: {}",
                service.name,
                service.runtime,
                SERVICE_RUNTIMES.join(", ")
            )));
        }
        let notices = if environment_scoped
            && service.role == "static"
            && service.runtime != "static"
        {
            vec![DeployWarning {
                code: "deploy-vocabulary-migration".to_owned(),
                scope: "service".to_owned(),
                target: Some(service.name.clone()),
                message: format!(
                    "service `{}` is `role = \"static\"` but declares `runtime = \"{}\"`; declare `runtime = \"static\"` instead. The approximation is accepted for one release",
                    service.name, service.runtime
                ),
                severity: "warn".to_owned(),
            }]
        } else {
            Vec::new()
        };
        push_warnings(&mut service.warnings, notices);
    }

    for service in &mut model.backing_services {
        if !BACKING_SERVICE_MODES.contains(&service.mode.as_str()) {
            return Err(RunnerError::task_invocation(format!(
                "backing service `{}` declares unsupported mode `{}` in `[deploy.model.backing_services]`; allowed modes: {}",
                service.name,
                service.mode,
                BACKING_SERVICE_MODES.join(", ")
            )));
        }
        let mentions_external = service
            .warnings
            .as_ref()
            .into_iter()
            .flatten()
            .any(|warning| {
                format!("{} {}", warning.code, warning.message)
                    .to_ascii_lowercase()
                    .contains("external")
            });
        let notices = if environment_scoped && service.mode == "managed" && mentions_external {
            vec![DeployWarning {
                code: "deploy-vocabulary-migration".to_owned(),
                scope: "backing_service".to_owned(),
                target: Some(service.name.clone()),
                message: format!(
                    "backing service `{}` is `mode = \"managed\"` but its warnings describe an external resource; declare `mode = \"external\"` instead. The approximation is accepted for one release",
                    service.name
                ),
                severity: "warn".to_owned(),
            }]
        } else {
            Vec::new()
        };
        push_warnings(&mut service.warnings, notices);
    }

    Ok(())
}

fn push_warnings(target: &mut Option<Vec<DeployWarning>>, warnings: Vec<DeployWarning>) {
    if warnings.is_empty() {
        return;
    }
    target.get_or_insert_with(Vec::new).extend(warnings);
}

