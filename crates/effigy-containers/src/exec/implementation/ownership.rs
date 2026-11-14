//! Verify project, checkout and scope ownership before container effects.

use super::*;

pub(super) fn compose_binding_labels(labels: Option<&serde_json::Value>) -> (Option<&str>, bool) {
    let label = |name: &str| {
        labels
            .and_then(|labels| labels.get(name))
            .and_then(|value| value.as_str())
    };
    let working_dir = checkout_label(
        label("com.docker.compose.project.working_dir"),
        label(crate::runtime::scope::CHECKOUT_LABEL),
        label(crate::runtime::scope::SCOPE_LABEL),
    );
    let oneoff_ok = label("com.docker.compose.oneoff")
        .is_none_or(|value| matches!(value.to_ascii_lowercase().as_str(), "false" | "0" | "no"));
    (working_dir, oneoff_ok)
}

// A scoped generation's Compose working directory is the immutable render
// directory. The explicit checkout label and matching scope locate its owner.
// Unscoped legacy containers keep the historical working-directory rule.
fn checkout_label<'a>(
    working_dir: Option<&'a str>,
    checkout: Option<&'a str>,
    scope: Option<&str>,
) -> Option<&'a str> {
    if scope.is_some() {
        checkout.or(working_dir)
    } else {
        working_dir.or(checkout)
    }
}

#[cfg(test)]
mod compose_binding_label_tests {
    use super::compose_binding_labels;
    use serde_json::json;

    #[test]
    fn docker_labels_use_project_working_dir() {
        let labels = json!({
            "com.docker.compose.project.working_dir": "/repo",
            "com.effigy.checkout": "/elsewhere",
            "com.docker.compose.oneoff": "False",
        });
        assert_eq!(compose_binding_labels(Some(&labels)), (Some("/repo"), true));
    }

    #[test]
    fn scoped_render_directory_does_not_replace_the_verified_checkout_label() {
        let mut labels = json!({
            "com.docker.compose.project.working_dir": "/repo/.effigy/state/objects/hash/checkout/.effigy/runtime/compose",
            "com.effigy.checkout": "/repo",
            "com.effigy.scope": "owned-scope",
            "com.docker.compose.oneoff": "False",
        });
        assert_eq!(compose_binding_labels(Some(&labels)), (Some("/repo"), true));
        labels["com.effigy.checkout"] = json!("/foreign");
        assert_eq!(
            compose_binding_labels(Some(&labels)),
            (Some("/foreign"), true)
        );
    }

    #[test]
    fn nerdctl_labels_fall_back_to_effigy_checkout() {
        let labels = json!({
            "com.docker.compose.project": "acme",
            "com.effigy.checkout": "/repo",
        });
        assert_eq!(compose_binding_labels(Some(&labels)), (Some("/repo"), true));
    }

    #[test]
    fn oneoff_and_unlabelled_containers_are_not_owned() {
        let oneoff = json!({"com.docker.compose.project.working_dir": "/repo", "com.docker.compose.oneoff": "True"});
        assert_eq!(
            compose_binding_labels(Some(&oneoff)),
            (Some("/repo"), false)
        );
        assert_eq!(compose_binding_labels(Some(&json!({}))), (None, true));
        assert_eq!(compose_binding_labels(None), (None, true));
    }
}

/// An inspected binding. Fields are private so only strict inspection can mint it.
/// Reverification is still required before an effect; this is not a remote lock.
#[derive(Debug, Clone)]
pub struct VerifiedComposeContainer {
    pub(super) row: RunningComposeContainer,
    pub(super) id: String,
    pub(super) checkout: std::path::PathBuf,
    pub(super) scope: Option<String>,
    pub(super) profile: String,
    pub(super) backend: crate::compose::ComposeBackend,
    // Preserve the strict builder witness, not an expired earlier probe budget.
    // Every reinspection uses its current caller's unchanged deadline.
    pub(super) workspace_artifact_builder: bool,
}

impl VerifiedComposeContainer {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn service(&self) -> Option<&str> {
        self.row.service.as_deref()
    }

    pub fn reverify(
        &self,
        repo_root: &Path,
        policy: &EffectiveContainerPolicy,
        deadline: Instant,
    ) -> Result<(), ContainerExecError> {
        let current = if self.workspace_artifact_builder {
            vec![verify_workspace_artifact_builder(
                repo_root, policy, &self.row, deadline,
            )?]
        } else {
            verify_owned_running_compose_identities(
                repo_root,
                policy,
                std::slice::from_ref(&self.row),
                deadline,
            )?
        };
        if current.len() == 1
            && current[0].id == self.id
            && current[0].checkout == self.checkout
            && current[0].scope == self.scope
            && current[0].profile == self.profile
            && current[0].backend == self.backend
        {
            return Ok(());
        }
        Err(ContainerExecError::refused(
            "inspect owned binding",
            "binding unavailable: container full ID or checkout/scope/backend binding changed before effect",
        ))
    }
}

pub fn verify_owned_running_compose_rows(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    rows: &[RunningComposeContainer],
    deadline: Instant,
) -> Result<Vec<RunningComposeContainer>, ContainerExecError> {
    verify_owned_running_compose_identities(repo_root, policy, rows, deadline)
        .map(|verified| verified.into_iter().map(|binding| binding.row).collect())
}

/// Whether all declared services, including the primary, are running. The
/// caller separately verifies ownership before using this to avoid Compose up.
pub fn compose_stack_is_running(
    policy: &EffectiveContainerPolicy,
    rows: &[RunningComposeContainer],
) -> Result<bool, ContainerExecError> {
    let mut declared = declared_service_names(policy, &policy.compose_files)?;
    declared.insert(policy.primary_service.clone());
    Ok(declared.iter().all(|service| {
        rows.iter().any(|row| {
            row.project_name.as_deref() == Some(policy.project_name.as_str())
                && row.service.as_deref() == Some(service.as_str())
                && !row.oneoff
                && row.status.is_running()
        })
    }))
}

pub fn verify_owned_running_compose_identities(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    rows: &[RunningComposeContainer],
    deadline: Instant,
) -> Result<Vec<VerifiedComposeContainer>, ContainerExecError> {
    let ownership = OwnershipContext::resolve(repo_root, policy)?;
    let mut verified = Vec::new();
    for row in rows
        .iter()
        .filter(|row| row.project_name.as_deref() == Some(&policy.project_name))
    {
        if row.oneoff {
            continue;
        }
        let remaining =
            crate::exec::process::remaining_until_deadline(deadline, "inspect owned binding")?;
        let output = run_runtime_command_capture_for_policy_with_absolute_deadline(
            repo_root,
            policy,
            &inspect_owned_binding_args(row),
            "inspect owned binding",
            deadline,
            remaining,
        )?;
        verified.push(ownership.verify_inspected(repo_root, policy, row, &output.stdout)?);
    }
    Ok(verified)
}

/// The full ID of one shared-service container the restart-policy apply may
/// update. A shared stack belongs to no checkout, so its container is proven by
/// its own inspected identity: full ID, shared project, declared service,
/// running state and the render label equal to the render `up` just published.
pub fn verify_shared_render_container(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    project_name: &str,
    service: &str,
    container_name: &str,
    render_digest: &str,
    deadline: Instant,
) -> Result<String, ContainerExecError> {
    let remaining =
        crate::exec::process::remaining_until_deadline(deadline, "inspect shared container")?;
    let output = run_runtime_command_capture_for_policy_with_absolute_deadline(
        repo_root,
        policy,
        &[OsString::from("inspect"), OsString::from(container_name)],
        "inspect shared container",
        deadline,
        remaining,
    )?;
    shared_render_identity(
        &output.stdout,
        project_name,
        service,
        container_name,
        render_digest,
    )
}

/// Prove one inspect record is the shared container `container_name` of
/// `project_name` and `service`, running and rendered from `render_digest`,
/// and return its full ID. Anything else is refused with its reason.
pub fn shared_render_identity(
    stdout: &[u8],
    project_name: &str,
    service: &str,
    container_name: &str,
    render_digest: &str,
) -> Result<String, ContainerExecError> {
    let refuse = |reason: String| ContainerExecError::Failure {
        command: "inspect shared container".to_owned(),
        code: None,
        stdout: String::new(),
        stderr: format!("binding unavailable: {reason}"),
    };
    let identity =
        crate::exec::parse::parse_inspected_container_identity(&String::from_utf8_lossy(stdout))
            .map_err(|error| {
                refuse(format!(
                    "no canonical full identity for `{container_name}`: {error}"
                ))
            })?;
    let raw: serde_json::Value = serde_json::from_slice(stdout)
        .map_err(|_| refuse(format!("inspect of `{container_name}` is not JSON")))?;
    let render = raw
        .as_array()
        .and_then(|records| records.first())
        .and_then(|record| record.get("Config"))
        .and_then(|config| config.get("Labels"))
        .and_then(|labels| labels.get(crate::runtime::scope::SHARED_RENDER_LABEL))
        .and_then(|value| value.as_str());
    if identity.name != container_name {
        return Err(refuse(format!(
            "`{container_name}` now inspects as `{}`",
            identity.name
        )));
    }
    if identity.project.as_deref() != Some(project_name) {
        return Err(refuse(format!(
            "`{container_name}` belongs to project `{}`, not shared project `{project_name}`",
            identity.project.as_deref().unwrap_or("(none)")
        )));
    }
    if identity.service.as_deref() != Some(service) || identity.oneoff {
        return Err(refuse(format!(
            "`{container_name}` is not the declared shared service `{service}`"
        )));
    }
    if !(identity.running && crate::exec::parse::compose_status_is_running(&identity.status)) {
        return Err(refuse(format!("`{container_name}` is not running")));
    }
    match render {
        Some(rendered) if rendered == render_digest => Ok(identity.id),
        Some(rendered) => Err(refuse(format!(
            "`{container_name}` was rendered from `{rendered}`, not the current render `{render_digest}`"
        ))),
        None => Err(refuse(format!(
            "`{container_name}` carries no shared render label"
        ))),
    }
}

const OWNED_SERVICE_LISTING_FORMAT: &str = "{{.ID}}\t{{.Status}}\t{{.Label \"com.docker.compose.oneoff\"}}\t{{.Label \"com.docker.compose.project.working_dir\"}}\t{{.Label \"com.effigy.checkout\"}}\t{{.Label \"com.effigy.scope\"}}";

/// Runtime `ps` arguments listing the containers of one project service with
/// the fields [`OwnershipContext::verify_listed_service`] checks.
pub fn owned_service_listing_args(
    policy: &EffectiveContainerPolicy,
    service: &str,
) -> Vec<OsString> {
    [
        "ps".to_owned(),
        "--no-trunc".to_owned(),
        "--filter".to_owned(),
        format!("label=com.docker.compose.project={}", policy.project_name),
        "--filter".to_owned(),
        format!("label=com.docker.compose.service={service}"),
        "--format".to_owned(),
        OWNED_SERVICE_LISTING_FORMAT.to_owned(),
    ]
    .into_iter()
    .map(OsString::from)
    .collect()
}

/// Runtime arguments that inspect one listed container for ownership checks.
pub(super) fn inspect_owned_binding_args(row: &RunningComposeContainer) -> [OsString; 2] {
    [
        OsString::from("inspect"),
        OsString::from(&row.container_name),
    ]
}

/// The service names the given Compose files declare.
pub(in crate::exec) fn declared_service_names(
    policy: &EffectiveContainerPolicy,
    compose_files: &[PathBuf],
) -> Result<BTreeSet<String>, ContainerExecError> {
    let mut names = BTreeSet::new();
    for compose_file in compose_files {
        let content =
            policy
                .read_compose(compose_file)
                .map_err(|error| ContainerExecError::Failure {
                    command: format!("read compose file {}", compose_file.display()),
                    code: None,
                    stdout: String::new(),
                    stderr: error.to_string(),
                })?;
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&content).map_err(|error| ContainerExecError::Failure {
                command: format!("parse compose file {}", compose_file.display()),
                code: None,
                stdout: String::new(),
                stderr: error.to_string(),
            })?;
        let Some(services) = parsed
            .get("services")
            .and_then(serde_yaml::Value::as_mapping)
        else {
            continue;
        };
        for key in services.keys() {
            if let Some(name) = key.as_str() {
                names.insert(name.to_owned());
            }
        }
    }
    Ok(names)
}

/// The selected checkout's identity, resolved once and compared against each
/// inspected container. Callers that must account every runtime capture (the
/// bounded command path) run `inspect` themselves and pass its output to
/// [`OwnershipContext::verify_inspected`].
pub struct OwnershipContext {
    pub(super) declared: BTreeSet<String>,
    pub(super) canonical: std::path::PathBuf,
    pub(super) scope_token: Option<String>,
    /// The checkout's own staged Compose render, when this environment stages
    /// one under the runtime profile's mount (effigy#715). A Compose container
    /// started from it reports that directory as its project working directory.
    pub(super) staged_render: Option<crate::artifact_source::StagePlan>,
}

impl OwnershipContext {
    pub fn resolve(
        repo_root: &Path,
        policy: &EffectiveContainerPolicy,
    ) -> Result<Self, ContainerExecError> {
        let declared = declared_service_names(policy, &policy.compose_files)?;
        // Ownership is the checkout's, not a nested catalog directory's: the
        // generated labels name the enclosing checkout (contract 052 §7.1).
        let canonical = effigy_core::worktree_scope::checkout_root(repo_root)
            .canonicalize()
            .map_err(|error| ContainerExecError::Launch {
                command: "verify selected checkout".to_owned(),
                error,
            })?;
        let scope_token = effigy_core::worktree_scope::resolve(repo_root)
            .map_err(|error| ContainerExecError::Launch {
                command: "verify selected scope".to_owned(),
                error,
            })?
            .map(|scope| {
                std::fs::read_to_string(
                    scope
                        .token_dir
                        .join(effigy_core::worktree_scope::SCOPE_FILE),
                )
            })
            .transpose()
            .map_err(|error| ContainerExecError::Launch {
                command: "verify selected scope token".to_owned(),
                error,
            })?;
        let staged_render = crate::artifact_source::policy_stage(policy).map_err(|error| {
            ContainerExecError::refused("resolve owned Compose render", error.to_string())
        })?;
        Ok(Self {
            declared,
            canonical,
            scope_token,
            staged_render,
        })
    }

    /// The identity fields every admitted container must satisfy regardless of
    /// checkout or scope labels: the listed container's name, the selected
    /// project, a declared service, not one-off, and - only when the caller
    /// requires an active workload - running.
    pub(super) fn inspected_identity_matches(
        &self,
        policy: &EffectiveContainerPolicy,
        row: &RunningComposeContainer,
        identity: &crate::exec::parse::InspectedContainerIdentity,
        oneoff_label_ok: bool,
        require_running: bool,
    ) -> bool {
        identity.name == row.container_name
            && identity.project.as_deref() == Some(policy.project_name.as_str())
            && identity.service == row.service
            && identity
                .service
                .as_ref()
                .is_some_and(|service| self.declared.contains(service))
            && (!require_running
                || (identity.running
                    && crate::exec::parse::compose_status_is_running(&identity.status)))
            && !identity.oneoff
            && oneoff_label_ok
    }

    /// Whether a container's checkout label and scope label name this
    /// checkout and scope.
    pub(super) fn owns(&self, checkout: Option<&str>, scope: Option<&str>) -> bool {
        let scope_matches = match (self.scope_token.as_deref(), scope) {
            (None, None) => true,
            (Some(token), Some(value)) => !token.trim().is_empty() && value == token.trim(),
            _ => false,
        };
        if !scope_matches {
            return false;
        }
        let Some(path) = checkout.and_then(|path| Path::new(path).canonicalize().ok()) else {
            return false;
        };
        path == self.canonical
            || self
                .staged_render
                .as_ref()
                .is_some_and(|plan| plan.owns_staged_render_directory(&path))
    }

    /// Verify every full-ID project row before teardown, including stopped
    /// containers and orphans. Unlike exec/publication this needs no running
    /// state, inspect snapshot, declared service, or port associations.
    pub(in crate::exec) fn verify_shutdown_listing(
        &self,
        policy: &EffectiveContainerPolicy,
        stdout: &[u8],
    ) -> Result<(), ContainerExecError> {
        for line in String::from_utf8_lossy(stdout)
            .lines()
            .filter(|line| !line.trim().is_empty())
        {
            let fields = line.split('\t').collect::<Vec<_>>();
            if let [id, project, working_dir, checkout, scope] = fields[..] {
                fn present(value: &str) -> Option<&str> {
                    (!value.is_empty()).then_some(value)
                }
                if crate::exec::parse::parse_full_container_id(id).is_some()
                    && project == policy.project_name
                    && self.owns(
                        checkout_label(present(working_dir), present(checkout), present(scope)),
                        present(scope),
                    )
                {
                    continue;
                }
            }
            return Err(ContainerExecError::Failure {
                command: "verify managed shutdown ownership".to_owned(), code: None,
                stdout: String::new(), stderr: "project container has unknown or foreign full-ID/checkout/scope ownership; no teardown ran".to_owned(),
            });
        }
        Ok(())
    }

    /// Prove that an [`owned_service_listing_args`] listing holds exactly one
    /// running, non-oneoff `service` container of this checkout and scope, and
    /// return its full ID. Callers that must keep runtime captures small (the
    /// bounded command path) use this instead of a full `inspect`.
    pub fn verify_listed_service(
        &self,
        service: &str,
        listing_stdout: &[u8],
    ) -> Result<String, ContainerExecError> {
        let refuse = |reason: &str| {
            ContainerExecError::refused(
                "list owned service",
                format!("binding unavailable: {reason}"),
            )
        };
        if !self.declared.contains(service) {
            return Err(refuse(
                "service is not declared by the selected Compose files",
            ));
        }
        let listing = String::from_utf8_lossy(listing_stdout);
        let mut rows = listing.lines().filter(|line| !line.trim().is_empty());
        let row = rows
            .next()
            .ok_or_else(|| refuse("no running container for the service"))?;
        if rows.next().is_some() {
            return Err(refuse("more than one container for the service"));
        }
        let fields = row.split('\t').map(str::trim).collect::<Vec<_>>();
        let [id, status, oneoff, working_dir, checkout, scope] = fields[..] else {
            return Err(refuse("service listing is malformed"));
        };
        // Listings print an empty string for an absent label.
        fn present(value: &str) -> Option<&str> {
            Some(value).filter(|value| !value.is_empty())
        }
        let oneoff = matches!(oneoff.to_ascii_lowercase().as_str(), "true" | "1" | "yes");
        if id.len() != 64
            || !id.bytes().all(|byte| byte.is_ascii_hexdigit())
            || !crate::exec::parse::compose_status_is_running(status)
            || oneoff
            || !self.owns(
                checkout_label(present(working_dir), present(checkout), present(scope)),
                present(scope),
            )
        {
            return Err(refuse(
                "full-ID/oneoff/running/checkout/scope ownership changed or is unknown",
            ));
        }
        Ok(id.to_owned())
    }

    /// Prove that `inspect_stdout` describes `row` as a running, non-oneoff
    /// service of this checkout and scope.
    pub fn verify_inspected(
        &self,
        repo_root: &Path,
        policy: &EffectiveContainerPolicy,
        row: &RunningComposeContainer,
        inspect_stdout: &[u8],
    ) -> Result<VerifiedComposeContainer, ContainerExecError> {
        let raw: serde_json::Value = serde_json::from_slice(inspect_stdout).map_err(|error| {
            ContainerExecError::refused(
                "inspect owned binding",
                format!("binding unavailable: inspect JSON is invalid: {error}"),
            )
        })?;
        let identity =
            crate::exec::parse::inspected_identity_from_value(&raw, false).map_err(|error| {
                // The parser's own cause is the diagnosis: a missing canonical
                // identity is one reason among malformed JSON, a wrong record
                // count, a short id, a missing name and a missing State.Status.
                ContainerExecError::refused(
                    "inspect owned binding",
                    format!(
                        "binding unavailable: canonical container identity is unverified: {error}"
                    ),
                )
            })?;
        let record = raw.as_array().and_then(|records| records.first());
        let labels = record
            .and_then(|record| record.get("Config"))
            .and_then(|config| config.get("Labels"));
        let (working_dir, oneoff_label_ok) = compose_binding_labels(labels);
        let inspected_scope = labels
            .and_then(|labels| labels.get(crate::runtime::scope::SCOPE_LABEL))
            .map(|value| value.as_str().unwrap_or_default());
        let owned = self.inspected_identity_matches(policy, row, &identity, oneoff_label_ok, true)
            && self.owns(working_dir, inspected_scope);
        if !owned {
            return Err(ContainerExecError::refused(
                "inspect owned binding",
                format!(
                    "binding unavailable: full-ID/project/service/oneoff/running/checkout/scope ownership does not match owned project `{}` at `{}`",
                    policy.project_name,
                    self.canonical.display(),
                ),
            ));
        }
        let Some(ports) = record
            .and_then(|record| record.get("NetworkSettings"))
            .and_then(|network| network.get("Ports"))
            .and_then(|ports| ports.as_object())
        else {
            return Err(ContainerExecError::refused(
                "inspect owned binding",
                "binding unavailable: fresh published-port associations are missing",
            ));
        };
        let mut current = row.clone();
        current.ports.clear();
        for (container_port, bindings) in ports {
            for binding in bindings.as_array().into_iter().flatten() {
                let host_ip = binding.get("HostIp").and_then(|value| value.as_str());
                let host_port = binding.get("HostPort").and_then(|value| value.as_str());
                if let (Some(ip), Some(port)) = (host_ip, host_port) {
                    if matches!(ip, "127.0.0.1" | "0.0.0.0")
                        && port.parse::<u16>().is_ok_and(|port| port != 0)
                    {
                        current
                            .ports
                            .push(format!("{ip}:{port}->{container_port}").into());
                    }
                }
            }
        }
        Ok(VerifiedComposeContainer {
            workspace_artifact_builder: false,
            row: current,
            id: identity.id,
            checkout: self.canonical.clone(),
            scope: self
                .scope_token
                .as_deref()
                .map(|scope| scope.trim().to_owned()),
            profile: policy.profile.clone(),
            backend: crate::compose::resolve_compose_backend_for_repo(repo_root, policy),
        })
    }
}
