use crate::HelpTopic;

/// Primary operator-job group that owns a general-help entry.
///
/// Grouping is a discovery concern only: no `effigy <group> <command>`
/// execution route exists, and group words stay available to manifest task
/// selectors (contract `043`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HelpGroup {
    Work,
    Local,
    Repo,
    Deliver,
    Extend,
    Admin,
}

impl HelpGroup {
    /// Every group, in the order general help renders them.
    pub const ALL: &'static [HelpGroup] = &[
        HelpGroup::Work,
        HelpGroup::Local,
        HelpGroup::Repo,
        HelpGroup::Deliver,
        HelpGroup::Extend,
        HelpGroup::Admin,
    ];

    /// Topic word accepted by `effigy help <group>`.
    pub fn slug(self) -> &'static str {
        match self {
            HelpGroup::Work => "work",
            HelpGroup::Local => "local",
            HelpGroup::Repo => "repo",
            HelpGroup::Deliver => "deliver",
            HelpGroup::Extend => "extend",
            HelpGroup::Admin => "admin",
        }
    }

    /// Section title used by general help and group help.
    pub fn title(self) -> &'static str {
        match self {
            HelpGroup::Work => "Work Commands",
            HelpGroup::Local => "Local Commands",
            HelpGroup::Repo => "Repo Commands",
            HelpGroup::Deliver => "Deliver Commands",
            HelpGroup::Extend => "Extend Commands",
            HelpGroup::Admin => "Admin Commands",
        }
    }

    /// One-line description of the job the group covers.
    pub fn summary(self) -> &'static str {
        match self {
            HelpGroup::Work => "Run, target, and diagnose repository tasks",
            HelpGroup::Local => {
                "Operate local containers, systems, workspaces, and the host gateway"
            }
            HelpGroup::Repo => "Understand the repository and its documentation",
            HelpGroup::Deliver => {
                "Stage artifacts and state, then deploy, release, and bootstrap repositories"
            }
            HelpGroup::Extend => "Extend Effigy with external skills and Rhai scripting",
            HelpGroup::Admin => "Configure Effigy and manage machine-local state",
        }
    }

    /// Resolve `effigy help <group>`.
    pub fn from_slug(slug: &str) -> Option<Self> {
        HelpGroup::ALL
            .iter()
            .copied()
            .find(|group| group.slug() == slug)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandDescriptor {
    pub topic: HelpTopic,
    pub command_name: Option<&'static str>,
}

/// One row of the general-help inventory.
///
/// The inventory is the single typed owner of general help: each row names
/// exactly one primary [`HelpGroup`], so general help, group help, and the
/// ownership assertions all read the same table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeneralHelpEntry {
    /// The one primary group that owns this row.
    pub group: HelpGroup,
    pub command: &'static str,
    pub description: &'static str,
    /// Built-in name whose manifest deferral hides this row.
    pub deferred_builtin: Option<&'static str>,
    /// Name accepted by `effigy help <name>`, when the row has a typed panel.
    pub help_argument: Option<&'static str>,
}

pub const COMMAND_DESCRIPTORS: &[CommandDescriptor] = &[
    CommandDescriptor {
        topic: HelpTopic::General,
        command_name: Some("help"),
    },
    CommandDescriptor {
        topic: HelpTopic::Bundle,
        command_name: Some("bundle"),
    },
    CommandDescriptor {
        topic: HelpTopic::Deploy,
        command_name: Some("deploy"),
    },
    CommandDescriptor {
        topic: HelpTopic::Deps,
        command_name: Some("deps"),
    },
    CommandDescriptor {
        topic: HelpTopic::Secrets,
        command_name: Some("secrets"),
    },
    CommandDescriptor {
        topic: HelpTopic::Defer,
        command_name: Some("defer"),
    },
    CommandDescriptor {
        topic: HelpTopic::Exec,
        command_name: Some("exec"),
    },
    CommandDescriptor {
        topic: HelpTopic::State,
        command_name: None,
    },
    CommandDescriptor {
        topic: HelpTopic::Workspace,
        command_name: Some("workspace"),
    },
    CommandDescriptor {
        topic: HelpTopic::Gateway,
        command_name: Some("gateway"),
    },
    CommandDescriptor {
        topic: HelpTopic::Service,
        command_name: Some("service"),
    },
    CommandDescriptor {
        topic: HelpTopic::Rhai,
        command_name: Some("rhai"),
    },
    CommandDescriptor {
        topic: HelpTopic::Skill,
        command_name: Some("skill"),
    },
    CommandDescriptor {
        topic: HelpTopic::Docs,
        command_name: Some("docs"),
    },
    CommandDescriptor {
        topic: HelpTopic::Contracts,
        command_name: Some("contracts"),
    },
    CommandDescriptor {
        topic: HelpTopic::Container,
        command_name: Some("container"),
    },
    CommandDescriptor {
        topic: HelpTopic::Bootstrap,
        command_name: Some("bootstrap"),
    },
    CommandDescriptor {
        topic: HelpTopic::Uninstall,
        command_name: Some("uninstall"),
    },
    CommandDescriptor {
        topic: HelpTopic::Release,
        command_name: Some("release"),
    },
    CommandDescriptor {
        topic: HelpTopic::Doctor,
        command_name: Some("doctor"),
    },
    CommandDescriptor {
        topic: HelpTopic::Runs,
        command_name: Some("runs"),
    },
    CommandDescriptor {
        topic: HelpTopic::Tasks,
        command_name: None,
    },
    CommandDescriptor {
        topic: HelpTopic::Test,
        command_name: Some("test"),
    },
    CommandDescriptor {
        topic: HelpTopic::Init,
        command_name: Some("init"),
    },
    CommandDescriptor {
        topic: HelpTopic::Changelog,
        command_name: None,
    },
    CommandDescriptor {
        topic: HelpTopic::Fmt,
        command_name: Some("fmt"),
    },
];

/// General-help inventory, ordered by primary group.
///
/// Group membership matches the primary-ownership table in contract `043`.
pub const GENERAL_HELP_ENTRIES: &[GeneralHelpEntry] = &[
    // ---- work ---------------------------------------------------------------
    GeneralHelpEntry {
        group: HelpGroup::Work,
        command: "effigy <task>",
        description: "Resolve task across effective catalogs",
        deferred_builtin: None,
        help_argument: None,
    },
    GeneralHelpEntry {
        group: HelpGroup::Work,
        command: "effigy <catalog>/<task>",
        description: "Run task from explicit catalog alias",
        deferred_builtin: None,
        help_argument: None,
    },
    GeneralHelpEntry {
        group: HelpGroup::Work,
        command: "effigy <managed-task> --headless",
        description: "Follow a managed run as text (`--detach` starts it in the background); inspect it with task-local status, logs, and stop companions",
        deferred_builtin: None,
        help_argument: None,
    },
    GeneralHelpEntry {
        group: HelpGroup::Work,
        command: "effigy tasks",
        description: "List public tasks by group; --all and --json include internal tasks",
        deferred_builtin: Some("tasks"),
        help_argument: Some("tasks"),
    },
    GeneralHelpEntry {
        group: HelpGroup::Work,
        command: "effigy runs",
        description: "List runs; follow, inspect or stop one with `effigy run status|logs|stop <run>`",
        deferred_builtin: Some("runs"),
        help_argument: Some("runs"),
    },
    GeneralHelpEntry {
        group: HelpGroup::Work,
        command: "effigy test",
        description: "Run built-in auto-detected or `[test.suites]` tests; supports <catalog>/test targeting",
        deferred_builtin: Some("test"),
        help_argument: Some("test"),
    },
    GeneralHelpEntry {
        group: HelpGroup::Work,
        command: "effigy doctor",
        description: "Run remedial-first health checks for environment, manifests, and task references",
        deferred_builtin: Some("doctor"),
        help_argument: Some("doctor"),
    },
    GeneralHelpEntry {
        group: HelpGroup::Work,
        command: "effigy init",
        description: "Initialize baseline effigy.toml scaffold with safe overwrite/dry-run controls",
        deferred_builtin: Some("init"),
        help_argument: Some("init"),
    },
    // ---- local --------------------------------------------------------------
    GeneralHelpEntry {
        group: HelpGroup::Local,
        command: "effigy container",
        description: "Operate manifest-defined local container environments",
        deferred_builtin: Some("container"),
        help_argument: Some("container"),
    },
    GeneralHelpEntry {
        group: HelpGroup::Local,
        command: "effigy workspace",
        description: "Ensure the selected system is up, then open the resolved workspace shell",
        deferred_builtin: Some("workspace"),
        help_argument: Some("workspace"),
    },
    GeneralHelpEntry {
        group: HelpGroup::Local,
        command: "effigy gateway",
        description: "Operate the host-native local DNS and reverse-proxy gateway",
        deferred_builtin: Some("gateway"),
        help_argument: Some("gateway"),
    },
    GeneralHelpEntry {
        group: HelpGroup::Local,
        command: "effigy service",
        description: "Inspect catalog sources and services, and extract baseline fragments",
        deferred_builtin: Some("service"),
        help_argument: Some("service"),
    },
    GeneralHelpEntry {
        group: HelpGroup::Local,
        command: "effigy exec",
        description: "Run one ad-hoc command inside the manifest's default system workspace container",
        deferred_builtin: None,
        help_argument: Some("exec"),
    },
    // ---- repo ---------------------------------------------------------------
    GeneralHelpEntry {
        group: HelpGroup::Repo,
        command: "effigy scan",
        description: "Run the built-in repository scanners `god-files` and `duplicate-blocks`",
        deferred_builtin: None,
        help_argument: Some("scan"),
    },
    GeneralHelpEntry {
        group: HelpGroup::Repo,
        command: "effigy docs",
        description: "Run reusable docs QA checks",
        deferred_builtin: Some("docs"),
        help_argument: Some("docs"),
    },
    GeneralHelpEntry {
        group: HelpGroup::Repo,
        command: "effigy contracts",
        description: "Validate reusable JSON contract artifacts such as selection payloads",
        deferred_builtin: Some("contracts"),
        help_argument: Some("contracts"),
    },
    GeneralHelpEntry {
        group: HelpGroup::Repo,
        command: "effigy fmt rust <FILE>...",
        description: "Format only the named Rust files with their nearest rustfmt config",
        deferred_builtin: Some("fmt"),
        help_argument: Some("fmt"),
    },
    GeneralHelpEntry {
        group: HelpGroup::Repo,
        command: "effigy fmt-rust <FILE>...",
        description: "Format named Rust files with the built-in formatter when a repository task owns `effigy fmt`",
        deferred_builtin: Some("fmt-rust"),
        help_argument: Some("fmt-rust"),
    },
    // ---- deliver ------------------------------------------------------------
    GeneralHelpEntry {
        group: HelpGroup::Deliver,
        command: "effigy state",
        description: "Plan layered state stacks, and inspect, stage, or capture data artifacts",
        deferred_builtin: None,
        help_argument: Some("state"),
    },
    GeneralHelpEntry {
        group: HelpGroup::Deliver,
        command: "effigy deploy",
        description: "Inspect the provider-neutral production deployment model derived from the effective manifest",
        deferred_builtin: Some("deploy"),
        help_argument: Some("deploy"),
    },
    GeneralHelpEntry {
        group: HelpGroup::Deliver,
        command: "effigy release",
        description: "Inspect release readiness from changelog, version files, and optional gates",
        deferred_builtin: Some("release"),
        help_argument: Some("release"),
    },
    GeneralHelpEntry {
        group: HelpGroup::Deliver,
        command: "effigy bundle",
        description: "Inspect or refresh the active repo bundle source",
        deferred_builtin: Some("bundle"),
        help_argument: Some("bundle"),
    },
    GeneralHelpEntry {
        group: HelpGroup::Deliver,
        command: "effigy bootstrap",
        description: "Download and start a project: clone or use this checkout, prepare pinned sources and submodules, run its `setup` selectors, then its `start` selector",
        deferred_builtin: Some("bootstrap"),
        help_argument: Some("bootstrap"),
    },
    // ---- extend -------------------------------------------------------------
    GeneralHelpEntry {
        group: HelpGroup::Extend,
        command: "effigy skill",
        description: "Run tasks from one explicit external skill source against a separate consumer repository",
        deferred_builtin: None,
        help_argument: Some("skill"),
    },
    GeneralHelpEntry {
        group: HelpGroup::Extend,
        command: "effigy rhai surface",
        description: "Inspect the registered Rhai host API available to scripts",
        deferred_builtin: None,
        help_argument: Some("rhai"),
    },
    // ---- admin --------------------------------------------------------------
    GeneralHelpEntry {
        group: HelpGroup::Admin,
        command: "effigy config",
        description: "Show config keys/examples, bundle schema guidance, or inspect the effective composed manifest and focused path sources",
        deferred_builtin: None,
        help_argument: Some("config"),
    },
    GeneralHelpEntry {
        group: HelpGroup::Admin,
        command: "effigy deps",
        description: "Inspect dependency state, manage machine-local links, and author committed Bun pins",
        deferred_builtin: Some("deps"),
        help_argument: Some("deps"),
    },
    GeneralHelpEntry {
        group: HelpGroup::Admin,
        command: "effigy secrets",
        description: "Inspect declarations and manage the local encrypted secrets vault",
        deferred_builtin: Some("secrets"),
        help_argument: Some("secrets"),
    },
    GeneralHelpEntry {
        group: HelpGroup::Admin,
        command: "effigy defer",
        description: "Run the configured `[defer]` fallback explicitly instead of relying on selector miss routing",
        deferred_builtin: Some("defer"),
        help_argument: Some("defer"),
    },
    GeneralHelpEntry {
        group: HelpGroup::Admin,
        command: "effigy uninstall",
        description: "Plan or remove Effigy-owned local machine state",
        deferred_builtin: None,
        help_argument: Some("uninstall"),
    },
    GeneralHelpEntry {
        group: HelpGroup::Admin,
        command: "effigy version",
        description: "Print the current Effigy version (same as --version)",
        deferred_builtin: None,
        help_argument: Some("version"),
    },
    GeneralHelpEntry {
        group: HelpGroup::Admin,
        command: "effigy config completion",
        description: "Generate shell completion scripts and selector candidates",
        deferred_builtin: None,
        help_argument: None,
    },
    GeneralHelpEntry {
        group: HelpGroup::Admin,
        command: "effigy help",
        description: "Show general help (same as --help)",
        deferred_builtin: None,
        help_argument: Some("help"),
    },
];

/// Names accepted by `effigy help <command>` and the typed panel each renders.
///
/// Every pair must render the same facts as `effigy <command> --help`; the
/// parity test in this module parses both forms and compares them.
pub const HELP_COMMAND_TOPICS: &[(&str, HelpTopic)] = &[
    ("bootstrap", HelpTopic::Bootstrap),
    ("bundle", HelpTopic::Bundle),
    ("changelog", HelpTopic::Changelog),
    ("container", HelpTopic::Container),
    ("contracts", HelpTopic::Contracts),
    ("defer", HelpTopic::Defer),
    ("deploy", HelpTopic::Deploy),
    ("deps", HelpTopic::Deps),
    ("docs", HelpTopic::Docs),
    ("doctor", HelpTopic::Doctor),
    ("exec", HelpTopic::Exec),
    ("fmt", HelpTopic::Fmt),
    ("fmt-rust", HelpTopic::Fmt),
    ("gateway", HelpTopic::Gateway),
    ("help", HelpTopic::General),
    ("init", HelpTopic::Init),
    ("release", HelpTopic::Release),
    ("rhai", HelpTopic::Rhai),
    ("secrets", HelpTopic::Secrets),
    ("service", HelpTopic::Service),
    ("skill", HelpTopic::Skill),
    ("state", HelpTopic::State),
    ("runs", HelpTopic::Runs),
    ("tasks", HelpTopic::Tasks),
    ("test", HelpTopic::Test),
    ("uninstall", HelpTopic::Uninstall),
    ("version", HelpTopic::General),
    ("workspace", HelpTopic::Workspace),
];

pub fn command_descriptors() -> &'static [CommandDescriptor] {
    COMMAND_DESCRIPTORS
}

pub fn descriptor_for_topic(topic: HelpTopic) -> Option<&'static CommandDescriptor> {
    COMMAND_DESCRIPTORS
        .iter()
        .find(|descriptor| descriptor.topic == topic)
}

pub fn help_topic_for_command(command: &str) -> Option<HelpTopic> {
    COMMAND_DESCRIPTORS
        .iter()
        .find(|descriptor| descriptor.command_name == Some(command))
        .map(|descriptor| descriptor.topic)
}

/// Resolve `effigy help <command>` to its typed panel.
pub fn help_topic_for_help_argument(name: &str) -> Option<HelpTopic> {
    HELP_COMMAND_TOPICS
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .map(|(_, topic)| *topic)
}

/// Names accepted by `effigy help <command>` whose detailed help is owned by
/// the built-in itself instead of a typed [`HelpTopic`] panel.
///
/// `effigy config --help` and `effigy scan --help` already reach those owners
/// through the built-in registry, so `effigy help <name>` resolves to the very
/// same command value rather than re-rendering the panel here.
pub const HELP_COMMAND_BUILTIN_ROUTES: &[&str] = &["config", "scan"];

/// Resolve `effigy help <command>` for a built-in that owns its own help.
pub fn help_builtin_route(name: &str) -> Option<&'static str> {
    HELP_COMMAND_BUILTIN_ROUTES
        .iter()
        .copied()
        .find(|candidate| *candidate == name)
}

/// Every name `effigy help <command>` accepts, typed panels and built-in-owned
/// help alike.
pub fn help_command_names() -> impl Iterator<Item = &'static str> {
    HELP_COMMAND_TOPICS
        .iter()
        .map(|(name, _)| *name)
        .chain(HELP_COMMAND_BUILTIN_ROUTES.iter().copied())
}

/// The built-in whose repository deferral hides `effigy help <name>`.
///
/// When a manifest selector or `[defer] builtins` entry owns that name,
/// `effigy <command> --help` already routes to the repository instead of the
/// built-in panel, so `effigy help <command>` must not present the panel
/// either. Resolved by the name an operator typed, not by its help topic:
/// `fmt` and its escape spelling `fmt-rust` share one panel but defer
/// independently.
pub fn deferred_builtin_for_help_argument(name: &str) -> Option<&'static str> {
    GENERAL_HELP_ENTRIES
        .iter()
        .find(|entry| entry.help_argument == Some(name))
        .and_then(|entry| entry.deferred_builtin)
}

/// The built-in name whose repository deferral hides `effigy help <command>`
/// for `topic`, when the topic has one deferring built-in.
pub fn deferred_builtin_for_help_topic(topic: HelpTopic) -> Option<&'static str> {
    GENERAL_HELP_ENTRIES
        .iter()
        .find(|entry| {
            entry
                .help_argument
                .and_then(help_topic_for_help_argument)
                .is_some_and(|candidate| candidate == topic)
        })
        .and_then(|entry| entry.deferred_builtin)
}

pub fn general_help_entries() -> &'static [GeneralHelpEntry] {
    GENERAL_HELP_ENTRIES
}

pub fn general_help_entries_for_group(
    group: HelpGroup,
) -> impl Iterator<Item = &'static GeneralHelpEntry> {
    GENERAL_HELP_ENTRIES
        .iter()
        .filter(move |entry| entry.group == group)
}

/// Validate the selector shape required when `effigy skill run` has no
/// explicit `--path` source override.
///
/// Named lookup derives the installed agent skill name from the selector's
/// first `/`-separated segment, so the selector must be qualified and that
/// segment must be one plain directory name. Traversal, empty segments, and
/// separator characters are rejected before any filesystem probing.
pub fn validate_qualified_named_skill_selector(selector: &str) -> Result<(), String> {
    let Some((skill, task)) = selector.split_once('/') else {
        return Err(format!(
            "`effigy skill run {selector}` needs `--path <SKILL_DIR|EFFIGY_TOML>` or a qualified `<skill>/<task>` selector naming an installed agent skill"
        ));
    };
    if skill.is_empty() || task.is_empty() {
        return Err(format!(
            "`effigy skill run {selector}` is not a qualified `<skill>/<task>` selector; name one installed agent skill and its task"
        ));
    }
    let invalid_segment = skill == "."
        || skill == ".."
        || skill.starts_with('-')
        || skill.contains('\\')
        || skill.contains(':')
        || skill.chars().any(char::is_control);
    if invalid_segment {
        return Err(format!(
            "`effigy skill run {selector}` names an invalid skill segment `{skill}`; use one plain installed agent skill directory name"
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "command_surface/tests.rs"]
mod tests;
