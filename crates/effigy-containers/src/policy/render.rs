//! Host rendering is an in-memory file closure. Only Prepare publishes it.
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use effigy_kernel::objects::Closure;

use crate::{ContainerPolicyError, EffectiveContainerPolicy};

mod publication;
pub(crate) use publication::{checkout, checkout_for_root, recorded_backend, state_home};
pub use publication::{prepare, reset, verify};
mod vm;
pub use vm::compose_invocation_args;

pub const ROOT_VARIABLE: &str = "EFFIGY_RENDER_ROOT";
const ROOT_EXPRESSION: &str = "${EFFIGY_RENDER_ROOT:?Effigy render root is required}";

fn error(error: impl std::fmt::Display) -> ContainerPolicyError {
    ContainerPolicyError::TaskInvocation(format!("container render: {error}"))
}

#[derive(Debug, Clone)]
pub struct Rendered {
    pub closure: Closure,
    pub path: PathBuf,
    pub(crate) request: Request,
}

#[derive(Debug, Clone)]
pub(crate) enum Request {
    Named(Option<effigy_manifest::ManifestWorkspaceConfig>),
    Inline {
        container: effigy_manifest::ManifestInlineWorkspaceContainerConfig,
        workdir: Option<String>,
    },
}

/// Prepare the same workspace binding that was observed during planning.
/// Named and inline workspaces share publication but retain their own inputs.
pub fn prepare_resolved(
    policy: &EffectiveContainerPolicy,
) -> Result<EffectiveContainerPolicy, ContainerPolicyError> {
    let prepared = prepare_inputs(policy)?;
    prepare(&prepared)?;
    Ok(prepared)
}

/// Allocate Prepare inputs without publishing a desired generation.
/// Retained admission must be checked before calling `prepare`.
pub fn prepare_inputs(
    policy: &EffectiveContainerPolicy,
) -> Result<EffectiveContainerPolicy, ContainerPolicyError> {
    let Some(render) = &policy.render else {
        return Ok(policy.clone());
    };
    publication::prepare_identity(policy)?;
    effigy_core::runtime_dir::ensure_effigy_ignored_in_git_root(&policy.repo_root)
        .map_err(error)?;
    let resolve = || match &render.request {
        Request::Named(workspace) => super::load::load_container_policy_with_workspace(
            &policy.repo_root,
            Some(&policy.name),
            workspace.as_ref(),
        ),
        Request::Inline { container, workdir } => {
            super::inline_workspace::load_inline_workspace_container_policy(
                &policy.repo_root,
                &policy.name,
                container,
                workdir.as_deref(),
            )
        }
    };
    let prepared = resolve()?;
    let prepared = if super::ports::prepare_container_policy_ports(&prepared)? {
        resolve()?
    } else {
        prepared
    };
    // Owned mount directories can change canonical bind paths (for example
    // macOS /var). Establish them before the admission snapshot is captured.
    super::mounts::prepare_container_mount_sources(&prepared)?;
    resolve()
}

impl EffectiveContainerPolicy {
    /// Read the resolved snapshot, including before its first publication.
    /// Expand the reserved root before internal consumers parse bind syntax;
    /// the immutable file itself retains Compose interpolation.
    /// Hand-built policies without a render retain ordinary file semantics.
    pub fn read_compose(&self, path: &Path) -> Result<String, ContainerPolicyError> {
        if let Some(render) = &self.render {
            if let Ok(name) = path.strip_prefix(&render.path) {
                if let Some(bytes) = render
                    .closure
                    .files()
                    .get(&name.to_string_lossy().to_string())
                {
                    let content = String::from_utf8(bytes.clone()).map_err(error)?;
                    if !content.contains(ROOT_EXPRESSION) {
                        return Ok(content);
                    }
                    let mut document: serde_yaml::Value =
                        serde_yaml::from_str(&content).map_err(error)?;
                    relocate(
                        &mut document,
                        &[(ROOT_EXPRESSION.into(), render.path.display().to_string())],
                    );
                    return serde_yaml::to_string(&document).map_err(error);
                }
            }
        }
        std::fs::read_to_string(path).map_err(|error| ContainerPolicyError::Read {
            path: path.into(),
            error,
        })
    }
}

/// Logical paths preserve the old renderer's base-directory semantics until
/// every contributor has finished. No file is materialized along the way.
#[derive(Debug, Default)]
pub(crate) struct Draft {
    files: BTreeMap<PathBuf, Vec<u8>>,
    inputs: BTreeMap<String, Vec<u8>>,
}
impl Draft {
    pub(crate) fn declarations(&mut self, paths: &[PathBuf]) -> Result<(), ContainerPolicyError> {
        for (index, path) in paths.iter().enumerate() {
            self.inputs.insert(
                format!("inputs/declarations/{index}.toml"),
                std::fs::read(path).map_err(error)?,
            );
        }
        self.inputs.insert(
            "inputs/declarations.json".into(),
            serde_json::to_vec(paths).map_err(error)?,
        );
        Ok(())
    }

    pub(crate) fn insert(&mut self, path: PathBuf, bytes: impl Into<Vec<u8>>) {
        self.files.insert(path, bytes.into());
    }
    pub(crate) fn read(&mut self, path: &Path) -> Result<String, ContainerPolicyError> {
        if let Some(bytes) = self.files.get(path) {
            return String::from_utf8(bytes.clone()).map_err(error);
        }
        let bytes = std::fs::read(path).map_err(|error| ContainerPolicyError::Read {
            path: path.into(),
            error,
        })?;
        let text = String::from_utf8(bytes.clone()).map_err(error)?;
        self.insert(path.into(), bytes);
        Ok(text)
    }
    pub(crate) fn assembly(
        &mut self,
        directory: &Path,
        name: &str,
        assembly: &effigy_catalog::assembly::AssemblyResult,
    ) -> PathBuf {
        let path = directory.join(format!("{name}.generated.compose.yml"));
        self.insert(path.clone(), assembly.compose_yaml.as_bytes());
        for (name, bytes) in &assembly.dockerfiles {
            self.insert(
                directory
                    .join(".effigy-catalog")
                    .join(name)
                    .join("Dockerfile"),
                bytes.as_bytes(),
            );
        }
        for (name, bytes) in &assembly.config_files {
            self.insert(directory.join(name), bytes.as_bytes());
        }
        path
    }
    pub(crate) fn finish(
        mut self,
        policy: &mut EffectiveContainerPolicy,
        manifest: &str,
    ) -> Result<(), ContainerPolicyError> {
        // User overrides and direct Compose are inputs too, not mutable files
        // referenced from outside the admitted generation.
        let compose = policy
            .compose_files
            .iter()
            .cloned()
            .chain(
                policy
                    .shared_services
                    .iter()
                    .map(|s| s.compose_file.clone()),
            )
            .collect::<Vec<_>>();
        for path in &compose {
            self.read(path)?;
        }
        let names = self
            .files
            .keys()
            .map(|path| {
                let name = if let Ok(relative) = path.strip_prefix(&policy.repo_root) {
                    format!("checkout/{}", relative.display())
                } else {
                    let home = crate::effigy_home().ok_or_else(|| error("HOME is required"))?;
                    let relative = path.strip_prefix(home).map_err(error)?;
                    format!("shared/{}", relative.display())
                };
                Ok((path.clone(), name))
            })
            .collect::<Result<BTreeMap<_, _>, ContainerPolicyError>>()?;
        let mut files = self.inputs;
        let mut replacements = names
            .iter()
            .map(|(path, name)| {
                (
                    path.display().to_string(),
                    format!("{ROOT_EXPRESSION}/{name}"),
                )
            })
            .collect::<Vec<_>>();
        // Dockerfiles are relative to build.context, and bind sources are
        // relative to the first Compose input. Normalize before relocating.
        replacements.sort_by_key(|(path, _)| std::cmp::Reverse(path.len()));
        for (path, bytes) in self.files {
            let bytes = if matches!(
                path.extension().and_then(OsStr::to_str),
                Some("yml" | "yaml")
            ) {
                let mut value: serde_yaml::Value = serde_yaml::from_slice(&bytes).map_err(error)?;
                super::super::workspace::normalize_runtime_rewrite_paths(
                    &mut value,
                    path.parent()
                        .ok_or_else(|| error("Compose input has no parent"))?,
                );
                relocate_build_files(&mut value, &replacements);
                relocate(&mut value, &replacements);
                serde_yaml::to_string(&value).map_err(error)?.into_bytes()
            } else {
                bytes
            };
            files.insert(names[&path].clone(), bytes);
        }
        files.insert("inputs/manifest.toml".into(), manifest.as_bytes().to_vec());
        files.insert("inputs/environment.json".into(), serde_json::to_vec(&serde_json::json!({
            "repo_root":policy.repo_root,"name":policy.name,"profile":policy.profile,
            "project_name":policy.project_name,"primary_service":policy.primary_service,
            "backend":super::admission::backend_name(crate::compose::resolve_declared_backend(&policy.repo_root,&policy.name,policy.driver)),
        })).map_err(error)?);
        let closure = Closure::new(files).map_err(error)?;
        let path = publication::object_directory(&policy.repo_root)?.join(closure.digest());
        for file in &mut policy.compose_files {
            *file = path.join(&names[&*file]);
        }
        for shared in &mut policy.shared_services {
            shared.compose_file = path.join(&names[&shared.compose_file]);
        }
        policy.render = Some(Rendered {
            closure,
            path,
            request: Request::Named(None),
        });
        Ok(())
    }
}

fn relocate_build_files(value: &mut serde_yaml::Value, replacements: &[(String, String)]) {
    let Some(services) = value
        .get_mut("services")
        .and_then(serde_yaml::Value::as_mapping_mut)
    else {
        return;
    };
    for service in services.values_mut() {
        let Some(build) = service
            .get_mut("build")
            .and_then(serde_yaml::Value::as_mapping_mut)
        else {
            continue;
        };
        let context = build
            .get(serde_yaml::Value::String("context".into()))
            .and_then(serde_yaml::Value::as_str)
            .map(PathBuf::from);
        let Some(serde_yaml::Value::String(file)) =
            build.get_mut(serde_yaml::Value::String("dockerfile".into()))
        else {
            continue;
        };
        if let Some(context) = context {
            let resolved = context.join(&*file);
            if let Some((_, target)) = replacements
                .iter()
                .find(|(source, _)| Path::new(source) == resolved)
            {
                *file = target.clone();
            }
        }
    }
}

fn relocate(value: &mut serde_yaml::Value, replacements: &[(String, String)]) {
    match value {
        serde_yaml::Value::String(text) => {
            for (source, target) in replacements {
                *text = text.replace(source, target);
            }
        }
        serde_yaml::Value::Sequence(values) => {
            for value in values {
                relocate(value, replacements);
            }
        }
        serde_yaml::Value::Mapping(values) => {
            for value in values.values_mut() {
                relocate(value, replacements);
            }
        }
        _ => {}
    }
}

/// Supply the immutable generation root to Compose without process-global
/// environment mutation. Both direct and manager-backed adapters use this.
pub fn apply_environment(command: &mut std::process::Command, args: &[impl AsRef<OsStr>]) {
    if let Some(root) = render_root(args) {
        command.env(ROOT_VARIABLE, root);
    }
}

/// The immutable generation root this invocation's Compose files live under.
/// Every `-f` input inside a published closure names one object directory, so
/// the first `-f` ancestor under `objects` is the root Compose must interpolate.
fn render_root(args: &[impl AsRef<OsStr>]) -> Option<PathBuf> {
    for pair in args.windows(2) {
        if pair[0].as_ref() != OsStr::new("-f") {
            continue;
        }
        let path = Path::new(pair[1].as_ref());
        for root in path.ancestors() {
            let Some(name) = root.file_name().and_then(OsStr::to_str) else {
                continue;
            };
            if name.len() == 64
                && name.bytes().all(|c| c.is_ascii_hexdigit())
                && root.parent().and_then(Path::file_name) == Some(OsStr::new("objects"))
            {
                return Some(root.to_path_buf());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests;
