use std::ffi::OsString;
use std::path::{Path, PathBuf};

use effigy_core::resolver::{resolve_target_root, ResolvedTarget};

pub const CONTAINER_HANDOFF_ENV_NAME: &str = "EFFIGY_INTERNAL_CONTAINER_HANDOFF";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffigyRuntimeContext {
    invocation_cwd: PathBuf,
    command_root: PathBuf,
    repo_override: Option<PathBuf>,
    target: ResolvedRuntimeTarget,
    paths: RuntimePathSet,
    host: HostRuntimeInfo,
    container: ContainerRuntimeInfo,
    invocation_mode: RuntimeInvocationMode,
}

impl EffigyRuntimeContext {
    pub fn capture(
        cwd_override: Option<PathBuf>,
        repo_override: Option<PathBuf>,
    ) -> Result<Self, RuntimeContextError> {
        EffigyRuntimeContextBuilder::new()
            .cwd_override(cwd_override)
            .repo_override(repo_override)
            .capture()
    }

    pub fn capture_lossy(
        cwd_override: Option<PathBuf>,
        repo_override: Option<PathBuf>,
    ) -> Result<Self, RuntimeContextError> {
        EffigyRuntimeContextBuilder::new()
            .cwd_override(cwd_override)
            .repo_override(repo_override)
            .capture_lossy()
    }

    pub fn builder() -> EffigyRuntimeContextBuilder {
        EffigyRuntimeContextBuilder::new()
    }

    pub fn invocation_cwd(&self) -> &Path {
        &self.invocation_cwd
    }

    pub fn command_root(&self) -> &Path {
        &self.command_root
    }

    pub fn repo_override(&self) -> Option<&Path> {
        self.repo_override.as_deref()
    }

    pub fn target(&self) -> &ResolvedRuntimeTarget {
        &self.target
    }

    pub fn paths(&self) -> &RuntimePathSet {
        &self.paths
    }

    pub fn host(&self) -> &HostRuntimeInfo {
        &self.host
    }

    pub fn container(&self) -> &ContainerRuntimeInfo {
        &self.container
    }

    pub fn invocation_mode(&self) -> RuntimeInvocationMode {
        self.invocation_mode
    }

    #[cfg(test)]
    pub fn fake(invocation_cwd: PathBuf, command_root: PathBuf) -> Self {
        Self {
            invocation_cwd: invocation_cwd.clone(),
            command_root: command_root.clone(),
            repo_override: None,
            target: ResolvedRuntimeTarget {
                resolved_root: command_root,
                resolution_mode: "test".to_owned(),
                evidence: vec!["test context".to_owned()],
                warnings: Vec::new(),
            },
            paths: RuntimePathSet {
                home: Some(PathBuf::from("/tmp/home")),
                shell: Some(PathBuf::from("/bin/sh")),
                path: Some(OsString::from("/usr/bin")),
            },
            host: HostRuntimeInfo {
                os: "test-os".to_owned(),
                arch: "test-arch".to_owned(),
                no_color: false,
                ci: false,
            },
            container: ContainerRuntimeInfo {
                inside_container_handoff: false,
            },
            invocation_mode: RuntimeInvocationMode::Host,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRuntimeTarget {
    pub resolved_root: PathBuf,
    pub resolution_mode: String,
    pub evidence: Vec<String>,
    pub warnings: Vec<String>,
}

impl From<ResolvedTarget> for ResolvedRuntimeTarget {
    fn from(value: ResolvedTarget) -> Self {
        Self {
            resolved_root: value.resolved_root,
            resolution_mode: format!("{:?}", value.resolution_mode),
            evidence: value.evidence,
            warnings: value.warnings,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostRuntimeInfo {
    pub os: String,
    pub arch: String,
    pub no_color: bool,
    pub ci: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerRuntimeInfo {
    pub inside_container_handoff: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeInvocationMode {
    Host,
    ContainerHandoff,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimePathSet {
    pub home: Option<PathBuf>,
    pub shell: Option<PathBuf>,
    pub path: Option<OsString>,
}

#[derive(Debug, Clone, Default)]
pub struct EffigyRuntimeContextBuilder {
    cwd_override: Option<PathBuf>,
    repo_override: Option<PathBuf>,
    env: Option<CapturedEnv>,
}

impl EffigyRuntimeContextBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cwd_override(mut self, cwd: Option<PathBuf>) -> Self {
        self.cwd_override = cwd;
        self
    }

    pub fn repo_override(mut self, repo_override: Option<PathBuf>) -> Self {
        self.repo_override = repo_override;
        self
    }

    pub fn captured_env(mut self, env: CapturedEnv) -> Self {
        self.env = Some(env);
        self
    }

    pub fn capture(self) -> Result<EffigyRuntimeContext, RuntimeContextError> {
        let env = self.env.unwrap_or_else(CapturedEnv::from_process);
        let invocation_cwd = match self.cwd_override {
            Some(path) => path,
            None => std::env::current_dir().map_err(RuntimeContextError::CurrentDir)?,
        };
        let resolved = resolve_target_root(invocation_cwd.clone(), self.repo_override.clone())
            .map_err(|error| RuntimeContextError::Resolve(error.to_string()))?;
        let command_root = resolved.resolved_root.clone();
        let effective_repo_override = self.repo_override.as_ref().map(|_| command_root.clone());
        let inside_container_handoff = env.container_handoff.is_some();
        Ok(EffigyRuntimeContext {
            invocation_cwd,
            command_root,
            repo_override: effective_repo_override,
            target: resolved.into(),
            paths: RuntimePathSet {
                home: env.home.map(PathBuf::from),
                shell: env.shell.map(PathBuf::from),
                path: env.path,
            },
            host: HostRuntimeInfo {
                os: std::env::consts::OS.to_owned(),
                arch: std::env::consts::ARCH.to_owned(),
                no_color: env.no_color.is_some(),
                ci: env.ci.is_some(),
            },
            container: ContainerRuntimeInfo {
                inside_container_handoff,
            },
            invocation_mode: if inside_container_handoff {
                RuntimeInvocationMode::ContainerHandoff
            } else {
                RuntimeInvocationMode::Host
            },
        })
    }

    pub fn capture_lossy(self) -> Result<EffigyRuntimeContext, RuntimeContextError> {
        match self.clone().capture() {
            Ok(context) => Ok(context),
            Err(RuntimeContextError::Resolve(error)) => {
                let env = self.env.unwrap_or_else(CapturedEnv::from_process);
                let invocation_cwd = match self.cwd_override {
                    Some(path) => path,
                    None => std::env::current_dir().map_err(RuntimeContextError::CurrentDir)?,
                };
                let repo_override = self.repo_override;
                let command_root = repo_override
                    .clone()
                    .unwrap_or_else(|| invocation_cwd.clone());
                let inside_container_handoff = env.container_handoff.is_some();
                Ok(EffigyRuntimeContext {
                    invocation_cwd,
                    command_root: command_root.clone(),
                    repo_override,
                    target: ResolvedRuntimeTarget {
                        resolved_root: command_root,
                        resolution_mode: "LossyCwdFallback".to_owned(),
                        evidence: Vec::new(),
                        warnings: vec![format!(
                            "target root resolution failed; using cwd fallback: {error}"
                        )],
                    },
                    paths: RuntimePathSet {
                        home: env.home.map(PathBuf::from),
                        shell: env.shell.map(PathBuf::from),
                        path: env.path,
                    },
                    host: HostRuntimeInfo {
                        os: std::env::consts::OS.to_owned(),
                        arch: std::env::consts::ARCH.to_owned(),
                        no_color: env.no_color.is_some(),
                        ci: env.ci.is_some(),
                    },
                    container: ContainerRuntimeInfo {
                        inside_container_handoff,
                    },
                    invocation_mode: if inside_container_handoff {
                        RuntimeInvocationMode::ContainerHandoff
                    } else {
                        RuntimeInvocationMode::Host
                    },
                })
            }
            Err(error) => Err(error),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct CapturedEnv {
    pub home: Option<OsString>,
    pub path: Option<OsString>,
    pub shell: Option<OsString>,
    pub no_color: Option<OsString>,
    pub ci: Option<OsString>,
    pub container_handoff: Option<OsString>,
}

impl CapturedEnv {
    pub fn from_process() -> Self {
        Self {
            home: std::env::var_os("HOME"),
            path: std::env::var_os("PATH"),
            shell: std::env::var_os("SHELL"),
            no_color: std::env::var_os("NO_COLOR"),
            ci: std::env::var_os("CI"),
            container_handoff: std::env::var_os(CONTAINER_HANDOFF_ENV_NAME),
        }
    }
}

#[derive(Debug)]
pub enum RuntimeContextError {
    CurrentDir(std::io::Error),
    Resolve(String),
}

impl std::fmt::Display for RuntimeContextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CurrentDir(error) => write!(f, "failed to resolve current directory: {error}"),
            Self::Resolve(error) => write!(f, "failed to resolve runtime target: {error}"),
        }
    }
}

impl std::error::Error for RuntimeContextError {}

#[cfg(test)]
mod tests {
    use super::{CapturedEnv, EffigyRuntimeContext, RuntimeInvocationMode};
    use std::ffi::OsString;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_repo() -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "effigy-context-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).expect("mkdir temp repo");
        fs::write(root.join("Cargo.toml"), "[package]\nname = \"ctx\"\n").expect("write manifest");
        root
    }

    #[test]
    fn captures_resolved_root_from_cwd() {
        let root = temp_repo();
        let nested = root.join("nested");
        fs::create_dir_all(&nested).expect("mkdir nested");

        let context = EffigyRuntimeContext::builder()
            .cwd_override(Some(nested))
            .captured_env(CapturedEnv::default())
            .capture()
            .expect("capture context");

        let root = root.canonicalize().expect("canonical root");
        assert_eq!(context.command_root(), root.as_path());
        assert_eq!(context.target().resolved_root, root);
        assert_eq!(context.invocation_mode(), RuntimeInvocationMode::Host);
    }

    #[test]
    fn repo_override_wins_over_cwd() {
        let cwd_root = temp_repo();
        let override_root = temp_repo();

        let context = EffigyRuntimeContext::builder()
            .cwd_override(Some(cwd_root))
            .repo_override(Some(override_root.clone()))
            .captured_env(CapturedEnv::default())
            .capture()
            .expect("capture context");

        let override_root = override_root.canonicalize().expect("canonical override");
        assert_eq!(context.command_root(), override_root.as_path());
        assert_eq!(context.repo_override(), Some(override_root.as_path()));
    }

    #[test]
    fn detects_container_handoff_from_captured_env() {
        let root = temp_repo();
        let context = EffigyRuntimeContext::builder()
            .cwd_override(Some(root))
            .captured_env(CapturedEnv {
                container_handoff: Some(OsString::from("1")),
                ..CapturedEnv::default()
            })
            .capture()
            .expect("capture context");

        assert!(context.container().inside_container_handoff);
        assert_eq!(
            context.invocation_mode(),
            RuntimeInvocationMode::ContainerHandoff
        );
    }

    #[test]
    fn lossy_capture_falls_back_to_cwd_when_root_is_unresolved() {
        let root =
            std::env::temp_dir().join(format!("effigy-context-no-root-{}", std::process::id()));
        fs::create_dir_all(&root).expect("mkdir unresolved root");

        let context = EffigyRuntimeContext::builder()
            .cwd_override(Some(root.clone()))
            .captured_env(CapturedEnv::default())
            .capture_lossy()
            .expect("lossy capture context");

        assert_eq!(context.command_root(), root.as_path());
        assert_eq!(context.target().resolved_root, root);
        assert_eq!(context.target().resolution_mode, "LossyCwdFallback");
        assert_eq!(context.target().warnings.len(), 1);
    }
}
