//! Parsed command line for Effigy.
//!
//! The stable entrypoint for tools and tests is [`parse_command`], which returns
//! a [`Command`] value tree. Human-readable `--help` panels live under
//! [`help`] ([`HelpTopic`] selects the panel). Global `--json` is threaded by
//! the runner via [`strip_global_json_flags`] and [`apply_global_json_flag`].
//!
//! End-user documentation: repository `docs/guides/025-command-reference-matrix.md`.

use std::path::PathBuf;

mod command_parsing;
pub mod command_surface;
mod global_json;
pub mod header;
pub mod help;
mod value_parsing;

pub use command_surface::HelpGroup;
pub use global_json::{runtime_flag_present_before_passthrough, GlobalCliOptions};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Version,
    Bundle(BundleArgs),
    Changelog(ChangelogArgs),
    Deploy(DeployArgs),
    Deps(DepsArgs),
    Secrets(SecretsArgs),
    Defer(DeferArgs),
    Exec(ExecArgs),
    State(StateArgs),
    System(SystemArgs),
    Workspace(WorkspaceArgs),
    Gateway(GatewayArgs),
    Service(ServiceArgs),
    Demo(DemoArgs),
    Graph(GraphArgs),
    Rhai(RhaiArgs),
    Skill(SkillArgs),
    Docs(DocsArgs),
    Contracts(ContractsArgs),
    Artifact(ArtifactArgs),
    Container(ContainerArgs),
    Bootstrap(BootstrapArgs),
    Uninstall(UninstallArgs),
    Release(ReleaseArgs),
    Doctor(DoctorArgs),
    Tasks(TasksArgs),
    Drafts(DraftsArgs),
    Draft(DraftArgs),
    Task(TaskInvocation),
    #[doc(hidden)]
    InternalScriptRun(InternalScriptRunArgs),
    #[doc(hidden)]
    InternalGateway(InternalGatewayArgs),
    #[doc(hidden)]
    InternalGatewayIdentity(InternalGatewayIdentityArgs),
    #[doc(hidden)]
    InternalGatewayLegacyCandidate(InternalGatewayLegacyCandidateArgs),
    #[doc(hidden)]
    InternalGatewayLegacyStop(InternalGatewayLegacyStopArgs),
    #[doc(hidden)]
    InternalContainerLeaseReaper(InternalContainerLeaseReaperArgs),
    #[doc(hidden)]
    InternalHostProcessSupervise(InternalHostProcessSuperviseArgs),
    #[doc(hidden)]
    InternalHostProcessStop(InternalHostProcessStopArgs),
    Help(HelpTopic),
    /// Discovery-only help for one operator-job group. Grouping never adds an
    /// execution route: see contract `043`.
    HelpGroup(HelpGroup),
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

#[derive(Debug, Clone, PartialEq, Eq)]
#[doc(hidden)]
pub struct InternalHostProcessSuperviseArgs {
    pub repo_root: PathBuf,
    pub container_name: String,
    pub process_name: String,
    pub run: String,
    pub pid_file: PathBuf,
    pub log_file: PathBuf,
    /// `on-failure`, `always`, or `never`.
    pub restart: String,
    pub restart_delay_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[doc(hidden)]
pub struct InternalHostProcessStopArgs {
    pub pid_file: PathBuf,
    /// Signal name (e.g. `SIGTERM`).
    pub signal: String,
    pub grace_secs: u64,
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
    System,
    Workspace,
    Gateway,
    Service,
    Demo,
    Graph,
    Rhai,
    Skill,
    Docs,
    Contracts,
    Artifact,
    Container,
    Bootstrap,
    Release,
    Doctor,
    Tasks,
    Test,
    Watch,
    Init,
    Migrate,
    Uninstall,
    Drafts,
    Draft,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangelogArgs {
    pub subcommand: ChangelogSubcommand,
    pub file: Option<PathBuf>,
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
/// `Model` and `Export` keep the static deployment-file generation surface.
/// The transaction variants (`Plan`, `Apply`, `Status`, `History`, and
/// `Redeploy`) operate on named `[deploy.<env>]` manifest environments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeploySubcommand {
    /// Derive the provider-neutral deployment model.
    Model,
    /// Export provider files without live provider mutation.
    Export {
        /// Provider package id from `[deploy.providers.<id>]`.
        provider: String,
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
pub struct GraphArgs {
    pub subcommand: GraphSubcommand,
    pub repo_override: Option<PathBuf>,
    pub output_json: bool,
    /// Explicit catalog scope selector (`--catalog <alias>`).
    pub catalog: Option<String>,
    /// Explicit fan-out selector (`--all-catalogs`).
    pub all_catalogs: bool,
    /// Read the last complete graph snapshot without waiting for or starting a refresh.
    pub stale_index: bool,
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
pub enum GraphSubcommand {
    Index,
    Status {
        refresh: bool,
    },
    Watch {
        debounce_ms: u64,
    },
    Search {
        query: String,
        limit: Option<usize>,
    },
    Files {
        limit: Option<usize>,
    },
    Node {
        id: String,
    },
    Callers {
        id: String,
        limit: Option<usize>,
    },
    Callees {
        id: String,
        limit: Option<usize>,
    },
    Impact {
        target: String,
        limit: Option<usize>,
    },
    Affected {
        changed_paths: Vec<String>,
        read_stdin: bool,
        depth: usize,
        limit: Option<usize>,
    },
    Context {
        request: String,
        max_files: Option<usize>,
        max_bytes: Option<usize>,
        languages: Vec<String>,
        paths: Vec<String>,
    },
    Explore {
        request: String,
        max_files: Option<usize>,
        max_bytes: Option<usize>,
        languages: Vec<String>,
        paths: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DemoArgs {
    pub subcommand: DemoSubcommand,
    pub repo_override: Option<PathBuf>,
    pub output_json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DemoListQuery {
    pub search: Option<String>,
    pub owner: Option<String>,
    pub tag: Option<String>,
    pub mode: Option<DemoListMode>,
    pub cover: Option<String>,
    pub status: Option<DemoListStatus>,
    pub gap: Option<DemoListGap>,
    pub stale_only: bool,
    pub group_by: Option<DemoListGroupBy>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DemoListMode {
    Headless,
    Interactive,
    Hybrid,
}

impl DemoListMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Headless => "headless",
            Self::Interactive => "interactive",
            Self::Hybrid => "hybrid",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DemoListStatus {
    Planned,
    Ready,
    Running,
    Passed,
    Failed,
    Broken,
    Missing,
}

impl DemoListStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Ready => "ready",
            Self::Running => "running",
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Broken => "broken",
            Self::Missing => "missing",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DemoListGap {
    Existing,
    Planned,
    Missing,
    Broken,
    Stale,
}

impl DemoListGap {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Existing => "existing",
            Self::Planned => "planned",
            Self::Missing => "missing",
            Self::Broken => "broken",
            Self::Stale => "stale",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DemoListGroupBy {
    Owner,
    Tag,
    Mode,
    Cover,
    Status,
    Gap,
}

impl DemoListGroupBy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Tag => "tag",
            Self::Mode => "mode",
            Self::Cover => "cover",
            Self::Status => "status",
            Self::Gap => "gap",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DemoSubcommand {
    Browser {
        group_by: Option<DemoListGroupBy>,
    },
    List {
        query: DemoListQuery,
    },
    Inspect {
        demo_id: String,
    },
    History {
        demo_id: String,
        limit: Option<usize>,
        outcome: Option<DemoHistoryOutcome>,
        attempt_id: Option<String>,
        attempt_ordinal: Option<usize>,
    },
    Run {
        demo_id: String,
    },
    Stop {
        demo_id: String,
    },
    Input {
        demo_id: String,
        text: String,
        append_newline: bool,
    },
    Resize {
        demo_id: String,
        cols: u16,
        rows: u16,
    },
    Rerun {
        demo_id: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DemoHistoryOutcome {
    Passed,
    Failed,
    Terminated,
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
}

impl DemoHistoryOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Terminated => "terminated",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocsSubcommand {
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
    /// Bounded documentation evidence retrieval over the shared graph.
    Context {
        query: String,
        max_sections: Option<usize>,
        max_bytes: Option<usize>,
        max_hops: Option<usize>,
        /// Portfolio file (or directory) naming where opted-in repositories live.
        sources: Option<PathBuf>,
        /// Repeatable handle filter applied to the enumerated repositories.
        only: Vec<String>,
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
    pub output_json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceArgs {
    pub subcommand: ServiceSubcommand,
    pub repo_override: Option<PathBuf>,
    pub output_json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactArgs {
    pub subcommand: ArtifactSubcommand,
    pub repo_override: Option<PathBuf>,
    pub output_json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactSubcommand {
    Inspect {
        source: String,
        farmyard_handoff: bool,
    },
    Stage {
        source: String,
        farmyard_handoff: bool,
    },
    Capture {
        source: String,
        destination: String,
        kind: Option<String>,
        environment_label: Option<String>,
        farmyard_handoff: bool,
        push: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerArgs {
    pub subcommand: ContainerSubcommand,
    pub repo_override: Option<PathBuf>,
    pub output_json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemArgs {
    pub subcommand: SystemSubcommand,
    pub system: Option<String>,
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
    pub subcommand: BootstrapSubcommand,
    pub output_json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UninstallArgs {
    pub plan: bool,
    pub yes: bool,
    pub output_json: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapBackendOverride {
    Containerd,
    Docker,
}

impl BootstrapBackendOverride {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Containerd => "containerd",
            Self::Docker => "docker",
        }
    }
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
pub enum BootstrapSubcommand {
    Clone {
        repo_url: String,
        path: Option<PathBuf>,
        branch: Option<String>,
        backend: Option<BootstrapBackendOverride>,
        db_seeds: Vec<BootstrapDbSeedInput>,
        fresh: bool,
        no_prompt: bool,
        reuse_path: bool,
        start: bool,
        plan: bool,
    },
    Teardown {
        yes: bool,
    },
    DepsSync {
        mode: BootstrapDepsSyncMode,
        refresh_lock: bool,
        paths: Vec<String>,
    },
    ChildrenStatus,
    ChildrenSync {
        fetch_only: bool,
        checkout: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapDepsSyncMode {
    Both,
    JsOnly,
    RustOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceSubcommand {
    List,
    Extract {
        service: String,
        dir: Option<PathBuf>,
    },
    Pack(ServicePackSubcommand),
}

/// Management surface for independently versioned catalog packs, nested under
/// the existing `service` owner.
///
/// `update` resolves the compiled official `stable` channel to a digest, then
/// reuses the install transaction. It takes no coordinate arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServicePackSubcommand {
    /// Report the active selection, lineage, and health.
    Status,
    /// Install an explicit candidate and activate it after validation.
    Install { source: ServicePackInstallSource },
    /// Resolve the official `stable` channel and install that digest.
    Update,
    /// Select the previous validated installed pack.
    Rollback,
    /// Select the compiled baseline, retaining installed content.
    Reset,
}

/// Where an explicit install candidate comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServicePackInstallSource {
    /// Digest-addressed `oci://` reference.
    Oci { reference: String },
    /// Explicitly operator-selected local directory.
    Path { path: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BundleSubcommand {
    Inspect,
    Sync,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatewaySubcommand {
    Up,
    Down,
    Status,
    Repair { yes: bool },
    Recover { yes: bool, adopt_candidate: bool },
    SetupTls,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[doc(hidden)]
pub struct InternalGatewayArgs;

#[derive(Debug, Clone, PartialEq, Eq)]
#[doc(hidden)]
pub struct InternalGatewayIdentityArgs {
    pub digest: String,
    pub target_digest: String,
    pub owner_uid: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[doc(hidden)]
pub struct InternalGatewayLegacyCandidateArgs {
    pub target_digest: String,
    pub record_digest: String,
    pub owner_uid: u32,
    pub directory_owner_uid: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[doc(hidden)]
pub enum GatewayLegacyStopPhase {
    Term,
    Kill,
}

impl GatewayLegacyStopPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Term => "term",
            Self::Kill => "kill",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[doc(hidden)]
pub struct InternalGatewayLegacyStopArgs {
    pub target_digest: String,
    pub record_digest: String,
    pub owner_uid: u32,
    pub candidate_digest: String,
    pub phase: GatewayLegacyStopPhase,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerSubcommand {
    Up {
        name: Option<String>,
        attach: bool,
        detach: bool,
    },
    Down {
        name: Option<String>,
        global: bool,
    },
    Status {
        name: Option<String>,
        global: bool,
    },
    Stats {
        global: bool,
    },
    Logs {
        name: Option<String>,
        service: Option<String>,
        follow: bool,
    },
    Shell {
        name: Option<String>,
        service: Option<String>,
        command: Option<String>,
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
    Eject {
        name: Option<String>,
    },
    Scope,
    Hosts {
        name: Option<String>,
    },
    Retire {
        name: Option<String>,
        scope: Option<String>,
        yes: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerProfileSubcommand {
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SystemSubcommand {
    Up,
    Down,
    Status,
    Logs { follow: bool },
    Repair,
    ResetRuntime,
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
    ValidateSelection {
        contract_path: Option<PathBuf>,
        artifact_path: Option<PathBuf>,
    },
    CheckJson {
        index_path: Option<PathBuf>,
        mode: ContractsCheckMode,
        changed_only_base: Option<String>,
        print_selected: ContractsSelectionPrintMode,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractsCheckMode {
    Fast,
    Full,
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
    Resume {
        allow_stale: bool,
    },
    VerifyInstall {
        tag: Option<String>,
        repo_url: Option<String>,
    },
    Validate {
        tag: Option<String>,
    },
    CheckBinary {
        binary_path: PathBuf,
        glibc_floor: String,
    },
    Preflight {
        tag: Option<String>,
        skip_docs: bool,
        skip_smoke: bool,
        output_path: Option<PathBuf>,
    },
    Proof {
        tag: String,
        crate_version: Option<String>,
        repo_url: String,
        brew_formula: String,
        skip_homebrew: bool,
        artifacts_dir: Option<PathBuf>,
    },
    Evidence {
        subcommand: ReleaseEvidenceSubcommand,
    },
    Simulate {
        version_override: Option<String>,
    },
    Prepare {
        plan: bool,
        check_gates: bool,
        yes: bool,
        version_override: Option<String>,
    },
    Execute {
        plan: bool,
        yes: bool,
        allow_stale: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseEvidenceSubcommand {
    Validate {
        artifacts_dir: PathBuf,
        expect_homebrew: bool,
    },
    Closeout {
        tag: String,
        artifacts_dir: PathBuf,
        output_path: Option<PathBuf>,
        owner: String,
        expect_homebrew: bool,
    },
    Summary {
        tag: String,
        artifacts_dir: PathBuf,
        crate_version: Option<String>,
        repo_url: String,
        brew_formula: String,
        homebrew_executed: bool,
        log_files: Vec<String>,
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

/// `effigy drafts [FILTER] [--json]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftsArgs {
    pub repo_override: Option<PathBuf>,
    pub filter: Option<String>,
    pub output_json: bool,
    pub pretty_json: bool,
}

/// `effigy draft <SELECTOR> [--json] [-- <ARGS>]`.
///
/// `args` carries the raw passthrough exactly as the canonical execution
/// pipeline expects it, so `--json`, `--repo`, and `--` keep normal task-run
/// semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftArgs {
    pub repo_override: Option<PathBuf>,
    pub selector: String,
    pub args: Vec<String>,
    pub output_json: bool,
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
mod gateway_legacy_recovery_command_tests {
    use super::{
        parse_command, Command, GatewayLegacyStopPhase, InternalGatewayLegacyCandidateArgs,
        InternalGatewayLegacyStopArgs,
    };

    #[test]
    fn gateway_legacy_candidate_requires_bounded_bindings() {
        let digest = "a".repeat(64);
        let parsed = parse_command([
            "__gateway-legacy-candidate".to_owned(),
            "--target-digest".to_owned(),
            digest.clone(),
            "--record-digest".to_owned(),
            digest.clone(),
            "--owner-uid".to_owned(),
            "501".to_owned(),
            "--directory-owner-uid".to_owned(),
            "501".to_owned(),
        ])
        .expect("parse bounded candidate arguments");
        assert_eq!(
            parsed,
            Command::InternalGatewayLegacyCandidate(InternalGatewayLegacyCandidateArgs {
                target_digest: digest.clone(),
                record_digest: digest,
                owner_uid: 501,
                directory_owner_uid: 501,
            })
        );
    }

    #[test]
    fn gateway_legacy_candidate_rejects_arbitrary_pid() {
        let digest = "a".repeat(64);
        assert!(parse_command(vec![
            "__gateway-legacy-candidate".to_owned(),
            "--pid".to_owned(),
            "42".to_owned(),
            "--target-digest".to_owned(),
            digest.clone(),
            "--record-digest".to_owned(),
            digest,
            "--owner-uid".to_owned(),
            "501".to_owned(),
            "--directory-owner-uid".to_owned(),
            "501".to_owned(),
        ])
        .is_err());
    }

    #[test]
    fn gateway_legacy_stop_requires_enumerated_phase() {
        let digest = "b".repeat(64);
        let parsed = parse_command([
            "__gateway-legacy-stop".to_owned(),
            "--target-digest".to_owned(),
            digest.clone(),
            "--record-digest".to_owned(),
            digest.clone(),
            "--owner-uid".to_owned(),
            "501".to_owned(),
            "--candidate-digest".to_owned(),
            digest.clone(),
            "--phase".to_owned(),
            "kill".to_owned(),
        ])
        .expect("parse bounded stop arguments");
        assert_eq!(
            parsed,
            Command::InternalGatewayLegacyStop(InternalGatewayLegacyStopArgs {
                target_digest: digest.clone(),
                record_digest: digest.clone(),
                owner_uid: 501,
                candidate_digest: digest,
                phase: GatewayLegacyStopPhase::Kill,
            })
        );
        assert!(parse_command(vec![
            "__gateway-legacy-stop".to_owned(),
            "--target-digest".to_owned(),
            "c".repeat(64),
            "--record-digest".to_owned(),
            "c".repeat(64),
            "--owner-uid".to_owned(),
            "501".to_owned(),
            "--candidate-digest".to_owned(),
            "c".repeat(64),
            "--phase".to_owned(),
            "term".to_owned(),
            "--pid".to_owned(),
            "9".to_owned(),
        ])
        .is_err());
        assert!(parse_command(vec![
            "__gateway-legacy-stop".to_owned(),
            "--target-digest".to_owned(),
            "c".repeat(64),
            "--record-digest".to_owned(),
            "c".repeat(64),
            "--owner-uid".to_owned(),
            "501".to_owned(),
            "--candidate-digest".to_owned(),
            "c".repeat(64),
            "--phase".to_owned(),
            "hup".to_owned(),
        ])
        .is_err());
    }
}
