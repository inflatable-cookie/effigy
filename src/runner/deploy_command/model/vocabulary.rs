//! The deployment model's two closed vocabularies.
//!
//! Each declaration is decoded once, in `derive_deploy_model`, into the type
//! that carries the fact from then on: the model output serializes these
//! types, so no later stage re-reads the manifest string or guesses a value
//! from a warning's prose.

use serde::Serialize;

use super::manifest::{ManifestDeployModelBackingService, ManifestDeployModelService};
use super::DeployWarning;
use crate::runner::error::RunnerError;

/// Service runtimes the deployment model understands.
///
/// `Static` describes output served as files with no application runtime;
/// provider packages use it instead of approximating a static site as `Node`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(in crate::runner::deploy_command) enum ServiceRuntime {
    Static,
    Node,
    Bun,
    Deno,
    Rust,
    Python,
    Go,
    Php,
    Ruby,
    Java,
}

const SERVICE_RUNTIMES: [ServiceRuntime; 10] = [
    ServiceRuntime::Static,
    ServiceRuntime::Node,
    ServiceRuntime::Bun,
    ServiceRuntime::Deno,
    ServiceRuntime::Rust,
    ServiceRuntime::Python,
    ServiceRuntime::Go,
    ServiceRuntime::Php,
    ServiceRuntime::Ruby,
    ServiceRuntime::Java,
];

impl ServiceRuntime {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Static => "static",
            Self::Node => "node",
            Self::Bun => "bun",
            Self::Deno => "deno",
            Self::Rust => "rust",
            Self::Python => "python",
            Self::Go => "go",
            Self::Php => "php",
            Self::Ruby => "ruby",
            Self::Java => "java",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        SERVICE_RUNTIMES
            .into_iter()
            .find(|runtime| runtime.as_str() == raw)
    }
}

/// Who owns a backing service.
///
/// `Managed` means the provider platform owns the resource. `External` means
/// the operator supplies an endpoint Effigy does not provision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(in crate::runner::deploy_command) enum BackingServiceMode {
    Managed,
    External,
}

const BACKING_SERVICE_MODES: [BackingServiceMode; 2] =
    [BackingServiceMode::Managed, BackingServiceMode::External];

impl BackingServiceMode {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Managed => "managed",
            Self::External => "external",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        BACKING_SERVICE_MODES
            .into_iter()
            .find(|mode| mode.as_str() == raw)
    }
}

/// Decode one declared service runtime, naming the allowed set when it is not
/// one of them.
pub(super) fn decode_service_runtime(
    service: &ManifestDeployModelService,
) -> Result<ServiceRuntime, RunnerError> {
    ServiceRuntime::parse(&service.runtime).ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "service `{}` declares unsupported runtime `{}` in `[deploy.model.services]`; allowed runtimes: {}",
            service.name,
            service.runtime,
            allowed(&SERVICE_RUNTIMES.map(ServiceRuntime::as_str)),
        ))
    })
}

/// Decode one declared backing-service mode, naming the allowed set when it is
/// not one of them.
pub(super) fn decode_backing_mode(
    service: &ManifestDeployModelBackingService,
) -> Result<BackingServiceMode, RunnerError> {
    BackingServiceMode::parse(&service.mode).ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "backing service `{}` declares unsupported mode `{}` in `[deploy.model.backing_services]`; allowed modes: {}",
            service.name,
            service.mode,
            allowed(&BACKING_SERVICE_MODES.map(BackingServiceMode::as_str)),
        ))
    })
}

/// The one-release nudge for a service that still describes a static site as
/// an application runtime.
///
/// Both facts the notice compares are decoded declarations, `role` and
/// `runtime`; nothing in a warning's free text can reclassify a service.
pub(super) fn static_site_approximation_notice(
    service: &ManifestDeployModelService,
    runtime: ServiceRuntime,
    environment_scoped: bool,
) -> Option<DeployWarning> {
    if !environment_scoped || service.role != "static" || runtime == ServiceRuntime::Static {
        return None;
    }
    Some(DeployWarning {
        code: "deploy-vocabulary-migration".to_owned(),
        scope: "service".to_owned(),
        target: Some(service.name.clone()),
        message: format!(
            "service `{}` is `role = \"static\"` but declares `runtime = \"{}\"`; declare `runtime = \"static\"` instead. The approximation is accepted for one release",
            service.name,
            runtime.as_str()
        ),
        severity: "warn".to_owned(),
    })
}

fn allowed(values: &[&'static str]) -> String {
    values.join(", ")
}

#[cfg(test)]
mod tests {
    use super::{
        decode_backing_mode, decode_service_runtime, static_site_approximation_notice,
        BackingServiceMode, ServiceRuntime,
    };
    use crate::runner::deploy_command::model::manifest::{
        ManifestDeployModelBackingService, ManifestDeployModelService,
    };
    use crate::runner::deploy_command::model::DeployWarning;

    fn service(runtime: &str) -> ManifestDeployModelService {
        ManifestDeployModelService {
            name: "web".to_owned(),
            role: "web".to_owned(),
            runtime: runtime.to_owned(),
            source_root: "app-web".to_owned(),
            build: None,
            start: None,
            release: None,
            health: None,
            output: None,
            port: None,
            domains: Vec::new(),
            env: None,
            secret_refs: None,
            volumes: None,
            warnings: None,
        }
    }

    fn backing_service(mode: &str, warnings: Option<Vec<DeployWarning>>) -> ManifestDeployModelBackingService {
        ManifestDeployModelBackingService {
            name: "store".to_owned(),
            kind: "object-storage".to_owned(),
            mode: mode.to_owned(),
            required: true,
            consumers: Vec::new(),
            warnings,
        }
    }

    #[test]
    fn a_declared_runtime_decodes_to_the_type_the_model_reports() {
        assert_eq!(
            decode_service_runtime(&service("node")).expect("node is a runtime"),
            ServiceRuntime::Node
        );
        assert_eq!(ServiceRuntime::Static.as_str(), "static");
        assert_eq!(
            serde_json::to_value(ServiceRuntime::Php).expect("serialize runtime"),
            serde_json::Value::String("php".to_owned())
        );
    }

    #[test]
    fn an_unsupported_runtime_is_refused_naming_the_allowed_set() {
        let error = decode_service_runtime(&service("js")).expect_err("js is not a runtime");
        let message = error.to_string();
        assert!(
            message.contains("unsupported runtime `js`"),
            "got: {message}"
        );
        assert!(
            message.contains("allowed runtimes: static, node, bun, deno, rust, python, go, php, ruby, java"),
            "message should name the whole closed set: {message}"
        );
    }

    #[test]
    fn a_backing_service_mode_decodes_once_and_is_never_read_from_warning_prose() {
        // The retired classification lowercased `code + message` and matched
        // "external", so this warning text used to turn a managed declaration
        // into a migration notice. The declared mode is the fact the model
        // reports; prose cannot reclassify it.
        let described_as_external = Some(vec![DeployWarning {
            code: "external-resource".to_owned(),
            scope: "backing_service".to_owned(),
            target: Some("store".to_owned()),
            message: "the endpoint is an external, operator-supplied resource".to_owned(),
            severity: "warn".to_owned(),
        }]);
        let managed = backing_service("managed", described_as_external);
        assert_eq!(
            decode_backing_mode(&managed).expect("managed is a mode"),
            BackingServiceMode::Managed
        );
        assert_eq!(BackingServiceMode::External.as_str(), "external");
    }

    #[test]
    fn an_unsupported_backing_mode_is_refused_naming_the_allowed_set() {
        let error = decode_backing_mode(&backing_service("magic", None))
            .expect_err("magic is not a mode");
        let message = error.to_string();
        assert!(
            message.contains("unsupported mode `magic`"),
            "got: {message}"
        );
        assert!(
            message.contains("allowed modes: managed, external"),
            "message should name the closed set: {message}"
        );
    }

    #[test]
    fn the_static_site_approximation_notice_compares_decoded_declarations_only() {
        let mut static_service = service("node");
        static_service.role = "static".to_owned();
        let notice = static_site_approximation_notice(&static_service, ServiceRuntime::Node, true)
            .expect("a static role with a node runtime is the accepted approximation");
        assert_eq!(notice.code, "deploy-vocabulary-migration");
        assert!(
            notice.message.contains("runtime = \"static\""),
            "notice should name the new value: {}",
            notice.message
        );
        assert!(
            static_site_approximation_notice(&static_service, ServiceRuntime::Node, false).is_none(),
            "the notice only appears for a declared environment"
        );
        assert!(
            static_site_approximation_notice(&static_service, ServiceRuntime::Static, true)
                .is_none(),
            "a static runtime is the current vocabulary, not an approximation"
        );
    }
}
