//! The exact backend of one environment (contract 052 §10.3).
//!
//! One precedence, used by every Compose, volume and data action: an explicit
//! `EFFIGY_COMPOSE_BACKEND`, the environment's `resources` record, the user
//! preference, then the driver default. An unreadable record is `Unknown`:
//! ordinary activation may fall through, but volume and data actions refuse
//! until the backend is named.
use std::path::Path;

use effigy_kernel::observe::Observed;
use effigy_kernel::store::BackendMetadata;
use effigy_manifest::ManifestContainerDriver;

use crate::{BackendId, ContainerPolicyError, EffectiveContainerPolicy};

/// The runtime endpoint volume commands use for the Docker engine. Every
/// other endpoint names a Colima profile.
pub const DOCKER_RUNTIME_ENDPOINT: &str = "docker";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendSource {
    /// `EFFIGY_COMPOSE_BACKEND` for this invocation.
    Environment,
    /// The environment's `resources` row, with the source that chose it.
    Recorded(String),
    UserPreference,
    DriverDefault,
}

impl BackendSource {
    pub fn label(&self) -> String {
        match self {
            Self::Environment => "environment".into(),
            Self::Recorded(source) => format!("recorded ({source})"),
            Self::UserPreference => "user-preference".into(),
            Self::DriverDefault => "driver-default".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExactBackend {
    pub id: BackendId,
    pub source: BackendSource,
    /// Why a recorded backend could not be read. Grants nothing (R3).
    pub unknown: Option<String>,
}

impl ExactBackend {
    /// The backend a volume or data action may target.
    pub fn require(&self, environment: &str) -> Result<&BackendId, ContainerPolicyError> {
        match &self.unknown {
            None => Ok(&self.id),
            Some(reason) => Err(ContainerPolicyError::TaskInvocation(format!(
                "container `{environment}` has an unknown recorded backend ({reason}); volume and data actions refuse until it is named, for example `EFFIGY_COMPOSE_BACKEND=docker` or `EFFIGY_COMPOSE_BACKEND=colima`"
            ))),
        }
    }
}

fn from_engine(engine: &str) -> Result<BackendId, String> {
    match engine {
        "docker-compose" => Ok(BackendId::docker_compose()),
        "colima-nerdctl" => Ok(BackendId::colima_nerdctl()),
        _ => Err(format!("recorded backend `{engine}` is not a known engine")),
    }
}

/// Resolve the exact backend. Observe-safe: reads the store, writes nothing.
pub fn exact_backend(
    repo_root: &Path,
    name: &str,
    driver: ManifestContainerDriver,
) -> ExactBackend {
    let mut unknown = None;
    let mut chosen =
        crate::compose::environment_backend_override().map(|id| (id, BackendSource::Environment));
    if chosen.is_none() {
        match crate::policy::render::recorded_backend(repo_root, name) {
            Observed::Verified(Some(record), _) => match from_engine(&record.engine) {
                Ok(id) => chosen = Some((id, BackendSource::Recorded(record.source))),
                Err(reason) => unknown = Some(reason),
            },
            Observed::Verified(None, _) | Observed::Absent(_) => {}
            Observed::Unknown(reason) => unknown = Some(format!("{reason}")),
        }
    }
    let (id, source) = chosen
        .or_else(|| {
            crate::user_global_backend_preference().map(|id| (id, BackendSource::UserPreference))
        })
        .unwrap_or_else(|| {
            let id = match driver {
                ManifestContainerDriver::Colima => BackendId::colima_nerdctl(),
            };
            (id, BackendSource::DriverDefault)
        });
    ExactBackend {
        id,
        source,
        unknown,
    }
}

/// Apply invocation override and user preference before ambient detection.
pub(crate) fn ambient_detection(
    mut detection: crate::ContainerBackendDetection,
) -> crate::ContainerBackendDetection {
    if detection.backend_override.is_none() {
        detection.backend_override = crate::user_global_backend_preference();
    }
    detection
}

/// Select the ambient engine without changing an explicitly named ID.
pub(crate) fn detected_backend(detection: &crate::ContainerBackendDetection) -> BackendId {
    detection.backend_override.clone().unwrap_or_else(|| {
        if detection.docker_cli_available {
            BackendId::docker_compose()
        } else {
            BackendId::colima_nerdctl()
        }
    })
}

/// The runtime endpoint for this environment's volume and data actions.
pub fn runtime_endpoint(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<String, ContainerPolicyError> {
    let exact = exact_backend(repo_root, &policy.name, policy.driver);
    Ok(
        if *exact.require(&policy.name)? == BackendId::docker_compose() {
            DOCKER_RUNTIME_ENDPOINT.to_owned()
        } else {
            policy.profile.clone()
        },
    )
}

/// The endpoint for profile-wide inventory with no environment: an explicit
/// profile, else an explicit Docker backend, else the user's Colima profile.
pub fn global_endpoint(profile: Option<&str>) -> String {
    if let Some(profile) = profile {
        return profile.to_owned();
    }
    if crate::compose::environment_backend_override() == Some(BackendId::docker_compose()) {
        return DOCKER_RUNTIME_ENDPOINT.to_owned();
    }
    crate::user_global_colima_profile().unwrap_or_else(|| crate::DEFAULT_COLIMA_PROFILE.to_owned())
}

/// The backend a runtime endpoint names. The Docker endpoint is exact. A
/// Colima profile name is what profile-only records (runtime scopes) carry, so
/// an explicit `EFFIGY_COMPOSE_BACKEND` still selects their engine; for an
/// environment that override is already part of its exact backend.
pub fn endpoint_backend(endpoint: &str) -> BackendId {
    if endpoint == DOCKER_RUNTIME_ENDPOINT {
        BackendId::docker_compose()
    } else {
        crate::compose::environment_backend_override().unwrap_or_else(BackendId::colima_nerdctl)
    }
}

/// The metadata Prepare records, or `None` when the record already holds it
/// or the backend is unknown (nothing is recorded from a guess).
pub(crate) fn metadata_to_record(policy: &EffectiveContainerPolicy) -> Option<BackendMetadata> {
    let exact = exact_backend(&policy.repo_root, &policy.name, policy.driver);
    if exact.unknown.is_some() || matches!(exact.source, BackendSource::Recorded(_)) {
        return None;
    }
    let selected = crate::environment::select_backend(&policy.repo_root, policy);
    if selected.engine != exact.id || selected.context.trim().is_empty() {
        return None;
    }
    Some(BackendMetadata {
        engine: exact.id.as_str().to_owned(),
        context: selected.context,
        source: exact.source.label(),
    })
}

#[cfg(test)]
#[path = "backend/tests.rs"]
mod tests;
