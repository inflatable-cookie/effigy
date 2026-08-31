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
            HelpGroup::Repo => "Understand the repository, its code graph, and its documentation",
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
        topic: HelpTopic::Papercuts,
        command_name: Some("papercuts"),
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
        topic: HelpTopic::System,
        command_name: Some("system"),
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
        topic: HelpTopic::Demo,
        command_name: Some("demo"),
    },
    CommandDescriptor {
        topic: HelpTopic::Graph,
        command_name: Some("graph"),
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
        topic: HelpTopic::Artifact,
        command_name: None,
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
        topic: HelpTopic::Tasks,
        command_name: None,
    },
    CommandDescriptor {
        topic: HelpTopic::Test,
        command_name: Some("test"),
    },
    CommandDescriptor {
        topic: HelpTopic::Watch,
        command_name: Some("watch"),
    },
    CommandDescriptor {
        topic: HelpTopic::Init,
        command_name: Some("init"),
    },
    CommandDescriptor {
        topic: HelpTopic::Migrate,
        command_name: None,
    },
    CommandDescriptor {
        topic: HelpTopic::Changelog,
        command_name: None,
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
        description: "Run a managed concurrent task without the terminal UI; inspect it with task-local status, logs, and stop companions",
        deferred_builtin: None,
        help_argument: None,
    },
    GeneralHelpEntry {
        group: HelpGroup::Work,
        command: "effigy tasks",
        description: "List effective catalogs/task commands and probe routing",
        deferred_builtin: Some("tasks"),
        help_argument: Some("tasks"),
    },
    GeneralHelpEntry {
        group: HelpGroup::Work,
        command: "effigy tasks migrate",
        description: "Import package scripts into `[tasks]` with preview/apply flow",
        deferred_builtin: None,
        help_argument: None,
    },
    GeneralHelpEntry {
        group: HelpGroup::Work,
        command: "effigy tasks unlock",
        description: "Manually clear lock scopes (`workspace`, `shared:*`, `task:*`, `profile:*/*`)",
        deferred_builtin: None,
        help_argument: None,
    },
    GeneralHelpEntry {
        group: HelpGroup::Work,
        command: "effigy tasks cache",
        description: "Inspect/invalidate phase-1 task cache metadata (`inspect`, `invalidate`)",
        deferred_builtin: None,
        help_argument: None,
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
        command: "effigy watch",
        description: "Watch mode phase-1 runtime with explicit owner policy and debounce/glob controls",
        deferred_builtin: Some("watch"),
        help_argument: Some("watch"),
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
        command: "effigy system",
        description: "Operate the manifest default system substrate through its default workspace container",
        deferred_builtin: Some("system"),
        help_argument: Some("system"),
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
        description: "Inspect the layered service catalog and extract bundled fragments for override ownership",
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
        command: "effigy graph",
        description: "Build and query the local deterministic code graph for agent navigation",
        deferred_builtin: Some("graph"),
        help_argument: Some("graph"),
    },
    GeneralHelpEntry {
        group: HelpGroup::Repo,
        command: "effigy scan",
        description: "Run built-in repository scanners such as `god-files` and `attention-markers`",
        deferred_builtin: None,
        help_argument: Some("scan"),
    },
    GeneralHelpEntry {
        group: HelpGroup::Repo,
        command: "effigy docs",
        description: "Run reusable docs QA checks and bounded `docs context` documentation retrieval",
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
        command: "effigy papercuts",
        description: "Discover project papercut queues for humans and agents",
        deferred_builtin: None,
        help_argument: Some("papercuts"),
    },
    // ---- deliver ------------------------------------------------------------
    GeneralHelpEntry {
        group: HelpGroup::Deliver,
        command: "effigy artifact",
        description: "Inspect and stage standalone seed/apply/capture data artifacts",
        deferred_builtin: Some("artifact"),
        help_argument: Some("artifact"),
    },
    GeneralHelpEntry {
        group: HelpGroup::Deliver,
        command: "effigy state",
        description: "Plan layered state-stack manifests and lineage without executing app hooks",
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
        description: "Clone/update a repo from a git URL and apply its repo-owned `[bootstrap]` contract",
        deferred_builtin: Some("bootstrap"),
        help_argument: Some("bootstrap"),
    },
    GeneralHelpEntry {
        group: HelpGroup::Deliver,
        command: "effigy demo",
        description: "List declared demos and inspect the latest known proof state without starting execution",
        deferred_builtin: Some("demo"),
        help_argument: Some("demo"),
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
    ("artifact", HelpTopic::Artifact),
    ("bootstrap", HelpTopic::Bootstrap),
    ("bundle", HelpTopic::Bundle),
    ("changelog", HelpTopic::Changelog),
    ("container", HelpTopic::Container),
    ("contracts", HelpTopic::Contracts),
    ("defer", HelpTopic::Defer),
    ("demo", HelpTopic::Demo),
    ("deploy", HelpTopic::Deploy),
    ("deps", HelpTopic::Deps),
    ("docs", HelpTopic::Docs),
    ("doctor", HelpTopic::Doctor),
    ("exec", HelpTopic::Exec),
    ("gateway", HelpTopic::Gateway),
    ("graph", HelpTopic::Graph),
    ("help", HelpTopic::General),
    ("init", HelpTopic::Init),
    ("papercuts", HelpTopic::Papercuts),
    ("release", HelpTopic::Release),
    ("rhai", HelpTopic::Rhai),
    ("secrets", HelpTopic::Secrets),
    ("service", HelpTopic::Service),
    ("skill", HelpTopic::Skill),
    ("state", HelpTopic::State),
    ("system", HelpTopic::System),
    ("tasks", HelpTopic::Tasks),
    ("test", HelpTopic::Test),
    ("uninstall", HelpTopic::Uninstall),
    ("version", HelpTopic::General),
    ("watch", HelpTopic::Watch),
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

/// The built-in name whose repository deferral hides `effigy help <command>`
/// for `topic`.
///
/// When a manifest selector or `[defer] builtins` entry owns that name,
/// `effigy <command> --help` already routes to the repository instead of the
/// built-in panel, so `effigy help <command>` must not present the panel
/// either.
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

#[cfg(test)]
#[path = "command_surface/tests.rs"]
mod tests;
