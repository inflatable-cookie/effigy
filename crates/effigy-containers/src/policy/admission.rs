//! Detached preparation snapshots and renderer-derived artifact receipts.
use crate::{ContainerPolicyError, EffectiveContainerPolicy};
use effigy_gateway::ports::sha256_hex;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct PolicyAdmission {
    identity: Value,
    declarations: BTreeSet<PathBuf>,
    files: BTreeMap<PathBuf, Option<String>>,
    marker: Option<Value>,
}
fn refusal() -> ContainerPolicyError {
    ContainerPolicyError::TaskInvocation("detached preparation refused: the container declaration, identity or Compose input changed while it was being admitted, or its admission record is unreadable; nothing was activated or published. Retry `effigy container up --detach`; if this persists, run `effigy container down` first".to_owned())
}
fn fingerprint(path: &Path) -> Result<Option<String>, ContainerPolicyError> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(sha256_hex(&bytes))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(refusal()),
    }
}
impl PolicyAdmission {
    pub(crate) fn capture(
        identity: Value,
        declarations: BTreeSet<PathBuf>,
        artifacts: Vec<PathBuf>,
        marker_path: PathBuf,
    ) -> Result<Self, ContainerPolicyError> {
        let files = declarations
            .iter()
            .cloned()
            .chain(artifacts)
            .map(|path| fingerprint(&path).map(|sha| (path, sha)))
            .collect::<Result<_, _>>()?;
        let marker = match std::fs::read(&marker_path) {
            Ok(bytes) => Some(serde_json::from_slice::<Value>(&bytes).map_err(|_| refusal())?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(_) => return Err(refusal()),
        };
        let admission = Self {
            identity,
            declarations,
            files,
            marker,
        };
        if let Some(marker) = &admission.marker {
            for (key, value) in admission.identity.as_object().ok_or_else(refusal)? {
                if marker.get(key) != Some(value) {
                    return Err(refusal());
                }
            }
            for input in marker["inputs"].as_array().ok_or_else(refusal)? {
                let path = PathBuf::from(input["path"].as_str().ok_or_else(refusal)?);
                let expected = admission
                    .files
                    .get(&path)
                    .and_then(Option::as_deref)
                    .ok_or_else(refusal)?;
                if input["sha256"].as_str() != Some(expected) {
                    return Err(refusal());
                }
            }
        }
        admission.verify()?;
        Ok(admission)
    }
    pub fn verify(&self) -> Result<(), ContainerPolicyError> {
        for (path, expected) in &self.files {
            if fingerprint(path)? != *expected {
                return Err(refusal());
            }
        }
        let root = Path::new(self.identity["repo_root"].as_str().ok_or_else(refusal)?);
        if root.canonicalize().map_err(|_| refusal())? != root {
            return Err(refusal());
        }
        let scope = effigy_core::worktree_scope::resolve(root)
            .map_err(|_| refusal())?
            .map(|scope| {
                std::fs::read_to_string(
                    scope
                        .token_dir
                        .join(effigy_core::worktree_scope::SCOPE_FILE),
                )
            })
            .transpose()
            .map_err(|_| refusal())?;
        if json!(scope) != self.identity["scope_token"] {
            return Err(refusal());
        }
        let backend = crate::compose::resolve_declared_backend(
            root,
            self.identity["container_name"]
                .as_str()
                .ok_or_else(refusal)?,
            effigy_manifest::ManifestContainerDriver::Colima,
        );
        if json!(backend_name(backend)) != self.identity["backend"] {
            return Err(refusal());
        }
        Ok(())
    }
    pub(crate) fn before_render_write(
        &self,
        path: &Path,
        bytes: &[u8],
    ) -> Result<(), ContainerPolicyError> {
        self.verify()?;
        if !self.files.contains_key(path) || self.declarations.contains(path) {
            return Err(refusal());
        }
        if let Some(marker) = &self.marker {
            for input in marker["inputs"].as_array().ok_or_else(refusal)? {
                if input["path"].as_str() == path.to_str()
                    && input["sha256"].as_str() != Some(sha256_hex(bytes).as_str())
                {
                    return Err(refusal());
                }
            }
        }
        Ok(())
    }
    pub(crate) fn record_rendered(
        &mut self,
        path: &Path,
        bytes: &[u8],
    ) -> Result<(), ContainerPolicyError> {
        let expected = Some(sha256_hex(bytes));
        if fingerprint(path)? != expected {
            return Err(refusal());
        }
        self.files.insert(path.to_owned(), expected);
        self.verify()
    }
    pub fn context_for_policy(
        &self,
        policy: &EffectiveContainerPolicy,
    ) -> Result<Value, ContainerPolicyError> {
        self.verify()?;
        for (key, value) in [
            ("container_name", policy.name.as_str()),
            ("profile", policy.profile.as_str()),
            ("project_name", policy.project_name.as_str()),
            ("primary_service", policy.primary_service.as_str()),
        ] {
            if self.identity[key].as_str() != Some(value) {
                return Err(refusal());
            }
        }
        let mut value = self.identity.clone();
        let paths = self
            .declarations
            .iter()
            .cloned()
            .chain(policy.compose_files.iter().cloned())
            .collect::<BTreeSet<_>>();
        let inputs = paths
            .into_iter()
            .map(|path| {
                let sha = self
                    .files
                    .get(&path)
                    .and_then(Option::as_ref)
                    .ok_or_else(refusal)?;
                Ok(json!({"path":path,"sha256":sha}))
            })
            .collect::<Result<Vec<_>, ContainerPolicyError>>()?;
        value["inputs"] = json!(inputs);
        Ok(value)
    }
    pub fn marker_required(&self) -> bool {
        self.marker.is_some()
    }
}
pub(crate) fn backend_name(backend: crate::compose::ComposeBackend) -> &'static str {
    match backend {
        crate::compose::ComposeBackend::Docker => "docker",
        crate::compose::ComposeBackend::ColimaNerdctl => "colima-nerdctl",
    }
}

#[cfg(test)]
mod drift_tests {
    use super::*;
    use crate::policy::load::load_admitted_container_policy;

    /// Private direct-Compose checkout with a no-port builder. Only the manifest
    /// and compose file are written, and the fresh directory is never a real repo.
    fn direct_builder_checkout(label: &str) -> PathBuf {
        let root = tempfile::Builder::new()
            .prefix(&format!("effigy-admission-{label}-"))
            .tempdir()
            .expect("fresh private admission fixture")
            .keep();
        let root = root
            .canonicalize()
            .expect("canonical private admission fixture");
        std::fs::create_dir_all(root.join("infra/release")).expect("mkdir compose dir");
        std::fs::write(
            root.join("infra/release/docker-compose.yml"),
            "services:\n  builder:\n    image: example\n",
        )
        .expect("write compose");
        std::fs::write(
            root.join("effigy.toml"),
            r#"[containers.builder]
driver = "colima"
startup = "detached"
profile = "dev"
compose_file = "infra/release/docker-compose.yml"
project_name = "admission-drift-builder"
primary_service = "builder"
"#,
        )
        .expect("write manifest");
        root
    }

    #[test]
    fn direct_builder_admission_refuses_rendered_override_drift() {
        let root = direct_builder_checkout("override");
        let marker_root = root.join("markers");
        let (policy, admission) =
            load_admitted_container_policy(&root, Some("builder"), &marker_root)
                .expect("admit direct builder");
        admission
            .context_for_policy(&policy)
            .expect("fresh admission");
        let rendered_override = policy.compose_files[1].clone();
        assert!(rendered_override.ends_with("builder.ownership.compose.yml"));

        std::fs::write(&rendered_override, "services: {}\n").expect("drift override");

        assert!(
            admission.verify().is_err(),
            "a changed rendered override must refuse admission"
        );
    }

    #[test]
    fn direct_builder_admission_refuses_declared_compose_drift() {
        let root = direct_builder_checkout("declaration");
        let marker_root = root.join("markers");
        let (policy, admission) =
            load_admitted_container_policy(&root, Some("builder"), &marker_root)
                .expect("admit direct builder");
        admission
            .context_for_policy(&policy)
            .expect("fresh admission");

        std::fs::write(
            root.join("infra/release/docker-compose.yml"),
            "services:\n  builder:\n    image: other\n",
        )
        .expect("drift declaration");

        assert!(
            admission.verify().is_err(),
            "a changed declared compose file must refuse admission"
        );
    }

    #[test]
    fn direct_builder_admission_refuses_manifest_source_drift() {
        let root = direct_builder_checkout("manifest");
        let marker_root = root.join("markers");
        let (policy, admission) =
            load_admitted_container_policy(&root, Some("builder"), &marker_root)
                .expect("admit direct builder");
        admission
            .context_for_policy(&policy)
            .expect("fresh admission");

        let manifest = std::fs::read_to_string(root.join("effigy.toml")).expect("read manifest");
        std::fs::write(
            root.join("effigy.toml"),
            format!("{manifest}\n# source drift\n"),
        )
        .expect("drift manifest");

        assert!(
            admission.verify().is_err(),
            "a changed manifest source must refuse admission"
        );
    }
}
