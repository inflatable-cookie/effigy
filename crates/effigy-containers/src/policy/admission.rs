//! Detached admission pins a kernel object reference, never mutable render files.
use crate::{ContainerPolicyError, EffectiveContainerPolicy};
use serde_json::{json, Value};
use std::path::Path;

#[derive(Debug)]
pub struct PolicyAdmission {
    policy: EffectiveContainerPolicy,
    context: Value,
}
fn refusal() -> ContainerPolicyError {
    ContainerPolicyError::TaskInvocation("detached preparation refused: the immutable environment generation or its admission record changed or is unreadable; retry `effigy container up --detach`".into())
}

pub fn generation_context(
    policy: &EffectiveContainerPolicy,
) -> Result<Value, ContainerPolicyError> {
    let render = policy.render.as_ref().ok_or_else(refusal)?;
    let scope = effigy_core::worktree_scope::load(&policy.repo_root).map_err(|_| refusal())?;
    let checkout = std::fs::canonicalize(&policy.repo_root).map_err(|_| refusal())?;
    Ok(
        json!({"schema":"effigy.container-partial-activation.v3","repo_root":checkout,
        "scope_token":scope,"container_name":policy.name,"profile":policy.profile,
        "project_name":policy.project_name,"primary_service":policy.primary_service,
        "backend":backend_name(crate::compose::resolve_declared_backend(&policy.repo_root,&policy.name,policy.driver)),
        "compose_source":match policy.compose_source {crate::EffectiveComposeSource::Direct=>"direct",crate::EffectiveComposeSource::Generated=>"generated"},
        "generation":render.closure.digest(),"path":render.path}),
    )
}

/// Reads a v0.14.1 admission receipt for the environment importer's
/// adoption check. Hashing is confined to it; every new admission and
/// publication check uses object references.
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
    /// Publishes the policy's render as desired and pins it. The environment
    /// record keeps the admitted generation; no marker file is read or
    /// written (contract 052 M3).
    pub(crate) fn capture(policy: &EffectiveContainerPolicy) -> Result<Self, ContainerPolicyError> {
        let context = generation_context(policy)?;
        super::render::prepare(policy)?;
        let admission = Self {
            policy: policy.clone(),
            context,
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
}

pub(crate) fn backend_name(backend: crate::compose::ComposeBackend) -> &'static str {
    match backend {
        crate::compose::ComposeBackend::Docker => "docker",
        crate::compose::ComposeBackend::ColimaNerdctl => "colima-nerdctl",
    }
}
