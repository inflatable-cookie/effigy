use std::collections::BTreeMap;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_yaml::Value as YamlValue;

use crate::error::CatalogError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EffectiveStackPlan {
    pub project_name: String,
    pub network_name: String,
    pub services: IndexMap<String, StackServicePlan>,
}

impl EffectiveStackPlan {
    pub(crate) fn from_service_definitions(
        project_name: &str,
        definitions: &IndexMap<String, YamlValue>,
    ) -> Result<Self, CatalogError> {
        let services = definitions
            .iter()
            .map(|(name, definition)| {
                StackServicePlan::from_yaml(name, definition).map(|service| (name.clone(), service))
            })
            .collect::<Result<IndexMap<_, _>, _>>()?;
        Ok(Self {
            project_name: project_name.to_owned(),
            network_name: format!("{project_name}-default"),
            services,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StackServicePlan {
    pub name: String,
    pub image: Option<String>,
    pub build: Option<StackBuildPlan>,
    pub command: Option<StackCommandPlan>,
    pub environment: BTreeMap<String, String>,
    pub user: Option<String>,
    pub working_dir: Option<String>,
    pub mounts: Vec<StackMountPlan>,
    pub tmpfs: Vec<String>,
    pub ports: Vec<StackPortPlan>,
    pub dependencies: Vec<StackDependencyPlan>,
    pub readiness: Option<StackReadinessPlan>,
    pub resources: StackResourcePlan,
}

impl StackServicePlan {
    fn from_yaml(name: &str, value: &YamlValue) -> Result<Self, CatalogError> {
        let raw: RawService = serde_yaml::from_value(value.clone()).map_err(|error| {
            CatalogError::UnsupportedStackPlan {
                service: name.to_owned(),
                reason: error.to_string(),
            }
        })?;
        if raw.image.is_none() && raw.build.is_none() {
            return Err(stack_error(
                name,
                "service must define either `image` or `build`",
            ));
        }
        Ok(Self {
            name: name.to_owned(),
            image: raw.image,
            build: raw
                .build
                .map(|build| StackBuildPlan::from_raw(name, build))
                .transpose()?,
            command: raw.command.map(StackCommandPlan::from),
            environment: parse_environment(name, raw.environment)?,
            user: raw.user,
            working_dir: raw.working_dir,
            mounts: raw
                .volumes
                .into_iter()
                .map(|mount| parse_mount(name, &mount))
                .collect::<Result<Vec<_>, _>>()?,
            tmpfs: raw.tmpfs,
            ports: raw
                .ports
                .into_iter()
                .map(|port| parse_port(name, &port))
                .collect::<Result<Vec<_>, _>>()?,
            dependencies: raw.depends_on.into_plans(),
            readiness: raw.healthcheck.map(StackReadinessPlan::from),
            resources: raw.deploy.into(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StackBuildPlan {
    pub context: String,
    pub dockerfile: Option<String>,
    pub args: BTreeMap<String, String>,
    pub target: Option<String>,
}

impl StackBuildPlan {
    fn from_raw(service: &str, raw: RawBuild) -> Result<Self, CatalogError> {
        match raw {
            RawBuild::Context(context) => Ok(Self {
                context,
                dockerfile: None,
                args: BTreeMap::new(),
                target: None,
            }),
            RawBuild::Detailed(raw) => Ok(Self {
                context: raw.context,
                dockerfile: raw.dockerfile,
                args: raw
                    .args
                    .into_iter()
                    .map(|(key, value)| {
                        scalar_string(service, &format!("build.args.{key}"), value)
                            .map(|value| (key, value))
                    })
                    .collect::<Result<_, _>>()?,
                target: raw.target,
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "form", content = "value", rename_all = "kebab-case")]
pub enum StackCommandPlan {
    Shell(String),
    Exec(Vec<String>),
}

impl From<RawStringOrList> for StackCommandPlan {
    fn from(value: RawStringOrList) -> Self {
        match value {
            RawStringOrList::String(value) => Self::Shell(value),
            RawStringOrList::List(value) => Self::Exec(value),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StackMountKind {
    Bind,
    Volume,
    Anonymous,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StackMountPlan {
    pub kind: StackMountKind,
    pub source: Option<String>,
    pub target: String,
    pub read_only: bool,
    pub options: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StackPortPlan {
    pub host_ip: Option<String>,
    pub host_port: Option<u16>,
    pub container_port: u16,
    pub protocol: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StackDependencyPlan {
    pub service: String,
    pub condition: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StackReadinessPlan {
    pub command: StackCommandPlan,
    pub interval: Option<String>,
    pub timeout: Option<String>,
    pub retries: Option<u32>,
    pub start_period: Option<String>,
}

impl From<RawHealthcheck> for StackReadinessPlan {
    fn from(value: RawHealthcheck) -> Self {
        Self {
            command: value.test.into(),
            interval: value.interval,
            timeout: value.timeout,
            retries: value.retries,
            start_period: value.start_period,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct StackResourcePlan {
    pub memory: Option<String>,
    pub cpus: Option<String>,
}

impl From<Option<RawDeploy>> for StackResourcePlan {
    fn from(value: Option<RawDeploy>) -> Self {
        let limits = value.and_then(|deploy| deploy.resources.and_then(|value| value.limits));
        Self {
            memory: limits.as_ref().and_then(|value| value.memory.clone()),
            cpus: limits.and_then(|value| value.cpus),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawService {
    image: Option<String>,
    build: Option<RawBuild>,
    command: Option<RawStringOrList>,
    #[serde(default)]
    environment: Option<RawEnvironment>,
    user: Option<String>,
    working_dir: Option<String>,
    #[serde(default)]
    volumes: Vec<String>,
    #[serde(default)]
    tmpfs: Vec<String>,
    #[serde(default)]
    ports: Vec<String>,
    #[serde(default)]
    depends_on: RawDependsOn,
    healthcheck: Option<RawHealthcheck>,
    deploy: Option<RawDeploy>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawBuild {
    Context(String),
    Detailed(RawBuildDetail),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBuildDetail {
    context: String,
    dockerfile: Option<String>,
    #[serde(default)]
    args: BTreeMap<String, YamlValue>,
    target: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawStringOrList {
    String(String),
    List(Vec<String>),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawEnvironment {
    Mapping(BTreeMap<String, YamlValue>),
    Sequence(Vec<String>),
}

#[derive(Debug, Default, Deserialize)]
#[serde(untagged)]
enum RawDependsOn {
    #[default]
    Empty,
    Mapping(BTreeMap<String, RawDependency>),
    Sequence(Vec<String>),
}

impl RawDependsOn {
    fn into_plans(self) -> Vec<StackDependencyPlan> {
        match self {
            Self::Empty => Vec::new(),
            Self::Sequence(services) => services
                .into_iter()
                .map(|service| StackDependencyPlan {
                    service,
                    condition: "service-started".to_owned(),
                })
                .collect(),
            Self::Mapping(services) => services
                .into_iter()
                .map(|(service, dependency)| StackDependencyPlan {
                    service,
                    condition: dependency
                        .condition
                        .unwrap_or_else(|| "service-started".to_owned())
                        .replace('_', "-"),
                })
                .collect(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDependency {
    condition: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawHealthcheck {
    test: RawStringOrList,
    interval: Option<String>,
    timeout: Option<String>,
    retries: Option<u32>,
    start_period: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDeploy {
    resources: Option<RawResources>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawResources {
    limits: Option<RawResourceLimits>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawResourceLimits {
    memory: Option<String>,
    cpus: Option<String>,
}

fn parse_environment(
    service: &str,
    raw: Option<RawEnvironment>,
) -> Result<BTreeMap<String, String>, CatalogError> {
    match raw {
        None => Ok(BTreeMap::new()),
        Some(RawEnvironment::Mapping(values)) => values
            .into_iter()
            .map(|(key, value)| {
                scalar_string(service, &format!("environment.{key}"), value)
                    .map(|value| (key, value))
            })
            .collect(),
        Some(RawEnvironment::Sequence(values)) => values
            .into_iter()
            .map(|entry| {
                let (key, value) = entry.split_once('=').ok_or_else(|| {
                    stack_error(service, format!("environment entry `{entry}` has no `=`"))
                })?;
                Ok((key.to_owned(), value.to_owned()))
            })
            .collect(),
    }
}

fn parse_mount(service: &str, raw: &str) -> Result<StackMountPlan, CatalogError> {
    let parts = raw.split(':').collect::<Vec<_>>();
    let (source, target, options) = match parts.as_slice() {
        [target] => (None, *target, Vec::new()),
        [source, target] => (Some((*source).to_owned()), *target, Vec::new()),
        [source, target, rest @ ..] => (
            Some((*source).to_owned()),
            *target,
            rest.iter()
                .flat_map(|value| value.split(','))
                .map(str::to_owned)
                .collect(),
        ),
        [] => return Err(stack_error(service, "empty mount entry")),
    };
    if target.trim().is_empty() {
        return Err(stack_error(service, format!("mount `{raw}` has no target")));
    }
    let kind = match source.as_deref() {
        None => StackMountKind::Anonymous,
        Some(value) if is_bind_source(value) => StackMountKind::Bind,
        Some(_) => StackMountKind::Volume,
    };
    let read_only = options.iter().any(|option| option == "ro");
    Ok(StackMountPlan {
        kind,
        source,
        target: target.to_owned(),
        read_only,
        options,
    })
}

fn parse_port(service: &str, raw: &str) -> Result<StackPortPlan, CatalogError> {
    let (without_protocol, protocol) = raw
        .rsplit_once('/')
        .map_or((raw, "tcp"), |(value, protocol)| (value, protocol));
    let parts = without_protocol.split(':').collect::<Vec<_>>();
    let (host_ip, host_port, container_port) = match parts.as_slice() {
        [container] => (None, None, parse_u16(service, "container port", container)?),
        [host, container] => (
            None,
            Some(parse_u16(service, "host port", host)?),
            parse_u16(service, "container port", container)?,
        ),
        [host_ip, host, container] => (
            Some((*host_ip).to_owned()),
            Some(parse_u16(service, "host port", host)?),
            parse_u16(service, "container port", container)?,
        ),
        _ => return Err(stack_error(service, format!("unsupported port `{raw}`"))),
    };
    Ok(StackPortPlan {
        host_ip,
        host_port,
        container_port,
        protocol: protocol.to_owned(),
    })
}

fn parse_u16(service: &str, label: &str, value: &str) -> Result<u16, CatalogError> {
    value
        .parse::<u16>()
        .map_err(|_| stack_error(service, format!("invalid {label} `{value}`")))
}

fn scalar_string(service: &str, field: &str, value: YamlValue) -> Result<String, CatalogError> {
    match value {
        YamlValue::String(value) => Ok(value),
        YamlValue::Number(value) => Ok(value.to_string()),
        YamlValue::Bool(value) => Ok(value.to_string()),
        YamlValue::Null => Ok(String::new()),
        _ => Err(stack_error(
            service,
            format!("`{field}` must be a scalar value"),
        )),
    }
}

fn is_bind_source(value: &str) -> bool {
    value.starts_with('/') || value.starts_with('.') || value.starts_with('~')
}

fn stack_error(service: &str, reason: impl Into<String>) -> CatalogError {
    CatalogError::UnsupportedStackPlan {
        service: service.to_owned(),
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unmodelled_service_fields() {
        let value: YamlValue = serde_yaml::from_str("image: test\nprivileged: true\n").unwrap();

        let error = StackServicePlan::from_yaml("app", &value).unwrap_err();

        assert!(error.to_string().contains("unknown field `privileged`"));
    }

    #[test]
    fn parses_bind_volume_and_udp_port() {
        let mount = parse_mount("app", "/tmp/repo:/workspace:ro").unwrap();
        let port = parse_port("app", "127.0.0.1:5353:53/udp").unwrap();

        assert_eq!(mount.kind, StackMountKind::Bind);
        assert!(mount.read_only);
        assert_eq!(port.host_ip.as_deref(), Some("127.0.0.1"));
        assert_eq!(port.host_port, Some(5353));
        assert_eq!(port.container_port, 53);
        assert_eq!(port.protocol, "udp");
    }
}
