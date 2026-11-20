//! Verify project, checkout and scope ownership before container effects.

use super::*;

fn label_value<'a>(labels: Option<&'a serde_json::Value>, name: &str) -> Option<&'a str> {
    labels
        .and_then(|labels| labels.get(name))
        .and_then(|value| value.as_str())
}

/// The raw Compose working directory and explicit checkout labels, before the
/// checkout is chosen from them. [`OwnershipContext::owns_binding`] judges both.
pub(super) fn compose_binding_sources(
    labels: Option<&serde_json::Value>,
) -> (Option<&str>, Option<&str>) {
    (
        label_value(labels, "com.docker.compose.project.working_dir"),
        label_value(labels, crate::runtime::scope::CHECKOUT_LABEL),
    )
}

pub(super) fn compose_binding_labels(labels: Option<&serde_json::Value>) -> (Option<&str>, bool) {
    let label = |name: &str| label_value(labels, name);
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

/// Whether `name` is one immutable render generation's object digest: a
/// 64-character lowercase hexadecimal name under `.effigy/state/objects`.
fn is_object_digest(name: &str) -> bool {
    name.len() == 64
        && name
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// The checkout root `path` is an immutable Compose render directory of, when
/// it is exactly `<root>/.effigy/state/objects/<64-hex>/checkout/.effigy/runtime/compose`.
/// Docker Desktop and Native Linux stamp that directory as the project working
/// directory of an unscoped primary checkout. Any other shape, including a
/// malformed digest, returns `None`.
fn immutable_render_checkout(path: &Path) -> Option<&Path> {
    // Matched from the leaf upward; `None` marks the digest segment.
    const SUFFIX: [Option<&str>; 8] = [
        Some("compose"),
        Some("runtime"),
        Some(".effigy"),
        Some("checkout"),
        None,
        Some("objects"),
        Some("state"),
        Some(".effigy"),
    ];
    let mut current = path;
    for expected in SUFFIX {
        let name = current.file_name()?.to_str()?;
        let matches = match expected {
            Some(literal) => name == literal,
            None => is_object_digest(name),
        };
        if !matches {
            return None;
        }
        current = current.parent()?;
    }
    Some(current)
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
            "container binding changed before effect\ncontainer full ID or checkout/scope/backend binding changed\nnext: retry the command or run `effigy container status`",
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
    /// checkout and scope. An unscoped container's own immutable render
    /// directory names this checkout too; [`Self::owns_binding`] adds the
    /// explicit-label cross-check that such a render needs.
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
        let own_render =
            scope.is_none() && immutable_render_checkout(&path) == Some(self.canonical.as_path());
        path == self.canonical
            || own_render
            || self
                .staged_render
                .as_ref()
                .is_some_and(|plan| plan.owns_staged_render_directory(&path))
    }

    /// Whether a container's Compose working directory and checkout label
    /// together name this checkout and scope.
    ///
    /// Docker Desktop and Native Linux stamp an unscoped primary checkout's
    /// immutable render directory as its project working directory, so the
    /// label Compose selects is the render, not the checkout. Such a container
    /// is owned only when its explicit `com.effigy.checkout` label also
    /// canonicalizes to this checkout: the render names a generation, the label
    /// names the owner. Every other working directory is judged by
    /// [`Self::owns`] over the label Compose selects.
    pub(super) fn owns_binding(
        &self,
        working_dir: Option<&str>,
        checkout: Option<&str>,
        scope: Option<&str>,
    ) -> bool {
        let own_render = scope.is_none()
            && working_dir
                .and_then(|dir| Path::new(dir).canonicalize().ok())
                .is_some_and(|dir| {
                    immutable_render_checkout(&dir) == Some(self.canonical.as_path())
                });
        if own_render {
            let explicit = checkout.and_then(|value| Path::new(value).canonicalize().ok());
            return explicit.as_deref() == Some(self.canonical.as_path())
                && self.owns(checkout, scope);
        }
        self.owns(checkout_label(working_dir, checkout, scope), scope)
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
                    && self.owns_binding(present(working_dir), present(checkout), present(scope))
                {
                    continue;
                }
            }
            return Err(ContainerExecError::Failure {
                command: "verify managed shutdown ownership".to_owned(),
                code: None,
                stdout: String::new(),
                stderr: format!(
                    "Refused container teardown: project `{}` has an unknown or foreign container\nno teardown ran (full-ID/checkout/scope ownership could not be verified)\nproject: {}\ncheckout: {}\nnext: inspect running containers with `effigy container status`",
                    policy.project_name,
                    policy.project_name,
                    self.canonical.display(),
                ),
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
        let refuse = |reason: String| ContainerExecError::refused("list owned service", reason);
        if !self.declared.contains(service) {
            return Err(refuse(format!(
                "service `{service}` is not declared by the selected Compose files\nnext: check the service name in `effigy.toml` or Compose configuration"
            )));
        }
        let listing = String::from_utf8_lossy(listing_stdout);
        let mut rows = listing.lines().filter(|line| !line.trim().is_empty());
        let row = rows.next().ok_or_else(|| {
            refuse(format!(
                "container service `{service}` is not running\nnext: inspect with `effigy container status` or start it with `effigy container up`"
            ))
        })?;
        if rows.next().is_some() {
            return Err(refuse(format!(
                "more than one running container found for service `{service}`\nnext: inspect running containers with `effigy container status`"
            )));
        }
        let fields = row.split('\t').map(str::trim).collect::<Vec<_>>();
        let [id, status, oneoff, working_dir, checkout, scope] = fields[..] else {
            return Err(refuse(
                "binding unavailable: service listing is malformed\nnext: inspect running containers with `effigy container status`".to_owned(),
            ));
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
            || !self.owns_binding(present(working_dir), present(checkout), present(scope))
        {
            let mut reason = format!(
                "container for service `{service}` is not owned by this checkout or is not running\nfull-ID/oneoff/running/checkout/scope ownership changed or is unknown\nservice: {service}\ncheckout: {}",
                self.canonical.display(),
            );
            if let Some(dir) = present(working_dir) {
                reason.push_str(&format!("\ncompose_working_dir: {dir}"));
            }
            reason.push_str("\nnext: inspect running containers with `effigy container status`");
            return Err(refuse(reason));
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
        let oneoff_label_ok = compose_binding_labels(labels).1;
        let (inspected_working_dir, inspected_checkout) = compose_binding_sources(labels);
        let inspected_scope = labels
            .and_then(|labels| labels.get(crate::runtime::scope::SCOPE_LABEL))
            .map(|value| value.as_str().unwrap_or_default());
        let owned = self.inspected_identity_matches(policy, row, &identity, oneoff_label_ok, true)
            && self.owns_binding(inspected_working_dir, inspected_checkout, inspected_scope);
        if !owned {
            let mut detail = format!(
                "container `{}` does not match owned project `{}`\nfull-ID/project/service/oneoff/running/checkout/scope ownership does not match\nproject: {}\ncheckout: {}",
                row.container_name,
                policy.project_name,
                policy.project_name,
                self.canonical.display(),
            );
            if let Some(working_dir) = inspected_working_dir {
                detail.push_str(&format!("\ncompose_working_dir: {working_dir}"));
            }
            if let Some(checkout) = inspected_checkout {
                detail.push_str(&format!("\ncontainer_checkout: {checkout}"));
            }
            detail.push_str("\nnext: inspect running containers with `effigy container status`");
            return Err(ContainerExecError::refused("inspect owned binding", detail));
        }
        let Some(ports) = record
            .and_then(|record| record.get("NetworkSettings"))
            .and_then(|network| network.get("Ports"))
            .and_then(|ports| ports.as_object())
        else {
            return Err(ContainerExecError::refused(
                "inspect owned binding",
                format!(
                    "container `{}` has no published-port associations\nbinding unavailable: fresh published-port associations are missing\nproject: {}\nnext: inspect running containers with `effigy container status`",
                    row.container_name,
                    policy.project_name,
                ),
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

#[cfg(test)]
mod immutable_render_ownership_tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::fs;

    use crate::compose::ComposeBackend;

    fn digest() -> String {
        "0123456789abcdef".repeat(4)
    }

    fn other_digest() -> String {
        "fedcba9876543210".repeat(4)
    }

    struct Fixture {
        _temp: tempfile::TempDir,
        root: PathBuf,
        foreign: PathBuf,
        context: OwnershipContext,
        policy: EffectiveContainerPolicy,
    }

    fn fixture() -> Fixture {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("repo");
        let foreign = temp.path().join("foreign");
        fs::create_dir_all(&root).expect("repo");
        fs::create_dir_all(&foreign).expect("foreign");
        let root = root.canonicalize().expect("canonical repo");
        let foreign = foreign.canonicalize().expect("canonical foreign");
        let policy = crate::policy::model::test_policy(&root);
        let context = OwnershipContext {
            declared: BTreeSet::from(["app".to_owned()]),
            canonical: root.clone(),
            scope_token: None,
            staged_render: None,
        };
        Fixture {
            _temp: temp,
            root,
            foreign,
            context,
            policy,
        }
    }

    /// Create `<checkout>/.effigy/state/objects/<digest>/checkout/.effigy/runtime/compose`
    /// and return its path as Compose would label it.
    fn render_dir(checkout: &Path, digest: &str) -> String {
        let dir = checkout
            .join(".effigy/state/objects")
            .join(digest)
            .join("checkout/.effigy/runtime/compose");
        fs::create_dir_all(&dir).expect("render directory");
        dir.display().to_string()
    }

    fn inspect(project: &str, working_dir: &str, checkout: Option<&str>) -> Vec<u8> {
        let mut labels = serde_json::json!({
            "com.docker.compose.project": project,
            "com.docker.compose.service": "app",
            "com.docker.compose.oneoff": "False",
            "com.docker.compose.project.working_dir": working_dir,
        });
        if let Some(checkout) = checkout {
            labels[crate::runtime::scope::CHECKOUT_LABEL] = serde_json::json!(checkout);
        }
        serde_json::to_vec(&serde_json::json!([{
            "Id": "ab".repeat(32),
            "Name": "/app-1",
            "State": {"Status": "running", "Running": true},
            "Config": {"Labels": labels},
            "NetworkSettings": {"Ports": {}},
            "Mounts": [],
        }]))
        .expect("inspect json")
    }

    fn row(project: &str) -> RunningComposeContainer {
        RunningComposeContainer {
            container_name: "app-1".to_owned(),
            status: "Up".into(),
            ports: vec![],
            project_name: Some(project.to_owned()),
            working_dir: None,
            service: Some("app".to_owned()),
            oneoff: false,
        }
    }

    /// Whether each verifier accepts a container with this working directory
    /// and checkout label, in order: shutdown listing, service listing, inspect.
    fn verdicts(fixture: &Fixture, working_dir: &str, checkout: Option<&str>) -> [bool; 3] {
        let project = fixture.policy.project_name.clone();
        let id = "ab".repeat(32);
        let label = checkout.unwrap_or("");
        let shutdown = format!("{id}\t{project}\t{working_dir}\t{label}\t\n");
        let listed = format!("{id}\trunning\tfalse\t{working_dir}\t{label}\t\n");
        let inspected_json = inspect(&project, working_dir, checkout);
        let inspected =
            crate::compose::with_test_compose_backend(ComposeBackend::ColimaNerdctl, || {
                fixture
                    .context
                    .verify_inspected(
                        &fixture.root,
                        &fixture.policy,
                        &row(&project),
                        &inspected_json,
                    )
                    .is_ok()
            });
        [
            fixture
                .context
                .verify_shutdown_listing(&fixture.policy, shutdown.as_bytes())
                .is_ok(),
            fixture
                .context
                .verify_listed_service("app", listed.as_bytes())
                .is_ok(),
            inspected,
        ]
    }

    #[test]
    fn unscoped_checkout_accepts_its_own_immutable_render_in_every_verifier() {
        let fixture = fixture();
        let root = fixture.root.display().to_string();
        let current = render_dir(&fixture.root, &digest());
        let previous = render_dir(&fixture.root, &other_digest());

        assert_eq!(verdicts(&fixture, &current, Some(&root)), [true; 3]);
        // A previous valid generation still reconciles across edits.
        assert_eq!(verdicts(&fixture, &previous, Some(&root)), [true; 3]);
    }

    #[test]
    fn immutable_render_directories_are_refused_unless_they_belong_to_this_checkout() {
        let fixture = fixture();
        let root = fixture.root.display().to_string();
        let foreign = fixture.foreign.display().to_string();
        let own = render_dir(&fixture.root, &digest());
        let foreign_render = render_dir(&fixture.foreign, &digest());

        // A foreign checkout's render, with this checkout's label or its own.
        assert_eq!(
            verdicts(&fixture, &foreign_render, Some(&root)),
            [false; 3],
            "foreign render with spoofed local label"
        );
        assert_eq!(
            verdicts(&fixture, &foreign_render, Some(&foreign)),
            [false; 3],
            "foreign render with its own label"
        );
        // This checkout's own render, with a label naming another checkout.
        assert_eq!(
            verdicts(&fixture, &own, Some(&foreign)),
            [false; 3],
            "own render with foreign label"
        );
        assert_eq!(
            verdicts(&fixture, &own, None),
            [false; 3],
            "own render without a checkout label"
        );
        assert_eq!(
            verdicts(&fixture, &own, Some(&format!("{root}/.effigy"))),
            [false; 3],
            "own render with a label below the checkout root"
        );
        // Spoofing through the working directory alone is still refused.
        assert_eq!(
            verdicts(&fixture, &foreign, Some(&root)),
            [false; 3],
            "foreign working directory with spoofed local label"
        );
    }

    #[test]
    fn malformed_digest_directories_are_refused() {
        let fixture = fixture();
        let root = fixture.root.display().to_string();
        let uppercase = "0123456789ABCDEF".repeat(4);
        let short = "0123456789abcdef";
        let non_hex = "g".repeat(64);
        for digest in [uppercase.as_str(), short, non_hex.as_str()] {
            let malformed = render_dir(&fixture.root, digest);
            assert_eq!(
                verdicts(&fixture, &malformed, Some(&root)),
                [false; 3],
                "malformed digest `{digest}`"
            );
        }
        // A `..` segment resolves outside the object directory before any check.
        fs::create_dir_all(fixture.root.join(".effigy/state/objects")).expect("objects");
        let escaped = fixture
            .root
            .join(".effigy/state/checkout/.effigy/runtime/compose");
        fs::create_dir_all(&escaped).expect("escaped render");
        let dotdot = format!("{root}/.effigy/state/objects/../checkout/.effigy/runtime/compose");
        assert_eq!(
            verdicts(&fixture, &dotdot, Some(&root)),
            [false; 3],
            "dot-dot render path"
        );
    }

    #[test]
    fn immutable_render_checkout_names_only_the_exact_render_shape() {
        let fixture = fixture();
        let own = PathBuf::from(render_dir(&fixture.root, &digest()));
        assert_eq!(
            immutable_render_checkout(&own),
            Some(fixture.root.as_path())
        );
        let short = own.display().to_string().replace(&digest(), "abc");
        assert_eq!(immutable_render_checkout(Path::new(&short)), None);
        let wrong_tail = own
            .display()
            .to_string()
            .replace("/runtime/compose", "/runtime/other");
        assert_eq!(immutable_render_checkout(Path::new(&wrong_tail)), None);
    }
}
