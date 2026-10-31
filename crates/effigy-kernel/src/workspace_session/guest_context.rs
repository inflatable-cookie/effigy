//! Guest-local fields frozen before publication; the v2 parser validates every
//! field against these inputs rather than admitting arbitrary document data.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuestPaths {
    pub host_checkout: String,
    pub guest_checkout: String,
    pub workdir: String,
    pub user: String,
    pub home: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuestSourcePin {
    pub kind: String,
    pub name: String,
    pub digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuestRoute {
    pub declared_domain: String,
    pub domain: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    pub tls: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuestContextInputs {
    /// Exact host active version, including local-build identity when present.
    pub version: String,
    pub protocol: u32,
    pub host_user: String,
    pub checkout_id: String,
    pub environment_resource: String,
    pub profile: String,
    pub runtime: Option<String>,
    pub vm_type: Option<String>,
    pub routes: Vec<GuestRoute>,
    pub paths: GuestPaths,
    pub sources: Vec<GuestSourcePin>,
}

fn absolute(path: &str) -> bool {
    path.starts_with('/')
        && !path.contains('\0')
        && !path.split('/').any(|part| part == ".." || part == ".")
}
impl GuestContextInputs {
    pub fn validate(&self) -> Result<(), RunError> {
        if self.version.trim().is_empty() || self.version.contains('\0') {
            return Err(RunError::refused(
                "guest version must be nonempty and contain no NUL",
            ));
        }
        if self.protocol != 1
            || [
                &self.host_user,
                &self.checkout_id,
                &self.environment_resource,
                &self.profile,
            ]
            .iter()
            .any(|value| value.trim().is_empty() || value.contains('\0'))
            || [&self.runtime, &self.vm_type].iter().any(|value| {
                value
                    .as_ref()
                    .is_some_and(|value| value.trim().is_empty() || value.contains('\0'))
            })
        {
            return Err(RunError::refused(
                "guest protocol, host, checkout, environment or backend identity is incomplete",
            ));
        }
        let mut routes = std::collections::BTreeSet::new();
        for route in &self.routes {
            if [&route.declared_domain, &route.domain].iter().any(|name| {
                name.is_empty() || name.chars().any(char::is_whitespace) || name.contains('\0')
            }) || route.port == Some(0)
                || route
                    .service
                    .as_ref()
                    .is_some_and(|name| name.trim().is_empty() || name.contains('\0'))
                || !routes.insert((&route.domain, &route.service, route.port))
            {
                return Err(RunError::refused(
                    "guest route identity is malformed or duplicated",
                ));
            }
        }
        for (field, path) in [
            ("host_checkout", &self.paths.host_checkout),
            ("guest_checkout", &self.paths.guest_checkout),
            ("workdir", &self.paths.workdir),
            ("home", &self.paths.home),
        ] {
            if !absolute(path) {
                return Err(RunError::refused(format!(
                    "guest paths.{field} must be absolute without dot components or NUL"
                )));
            }
        }
        if self.paths.user.trim().is_empty() || self.paths.user.contains('\0') {
            return Err(RunError::refused(
                "guest paths.user must be nonempty and contain no NUL",
            ));
        }
        let mut seen = std::collections::BTreeSet::new();
        for pin in &self.sources {
            if !matches!(
                pin.kind.as_str(),
                "bundle" | "catalog" | "manifest" | "source-lock" | "composed-manifest"
            ) || pin.name.trim().is_empty()
                || Digest::parse_hex(pin.digest.strip_prefix("sha256:").unwrap_or("")).is_none()
                || !seen.insert((&pin.kind, &pin.name))
                || pin
                    .snapshot
                    .as_ref()
                    .is_some_and(|snapshot| !absolute(snapshot))
            {
                return Err(RunError::refused(format!("guest source pin `{}` has an invalid kind, name, digest, duplicate identity or snapshot path", pin.name)));
            }
        }
        Ok(())
    }
}
