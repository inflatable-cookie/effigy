//! Parsed command line for Effigy.
//!
//! The stable entrypoint for tools and tests is [`parse_command`], which returns
//! a [`Command`] value tree. Human-readable `--help` panels live under
//! [`help`] ([`HelpTopic`] selects the panel). Global `--json` is threaded by
//! the runner via [`strip_global_json_flags`] and [`apply_global_json_flag`].
//!
//! End-user documentation: repository `docs/guides/025-command-reference-matrix.md`.

use std::path::PathBuf;

pub mod builtin_names;
mod command_parsing;
pub mod command_surface;
mod global_json;
pub mod header;
pub mod help;
pub mod usage_hint;
mod value_parsing;

pub use command_surface::HelpGroup;
pub use global_json::{runtime_flag_present_before_passthrough, GlobalCliOptions};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Version,
    Bundle(BundleArgs),
    Changelog(ChangelogArgs),
    Fmt(FmtArgs),
    Deploy(DeployArgs),
    Deps(DepsArgs),
    Secrets(SecretsArgs),
    Defer(DeferArgs),
    Exec(ExecArgs),
    State(StateArgs),
    Workspace(WorkspaceArgs),
    Gateway(GatewayArgs),
    Service(ServiceArgs),
    Rhai(RhaiArgs),
    Skill(SkillArgs),
    Docs(DocsArgs),
    Contracts(ContractsArgs),
    Container(ContainerArgs),
    Bootstrap(BootstrapArgs),
    Uninstall(UninstallArgs),
    Release(ReleaseArgs),
    Doctor(DoctorArgs),
    Tasks(TasksArgs),
    /// `effigy runs` and `effigy run status|logs|stop <id>`.
    Runs(RunsArgs),
    Task(TaskInvocation),
    #[doc(hidden)]
    InternalScriptRun(InternalScriptRunArgs),
    #[doc(hidden)]
    InternalGateway(InternalGatewayArgs),
    #[doc(hidden)]
    InternalGatewayIdentity(InternalGatewayIdentityArgs),
    #[doc(hidden)]
    InternalGuestProbe(GuestProbeArgs),
    #[doc(hidden)]
    InternalContainerLeaseReaper(InternalContainerLeaseReaperArgs),
    Help(HelpTopic),
    /// Discovery-only help for one operator-job group. Grouping never adds an
    /// execution route: see contract `043`.
    HelpGroup(HelpGroup),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuestProbeArgs {
    Identity,
    Resolve,
    ContextDial {
        context: String,
        digest: String,
        timeout_ms: u64,
    },
    Dial {
        endpoint: String,
        pin: String,
        timeout_ms: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[doc(hidden)]
pub struct InternalScriptRunArgs {
    pub file: PathBuf,
    pub repo_root: Option<PathBuf>,
    pub task_name: Option<String>,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[doc(hidden)]
pub struct InternalContainerLeaseReaperArgs {
    pub repo_root: PathBuf,
    pub container_name: String,
    pub token: String,
}

/// Which `effigy <topic> --help` panel to render.
///
/// `effigy help <command>` reuses these same panels, so both spellings render
/// identical facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpTopic {
    General,
    Bundle,
    Changelog,
    Deploy,
    Deps,
    Secrets,
    Defer,
    Exec,
    State,
    Workspace,
    Gateway,
    Service,
    Rhai,
    Skill,
    Docs,
    Contracts,
    Container,
    Bootstrap,
    Release,
    Doctor,
    Tasks,
    Test,
    Init,
    Uninstall,
    Runs,
    Fmt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangelogArgs {
    pub subcommand: ChangelogSubcommand,
    pub file: Option<PathBuf>,
    pub repo_override: Option<PathBuf>,
    pub output_json: bool,
}

/// `effigy fmt ...`: format an explicit, declared file set.
///
/// The route exists because `cargo fmt --manifest-path` reads a workspace and
/// can trail path-linked consumers into unrelated crates. An explicit file set
/// plus rustfmt's `skip_children` keeps a format pass inside the named files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FmtArgs {
    pub subcommand: FmtSubcommand,
    pub repo_override: Option<PathBuf>,
    pub output_json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeployArgs {
    /// Selected deployment command.
    pub subcommand: DeploySubcommand,
    /// Optional repository root used to resolve manifest config and reports.
    pub repo_override: Option<PathBuf>,
    /// Emit the command result as a versioned JSON payload.
    pub output_json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DepsArgs {
    pub subcommand: DepsSubcommand,
    pub repo_override: Option<PathBuf>,
    pub output_json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DepsSubcommand {
    Status {
        manager: Option<DepsManager>,
    },
    Link {
        manager: DepsManager,
        library_path: PathBuf,
        dry_run: bool,
    },
    Unlink {
        manager: DepsManager,
        library_path: PathBuf,
        dry_run: bool,
    },
    Pin {
        manager: DepsManager,
        library_path: PathBuf,
        dry_run: bool,
    },
    Unpin {
        manager: DepsManager,
        library_path: PathBuf,
        dry_run: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepsManager {
    Cargo,
    Bun,
}

impl DepsManager {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cargo => "cargo",
            Self::Bun => "bun",
        }
    }
}

/// Parsed `effigy deploy` subcommands.
///
/// `Model` and `Export` describe a named `[deploy.<env>]` environment and its
/// deployment model. The transaction variants (`Plan`, `Apply`, `Status`,
/// `History`, and `Redeploy`) operate on the same named environments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeploySubcommand {
    /// Derive the provider-neutral deployment model for an environment.
    Model {
        /// Name from `[deploy.<env>]`; omitted projects may default.
        env: Option<String>,
    },
    /// Export provider files without live provider mutation.
    Export {
        /// Provider package id from `[deploy.providers.<id>]`.
        provider: String,
        /// Name from `[deploy.<env>]`; omitted projects may default.
        env: Option<String>,
        /// Output directory for generated files.
        path: PathBuf,
        /// Preview generated paths without writing files.
        plan: bool,
    },
    /// Resolve a deployment transaction and optionally persist its report.
    Plan {
        /// Name from `[deploy.<env>]`.
        env: String,
        /// Persist the plan under `.effigy/reports/deploy/<env>/`.
        write_report: bool,
    },
    /// Apply a deployment transaction after explicit confirmation.
    Apply {
        /// Name from `[deploy.<env>]`.
        env: String,
        /// Required confirmation flag for mutation-capable execution.
        yes: bool,
    },
    /// Inspect active/latest deployment state for an environment.
    Status {
        /// Name from `[deploy.<env>]`.
        env: String,
    },
    /// List persisted deployment reports for an environment.
    History {
        /// Name from `[deploy.<env>]`.
        env: String,
        /// Optional maximum number of history rows to return.
        limit: Option<usize>,
    },
    /// Replay a recorded deployment transaction when inputs remain reproducible.
    Redeploy {
        /// Name from `[deploy.<env>]`.
        env: String,
        /// Recorded deployment id from deploy history.
        deployment: String,
        /// Required confirmation flag for replay execution.
        yes: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretsArgs {
    pub subcommand: SecretsSubcommand,
    pub repo_override: Option<PathBuf>,
    pub output_json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretsSubcommand {
    List,
    Doctor,
    Init,
    Unlock,
    Lock,
    Import {
        input: PathBuf,
    },
    Set {
        name: String,
    },
    Get {
        name: String,
    },
    Unset {
        name: String,
    },
    ChangePassphrase,
    Export {
        format: SecretsExportFormat,
        output: PathBuf,
        yes: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretsExportFormat {
    Env,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocsArgs {
    pub subcommand: DocsSubcommand,
    pub repo_override: Option<PathBuf>,
    pub output_json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RhaiArgs {
    pub subcommand: RhaiSubcommand,
    pub output_json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RhaiSubcommand {
    Surface,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateArgs {
    pub subcommand: StateSubcommand,
    pub repo_override: Option<PathBuf>,
    pub output_json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateSubcommand {
    Plan {
        manifest: Option<PathBuf>,
        stack: Option<String>,
        write_report: bool,
    },
    Apply {
        manifest: Option<PathBuf>,
        stack: Option<String>,
        yes: bool,
        skip_layers: Vec<String>,
    },
    Capture {
        manifest: Option<PathBuf>,
        stack: Option<String>,
        profile: Option<String>,
        role: Option<String>,
        source_env: Option<String>,
        key: Option<String>,
        source: Option<String>,
        destination_ref: Option<String>,
        hook: Option<String>,
        task: Option<String>,
        yes: bool,
        push: bool,
    },
    CaptureSet {
        stack: String,
        profiles: Vec<String>,
        key: Option<String>,
        yes: bool,
        push: bool,
    },
    History {
        stack: String,
        kind: Option<String>,
        limit: Option<usize>,
        lineage: Option<String>,
    },
    ArtifactInspect {
        source: String,
        farmyard_handoff: bool,
    },
    ArtifactStage {
        source: String,
        farmyard_handoff: bool,
    },
    ArtifactCapture {
        source: String,
        destination: String,
        kind: Option<String>,
        environment_label: Option<String>,
        farmyard_handoff: bool,
        push: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocsSubcommand {
    /// Resolve the bounded three-leaf host recipe; this is not an execution grant.
    Check {
        kind: DocsCheckKind,
        paths: Vec<PathBuf>,
        file: Option<PathBuf>,
        section: Option<String>,
        min_blocks: Option<usize>,
        required_text: Vec<String>,
        required_blocks: Vec<DocsBlockRequirement>,
        required_headings: Vec<String>,
        forbidden_text: Vec<String>,
        policy_index: Box<Option<String>>,
        dir: Box<Option<PathBuf>>,
        index: Box<Option<PathBuf>>,
        policy_name: Box<Option<String>>,
    },
    AddLogIndex {
        log_path: PathBuf,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocsCheckKind {
    Links,
    JsonExamples,
    Headings,
    Paths,
    Contains,
    Forbidden,
    Index,
    NextAction,
    WorkflowPaths,
}

impl DocsCheckKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Links => "links",
            Self::JsonExamples => "json-examples",
            Self::Headings => "headings",
            Self::Paths => "paths",
            Self::Contains => "contains",
            Self::Forbidden => "forbidden",
            Self::Index => "index",
            Self::NextAction => "next-action",
            Self::WorkflowPaths => "workflow-paths",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocsBlockRequirement {
    pub block_index: usize,
    pub needle: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractsArgs {
    pub subcommand: ContractsSubcommand,
    pub repo_override: Option<PathBuf>,
    pub output_json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecArgs {
    pub repo_override: Option<PathBuf>,
    pub output_json: bool,
    pub service: Option<String>,
    pub command: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeferArgs {
    pub task: TaskInvocation,
    pub repo_override: Option<PathBuf>,
    pub output_json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleArgs {
    pub subcommand: BundleSubcommand,
    pub repo_override: Option<PathBuf>,
    pub output_json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayArgs {
    pub subcommand: GatewaySubcommand,
    pub private: Option<GatewayPrivateArgs>,
    pub output_json: bool,
}

/// Explicit fixture-owned gateway state and optional loopback bind overrides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayPrivateArgs {
    pub state_root: PathBuf,
    pub dns_addr: Option<std::net::SocketAddr>,
    pub proxy_addr: Option<std::net::SocketAddr>,
    pub https_addr: Option<std::net::SocketAddr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceArgs {
    pub subcommand: ServiceSubcommand,
    pub repo_override: Option<PathBuf>,
    pub output_json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerArgs {
    pub subcommand: ContainerSubcommand,
    pub repo_override: Option<PathBuf>,
    pub output_json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceArgs {
    pub workspace: Option<String>,
    pub system: Option<String>,
    pub repo_override: Option<PathBuf>,
    pub output_json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapArgs {
    /// The git URL to acquire. `None` uses the existing checkout.
    pub repo_url: Option<String>,
    pub path: Option<PathBuf>,
    pub branch: Option<String>,
    pub no_start: bool,
    pub plan: bool,
    pub output_json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UninstallArgs {
    pub plan: bool,
    pub yes: bool,
    pub output_json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapDbSeedInput {
    pub target: Option<String>,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerDbDumpInput {
    pub target: Option<String>,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceSubcommand {
    List,
    Extract {
        service: String,
        dir: Option<PathBuf>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BundleSubcommand {
    Inspect,
    Sync,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatewaySubcommand {
    Setup { suffixes: Vec<String>, remove: bool },
    Up,
    Down,
    Status,
    SetupTls,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[doc(hidden)]
pub struct InternalGatewayArgs {
    pub private: Option<GatewayPrivateArgs>,
    pub listener_handoff: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[doc(hidden)]
pub struct InternalGatewayIdentityArgs {
    pub digest: String,
    pub target_digest: String,
    pub owner_uid: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerSubcommand {
    Up {
        name: Option<String>,
        /// Select the container through `[systems.<NAME>]`'s default
        /// workspace instead of naming it directly.
        system: Option<String>,
        attach: bool,
        detach: bool,
    },
    Down {
        name: Option<String>,
        system: Option<String>,
        global: bool,
    },
    Status {
        name: Option<String>,
        system: Option<String>,
        global: bool,
    },
    Stats {
        global: bool,
    },
    Logs {
        name: Option<String>,
        system: Option<String>,
        service: Option<String>,
        follow: bool,
    },
    /// Restart only the named environment-owned host processes, and the
    /// processes that depend on them.
    Restart {
        name: Option<String>,
        processes: Vec<String>,
    },
    Shell {
        name: Option<String>,
        service: Option<String>,
        command: Option<String>,
        bounded: Option<BoundedExecLimits>,
    },
    Reset {
        name: Option<String>,
        keep_data: bool,
        wipe_data: bool,
        yes: bool,
    },
    Cache {
        name: Option<String>,
        subcommand: ContainerCacheSubcommand,
    },
    Volume {
        subcommand: ContainerVolumeSubcommand,
    },
    Profile {
        subcommand: ContainerProfileSubcommand,
    },
    Data {
        name: Option<String>,
        subcommand: ContainerDataSubcommand,
    },
    Render {
        name: Option<String>,
    },
    Migrate {
        name: Option<String>,
    },
    Scope,
    Hosts {
        name: Option<String>,
    },
    Retire {
        name: Option<String>,
        scope: Option<String>,
        /// Retire every labelled stack whose checkout no longer exists.
        dead: bool,
        /// Keep named data volumes and list them.
        keep_data: bool,
        yes: bool,
    },
}

/// Strict caller-supplied limits for the opt-in bounded finite named
/// container-command surface (`effigy container <NAME> shell --bounded`).
///
/// `deadline_ms` is one absolute deadline for the whole operation. The two
/// stream limits are cumulative hard retention budgets in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoundedExecLimits {
    pub deadline_ms: u64,
    pub stdout_limit_bytes: u64,
    pub stderr_limit_bytes: u64,
}

/// Longest accepted bounded-command deadline (one hour).
pub const BOUNDED_EXEC_MAX_DEADLINE_MS: u64 = 60 * 60 * 1000;
/// Largest accepted per-stream retention budget (64 MiB).
pub const BOUNDED_EXEC_MAX_STREAM_BYTES: u64 = 64 * 1024 * 1024;

impl BoundedExecLimits {
    /// Validate every bound before any execution. Missing, zero, and
    /// out-of-range bounds are refused; there is no partial-bound execution.
    pub fn validate(&self) -> Result<(), String> {
        if self.deadline_ms == 0 || self.deadline_ms > BOUNDED_EXEC_MAX_DEADLINE_MS {
            return Err(format!(
                "`--deadline-ms` must be between 1 and {BOUNDED_EXEC_MAX_DEADLINE_MS}"
            ));
        }
        if self.stdout_limit_bytes == 0 || self.stdout_limit_bytes > BOUNDED_EXEC_MAX_STREAM_BYTES {
            return Err(format!(
                "`--stdout-limit-bytes` must be between 1 and {BOUNDED_EXEC_MAX_STREAM_BYTES}"
            ));
        }
        if self.stderr_limit_bytes == 0 || self.stderr_limit_bytes > BOUNDED_EXEC_MAX_STREAM_BYTES {
            return Err(format!(
                "`--stderr-limit-bytes` must be between 1 and {BOUNDED_EXEC_MAX_STREAM_BYTES}"
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerProfileSubcommand {
    /// Recover the Colima runtime in place, as `effigy system repair` did.
    Repair {
        system: Option<String>,
    },
    /// Stop the Colima runtime without deleting it, as
    /// `effigy system reset-runtime` did.
    ResetRuntime {
        system: Option<String>,
    },
    Status {
        profile: Option<String>,
    },
    Resize {
        profile: Option<String>,
    },
    Purge {
        profile: Option<String>,
        yes: bool,
    },
    Recreate {
        profile: Option<String>,
        disk_gib: Option<u64>,
        yes: bool,
    },
    /// Delete only the stale nerdctl health-check pid files the VM's `/run`
    /// accumulated, and report the counts around it (effigy#291).
    PruneHealthPidFiles {
        profile: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerCacheSubcommand {
    List {
        global: bool,
        project: Option<String>,
        kind: Option<String>,
    },
    Prune {
        global: bool,
        yes: bool,
        project: Option<String>,
        kind: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerVolumeSubcommand {
    List {
        global: bool,
        orphans: bool,
        dormant: bool,
    },
    Prune {
        global: bool,
        yes: bool,
        orphans: bool,
        dormant: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerDataSubcommand {
    List,
    Export {
        volume: String,
        path: PathBuf,
    },
    Dump {
        db_dumps: Vec<ContainerDbDumpInput>,
        push: bool,
    },
    Import {
        volume: String,
        path: PathBuf,
        yes: bool,
    },
    PullProduction {
        yes: bool,
    },
    Seed {
        db_seeds: Vec<BootstrapDbSeedInput>,
        no_prompt: bool,
        yes: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractsSubcommand {
    CheckJson {
        index_path: Option<PathBuf>,
        contract_path: Option<PathBuf>,
        artifact_path: Option<PathBuf>,
        changed_only_base: Option<String>,
        print_selected: ContractsSelectionPrintMode,
        validate_selection: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractsSelectionPrintMode {
    None,
    Text,
    Json,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangelogSubcommand {
    Validate,
    Format { write: bool },
    Analyze,
    Extract { version: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FmtSubcommand {
    /// Format the named Rust files in place.
    Rust { files: Vec<PathBuf> },
    /// Report the named Rust files that are not formatted, writing nothing.
    RustCheck { files: Vec<PathBuf> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseArgs {
    pub subcommand: ReleaseSubcommand,
    pub repo_override: Option<PathBuf>,
    pub output_json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseSubcommand {
    Status {
        check_gates: bool,
    },
    Gates,
    /// `effigy release --plan`: the whole pipeline, no mutation (contract 054
    /// §5). Replaces the retired `simulate`.
    Plan {
        version_override: Option<String>,
    },
    /// `effigy release run [--to <stage>]`: one durable Run (054 §3, §5).
    Run {
        to: Option<String>,
    },
    /// `effigy release resume <run>`: continue from the last checkpoint.
    ResumeRun {
        run: String,
    },
    /// Settle an interrupted invocation without advancing stages.
    Settle {
        run: String,
    },
    /// `effigy release approve <run>`: pass an approval checkpoint.
    Approve {
        run: String,
    },
    /// `effigy release decide <run> --published|--not-published`: record an
    /// operator's ruling on an unknown publish outcome (contract 054 §7).
    Decide {
        run: String,
        published: bool,
    },
    /// `effigy release receipt <run>`: print the Run's receipt.
    Receipt {
        run: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorArgs {
    pub repo_override: Option<PathBuf>,
    pub output_json: bool,
    pub fix: bool,
    pub verbose: bool,
    pub deep: bool,
    pub catalog: Option<String>,
    pub all_catalogs: bool,
    pub refresh: bool,
    pub explain: Option<TaskInvocation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TasksArgs {
    /// Include internal tasks in text discovery. JSON always includes them.
    pub all: bool,
    pub repo_override: Option<PathBuf>,
    pub task_name: Option<String>,
    pub resolve_selector: Option<String>,
    pub status_selector: Option<String>,
    pub status_all: bool,
    pub output_json: bool,
    pub pretty_json: bool,
    /// Explicit bounded-QA-group subcommand (`tasks qa-groups` /
    /// `tasks qa-group`). Recognized only as the exact leading route so group
    /// verbs never take over task names or top-level selectors.
    pub qa: Option<TasksQaCommand>,
}

impl Default for TasksArgs {
    fn default() -> Self {
        Self {
            all: false,
            repo_override: None,
            task_name: None,
            resolve_selector: None,
            status_selector: None,
            status_all: false,
            output_json: false,
            pretty_json: true,
            qa: None,
        }
    }
}

/// Run control over the kernel run journal (Q-006): every run, heavy or not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunsArgs {
    pub repo_override: Option<PathBuf>,
    pub output_json: bool,
    pub action: RunsAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunsAction {
    /// `effigy runs [--limit N] [--all]`: this checkout's runs, or every
    /// run on the host with `--all`.
    List { limit: usize, all: bool },
    /// `effigy run status <run>`: a run id or a `--run-id` key.
    Status { run: String },
    /// `effigy run logs <run> [--follow] [--stdout-offset N] [--stderr-offset N]`.
    Logs {
        run: String,
        follow: bool,
        stdout_offset: u64,
        stderr_offset: u64,
    },
    /// `effigy run stop <run> [--wait-ms N]`: the kernel stop path.
    Stop { run: String, wait_ms: Option<u64> },
}

/// Bounded QA-group surfaces under `effigy tasks` (contract 051).
///
/// The multiword `tasks qa-groups` / `tasks qa-group` forms reserve nothing
/// in the top-level selector space, and no `effigy qa-groups` bare command
/// exists. `GroupStop` parses so the runner can reject it before any side
/// effect with the precise supervision prerequisite named.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TasksQaCommand {
    /// `tasks qa-groups list [FILTER] [--file PATH] [--json]`
    GroupsList {
        filter: Option<String>,
        file: Option<PathBuf>,
        output_json: bool,
        pretty_json: bool,
    },
    /// `tasks qa-group run <SELECTOR> [--file PATH] [--scope TOKEN]... [--plan] [--json]`
    GroupRun {
        selector: String,
        file: Option<PathBuf>,
        scopes: Vec<String>,
        plan: bool,
        output_json: bool,
    },
    /// `tasks qa-group status <RUN_ID> [--json]`
    GroupStatus { run_id: String, output_json: bool },
    /// `tasks qa-group logs <RUN_ID> [--follow]`
    GroupLogs { run_id: String, follow: bool },
    /// `tasks qa-group stop <RUN_ID> [--json]` — always rejected before side
    /// effects until run-scoped stop/signal attribution lands.
    GroupStop { run_id: String, output_json: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillArgs {
    pub subcommand: SkillSubcommand,
    pub output_json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillSubcommand {
    Tasks {
        path: PathBuf,
    },
    Run {
        /// Explicit `--path` source override. When absent, the qualified
        /// selector's first segment selects an installed agent skill by name.
        path: Option<PathBuf>,
        stdio: SkillStdioMode,
        task: TaskInvocation,
        repo_override: Option<PathBuf>,
    },
}

/// Transport selection for `effigy skill run`.
///
/// `Default` keeps Effigy's normal resolution report and JSON envelope.
/// `Passthrough` hands raw stdin/stdout/stderr and the exit status to the
/// selected task, so Effigy adds no rendered output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SkillStdioMode {
    #[default]
    Default,
    Passthrough,
}

impl SkillStdioMode {
    pub fn is_passthrough(self) -> bool {
        matches!(self, Self::Passthrough)
    }
}

impl SkillArgs {
    pub fn stdio_passthrough(&self) -> bool {
        matches!(
            &self.subcommand,
            SkillSubcommand::Run {
                stdio: SkillStdioMode::Passthrough,
                ..
            }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskInvocation {
    pub name: String,
    pub args: Vec<String>,
}

/// Internal argument used to carry a parsed global `--run-id` key to the
/// selector runner without exposing it to the selected task's own arguments.
#[doc(hidden)]
pub const INTERNAL_RUN_ID_ARG: &str = "--__effigy-run-id";

/// The refusal for the retired `--host-run-request-id` flag.
pub const RETIRED_HOST_RUN_REQUEST_ID: &str = "`--host-run-request-id` was retired with the host-run client (Q-006). Use `effigy --run-id <key> <task>`: a repeat with the same key attaches to that run and never starts a second; inspect it with `effigy run status <key>`";

/// The refusal for the retired `effigy tasks request status|follow`.
pub const RETIRED_TASKS_REQUEST: &str = "`effigy tasks request status|follow` was retired with the host-run client (Q-006). Start a run with `effigy --run-id <key> <task>`, then use `effigy run status <key>`, `effigy run logs <key> --follow` or `effigy run stop <key>`";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliParseError {
    MissingRepoValue,
    MissingTaskNameValue,
    MissingResolveSelectorValue,
    MissingStatusSelectorValue,
    MissingPrettyValue,
    MissingQaGroupAction,
    MissingQaGroupFilePath,
    MissingQaGroupSelector,
    MissingQaGroupRunId,
    MissingScopeTokenValue,
    MissingFlagValue {
        flag: String,
    },
    InvalidPrettyValue(String),
    InvalidFlagValue {
        flag: String,
        value: String,
        expected: String,
    },
    InvalidArguments(String),
    UnknownArgument(String),
}

impl std::fmt::Display for CliParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CliParseError::MissingRepoValue => write!(f, "--repo requires a value"),
            CliParseError::MissingTaskNameValue => write!(f, "--task requires a value"),
            CliParseError::MissingResolveSelectorValue => write!(f, "--resolve requires a value"),
            CliParseError::MissingStatusSelectorValue => {
                write!(f, "`tasks status` requires a selector")
            }
            CliParseError::MissingQaGroupAction => {
                write!(
                    f,
                    "`tasks qa-group` requires an action (run, status, logs, or stop)"
                )
            }
            CliParseError::MissingQaGroupFilePath => {
                write!(f, "--file requires a path")
            }
            CliParseError::MissingQaGroupSelector => {
                write!(f, "`tasks qa-group run` requires a group selector")
            }
            CliParseError::MissingQaGroupRunId => {
                write!(f, "this qa-group command requires a run ID")
            }
            CliParseError::MissingScopeTokenValue => {
                write!(
                    f,
                    "--scope requires a typed token (for example path:crates/x/src/lib.rs)"
                )
            }
            CliParseError::MissingPrettyValue => {
                write!(f, "--pretty requires a value (`true` or `false`)")
            }
            CliParseError::MissingFlagValue { flag } => write!(f, "{flag} requires a value"),
            CliParseError::InvalidPrettyValue(value) => write!(
                f,
                "--pretty value `{value}` is invalid (expected `true` or `false`)"
            ),
            CliParseError::InvalidFlagValue {
                flag,
                value,
                expected,
            } => write!(f, "{flag} value `{value}` is invalid (expected {expected})"),
            CliParseError::InvalidArguments(message) => write!(f, "{message}"),
            CliParseError::UnknownArgument(arg) => write!(f, "unknown argument: {arg}"),
        }
    }
}

impl std::error::Error for CliParseError {}

pub fn strip_global_json_flags(args: Vec<String>) -> (Vec<String>, bool) {
    global_json::strip_global_json_flags(args)
}

pub fn strip_global_json_flag(args: Vec<String>) -> (Vec<String>, bool) {
    strip_global_json_flags(args)
}

pub fn strip_global_cli_flags(
    args: Vec<String>,
) -> Result<(Vec<String>, GlobalCliOptions), CliParseError> {
    global_json::strip_global_cli_flags(args)
}

pub fn apply_global_json_flag(cmd: Command, json_mode: bool) -> Command {
    global_json::apply_global_json_flag(cmd, json_mode)
}

pub fn apply_global_cli_flags(
    cmd: Command,
    options: &GlobalCliOptions,
) -> Result<Command, CliParseError> {
    global_json::apply_global_cli_options(cmd, options)
}

pub fn command_requests_json(cmd: &Command, global_json_mode: bool) -> bool {
    global_json::command_requests_json(cmd, global_json_mode)
}

pub fn parse_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let args = args.into_iter().collect::<Vec<_>>();
    let (args, options) = global_json::strip_global_cli_flags(args)?;
    let command = command_parsing::parse_command(args)?;
    global_json::apply_global_cli_options(command, &options)
}

fn unknown_argument(arg: impl Into<String>) -> CliParseError {
    CliParseError::UnknownArgument(arg.into())
}

#[cfg(test)]
mod gateway_identity_command_tests {
    use super::{parse_command, Command, InternalGatewayIdentityArgs};

    #[test]
    fn gateway_identity_internal_reader_requires_bounded_bindings() {
        let digest = "a".repeat(64);
        let target_digest = "b".repeat(64);
        let parsed = parse_command([
            "__gateway-identity".to_owned(),
            "--digest".to_owned(),
            digest.clone(),
            "--target-digest".to_owned(),
            target_digest.clone(),
            "--owner-uid".to_owned(),
            "501".to_owned(),
        ])
        .expect("parse bounded identity-reader arguments");
        assert_eq!(
            parsed,
            Command::InternalGatewayIdentity(InternalGatewayIdentityArgs {
                digest,
                target_digest,
                owner_uid: 501,
            })
        );
    }

    #[test]
    fn gateway_identity_internal_reader_rejects_arbitrary_pid_and_malformed_hashes() {
        let digest = "a".repeat(64);
        let target_digest = "b".repeat(64);
        for args in [
            vec![
                "__gateway-identity".to_owned(),
                "--pid".to_owned(),
                "42".to_owned(),
                "--digest".to_owned(),
                digest.clone(),
                "--target-digest".to_owned(),
                target_digest.clone(),
                "--owner-uid".to_owned(),
                "501".to_owned(),
            ],
            vec![
                "__gateway-identity".to_owned(),
                "--digest".to_owned(),
                "short".to_owned(),
                "--target-digest".to_owned(),
                target_digest.clone(),
                "--owner-uid".to_owned(),
                "501".to_owned(),
            ],
        ] {
            assert!(parse_command(args).is_err());
        }
    }
}

#[cfg(test)]
mod retirement_tests {
    #[test]
    fn retirement_commands_refuse_with_actionable_messages() {
        for (args, replacement) in [
            (vec!["watch", "--help"], "native watch"),
            (
                vec!["tasks", "migrate", "--apply"],
                "define tasks explicitly",
            ),
            (
                vec!["gateway", "recover", "--adopt-candidate"],
                "host administrator",
            ),
            (vec!["docs", "qualified", "resolve"], "qa:docs"),
        ] {
            let error = super::parse_command(args.into_iter().map(str::to_owned))
                .expect_err("retired command must refuse")
                .to_string();
            assert!(error.contains("retired"), "{error}");
            assert!(error.contains(replacement), "{error}");
        }
    }
}

#[cfg(test)]
mod task_visibility_tests {
    use super::{parse_command, Command};

    #[test]
    fn task_visibility_all_is_distinct_from_status_all() {
        let Command::Tasks(list) =
            parse_command(["tasks", "--all"].map(str::to_owned)).expect("all inventory")
        else {
            panic!("tasks command");
        };
        assert!(list.all);
        assert!(!list.status_all);
        let Command::Tasks(status) =
            parse_command(["tasks", "status", "--all"].map(str::to_owned)).expect("all status")
        else {
            panic!("tasks command");
        };
        assert!(!status.all);
        assert!(status.status_all);
        assert!(parse_command(["tasks", "--all", "status", "build"].map(str::to_owned)).is_err());
    }
}
