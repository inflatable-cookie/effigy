use std::path::PathBuf;

mod command_parsing;
mod global_json;
pub mod header;
pub mod help;
mod value_parsing;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Version,
    Changelog(ChangelogArgs),
    Exec(ExecArgs),
    Gateway(GatewayArgs),
    Service(ServiceArgs),
    Demo(DemoArgs),
    Docs(DocsArgs),
    Contracts(ContractsArgs),
    Distribution(DistributionArgs),
    Container(ContainerArgs),
    Bootstrap(BootstrapArgs),
    Release(ReleaseArgs),
    Doctor(DoctorArgs),
    Tasks(TasksArgs),
    Task(TaskInvocation),
    #[doc(hidden)]
    InternalRhai(InternalRhaiArgs),
    #[doc(hidden)]
    InternalGateway(InternalGatewayArgs),
    Help(HelpTopic),
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[doc(hidden)]
pub struct InternalRhaiArgs {
    pub file: PathBuf,
    pub repo_root: Option<PathBuf>,
    pub task_name: Option<String>,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpTopic {
    General,
    Changelog,
    Exec,
    Gateway,
    Service,
    Demo,
    Docs,
    Contracts,
    Distribution,
    Container,
    Bootstrap,
    Release,
    Doctor,
    Tasks,
    Test,
    Watch,
    Init,
    Migrate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangelogArgs {
    pub subcommand: ChangelogSubcommand,
    pub file: Option<PathBuf>,
    pub output_json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocsArgs {
    pub subcommand: DocsSubcommand,
    pub repo_override: Option<PathBuf>,
    pub output_json: bool,
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
    CheckLinks {
        paths: Vec<PathBuf>,
    },
    CheckJsonExamples {
        file: Option<PathBuf>,
        section: Option<String>,
        min_blocks: Option<usize>,
        required: Vec<String>,
        required_blocks: Vec<DocsBlockRequirement>,
    },
    CheckHeadings {
        paths: Vec<PathBuf>,
        required_headings: Vec<String>,
    },
    CheckPaths {
        paths: Vec<PathBuf>,
    },
    CheckContains {
        paths: Vec<PathBuf>,
        required_text: Vec<String>,
    },
    CheckForbidden {
        paths: Vec<PathBuf>,
        forbidden_text: Vec<String>,
    },
    CheckIndex {
        policy_index: Option<String>,
        dir: Option<PathBuf>,
        index: Option<PathBuf>,
    },
    CheckNextAction {
        policy_name: Option<String>,
    },
    CheckWorkflowPaths {
        dir: Option<PathBuf>,
    },
    AddLogIndex {
        log_path: PathBuf,
    },
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
pub struct DistributionArgs {
    pub subcommand: DistributionSubcommand,
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
pub struct BootstrapArgs {
    pub repo_url: String,
    pub path: Option<PathBuf>,
    pub branch: Option<String>,
    pub start: bool,
    pub plan: bool,
    pub output_json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DistributionSubcommand {
    ValidateMetadata {
        tag: Option<String>,
    },
    CheckGlibcFloor {
        binary_path: PathBuf,
        max_glibc: String,
    },
    Preflight {
        tag: Option<String>,
        skip_docs: bool,
        skip_smoke: bool,
        output_path: Option<PathBuf>,
    },
    FirstPublish {
        tag: String,
        crate_version: Option<String>,
        repo_url: String,
        brew_formula: String,
        skip_homebrew: bool,
        artifacts_dir: Option<PathBuf>,
    },
    ValidateArtifacts {
        artifacts_dir: PathBuf,
        expect_homebrew: bool,
    },
    GenerateCloseout {
        tag: String,
        artifacts_dir: PathBuf,
        output_path: Option<PathBuf>,
        owner: String,
        expect_homebrew: bool,
    },
    WriteSummary {
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
pub enum ServiceSubcommand {
    List,
    Extract {
        service: String,
        dir: Option<PathBuf>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatewaySubcommand {
    Up,
    Down,
    Status,
    SetupTls,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[doc(hidden)]
pub struct InternalGatewayArgs;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerSubcommand {
    Up {
        name: Option<String>,
        attach: bool,
        detach: bool,
    },
    Down {
        name: Option<String>,
    },
    Status {
        name: Option<String>,
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
    },
    Eject {
        name: Option<String>,
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
pub struct DoctorArgs {
    pub repo_override: Option<PathBuf>,
    pub output_json: bool,
    pub fix: bool,
    pub verbose: bool,
    pub explain: Option<TaskInvocation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TasksArgs {
    pub repo_override: Option<PathBuf>,
    pub task_name: Option<String>,
    pub resolve_selector: Option<String>,
    pub output_json: bool,
    pub pretty_json: bool,
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
    MissingPrettyValue,
    MissingFlagValue {
        flag: String,
    },
    InvalidPrettyValue(String),
    InvalidFlagValue {
        flag: String,
        value: String,
        expected: String,
    },
    UnknownArgument(String),
}

impl std::fmt::Display for CliParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CliParseError::MissingRepoValue => write!(f, "--repo requires a value"),
            CliParseError::MissingTaskNameValue => write!(f, "--task requires a value"),
            CliParseError::MissingResolveSelectorValue => write!(f, "--resolve requires a value"),
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

pub fn apply_global_json_flag(cmd: Command, json_mode: bool) -> Command {
    global_json::apply_global_json_flag(cmd, json_mode)
}

pub fn command_requests_json(cmd: &Command, global_json_mode: bool) -> bool {
    global_json::command_requests_json(cmd, global_json_mode)
}

pub fn parse_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    command_parsing::parse_command(args)
}

fn unknown_argument(arg: impl Into<String>) -> CliParseError {
    CliParseError::UnknownArgument(arg.into())
}
