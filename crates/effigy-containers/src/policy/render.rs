//! Host rendering is an in-memory file closure. Only Prepare publishes it.
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use effigy_kernel::objects::Closure;

use crate::{ContainerPolicyError, EffectiveContainerPolicy};

mod publication;
pub(crate) use publication::{checkout, recorded_backend, state_home};
pub use publication::{prepare, publish_shared_render, reset, verify};
mod vm;
pub use vm::compose_invocation_args;

pub(crate) mod healthcheck;

fn error(error: impl std::fmt::Display) -> ContainerPolicyError {
    ContainerPolicyError::TaskInvocation(format!("container render: {error}"))
}

#[derive(Debug, Clone)]
pub struct Rendered {
    pub closure: Closure,
    pub path: PathBuf,
    /// Shared services render outside the checkout's generation: one
    /// machine-wide, content-addressed directory each (effigy#377).
    pub shared: Vec<crate::SharedRender>,
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

/// Publish one policy's frozen render closure through the kernel owner.
///
/// This is the existing Prepare publication: the closure is staged under the
/// kernel's private staging directory and promoted atomically into its
/// content-addressed object directory (no-replace, exact digest). A caller that
/// must read the render before the one apply has published it — a preflight that
/// asks Compose for the render's own config hash — uses this instead of writing
/// objects itself.
pub use publication::prepare as publish_render;

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
    /// Render the workspace channel alias supplied by backend qualification.
    /// This is an in-memory render only: it neither chooses an address nor
    /// creates a network. Its resulting closure is what activation must freeze.
    pub fn with_workspace_host_alias(&self, address: &str) -> Result<Self, ContainerPolicyError> {
        if address != "host-gateway"
            && !address
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| !ip.is_unspecified() && !ip.is_multicast())
        {
            return Err(error(
                "workspace alias needs an observed address or the backend host-gateway token",
            ));
        }
        let render = self
            .render
            .as_ref()
            .ok_or_else(|| error("workspace alias requires a snapshotted Compose render"))?;
        let mut existing = false;
        for path in &self.compose_files {
            let document: serde_yaml::Value =
                serde_yaml::from_str(&self.read_compose(path)?).map_err(error)?;
            let Some(hosts) = document
                .get("services")
                .and_then(|services| services.get(&self.primary_service))
                .and_then(|service| service.get("extra_hosts"))
            else {
                continue;
            };
            let values = match hosts {
                serde_yaml::Value::Mapping(mapping) => mapping
                    .iter()
                    .filter(|(key, _)| key.as_str() == Some("effigy-host"))
                    .map(|(_, value)| value.as_str().map(str::to_owned))
                    .collect::<Vec<_>>(),
                serde_yaml::Value::Sequence(entries) => entries
                    .iter()
                    .filter_map(|entry| {
                        entry
                            .as_str()
                            .and_then(|entry| {
                                entry
                                    .strip_prefix("effigy-host:")
                                    .or_else(|| entry.strip_prefix("effigy-host="))
                            })
                            .map(|value| Some(value.to_owned()))
                    })
                    .collect::<Vec<_>>(),
                _ => return Err(error("workspace extra_hosts must be a mapping or sequence")),
            };
            for value in values {
                if existing || value.as_deref() != Some(address) {
                    return Err(error(
                        "workspace alias conflicts with an existing Compose entry",
                    ));
                }
                existing = true;
            }
        }
        if existing {
            return Ok(self.clone());
        }
        let name = "checkout/.effigy/runtime/workspace-channel.override.yml";
        let mut files = render.closure.files().clone();
        let bytes = serde_yaml::to_string(&serde_json::json!({
            "services": { (&self.primary_service): { "extra_hosts": { "effigy-host": address } } }
        }))
        .map_err(error)?
        .into_bytes();
        if files.insert(name.into(), bytes).is_some() {
            return Err(error("workspace alias render path is already occupied"));
        }
        let closure = Closure::new(files).map_err(error)?;
        let path = publication::object_directory(&self.repo_root)?.join(closure.digest());
        let mut next = self.clone();
        for file in &mut next.compose_files {
            let relative = file.strip_prefix(&render.path).map_err(error)?;
            *file = path.join(relative);
        }
        next.compose_files.push(path.join(name));
        if let Some(render) = &mut next.render {
            render.path = path;
            render.closure = closure;
        }
        Ok(next)
    }

    /// Whether this render binds a workspace to a service it defines.
    pub(crate) fn has_selected_workspace_service(&self) -> bool {
        let Some(render) = &self.render else {
            return false;
        };
        if matches!(&render.request, Request::Named(None)) {
            return false;
        }
        self.compose_files
            .first()
            .and_then(|file| self.read_compose(file).ok())
            .and_then(|content| serde_yaml::from_str::<serde_yaml::Value>(&content).ok())
            .and_then(|document| {
                document
                    .get("services")?
                    .get(&self.primary_service)
                    .cloned()
            })
            .is_some_and(|service| service.is_mapping())
    }

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
                    let defining = self
                        .compose_files
                        .first()
                        .is_some_and(|first| first == path);
                    resolve_generation_paths(&mut document, self.project_directory(), defining);
                    return serde_yaml::to_string(&document).map_err(error);
                }
            }
        }
        std::fs::read_to_string(path).map_err(|error| ContainerPolicyError::Read {
            path: path.into(),
            error,
        })
    }

    /// The Compose project directory of the generation's invocation: the
    /// directory of its first `-f` input, which is where Compose resolves every
    /// relative path.
    fn project_directory(&self) -> &Path {
        self.compose_files
            .first()
            .and_then(|input| input.parent())
            .unwrap_or(Path::new("."))
    }
}

/// Logical paths preserve the old renderer's base-directory semantics until
/// every contributor has finished. No file is materialized along the way.
#[derive(Debug, Default)]
pub(crate) struct Draft {
    files: BTreeMap<PathBuf, Vec<u8>>,
    inputs: BTreeMap<String, Vec<u8>>,
    shared: Vec<PendingSharedRender>,
}

/// A shared service's assembled input. `Draft::finish` completes it, because
/// the backend decides part of its normalisation and the digest covers the
/// bytes Compose reads.
#[derive(Debug)]
pub(crate) struct PendingSharedRender {
    pub project_name: String,
    /// `<effigy home>/shared-services/<project>`: the digest names the
    /// subdirectory the files are published in.
    pub directory: PathBuf,
    pub compose_yaml: String,
    pub config_files: std::collections::HashMap<String, String>,
}
impl Draft {
    /// Defer one shared service's render. It belongs to no checkout, so it is
    /// published outside the generation (effigy#377).
    pub(crate) fn shared_render(
        &mut self,
        project_name: String,
        directory: PathBuf,
        compose_yaml: String,
        config_files: std::collections::HashMap<String, String>,
    ) {
        self.shared.push(PendingSharedRender {
            project_name,
            directory,
            compose_yaml,
            config_files,
        });
    }
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
    /// Add one assembly to the render. `directory` holds the Compose file;
    /// `configs` is where the catalog's own files are published, because a
    /// fragment names them relative to the `repo_root` it was rendered with.
    /// A shared service renders with its own state directory as that root, so
    /// its init SQL lands where its bind source already points (effigy#371).
    pub(crate) fn assembly(
        &mut self,
        directory: &Path,
        configs: &Path,
        name: &str,
        assembly: &effigy_catalog::assembly::AssemblyResult,
    ) -> Result<PathBuf, ContainerPolicyError> {
        let path = directory.join(format!("{name}.generated.compose.yml"));
        let compose = inline_catalog_dockerfiles(directory, assembly);
        self.insert(path.clone(), compose.into_bytes());
        for (name, bytes) in &assembly.config_files {
            self.insert(configs.join(name), bytes.as_bytes());
        }
        Ok(path)
    }
    pub(crate) fn finish(
        mut self,
        policy: &mut EffectiveContainerPolicy,
        manifest: &str,
    ) -> Result<(), ContainerPolicyError> {
        // User overrides and direct Compose are inputs too, not mutable files
        // referenced from outside the admitted generation.
        let compose = policy.compose_files.clone();
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
                let name = match path.strip_prefix(&policy.repo_root) {
                    Ok(relative) => format!("checkout/{}", relative.display()),
                    Err(_) => {
                        let home = crate::effigy_home().ok_or_else(|| error("HOME is required"))?;
                        let relative = path.strip_prefix(home).map_err(error)?;
                        format!("external/{}", relative.display())
                    }
                };
                Ok((path.clone(), name))
            })
            .collect::<Result<BTreeMap<_, _>, ContainerPolicyError>>()?;
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
                let directory = first_input
                    .as_ref()
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
                let defining = first_input.as_deref() == Some(path.as_path());
                super::super::workspace::normalize_runtime_rewrite_paths(
                    &mut value,
                    path.parent()
                        .ok_or_else(|| error("Compose input has no parent"))?,
                    defining,
                );
                relocate(&mut value, &replacements);
                if backend == crate::compose::ComposeBackend::ColimaNerdctl {
                    healthcheck::normalize_exec_form(&mut value);
                }
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
        let mut shared = Vec::new();
        for pending in self.shared {
            let project = pending.project_name.clone();
            let (render, digest, compose_file) = finish_shared_render(pending, backend)?;
            if let Some(binding) = policy
                .shared_services
                .iter_mut()
                .find(|service| service.project_name == project)
            {
                binding.compose_file = compose_file;
                binding.render_digest = digest;
            }
            shared.push(render);
        }
        policy.render = Some(Rendered {
            closure,
            path,
            shared,
            request: Request::Named(None),
        });
        Ok(())
    }
}

/// Finish a shared service's render at its machine-wide directory.
///
/// `directory` is `<effigy home>/shared-services/<project>` and the digest names
/// its subdirectory, so the render a checkout publishes is the one every other
/// checkout computes: same bytes, same absolute path, same Compose config-hash
/// (effigy#377). The bind sources stay absolute, inside that directory.
fn finish_shared_render(
    pending: PendingSharedRender,
    backend: crate::compose::ComposeBackend,
) -> Result<(crate::SharedRender, String, PathBuf), ContainerPolicyError> {
    let compose_name = format!("{}.generated.compose.yml", pending.project_name);
    let mut document: serde_yaml::Value = serde_yaml::from_str(&pending.compose_yaml)
        .map_err(|e| error(format!("shared {}: {e}", pending.project_name)))?;
    // The rules the environment's own render applies, because this file is
    // what Compose reads.
    super::super::workspace::normalize_runtime_rewrite_paths(
        &mut document,
        &pending.directory,
        true,
    );
    if backend == crate::compose::ComposeBackend::ColimaNerdctl {
        healthcheck::normalize_exec_form(&mut document);
    }
    let mut content = serde_yaml::to_string(&document)
        .map_err(error)?
        .into_bytes();
    let mut configs = pending.config_files.iter().collect::<Vec<_>>();
    configs.sort_by(|left, right| left.0.cmp(right.0));
    for (name, body) in &configs {
        content.extend_from_slice(name.as_bytes());
        content.push(0);
        content.extend_from_slice(body.as_bytes());
        content.push(0);
    }
    let digest = effigy_gateway::ports::sha256_hex(&content);
    label_shared_render(&mut document, &digest);
    let directory = pending.directory.join(&digest);
    relocate(
        &mut document,
        &[(
            pending.directory.display().to_string(),
            directory.display().to_string(),
        )],
    );
    let mut files = BTreeMap::new();
    files.insert(
        PathBuf::from(&compose_name),
        serde_yaml::to_string(&document)
            .map_err(error)?
            .into_bytes(),
    );
    for (name, body) in pending.config_files {
        files.insert(
            Path::new(crate::GENERATED_RUNTIME_COMPOSE_DIR).join(name),
            body.into_bytes(),
        );
    }
    let compose_file = directory.join(&compose_name);
    Ok((
        crate::SharedRender { directory, files },
        digest,
        compose_file,
    ))
}

/// The render names itself on the service it creates, so `up` reads from the
/// runtime whether the shared container it finds is this render (effigy#377).
fn label_shared_render(document: &mut serde_yaml::Value, digest: &str) {
    let Some(services) = document
        .get_mut("services")
        .and_then(serde_yaml::Value::as_mapping_mut)
    else {
        return;
    };
    for service in services.values_mut() {
        let Some(service) = service.as_mapping_mut() else {
            continue;
        };
        let key = serde_yaml::Value::String("labels".into());
        if !matches!(service.get(&key), Some(serde_yaml::Value::Mapping(_))) {
            service.insert(
                key.clone(),
                serde_yaml::Value::Mapping(serde_yaml::Mapping::new()),
            );
        }
        if let Some(serde_yaml::Value::Mapping(labels)) = service.get_mut(&key) {
            labels.insert(
                serde_yaml::Value::String(crate::SHARED_RENDER_LABEL.into()),
                serde_yaml::Value::String(digest.to_owned()),
            );
        }
    }
}

fn backend_name(backend: crate::compose::ComposeBackend) -> &'static str {
    match backend {
        crate::compose::ComposeBackend::Docker => "docker",
        crate::compose::ComposeBackend::ColimaNerdctl => "colima-nerdctl",
    }
}

/// The assembly's Compose document, with every catalog Dockerfile inlined.
///
/// Compose resolves `build.dockerfile` against the build context, which is the
/// checkout, while the generation holds the file. The Dockerfile's content is
/// already part of the generation, so the render names the content directly:
/// no path is resolved against the context and no extra copy is published.
fn inline_catalog_dockerfiles(
    directory: &Path,
    assembly: &effigy_catalog::assembly::AssemblyResult,
) -> String {
    let Ok(mut document) = serde_yaml::from_str::<serde_yaml::Value>(&assembly.compose_yaml) else {
        return assembly.compose_yaml.clone();
    };
    let Some(services) = document
        .get_mut("services")
        .and_then(serde_yaml::Value::as_mapping_mut)
    else {
        return assembly.compose_yaml.clone();
    };
    for service in services.values_mut() {
        let Some(build) = service
            .get_mut("build")
            .and_then(serde_yaml::Value::as_mapping_mut)
        else {
            continue;
        };
        let key = serde_yaml::Value::String("dockerfile".into());
        let Some(serde_yaml::Value::String(dockerfile)) = build.get(&key) else {
            continue;
        };
        // Compose resolves the path against the build context, which is what
        // decides whether it names a Dockerfile this render publishes.
        let Some(serde_yaml::Value::String(context)) =
            build.get(serde_yaml::Value::String("context".into()))
        else {
            continue;
        };
        let resolved = Path::new(&context).join(dockerfile);
        let Some((_, content)) = assembly.dockerfiles.iter().find(|(catalog, _)| {
            resolved
                == directory
                    .join(".effigy-catalog")
                    .join(catalog)
                    .join("Dockerfile")
        }) else {
            // A fragment may name a Dockerfile in the project itself. That one
            // is not part of the assembly, and its build context is absolute,
            // so Compose reads it where the project keeps it.
            continue;
        };
        build.remove(&key);
        build.insert(
            serde_yaml::Value::String("dockerfile_inline".into()),
            serde_yaml::Value::String(compose_literal(content)),
        );
    }
    serde_yaml::to_string(&document).unwrap_or_else(|_| assembly.compose_yaml.clone())
}

/// Compose interpolates every string in a Compose file, `dockerfile_inline`
/// included, so a Dockerfile's own `$VAR` and `${VAR}` would be substituted or
/// blanked before BuildKit saw it. `$$` is Compose's literal dollar, so the
/// content reaches the build byte for byte (effigy#366).
fn compose_literal(content: &str) -> String {
    content.replace('$', "$$")
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
fn resolve_generation_paths(
    document: &mut serde_yaml::Value,
    directory: &Path,
    defines_services: bool,
) {
    crate::workspace::normalize_runtime_rewrite_paths(document, directory, defines_services);
}

#[cfg(test)]
mod tests;
