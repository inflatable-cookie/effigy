//! Detached admission pins a kernel object reference, never mutable render files.
use crate::{ContainerPolicyError, EffectiveContainerPolicy};
use serde_json::{json, Value};
use std::path::Path;

#[derive(Debug)]
pub struct PolicyAdmission {
    policy: EffectiveContainerPolicy,
    context: Value,
    marker_required: bool,
}
fn refusal() -> ContainerPolicyError {
    ContainerPolicyError::TaskInvocation("detached preparation refused: the immutable environment generation or its admission record changed or is unreadable; retry `effigy container up --detach`".into())
}

pub fn generation_context(
    policy: &EffectiveContainerPolicy,
) -> Result<Value, ContainerPolicyError> {
    let render = policy.render.as_ref().ok_or_else(refusal)?;
    let scope = effigy_core::worktree_scope::load(&policy.repo_root).map_err(|_| refusal())?;
    Ok(
        json!({"schema":"effigy.container-partial-activation.v3","repo_root":policy.repo_root,
        "scope_token":scope,"container_name":policy.name,"profile":policy.profile,
        "project_name":policy.project_name,"primary_service":policy.primary_service,
        "backend":backend_name(crate::compose::resolve_declared_backend(&policy.repo_root,&policy.name,policy.driver)),
        "compose_source":match policy.compose_source {crate::EffectiveComposeSource::Direct=>"direct",crate::EffectiveComposeSource::Generated=>"generated"},
        "generation":render.closure.digest(),"path":render.path}),
    )
}

/// One-release import of a v0.14.1 admission receipt. Hashing is confined to
/// this importer; all new admission and publication checks use object refs.
pub fn normalize_context(
    value: Value,
    policy: &EffectiveContainerPolicy,
) -> Result<Value, ContainerPolicyError> {
    let current = generation_context(policy)?;
    if value.get("generation").is_some() {
        return Ok(value);
    }
    if value["schema"] != "effigy.container-partial-activation.v2" {
        return Err(refusal());
    }
    for key in [
        "repo_root",
        "scope_token",
        "container_name",
        "profile",
        "project_name",
        "primary_service",
        "backend",
        "compose_source",
    ] {
        if value[key] != current[key] {
            return Err(refusal());
        }
    }
    for input in value["inputs"]
        .as_array()
        .filter(|inputs| !inputs.is_empty())
        .ok_or_else(refusal)?
    {
        let path = Path::new(input["path"].as_str().ok_or_else(refusal)?);
        let bytes = std::fs::read(path).map_err(|_| refusal())?;
        if input["sha256"].as_str() != Some(effigy_gateway::ports::sha256_hex(&bytes).as_str()) {
            return Err(refusal());
        }
    }
    Ok(current)
}

impl PolicyAdmission {
    pub(crate) fn capture(
        policy: &EffectiveContainerPolicy,
        marker_root: &Path,
    ) -> Result<Self, ContainerPolicyError> {
        let context = generation_context(policy)?;
        let clean = |value: &str| {
            value
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                        c
                    } else {
                        '-'
                    }
                })
                .collect::<String>()
        };
        let marker = marker_root
            .join(clean(&policy.profile))
            .join(clean(&policy.project_name))
            .join(format!("{}.partial.json", clean(&policy.name)));
        let mut import = false;
        let marker_required = match std::fs::read(&marker) {
            Ok(bytes) => {
                let value: Value = serde_json::from_slice(&bytes).map_err(|_| refusal())?;
                let legacy = value.get("generation").is_none();
                if normalize_context(value, policy)? != context {
                    return Err(refusal());
                }
                import = legacy;
                true
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
            Err(_) => return Err(refusal()),
        };
        // Validate retained admission before moving the desired reference.
        super::render::prepare(policy)?;
        if import {
            let bytes = serde_json::to_vec_pretty(&context).map_err(|_| refusal())?;
            effigy_catalog::atomic_write::write_atomic(&marker, &bytes).map_err(|_| refusal())?;
        }
        let admission = Self {
            policy: policy.clone(),
            context,
            marker_required,
        };
        admission.verify()?;
        Ok(admission)
    }
    pub fn verify(&self) -> Result<(), ContainerPolicyError> {
        super::render::verify(&self.policy)
    }
    pub fn context_for_policy(
        &self,
        policy: &EffectiveContainerPolicy,
    ) -> Result<Value, ContainerPolicyError> {
        self.verify()?;
        if generation_context(policy)? != self.context {
            return Err(refusal());
        }
        Ok(self.context.clone())
    }
    pub fn marker_required(&self) -> bool {
        self.marker_required
    }
}

pub(crate) fn backend_name(backend: crate::compose::ComposeBackend) -> &'static str {
    match backend {
        crate::compose::ComposeBackend::Docker => "docker",
        crate::compose::ComposeBackend::ColimaNerdctl => "colima-nerdctl",
    }
}
