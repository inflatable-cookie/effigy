use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::PathBuf;

use effigy_context::EffigyRuntimeContext;
use effigy_tasks::{CatalogSelectionMode, TaskRuntimeArgs, TaskSelector};

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

    pub fn into_dispatch_plan(self) -> Result<ExecutionDispatchPlan, ExecutionRequestError> {
        self.resolve().into_dispatch_plan()
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

impl ResolvedTaskExecutionPlan {
    pub fn into_dispatch_plan(self) -> Result<ExecutionDispatchPlan, ExecutionRequestError> {
        ExecutionDispatchPlan::from_resolved_task_plan(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionPreflightInput {
    pub selector: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub surface: ExecutionSurface,
}

impl ExecutionPreflightInput {
    pub fn new(
        selector: impl Into<String>,
        args: Vec<String>,
        cwd: PathBuf,
        surface: ExecutionSurface,
    ) -> Self {
        Self {
            selector: selector.into(),
            args,
            cwd,
            surface,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionRuntimeArgsPlan {
    pub raw_args: Vec<String>,
    pub exec_args: Vec<String>,
    pub repo_override: Option<PathBuf>,
    pub verbose_root: bool,
    pub env_schema_override: Option<PathBuf>,
    pub output_json: bool,
}

impl ExecutionRuntimeArgsPlan {
    pub fn from_args(args: &[String]) -> Result<Self, ExecutionRequestError> {
        let raw = effigy_tasks::parse_task_runtime_args(args)
            .map_err(ExecutionRequestError::InvalidRuntimeArgs)?;
        let (exec_args, output_json) = strip_task_json_flag(&raw.passthrough);

        Ok(Self {
            raw_args: raw.passthrough,
            exec_args,
            repo_override: raw.repo_override,
            verbose_root: raw.verbose_root,
            env_schema_override: raw.env_schema_override,
            output_json,
        })
    }

    pub fn raw_task_runtime_args(&self) -> TaskRuntimeArgs {
        TaskRuntimeArgs {
            repo_override: self.repo_override.clone(),
            verbose_root: self.verbose_root,
            env_schema_override: self.env_schema_override.clone(),
            passthrough: self.raw_args.clone(),
        }
    }

    pub fn exec_task_runtime_args(&self) -> TaskRuntimeArgs {
        TaskRuntimeArgs {
            repo_override: self.repo_override.clone(),
            verbose_root: self.verbose_root,
            env_schema_override: self.env_schema_override.clone(),
            passthrough: self.exec_args.clone(),
        }
    }
}

fn strip_task_json_flag(args: &[String]) -> (Vec<String>, bool) {
    let mut stripped = Vec::with_capacity(args.len());
    let mut json_mode = false;
    let mut passthrough_mode = false;
    for arg in args {
        if arg == "--" {
            passthrough_mode = true;
            stripped.push(arg.clone());
            continue;
        }
        if !passthrough_mode && arg == "--json" {
            json_mode = true;
            continue;
        }
        stripped.push(arg.clone());
    }
    (stripped, json_mode)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionPreflightPlan {
    pub input: ExecutionPreflightInput,
    pub runtime_args: Option<ExecutionRuntimeArgsPlan>,
    pub diagnostics: Vec<ExecutionPlanDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionDiscoveryInput {
    pub selector: String,
    pub cwd: PathBuf,
    pub repo_override: Option<PathBuf>,
}

impl ExecutionDiscoveryInput {
    pub fn new(selector: impl Into<String>, cwd: PathBuf, repo_override: Option<PathBuf>) -> Self {
        Self {
            selector: selector.into(),
            cwd,
            repo_override,
        }
    }

    pub fn resolve(
        self,
        invocation_cwd: PathBuf,
        resolved_root: PathBuf,
    ) -> Result<ExecutionDiscoveryPlan, ExecutionRequestError> {
        let selector = effigy_tasks::parse_task_selector(&self.selector)
            .map_err(ExecutionRequestError::InvalidTaskSelector)?;
        Ok(ExecutionDiscoveryPlan {
            invocation_cwd,
            resolved_root,
            selector,
            repo_override: self.repo_override,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionDiscoveryPlan {
    pub invocation_cwd: PathBuf,
    pub resolved_root: PathBuf,
    pub selector: TaskSelector,
    pub repo_override: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionSelectionInput {
    pub selector: TaskSelector,
    pub invocation_cwd: PathBuf,
    pub resolved_root: PathBuf,
}

impl ExecutionSelectionInput {
    pub fn from_discovery(plan: &ExecutionDiscoveryPlan) -> Self {
        Self {
            selector: plan.selector.clone(),
            invocation_cwd: plan.invocation_cwd.clone(),
            resolved_root: plan.resolved_root.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionSelectionCatalogSummary {
    pub alias: String,
    pub catalog_root: PathBuf,
    pub manifest_path: PathBuf,
    pub depth: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionSelectionPlan {
    pub input: ExecutionSelectionInput,
    pub catalog: ExecutionSelectionCatalogSummary,
    pub mode: CatalogSelectionMode,
    pub evidence: Vec<String>,
    pub task_name: String,
}

impl ExecutionSelectionPlan {
    pub fn new(
        input: ExecutionSelectionInput,
        catalog: ExecutionSelectionCatalogSummary,
        mode: CatalogSelectionMode,
        evidence: Vec<String>,
        task_name: impl Into<String>,
    ) -> Self {
        Self {
            input,
            catalog,
            mode,
            evidence,
            task_name: task_name.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionBindingInput {
    pub selection: ExecutionSelectionPlan,
    pub runtime_surface: String,
}

impl ExecutionBindingInput {
    pub fn new(selection: ExecutionSelectionPlan, runtime_surface: impl Into<String>) -> Self {
        Self {
            selection,
            runtime_surface: runtime_surface.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionBindingKind {
    None,
    Host,
    NamedContainer,
    InlineContainer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionBindingPlan {
    pub input: ExecutionBindingInput,
    pub kind: ExecutionBindingKind,
    pub requested_container_name: Option<String>,
    pub inline_workspace: bool,
}

impl ExecutionBindingPlan {
    pub fn new(
        input: ExecutionBindingInput,
        kind: ExecutionBindingKind,
        requested_container_name: Option<String>,
        inline_workspace: bool,
    ) -> Self {
        Self {
            input,
            kind,
            requested_container_name,
            inline_workspace,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionDispatchInput {
    pub request: TaskExecutionRequest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionDispatchPlan {
    pub request: TaskExecutionRequest,
    pub route: ExecutionRoute,
    pub selector: String,
    pub args: Vec<String>,
    pub effective_cwd: PathBuf,
    pub env_keys: Vec<String>,
    pub output_mode: ExecutionOutputMode,
    pub surface: ExecutionSurface,
    pub diagnostics: Vec<ExecutionPlanDiagnostic>,
}

impl ExecutionDispatchPlan {
    pub fn from_request(request: TaskExecutionRequest) -> Result<Self, ExecutionRequestError> {
        request.into_dispatch_plan()
    }

    pub fn from_resolved_task_plan(
        plan: ResolvedTaskExecutionPlan,
    ) -> Result<Self, ExecutionRequestError> {
        let ExecutionIntent::Task { selector, args } = &plan.request.invocation else {
            return Err(ExecutionRequestError::NonTaskInvocation);
        };
        let effective_cwd = plan
            .request
            .environment
            .cwd
            .clone()
            .unwrap_or_else(|| plan.request.runtime_context.invocation_cwd().to_path_buf());
        let mut env_keys = plan
            .request
            .environment
            .env
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        env_keys.sort();

        Ok(Self {
            request: plan.request.clone(),
            route: plan.route,
            selector: selector.clone(),
            args: args.clone(),
            effective_cwd,
            env_keys,
            output_mode: plan.request.output_mode,
            surface: plan.request.surface.clone(),
            diagnostics: Vec::new(),
        })
    }

    pub fn preflight_input(&self) -> ExecutionPreflightInput {
        ExecutionPreflightInput::new(
            self.selector.clone(),
            self.args.clone(),
            self.effective_cwd.clone(),
            self.surface.clone(),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionPlanDiagnostic {
    pub code: String,
    pub message: String,
}

impl ExecutionPlanDiagnostic {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
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
    NonTaskInvocation,
    InvalidRuntimeArgs(String),
    InvalidTaskSelector(String),
}

impl std::fmt::Display for ExecutionRequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingRuntimeContext => write!(f, "missing runtime context"),
            Self::MissingInvocation => write!(f, "missing execution invocation"),
            Self::NonTaskInvocation => {
                write!(f, "execution request must contain a task invocation")
            }
            Self::InvalidRuntimeArgs(error) => write!(f, "{error}"),
            Self::InvalidTaskSelector(error) => write!(f, "{error}"),
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
        CatalogSelectionMode, ExecutionBindingInput, ExecutionBindingKind, ExecutionBindingPlan,
        ExecutionDiscoveryInput, ExecutionDispatchPlan, ExecutionEnvironmentPlan, ExecutionIntent,
        ExecutionOutputMode, ExecutionRoute, ExecutionRunTarget, ExecutionRuntimeArgsPlan,
        ExecutionRuntimePolicy, ExecutionSelectionCatalogSummary, ExecutionSelectionInput,
        ExecutionSelectionPlan, ExecutionSurface, TaskExecutionRequestBuilder,
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

    #[test]
    fn dispatch_plan_normalizes_task_request_without_exposing_env_values() {
        let context = context("dispatch");
        let cwd = context.invocation_cwd().join("workspace");
        let request = TaskExecutionRequestBuilder::new()
            .runtime_context(context)
            .task("db:migrate", vec!["--fresh".to_owned()])
            .surface(ExecutionSurface::Bootstrap)
            .output_mode(ExecutionOutputMode::Json)
            .environment(
                ExecutionEnvironmentPlan::default()
                    .cwd(cwd.clone())
                    .env("BETA", OsString::from("two"))
                    .env("ALPHA", OsString::from("one")),
            )
            .build()
            .expect("request");

        let plan = ExecutionDispatchPlan::from_request(request).expect("dispatch plan");

        assert_eq!(plan.selector, "db:migrate");
        assert_eq!(plan.args, vec!["--fresh"]);
        assert_eq!(plan.effective_cwd, cwd);
        assert_eq!(plan.env_keys, vec!["ALPHA", "BETA"]);
        assert_eq!(plan.output_mode, ExecutionOutputMode::Json);
        assert_eq!(plan.surface, ExecutionSurface::Bootstrap);
        assert_eq!(plan.route, ExecutionRoute::Host);
    }

    #[test]
    fn dispatch_plan_exposes_preflight_input() {
        let context = context("dispatch-preflight");
        let cwd = context.invocation_cwd().join("workspace");
        let request = TaskExecutionRequestBuilder::new()
            .runtime_context(context)
            .task("db:migrate", vec!["--json".to_owned()])
            .surface(ExecutionSurface::Rhai)
            .environment(ExecutionEnvironmentPlan::default().cwd(cwd.clone()))
            .build()
            .expect("request");

        let input = ExecutionDispatchPlan::from_request(request)
            .expect("dispatch plan")
            .preflight_input();

        assert_eq!(input.selector, "db:migrate");
        assert_eq!(input.args, vec!["--json"]);
        assert_eq!(input.cwd, cwd);
        assert_eq!(input.surface, ExecutionSurface::Rhai);
    }

    #[test]
    fn runtime_args_plan_preserves_raw_args_and_strips_execution_json() {
        let repo = PathBuf::from("/tmp/repo");
        let schema = PathBuf::from("/tmp/schema.env");
        let args = vec![
            "--repo".to_owned(),
            repo.display().to_string(),
            "--env-schema".to_owned(),
            schema.display().to_string(),
            "--verbose-root".to_owned(),
            "--json".to_owned(),
            "--".to_owned(),
            "--json".to_owned(),
        ];

        let plan = ExecutionRuntimeArgsPlan::from_args(&args).expect("runtime args");

        assert_eq!(plan.repo_override, Some(repo.clone()));
        assert_eq!(plan.env_schema_override, Some(schema.clone()));
        assert!(plan.verbose_root);
        assert!(plan.output_json);
        assert_eq!(
            plan.raw_args,
            vec!["--json".to_owned(), "--".to_owned(), "--json".to_owned()]
        );
        assert_eq!(plan.exec_args, vec!["--".to_owned(), "--json".to_owned()]);

        let raw = plan.raw_task_runtime_args();
        let exec = plan.exec_task_runtime_args();
        assert_eq!(raw.repo_override, Some(repo.clone()));
        assert_eq!(exec.repo_override, Some(repo));
        assert_eq!(raw.env_schema_override, Some(schema.clone()));
        assert_eq!(exec.env_schema_override, Some(schema));
        assert_eq!(
            raw.passthrough,
            vec!["--json".to_owned(), "--".to_owned(), "--json".to_owned()]
        );
        assert_eq!(exec.passthrough, vec!["--".to_owned(), "--json".to_owned()]);
    }

    #[test]
    fn discovery_input_builds_selector_plan_with_paths() {
        let cwd = PathBuf::from("/tmp/repo/nested");
        let root = PathBuf::from("/tmp/repo");
        let repo_override = Some(root.clone());

        let plan = ExecutionDiscoveryInput::new("api/test", cwd.clone(), repo_override.clone())
            .resolve(cwd.clone(), root.clone())
            .expect("discovery plan");

        assert_eq!(plan.invocation_cwd, cwd);
        assert_eq!(plan.resolved_root, root);
        assert_eq!(plan.repo_override, repo_override);
        assert_eq!(plan.selector.prefix.as_deref(), Some("api"));
        assert_eq!(plan.selector.task_name, "test");
    }

    #[test]
    fn discovery_input_rejects_invalid_selector() {
        let error = ExecutionDiscoveryInput::new("api/", PathBuf::from("/tmp/repo"), None)
            .resolve(PathBuf::from("/tmp/repo"), PathBuf::from("/tmp/repo"))
            .expect_err("invalid selector");

        assert_eq!(
            error.to_string(),
            "task name must be `<task>` or `<catalog>/<task>`"
        );
    }

    #[test]
    fn selection_input_and_plan_summarize_selected_task() {
        let discovery =
            ExecutionDiscoveryInput::new("api/test", PathBuf::from("/tmp/repo/api"), None)
                .resolve(PathBuf::from("/tmp/repo/api"), PathBuf::from("/tmp/repo"))
                .expect("discovery");
        let input = ExecutionSelectionInput::from_discovery(&discovery);
        let plan = ExecutionSelectionPlan::new(
            input,
            ExecutionSelectionCatalogSummary {
                alias: "api".to_owned(),
                catalog_root: PathBuf::from("/tmp/repo/api"),
                manifest_path: PathBuf::from("/tmp/repo/api/effigy.toml"),
                depth: 1,
            },
            CatalogSelectionMode::ExplicitPrefix,
            vec!["selected catalog `api` by explicit prefix".to_owned()],
            "test",
        );

        assert_eq!(plan.input.selector.prefix.as_deref(), Some("api"));
        assert_eq!(plan.input.selector.task_name, "test");
        assert_eq!(plan.input.invocation_cwd, PathBuf::from("/tmp/repo/api"));
        assert_eq!(plan.input.resolved_root, PathBuf::from("/tmp/repo"));
        assert_eq!(plan.catalog.alias, "api");
        assert_eq!(plan.catalog.depth, 1);
        assert_eq!(plan.mode, CatalogSelectionMode::ExplicitPrefix);
        assert_eq!(
            plan.evidence,
            vec!["selected catalog `api` by explicit prefix".to_owned()]
        );
        assert_eq!(plan.task_name, "test");
    }

    #[test]
    fn binding_plan_summarizes_binding_resolution_without_task_model() {
        let discovery =
            ExecutionDiscoveryInput::new("api/test", PathBuf::from("/tmp/repo/api"), None)
                .resolve(PathBuf::from("/tmp/repo/api"), PathBuf::from("/tmp/repo"))
                .expect("discovery");
        let selection = ExecutionSelectionPlan::new(
            ExecutionSelectionInput::from_discovery(&discovery),
            ExecutionSelectionCatalogSummary {
                alias: "api".to_owned(),
                catalog_root: PathBuf::from("/tmp/repo/api"),
                manifest_path: PathBuf::from("/tmp/repo/api/effigy.toml"),
                depth: 1,
            },
            CatalogSelectionMode::CwdNearest,
            vec!["selected nearest in-scope catalog `api`".to_owned()],
            "test",
        );

        let plan = ExecutionBindingPlan::new(
            ExecutionBindingInput::new(selection, "standard task execution"),
            ExecutionBindingKind::NamedContainer,
            Some("web".to_owned()),
            false,
        );

        assert_eq!(plan.input.runtime_surface, "standard task execution");
        assert_eq!(plan.input.selection.task_name, "test");
        assert_eq!(plan.input.selection.catalog.alias, "api");
        assert_eq!(plan.kind, ExecutionBindingKind::NamedContainer);
        assert_eq!(plan.requested_container_name.as_deref(), Some("web"));
        assert!(!plan.inline_workspace);
    }

    #[test]
    fn dispatch_plan_is_equivalent_for_embedded_task_surfaces() {
        let surfaces = [
            ExecutionSurface::DirectCli,
            ExecutionSurface::Bootstrap,
            ExecutionSurface::Rhai,
            ExecutionSurface::RunArray,
        ];
        let context = context("dispatch-surfaces");
        let cwd = context.invocation_cwd().join("repo");

        let plans = surfaces
            .into_iter()
            .map(|surface| {
                let request = TaskExecutionRequestBuilder::new()
                    .runtime_context(context.clone())
                    .task("db:seed", vec!["--latest".to_owned()])
                    .surface(surface)
                    .environment(ExecutionEnvironmentPlan::default().cwd(cwd.clone()))
                    .build()
                    .expect("request");

                ExecutionDispatchPlan::from_request(request).expect("dispatch plan")
            })
            .collect::<Vec<_>>();

        for plan in &plans {
            assert_eq!(plan.selector, "db:seed");
            assert_eq!(plan.args, vec!["--latest"]);
            assert_eq!(plan.route, ExecutionRoute::Host);
            assert_eq!(plan.effective_cwd, cwd);
        }

        assert_eq!(plans[0].surface, ExecutionSurface::DirectCli);
        assert_eq!(plans[1].surface, ExecutionSurface::Bootstrap);
        assert_eq!(plans[2].surface, ExecutionSurface::Rhai);
        assert_eq!(plans[3].surface, ExecutionSurface::RunArray);
    }

    #[test]
    fn command_request_is_not_a_task_dispatch_plan() {
        let request = TaskExecutionRequestBuilder::new()
            .runtime_context(context("command-dispatch"))
            .command(vec!["echo".to_owned(), "ok".to_owned()])
            .build()
            .expect("request");

        let error = ExecutionDispatchPlan::from_request(request).expect_err("should fail");

        assert_eq!(
            error.to_string(),
            "execution request must contain a task invocation"
        );
    }
}
