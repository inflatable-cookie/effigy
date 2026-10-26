//! Host rendering is an in-memory file closure. Only Prepare publishes it.
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use effigy_kernel::objects::Closure;

use crate::{ContainerPolicyError, EffectiveContainerPolicy};

mod publication;
pub(crate) use publication::{checkout, recorded_backend, state_home};
pub use publication::{prepare, reset, verify};
mod vm;
pub use vm::compose_invocation_args;

mod healthcheck;
mod restart;

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
    // Owned mount directories can change canonical bind paths (for example
    // macOS /var). Establish them before the admission snapshot is captured.
    super::mounts::prepare_container_mount_sources(&prepared)?;
    resolve()
}

impl EffectiveContainerPolicy {
    /// Read the resolved snapshot, including before its first publication.
    /// A rendered input names other files of its generation relative to the
    /// Compose project directory, so readers resolve them where Compose will.
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
                    if !content.contains("./") && !content.contains("../") {
                        return Ok(content);
                    }
                    let mut document: serde_yaml::Value =
                        serde_yaml::from_str(&content).map_err(error)?;
                    resolve_generation_paths(&mut document, self.project_directory(path));
                    return serde_yaml::to_string(&document).map_err(error);
                }
            }
        }
        std::fs::read_to_string(path).map_err(|error| ContainerPolicyError::Read {
            path: path.into(),
            error,
        })
    }

    /// The Compose project directory of the invocation that reads `file`: the
    /// directory of its first `-f` input, which is where Compose resolves every
    /// relative path. A shared service is read by its own invocation.
    fn project_directory(&self, file: &Path) -> &Path {
        project_input(
            file,
            self.compose_files.first(),
            self.shared_services
                .iter()
                .map(|service| &service.compose_file),
        )
        .and_then(Path::parent)
        .unwrap_or(Path::new("."))
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
        // The backend this generation will run on decides one render rule:
        // nerdctl compose cannot run the `CMD` exec form of a health check.
        let backend = crate::compose::resolve_declared_backend(
            &policy.repo_root,
            &policy.name,
            policy.driver,
        );
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
        let shared_inputs = policy
            .shared_services
            .iter()
            .map(|service| service.compose_file.clone())
            .collect::<Vec<_>>();
        let first_input = policy.compose_files.first().cloned();
        let mut files = self.inputs;
        for (path, bytes) in self.files {
            let bytes = if matches!(
                path.extension().and_then(OsStr::to_str),
                Some("yml" | "yaml")
            ) {
                // Compose resolves every relative input against the directory
                // of the invocation's first `-f` file, so that directory is how
                // this document names the rest of the generation.
                let directory = project_input(&path, first_input.as_ref(), shared_inputs.iter())
                    .and_then(|input| names.get(input))
                    .and_then(|name| Path::new(name).parent())
                    .unwrap_or(Path::new(""));
                let mut replacements = names
                    .iter()
                    .map(|(source, name)| {
                        (
                            source.display().to_string(),
                            relative_name(directory, Path::new(name)),
                        )
                    })
                    .collect::<Vec<_>>();
                // Dockerfiles are relative to build.context, and bind sources
                // are relative to the project directory. Normalize first.
                replacements.sort_by_key(|(path, _)| std::cmp::Reverse(path.len()));
                let mut value: serde_yaml::Value = serde_yaml::from_slice(&bytes).map_err(error)?;
                super::super::workspace::normalize_runtime_rewrite_paths(
                    &mut value,
                    path.parent()
                        .ok_or_else(|| error("Compose input has no parent"))?,
                );
                relocate_build_files(&mut value, &replacements);
                relocate(&mut value, &replacements);
                if backend == crate::compose::ComposeBackend::ColimaNerdctl {
                    healthcheck::normalize_exec_form(&mut value);
                }
                // The runtime restarts a stopped service; Effigy never does.
                restart::apply(&mut value);
                serde_yaml::to_string(&value).map_err(error)?.into_bytes()
            } else {
                bytes
            };
            files.insert(names[&path].clone(), bytes);
        }
        files.insert("inputs/manifest.toml".into(), manifest.as_bytes().to_vec());
        files.insert(
            "inputs/environment.json".into(),
            serde_json::to_vec(&serde_json::json!({
                "repo_root":policy.repo_root,"name":policy.name,"profile":policy.profile,
                "project_name":policy.project_name,"primary_service":policy.primary_service,
                "backend":backend_name(backend),
            }))
            .map_err(error)?,
        );
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

fn backend_name(backend: crate::compose::ComposeBackend) -> &'static str {
    match backend {
        crate::compose::ComposeBackend::Docker => "docker",
        crate::compose::ComposeBackend::ColimaNerdctl => "colima-nerdctl",
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

/// The first `-f` input of the Compose invocation that reads `file`, which is
/// what decides its project directory. A shared service brings its own.
fn project_input<'a>(
    file: &Path,
    first: Option<&'a PathBuf>,
    shared: impl Iterator<Item = &'a PathBuf>,
) -> Option<&'a Path> {
    shared
        .into_iter()
        .find(|compose| compose.parent().is_some_and(|dir| file.starts_with(dir)))
        .or(first)
        .map(PathBuf::as_path)
}

/// `name` relative to the generation's `directory`, spelled the way Compose
/// reads a relative input: `./…` beside the project directory, `../…` outside.
fn relative_name(directory: &Path, name: &Path) -> String {
    let directory = directory.components().collect::<Vec<_>>();
    let name = name.components().collect::<Vec<_>>();
    let shared = directory
        .iter()
        .zip(&name)
        .take_while(|(left, right)| left == right)
        .count();
    let mut parts: Vec<String> = vec!["..".to_owned(); directory.len() - shared];
    parts.extend(
        name[shared..]
            .iter()
            .map(|part| part.as_os_str().to_string_lossy().into_owned()),
    );
    let relative = parts.join("/");
    if relative.starts_with("..") {
        relative
    } else {
        format!("./{relative}")
    }
}

/// Resolve the relative inputs a render names into the generation, so internal
/// readers see the files Compose reads. The generation is immutable, so its own
/// names are the only relative inputs that survive a render.
fn resolve_generation_paths(document: &mut serde_yaml::Value, directory: &Path) {
    crate::workspace::normalize_runtime_rewrite_paths(document, directory);
}

#[cfg(test)]
mod tests;
