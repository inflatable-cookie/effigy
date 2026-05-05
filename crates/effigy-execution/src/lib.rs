use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::PathBuf;

use effigy_context::EffigyRuntimeContext;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskExecutionRequest {
    pub runtime_context: EffigyRuntimeContext,
    pub invocation: ExecutionIntent,
    pub surface: ExecutionSurface,
    pub output_mode: ExecutionOutputMode,
    pub runtime_policy: ExecutionRuntimePolicy,
    pub handoff_policy: ExecutionHandoffPolicy,
    pub cleanup_policy: ExecutionCleanupPolicy,
    pub environment: ExecutionEnvironmentPlan,
}

impl TaskExecutionRequest {
    pub fn resolve(self) -> ResolvedTaskExecutionPlan {
        let route = match self.runtime_policy.run_in {
            ExecutionRunTarget::Host => ExecutionRoute::Host,
            ExecutionRunTarget::Container => {
                if self.runtime_context.container().inside_container_handoff {
                    ExecutionRoute::LocalContainerHandoff {
                        service: self.runtime_policy.service.clone(),
                    }
                } else {
                    ExecutionRoute::Container {
                        container: self.runtime_policy.container.clone(),
                        service: self.runtime_policy.service.clone(),
                    }
                }
            }
            ExecutionRunTarget::Either => {
                if self.runtime_context.container().inside_container_handoff {
                    ExecutionRoute::LocalContainerHandoff {
                        service: self.runtime_policy.service.clone(),
                    }
                } else {
                    ExecutionRoute::Host
                }
            }
        };

        ResolvedTaskExecutionPlan {
            request: self,
            route,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TaskExecutionRequestBuilder {
    runtime_context: Option<EffigyRuntimeContext>,
    invocation: Option<ExecutionIntent>,
    surface: Option<ExecutionSurface>,
    output_mode: Option<ExecutionOutputMode>,
    runtime_policy: Option<ExecutionRuntimePolicy>,
    handoff_policy: Option<ExecutionHandoffPolicy>,
    cleanup_policy: Option<ExecutionCleanupPolicy>,
    environment: Option<ExecutionEnvironmentPlan>,
}

impl TaskExecutionRequestBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn runtime_context(mut self, context: EffigyRuntimeContext) -> Self {
        self.runtime_context = Some(context);
        self
    }

    pub fn invocation(mut self, invocation: ExecutionIntent) -> Self {
        self.invocation = Some(invocation);
        self
    }

    pub fn task(mut self, selector: impl Into<String>, args: Vec<String>) -> Self {
        self.invocation = Some(ExecutionIntent::Task {
            selector: selector.into(),
            args,
        });
        self
    }

    pub fn command(mut self, command: Vec<String>) -> Self {
        self.invocation = Some(ExecutionIntent::Command { command });
        self
    }

    pub fn surface(mut self, surface: ExecutionSurface) -> Self {
        self.surface = Some(surface);
        self
    }

    pub fn output_mode(mut self, output_mode: ExecutionOutputMode) -> Self {
        self.output_mode = Some(output_mode);
        self
    }

    pub fn runtime_policy(mut self, runtime_policy: ExecutionRuntimePolicy) -> Self {
        self.runtime_policy = Some(runtime_policy);
        self
    }

    pub fn handoff_policy(mut self, handoff_policy: ExecutionHandoffPolicy) -> Self {
        self.handoff_policy = Some(handoff_policy);
        self
    }

    pub fn cleanup_policy(mut self, cleanup_policy: ExecutionCleanupPolicy) -> Self {
        self.cleanup_policy = Some(cleanup_policy);
        self
    }

    pub fn environment(mut self, environment: ExecutionEnvironmentPlan) -> Self {
        self.environment = Some(environment);
        self
    }

    pub fn build(self) -> Result<TaskExecutionRequest, ExecutionRequestError> {
        Ok(TaskExecutionRequest {
            runtime_context: self
                .runtime_context
                .ok_or(ExecutionRequestError::MissingRuntimeContext)?,
            invocation: self
                .invocation
                .ok_or(ExecutionRequestError::MissingInvocation)?,
            surface: self.surface.unwrap_or_default(),
            output_mode: self.output_mode.unwrap_or_default(),
            runtime_policy: self.runtime_policy.unwrap_or_default(),
            handoff_policy: self.handoff_policy.unwrap_or_default(),
            cleanup_policy: self.cleanup_policy.unwrap_or_default(),
            environment: self.environment.unwrap_or_default(),
        })
    }

    pub fn resolve(self) -> Result<ResolvedTaskExecutionPlan, ExecutionRequestError> {
        Ok(self.build()?.resolve())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTaskExecutionPlan {
    pub request: TaskExecutionRequest,
    pub route: ExecutionRoute,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ExecutionSurface {
    #[default]
    DirectCli,
    Deferral,
    Bootstrap,
    DataSeed,
    Rhai,
    RunArray,
    Demo,
    Managed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionIntent {
    Task { selector: String, args: Vec<String> },
    Command { command: Vec<String> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExecutionOutputMode {
    #[default]
    Capture,
    Stream,
    Tee,
    Json,
    Interactive,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionRuntimePolicy {
    pub run_in: ExecutionRunTarget,
    pub container: Option<String>,
    pub service: Option<String>,
}

impl ExecutionRuntimePolicy {
    pub fn host() -> Self {
        Self {
            run_in: ExecutionRunTarget::Host,
            container: None,
            service: None,
        }
    }

    pub fn container(container: impl Into<String>, service: Option<String>) -> Self {
        Self {
            run_in: ExecutionRunTarget::Container,
            container: Some(container.into()),
            service,
        }
    }

    pub fn either() -> Self {
        Self {
            run_in: ExecutionRunTarget::Either,
            container: None,
            service: None,
        }
    }
}

impl Default for ExecutionRuntimePolicy {
    fn default() -> Self {
        Self::either()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionRunTarget {
    Host,
    Container,
    Either,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExecutionHandoffPolicy {
    #[default]
    AllowContainerHandoff,
    ForceHostBoundary,
    RejectRecursiveHandoff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExecutionCleanupPolicy {
    #[default]
    Preserve,
    CleanupOnSuccess,
    CleanupAlways,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExecutionEnvironmentPlan {
    pub cwd: Option<PathBuf>,
    pub env: BTreeMap<String, OsString>,
    pub stdin_file: Option<PathBuf>,
}

impl ExecutionEnvironmentPlan {
    pub fn cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    pub fn stdin_file(mut self, stdin_file: impl Into<PathBuf>) -> Self {
        self.stdin_file = Some(stdin_file.into());
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<OsString>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionRoute {
    Host,
    Container {
        container: Option<String>,
        service: Option<String>,
    },
    LocalContainerHandoff {
        service: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionRequestError {
    MissingRuntimeContext,
    MissingInvocation,
}

impl std::fmt::Display for ExecutionRequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingRuntimeContext => write!(f, "missing runtime context"),
            Self::MissingInvocation => write!(f, "missing execution invocation"),
        }
    }
}

impl std::error::Error for ExecutionRequestError {}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use effigy_context::{CapturedEnv, EffigyRuntimeContext};

    use super::{
        ExecutionEnvironmentPlan, ExecutionIntent, ExecutionOutputMode, ExecutionRoute,
        ExecutionRunTarget, ExecutionRuntimePolicy, ExecutionSurface, TaskExecutionRequestBuilder,
    };

    fn temp_repo(name: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "effigy-execution-{name}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).expect("mkdir temp repo");
        fs::write(root.join("Cargo.toml"), "[package]\nname = \"ctx\"\n").expect("manifest");
        root
    }

    fn context(name: &str) -> EffigyRuntimeContext {
        EffigyRuntimeContext::builder()
            .cwd_override(Some(temp_repo(name)))
            .captured_env(CapturedEnv::default())
            .capture()
            .expect("runtime context")
    }

    #[test]
    fn builds_host_task_plan_from_runtime_context() {
        let plan = TaskExecutionRequestBuilder::new()
            .runtime_context(context("host"))
            .task("test", vec!["--unit".to_owned()])
            .surface(ExecutionSurface::DirectCli)
            .runtime_policy(ExecutionRuntimePolicy::host())
            .resolve()
            .expect("plan");

        assert_eq!(plan.route, ExecutionRoute::Host);
        assert_eq!(
            plan.request.invocation,
            ExecutionIntent::Task {
                selector: "test".to_owned(),
                args: vec!["--unit".to_owned()],
            }
        );
        assert_eq!(plan.request.runtime_policy.run_in, ExecutionRunTarget::Host);
    }

    #[test]
    fn builds_container_command_plan_with_stdin_file() {
        let plan = TaskExecutionRequestBuilder::new()
            .runtime_context(context("container"))
            .command(vec!["mysql".to_owned(), "app".to_owned()])
            .surface(ExecutionSurface::Rhai)
            .output_mode(ExecutionOutputMode::Capture)
            .runtime_policy(ExecutionRuntimePolicy::container(
                "web",
                Some("db".to_owned()),
            ))
            .environment(
                ExecutionEnvironmentPlan::default()
                    .stdin_file(".effigy/local/db-seeds/latest.sql")
                    .env("MYSQL_PWD", OsString::from("secret")),
            )
            .resolve()
            .expect("plan");

        assert_eq!(
            plan.route,
            ExecutionRoute::Container {
                container: Some("web".to_owned()),
                service: Some("db".to_owned()),
            }
        );
        assert_eq!(
            plan.request.environment.stdin_file,
            Some(PathBuf::from(".effigy/local/db-seeds/latest.sql"))
        );
    }

    #[test]
    fn direct_bootstrap_data_seed_and_rhai_task_surfaces_keep_plan_parity_for_same_inputs() {
        let context = context("surface-parity");
        let cwd = context.invocation_cwd().join("bundle");
        let environment = ExecutionEnvironmentPlan::default()
            .cwd(cwd)
            .stdin_file("bundle/database/seeds/latest.sql")
            .env("MYSQL_PWD", OsString::from("secret"));
        let runtime_policy = ExecutionRuntimePolicy::container("web", Some("db".to_owned()));

        let plans = [
            ExecutionSurface::DirectCli,
            ExecutionSurface::Bootstrap,
            ExecutionSurface::DataSeed,
            ExecutionSurface::Rhai,
        ]
        .into_iter()
        .map(|surface| {
            TaskExecutionRequestBuilder::new()
                .runtime_context(context.clone())
                .task("db:seed", vec!["--fresh".to_owned()])
                .surface(surface)
                .output_mode(ExecutionOutputMode::Capture)
                .runtime_policy(runtime_policy.clone())
                .environment(environment.clone())
                .resolve()
                .expect("plan")
        })
        .collect::<Vec<_>>();

        let expected_route = ExecutionRoute::Container {
            container: Some("web".to_owned()),
            service: Some("db".to_owned()),
        };
        for plan in &plans {
            assert_eq!(plan.route, expected_route);
            assert_eq!(
                plan.request.invocation,
                ExecutionIntent::Task {
                    selector: "db:seed".to_owned(),
                    args: vec!["--fresh".to_owned()],
                }
            );
            assert_eq!(plan.request.output_mode, ExecutionOutputMode::Capture);
            assert_eq!(plan.request.runtime_policy, runtime_policy);
            assert_eq!(plan.request.environment, environment);
            assert_eq!(plan.request.runtime_context, context);
        }
        assert_eq!(plans[0].request.surface, ExecutionSurface::DirectCli);
        assert_eq!(plans[1].request.surface, ExecutionSurface::Bootstrap);
        assert_eq!(plans[2].request.surface, ExecutionSurface::DataSeed);
        assert_eq!(plans[3].request.surface, ExecutionSurface::Rhai);
    }

    #[test]
    fn container_intent_in_handoff_routes_locally_without_losing_captured_paths() {
        let root = temp_repo("handoff");
        let nested = root.join("bundle/scripts");
        fs::create_dir_all(&nested).expect("mkdir nested");
        let root = root.canonicalize().expect("canonical root");
        let nested = nested.canonicalize().expect("canonical nested");
        let context = EffigyRuntimeContext::builder()
            .cwd_override(Some(nested.clone()))
            .captured_env(CapturedEnv {
                container_handoff: Some(OsString::from("1")),
                ..CapturedEnv::default()
            })
            .capture()
            .expect("runtime context");

        let plan = TaskExecutionRequestBuilder::new()
            .runtime_context(context)
            .command(vec!["mysql".to_owned()])
            .runtime_policy(ExecutionRuntimePolicy::container(
                "web",
                Some("db".to_owned()),
            ))
            .resolve()
            .expect("plan");

        assert_eq!(
            plan.route,
            ExecutionRoute::LocalContainerHandoff {
                service: Some("db".to_owned()),
            }
        );
        assert!(
            plan.request
                .runtime_context
                .container()
                .inside_container_handoff
        );
        assert_eq!(plan.request.runtime_context.invocation_cwd(), nested);
        assert_eq!(plan.request.runtime_context.command_root(), root);
        assert_eq!(
            plan.request.runtime_context.target().resolved_root,
            root,
            "execution plans must keep target repo authority from the captured context"
        );
    }
}
