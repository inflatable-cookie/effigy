use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant};

use effigy_catalog::stack_plan::{
    EffectiveStackPlan, StackCommandPlan, StackMountKind, StackReadinessPlan, StackServicePlan,
};

use crate::{
    BackendId, ContainerBackend, ContainerBackendCapabilities, ContainerBackendExecutionModel,
};

const EFFIGY_LABEL: &str = "com.effigy.prototype=true";

#[derive(Debug, Default)]
pub struct AppleContainerBackend;

impl ContainerBackend for AppleContainerBackend {
    fn id(&self) -> BackendId {
        BackendId::apple_container()
    }

    fn capabilities(&self) -> ContainerBackendCapabilities {
        ContainerBackendCapabilities {
            execution_model: ContainerBackendExecutionModel::Native,
            can_execute_stack_plan: true,
            can_attach: false,
            can_repair_runtime: false,
            can_copy: false,
            can_stream_logs: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppleCommand {
    pub description: String,
    pub program: OsString,
    pub args: Vec<OsString>,
}

impl AppleCommand {
    fn container(description: impl Into<String>, args: Vec<OsString>) -> Self {
        Self {
            description: description.into(),
            program: OsString::from("container"),
            args,
        }
    }

    pub fn command_line(&self) -> String {
        std::iter::once(self.program.to_string_lossy().into_owned())
            .chain(
                self.args
                    .iter()
                    .map(|arg| arg.to_string_lossy().into_owned()),
            )
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppleStackLifecyclePlan {
    pub project_name: String,
    pub network_create: AppleCommand,
    pub image_prepare: Vec<AppleCommand>,
    pub volume_create: Vec<AppleCommand>,
    pub container_create: Vec<AppleCommand>,
    pub start_order: Vec<String>,
    pub container_ids: BTreeMap<String, String>,
}

impl AppleStackLifecyclePlan {
    pub fn from_stack(stack: &EffectiveStackPlan, repo_root: &Path) -> Result<Self, AppleError> {
        validate_resource_name("project", &stack.project_name)?;
        validate_resource_name("network", &stack.network_name)?;
        let start_order = dependency_order(stack)?;
        let container_ids = stack
            .services
            .keys()
            .map(|service| {
                validate_resource_name("service", service)?;
                Ok((service.clone(), container_id(stack, service)))
            })
            .collect::<Result<BTreeMap<_, _>, AppleError>>()?;
        let network_create = AppleCommand::container(
            format!("create network {}", stack.network_name),
            os_args([
                "network",
                "create",
                "--label",
                EFFIGY_LABEL,
                stack.network_name.as_str(),
            ]),
        );

        let mut image_prepare = Vec::new();
        let mut volumes = BTreeSet::new();
        let mut container_create = Vec::new();
        for (service_name, service) in &stack.services {
            let image = if let Some(build) = &service.build {
                let tag = built_image_tag(stack, service_name);
                let context = resolve_repo_path(repo_root, &build.context);
                let mut args = os_args(["build", "--tag", tag.as_str()]);
                if let Some(dockerfile) = build.dockerfile.as_deref() {
                    args.extend(os_args([
                        "--file",
                        resolve_repo_path(repo_root, dockerfile)
                            .to_string_lossy()
                            .as_ref(),
                    ]));
                }
                for (key, value) in &build.args {
                    args.extend(os_args(["--build-arg", &format!("{key}={value}")]));
                }
                if let Some(target) = build.target.as_deref() {
                    args.extend(os_args(["--target", target]));
                }
                args.push(context.into_os_string());
                image_prepare.push(AppleCommand::container(
                    format!("build image for {service_name}"),
                    args,
                ));
                tag
            } else {
                let image = service
                    .image
                    .clone()
                    .ok_or_else(|| AppleError::InvalidPlan {
                        reason: format!("service `{service_name}` has no image or build"),
                    })?;
                image_prepare.push(AppleCommand::container(
                    format!("pull image for {service_name}"),
                    os_args(["image", "pull", "--progress", "plain", image.as_str()]),
                ));
                image
            };

            for mount in &service.mounts {
                if mount.kind == StackMountKind::Volume {
                    let source =
                        mount
                            .source
                            .as_deref()
                            .ok_or_else(|| AppleError::InvalidPlan {
                                reason: format!(
                                    "service `{service_name}` has a volume without a name"
                                ),
                            })?;
                    volumes.insert(source.to_owned());
                }
            }
            container_create.push(create_command(
                stack,
                service_name,
                service,
                &image,
                repo_root,
            )?);
        }
        let volume_create = volumes
            .into_iter()
            .map(|volume| {
                AppleCommand::container(
                    format!("create volume {volume}"),
                    os_args(["volume", "create", "--label", EFFIGY_LABEL, volume.as_str()]),
                )
            })
            .collect();

        Ok(Self {
            project_name: stack.project_name.clone(),
            network_create,
            image_prepare,
            volume_create,
            container_create,
            start_order,
            container_ids,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppleStackReport {
    pub project_name: String,
    pub container_ids: BTreeMap<String, String>,
    pub ipv4_addresses: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct AppleStackExecutor {
    readiness_timeout: Duration,
    readiness_interval: Duration,
}

impl Default for AppleStackExecutor {
    fn default() -> Self {
        Self {
            readiness_timeout: Duration::from_secs(90),
            readiness_interval: Duration::from_millis(500),
        }
    }
}

impl AppleStackExecutor {
    pub fn start(
        &self,
        stack: &EffectiveStackPlan,
        repo_root: &Path,
    ) -> Result<AppleStackReport, AppleError> {
        let plan = AppleStackLifecyclePlan::from_stack(stack, repo_root)?;
        self.delete_containers(&plan.container_ids, false)?;
        self.delete_network(&stack.network_name, false)?;
        run(&plan.network_create)?;
        for command in &plan.volume_create {
            run_allow_existing(command)?;
        }
        for command in &plan.image_prepare {
            run(command)?;
        }
        for command in &plan.container_create {
            run(command)?;
        }
        for service in &plan.start_order {
            let container_id = &plan.container_ids[service];
            run(&AppleCommand::container(
                format!("start {service}"),
                os_args(["start", container_id.as_str()]),
            ))?;
            self.wait_ready(container_id, &stack.services[service])?;
        }
        let ipv4_addresses = inspect_ipv4_addresses(&plan.container_ids)?;
        reconcile_hosts(stack, &plan.container_ids, &ipv4_addresses)?;
        Ok(AppleStackReport {
            project_name: stack.project_name.clone(),
            container_ids: plan.container_ids,
            ipv4_addresses,
        })
    }

    pub fn stop(&self, stack: &EffectiveStackPlan, remove_volumes: bool) -> Result<(), AppleError> {
        let container_ids = stack
            .services
            .keys()
            .map(|service| (service.clone(), container_id(stack, service)))
            .collect();
        self.delete_containers(&container_ids, true)?;
        self.delete_network(&stack.network_name, true)?;
        if remove_volumes {
            let volumes = stack
                .services
                .values()
                .flat_map(|service| &service.mounts)
                .filter(|mount| mount.kind == StackMountKind::Volume)
                .filter_map(|mount| mount.source.as_deref())
                .collect::<BTreeSet<_>>();
            for volume in volumes {
                let command = AppleCommand::container(
                    format!("delete volume {volume}"),
                    os_args(["volume", "delete", volume]),
                );
                run_allow_missing(&command)?;
            }
        }
        Ok(())
    }

    pub fn exec(
        &self,
        stack: &EffectiveStackPlan,
        service: &str,
        command: &[&str],
    ) -> Result<Output, AppleError> {
        let id = stack
            .services
            .contains_key(service)
            .then(|| container_id(stack, service))
            .ok_or_else(|| AppleError::InvalidPlan {
                reason: format!("unknown service `{service}`"),
            })?;
        let mut args = os_args(["exec", id.as_str()]);
        args.extend(command.iter().map(OsString::from));
        run_capture(&AppleCommand::container(format!("exec {service}"), args))
    }

    pub fn logs(
        &self,
        stack: &EffectiveStackPlan,
        service: &str,
        lines: usize,
    ) -> Result<Output, AppleError> {
        let id = container_id(stack, service);
        run_capture(&AppleCommand::container(
            format!("logs {service}"),
            os_args(["logs", "-n", &lines.to_string(), id.as_str()]),
        ))
    }

    fn wait_ready(&self, container_id: &str, service: &StackServicePlan) -> Result<(), AppleError> {
        let Some(readiness) = service.readiness.as_ref() else {
            return Ok(());
        };
        let deadline = Instant::now() + self.readiness_timeout;
        let command = readiness_command(container_id, readiness)?;
        loop {
            let output = Command::new(&command.program)
                .args(&command.args)
                .output()?;
            if output.status.success() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(command_error(&command, output));
            }
            std::thread::sleep(self.readiness_interval);
        }
    }

    fn delete_containers(
        &self,
        container_ids: &BTreeMap<String, String>,
        strict: bool,
    ) -> Result<(), AppleError> {
        for id in container_ids.values() {
            let command = AppleCommand::container(
                format!("delete container {id}"),
                os_args(["delete", "--force", id.as_str()]),
            );
            if strict {
                run_allow_missing(&command)?;
            } else {
                let _ = Command::new(&command.program).args(&command.args).output();
            }
        }
        Ok(())
    }

    fn delete_network(&self, network: &str, strict: bool) -> Result<(), AppleError> {
        let command = AppleCommand::container(
            format!("delete network {network}"),
            os_args(["network", "delete", network]),
        );
        if strict {
            run_allow_missing(&command).map(|_| ())
        } else {
            let _ = Command::new(&command.program).args(&command.args).output();
            Ok(())
        }
    }
}

#[derive(Debug)]
pub enum AppleError {
    InvalidPlan {
        reason: String,
    },
    Io(std::io::Error),
    Command {
        command: String,
        status: Option<i32>,
        stdout: String,
        stderr: String,
    },
    Inspect {
        reason: String,
    },
}

impl fmt::Display for AppleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPlan { reason } => write!(f, "invalid Apple stack plan: {reason}"),
            Self::Io(error) => write!(f, "Apple container I/O failed: {error}"),
            Self::Command {
                command,
                status,
                stderr,
                ..
            } => write!(
                f,
                "Apple container command `{command}` failed with status {status:?}: {}",
                stderr.trim()
            ),
            Self::Inspect { reason } => write!(f, "Apple container inspect failed: {reason}"),
        }
    }
}

impl std::error::Error for AppleError {}

impl From<std::io::Error> for AppleError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

fn create_command(
    stack: &EffectiveStackPlan,
    service_name: &str,
    service: &StackServicePlan,
    image: &str,
    repo_root: &Path,
) -> Result<AppleCommand, AppleError> {
    let id = container_id(stack, service_name);
    let mut args = os_args([
        "create",
        "--name",
        id.as_str(),
        "--network",
        stack.network_name.as_str(),
        "--label",
        EFFIGY_LABEL,
        "--label",
        &format!("com.effigy.project={}", stack.project_name),
        "--label",
        &format!("com.effigy.service={service_name}"),
    ]);
    for (key, value) in &service.environment {
        args.extend(os_args(["--env", &format!("{key}={value}")]));
    }
    if let Some(user) = service.user.as_deref() {
        args.extend(os_args(["--user", user]));
    }
    if let Some(workdir) = service.working_dir.as_deref() {
        args.extend(os_args(["--workdir", workdir]));
    }
    for mount in &service.mounts {
        let source = mount.source.as_deref().unwrap_or_default();
        let source = if mount.kind == StackMountKind::Bind {
            resolve_repo_path(repo_root, source)
                .to_string_lossy()
                .into_owned()
        } else {
            source.to_owned()
        };
        let kind = match mount.kind {
            StackMountKind::Bind => "bind",
            StackMountKind::Volume => "volume",
            StackMountKind::Anonymous => {
                return Err(AppleError::InvalidPlan {
                    reason: format!(
                        "service `{service_name}` uses unsupported anonymous mount `{}`",
                        mount.target
                    ),
                });
            }
        };
        let mut spec = format!("type={kind},source={source},target={}", mount.target);
        if mount.read_only {
            spec.push_str(",readonly");
        }
        args.extend(os_args(["--mount", spec.as_str()]));
    }
    for tmpfs in &service.tmpfs {
        args.extend(os_args(["--tmpfs", tmpfs]));
    }
    for port in &service.ports {
        let mut publish = String::new();
        if let Some(host_ip) = port.host_ip.as_deref() {
            publish.push_str(host_ip);
            publish.push(':');
        }
        if let Some(host_port) = port.host_port {
            publish.push_str(&host_port.to_string());
            publish.push(':');
        }
        publish.push_str(&port.container_port.to_string());
        if port.protocol != "tcp" {
            publish.push('/');
            publish.push_str(&port.protocol);
        }
        args.extend(os_args(["--publish", publish.as_str()]));
    }
    if let Some(memory) = service.resources.memory.as_deref() {
        args.extend(os_args(["--memory", memory]));
    }
    if let Some(cpus) = service.resources.cpus.as_deref() {
        args.extend(os_args(["--cpus", cpus]));
    }
    args.push(OsString::from(image));
    if let Some(command) = service.command.as_ref() {
        match command {
            StackCommandPlan::Shell(command) => args.extend(os_args(["sh", "-lc", command])),
            StackCommandPlan::Exec(command) => args.extend(command.iter().map(OsString::from)),
        }
    }
    Ok(AppleCommand::container(
        format!("create service {service_name}"),
        args,
    ))
}

fn readiness_command(
    container_id: &str,
    readiness: &StackReadinessPlan,
) -> Result<AppleCommand, AppleError> {
    let command = match &readiness.command {
        StackCommandPlan::Shell(command) => os_args(["sh", "-lc", command]),
        StackCommandPlan::Exec(parts) if parts.first().is_some_and(|value| value == "CMD") => {
            parts[1..].iter().map(OsString::from).collect()
        }
        StackCommandPlan::Exec(parts)
            if parts.first().is_some_and(|value| value == "CMD-SHELL") =>
        {
            let command = parts.get(1).ok_or_else(|| AppleError::InvalidPlan {
                reason: "CMD-SHELL readiness has no command".to_owned(),
            })?;
            os_args(["sh", "-lc", command])
        }
        StackCommandPlan::Exec(parts) => parts.iter().map(OsString::from).collect(),
    };
    let mut args = os_args(["exec", container_id]);
    args.extend(command);
    Ok(AppleCommand::container(
        format!("readiness for {container_id}"),
        args,
    ))
}

fn dependency_order(stack: &EffectiveStackPlan) -> Result<Vec<String>, AppleError> {
    let mut remaining = stack.services.keys().cloned().collect::<BTreeSet<_>>();
    let mut ordered = Vec::new();
    while !remaining.is_empty() {
        let ready = remaining
            .iter()
            .filter(|service| {
                stack.services[*service]
                    .dependencies
                    .iter()
                    .all(|dependency| ordered.contains(&dependency.service))
            })
            .cloned()
            .collect::<Vec<_>>();
        if ready.is_empty() {
            return Err(AppleError::InvalidPlan {
                reason: format!(
                    "dependency cycle or missing service among: {}",
                    remaining.into_iter().collect::<Vec<_>>().join(", ")
                ),
            });
        }
        for service in ready {
            remaining.remove(&service);
            ordered.push(service);
        }
    }
    Ok(ordered)
}

fn inspect_ipv4_addresses(
    container_ids: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, AppleError> {
    let command = AppleCommand::container(
        "inspect project containers",
        std::iter::once(OsString::from("inspect"))
            .chain(container_ids.values().map(OsString::from))
            .collect(),
    );
    let output = run_capture(&command)?;
    let entries: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|error| AppleError::Inspect {
            reason: error.to_string(),
        })?;
    let entries = entries.as_array().ok_or_else(|| AppleError::Inspect {
        reason: "inspect output is not an array".to_owned(),
    })?;
    let mut by_id = BTreeMap::new();
    for entry in entries {
        let id = entry
            .get("id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| AppleError::Inspect {
                reason: "inspect entry has no id".to_owned(),
            })?;
        let address = entry
            .pointer("/status/networks/0/ipv4Address")
            .and_then(serde_json::Value::as_str)
            .and_then(|value| value.split('/').next())
            .ok_or_else(|| AppleError::Inspect {
                reason: format!("container `{id}` has no IPv4 address"),
            })?;
        by_id.insert(id.to_owned(), address.to_owned());
    }
    container_ids
        .iter()
        .map(|(service, id)| {
            by_id
                .get(id)
                .cloned()
                .map(|address| (service.clone(), address))
                .ok_or_else(|| AppleError::Inspect {
                    reason: format!("inspect output omitted `{id}`"),
                })
        })
        .collect()
}

fn reconcile_hosts(
    stack: &EffectiveStackPlan,
    container_ids: &BTreeMap<String, String>,
    addresses: &BTreeMap<String, String>,
) -> Result<(), AppleError> {
    let begin = format!("# BEGIN EFFIGY {}", stack.project_name);
    let end = format!("# END EFFIGY {}", stack.project_name);
    let mut lines = vec![begin.clone()];
    for (service, address) in addresses {
        lines.push(format!(
            "{address} {service} {} {service}.{}.effigy",
            container_ids[service], stack.project_name
        ));
    }
    lines.push(end.clone());
    let block = lines.join("\\n");
    let script = format!(
        "sed -i '/^{begin}$/,/^{end}$/d' /etc/hosts; printf '%b\\n' '{block}' >> /etc/hosts"
    );
    for (service, id) in container_ids {
        let command = AppleCommand::container(
            format!("reconcile hosts for {service}"),
            os_args([
                "exec",
                "--uid",
                "0",
                id.as_str(),
                "sh",
                "-c",
                script.as_str(),
            ]),
        );
        run(&command)?;
    }
    Ok(())
}

fn run(command: &AppleCommand) -> Result<Output, AppleError> {
    let output = run_capture(command)?;
    Ok(output)
}

fn run_capture(command: &AppleCommand) -> Result<Output, AppleError> {
    let output = Command::new(&command.program)
        .args(&command.args)
        .output()?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(command_error(command, output))
    }
}

fn run_allow_existing(command: &AppleCommand) -> Result<Output, AppleError> {
    let output = Command::new(&command.program)
        .args(&command.args)
        .output()?;
    if output.status.success() || text_contains(&output, &["already exists", "in use"]) {
        Ok(output)
    } else {
        Err(command_error(command, output))
    }
}

fn run_allow_missing(command: &AppleCommand) -> Result<Output, AppleError> {
    let output = Command::new(&command.program)
        .args(&command.args)
        .output()?;
    if output.status.success()
        || text_contains(&output, &["not found", "does not exist", "no such"])
    {
        Ok(output)
    } else {
        Err(command_error(command, output))
    }
}

fn text_contains(output: &Output, needles: &[&str]) -> bool {
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
    .to_ascii_lowercase();
    needles.iter().any(|needle| text.contains(needle))
}

fn command_error(command: &AppleCommand, output: Output) -> AppleError {
    AppleError::Command {
        command: command.command_line(),
        status: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn built_image_tag(stack: &EffectiveStackPlan, service: &str) -> String {
    format!("effigy/{}-{service}:prototype", stack.project_name)
}

fn container_id(stack: &EffectiveStackPlan, service: &str) -> String {
    format!("{}-{service}", stack.project_name)
}

fn resolve_repo_path(repo_root: &Path, value: &str) -> PathBuf {
    let path = Path::new(value);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        repo_root.join(path)
    }
}

fn validate_resource_name(label: &str, value: &str) -> Result<(), AppleError> {
    if value.is_empty()
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
    {
        return Err(AppleError::InvalidPlan {
            reason: format!("{label} name `{value}` is not a safe runtime identifier"),
        });
    }
    Ok(())
}

fn os_args<'a>(values: impl IntoIterator<Item = &'a str>) -> Vec<OsString> {
    values.into_iter().map(OsString::from).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use effigy_catalog::stack_plan::{StackDependencyPlan, StackResourcePlan};
    use std::ffi::OsStr;

    fn service(name: &str, dependencies: &[&str]) -> StackServicePlan {
        StackServicePlan {
            name: name.to_owned(),
            image: Some("alpine:3.22".to_owned()),
            build: None,
            command: Some(StackCommandPlan::Exec(vec![
                "sleep".to_owned(),
                "300".to_owned(),
            ])),
            environment: BTreeMap::new(),
            user: None,
            working_dir: None,
            mounts: Vec::new(),
            tmpfs: Vec::new(),
            ports: Vec::new(),
            dependencies: dependencies
                .iter()
                .map(|dependency| StackDependencyPlan {
                    service: (*dependency).to_owned(),
                    condition: "service-started".to_owned(),
                })
                .collect(),
            readiness: None,
            resources: StackResourcePlan::default(),
        }
    }

    #[test]
    fn lifecycle_plan_orders_dependencies_and_uses_native_commands() {
        let stack = EffectiveStackPlan {
            project_name: "effigy-probe".to_owned(),
            network_name: "effigy-probe-default".to_owned(),
            services: [
                ("web".to_owned(), service("web", &["app"])),
                ("app".to_owned(), service("app", &[])),
            ]
            .into_iter()
            .collect(),
        };

        let plan = AppleStackLifecyclePlan::from_stack(&stack, Path::new("/tmp/repo"))
            .expect("lifecycle plan");

        assert_eq!(plan.start_order, vec!["app", "web"]);
        assert_eq!(plan.network_create.program, OsStr::new("container"));
        assert!(plan.container_create[0]
            .command_line()
            .contains("--network effigy-probe-default"));
    }

    #[test]
    fn dependency_cycle_fails_before_commands_run() {
        let stack = EffectiveStackPlan {
            project_name: "effigy-probe".to_owned(),
            network_name: "effigy-probe-default".to_owned(),
            services: [
                ("a".to_owned(), service("a", &["b"])),
                ("b".to_owned(), service("b", &["a"])),
            ]
            .into_iter()
            .collect(),
        };

        let error =
            AppleStackLifecyclePlan::from_stack(&stack, Path::new("/tmp/repo")).unwrap_err();

        assert!(error.to_string().contains("dependency cycle"));
    }

    #[test]
    fn prototype_capabilities_do_not_claim_unwired_manager_features() {
        let capabilities = AppleContainerBackend.capabilities();

        assert_eq!(
            capabilities.execution_model,
            ContainerBackendExecutionModel::Native
        );
        assert!(capabilities.can_execute_stack_plan);
        assert!(!capabilities.can_attach);
        assert!(!capabilities.can_repair_runtime);
        assert!(!capabilities.can_copy);
        assert!(!capabilities.can_stream_logs);
    }
}
