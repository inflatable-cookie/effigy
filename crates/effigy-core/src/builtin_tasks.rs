//! Shared built-in command names, routing and listing metadata.

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

/// Who wins when a repository task and a built-in claim the same word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinNameAuthority {
    /// The repository's task wins; the built-in names an explicit escape
    /// spelling when it has one.
    Shadowable,
    /// The built-in always owns the word.
    Reserved,
}

/// One built-in top-level command name and how it resolves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltinCommandName {
    /// The word an operator types after `effigy`.
    pub name: &'static str,
    /// Who wins when a repository task declares `name`.
    pub authority: BuiltinNameAuthority,
    /// Whether the parser claims the word before task routing. `true` means a
    /// repository task of that name only wins because it is resolved ahead of
    /// the built-in at parse time; `false` means the built-in is reached
    /// through ordinary task routing, where a declared task already wins.
    pub deferred_at_parse: bool,
    /// Explicit spelling that still reaches this built-in after a repository
    /// task has taken `name`.
    pub escape: Option<&'static str>,
    /// A retired word that keeps its classification because repositories still
    /// declare tasks with the name. It still answers with its retirement
    /// refusal, but it is not offered as a live command.
    pub retired: bool,
    /// The listing text for built-in tasks.
    pub description: Option<&'static str>,
    /// Position in the public built-in task listing.
    pub listing_order: usize,
    /// Typed panel accepted by `effigy help <name>`.
    pub help_topic: Option<HelpTopic>,
}

const fn shadowable(
    name: &'static str,
    description: Option<&'static str>,
    listing_order: usize,
    deferred_at_parse: bool,
    escape: Option<&'static str>,
    help_topic: Option<HelpTopic>,
) -> BuiltinCommandName {
    BuiltinCommandName {
        name,
        description,
        listing_order,
        authority: BuiltinNameAuthority::Shadowable,
        deferred_at_parse,
        escape,
        retired: false,
        help_topic,
    }
}

const fn retired_shadowable(name: &'static str, deferred_at_parse: bool) -> BuiltinCommandName {
    BuiltinCommandName {
        name,
        authority: BuiltinNameAuthority::Shadowable,
        description: None,
        listing_order: usize::MAX,
        deferred_at_parse,
        escape: None,
        retired: true,
        help_topic: None,
    }
}

const fn reserved(
    name: &'static str,
    description: Option<&'static str>,
    listing_order: usize,
    help_topic: HelpTopic,
) -> BuiltinCommandName {
    BuiltinCommandName {
        name,
        authority: BuiltinNameAuthority::Reserved,
        description,
        listing_order,
        deferred_at_parse: false,
        escape: None,
        retired: false,
        help_topic: Some(help_topic),
    }
}

/// Every built-in top-level command name and how it resolves.
///
/// `deferred_at_parse` is a fact about the parser, not a choice: a word the
/// parser claims (`effigy fmt rust …`) needs deferring before the built-in is
/// parsed, while a word that already reaches task routing (`effigy scan …`)
/// needs nothing. The guard test runs both kinds against a real binary.
pub const BUILTIN_COMMAND_NAMES: &[BuiltinCommandName] = &[
    // Retired words stay shadowable: repositories still declare tasks with
    // these names, and a retirement must not start running the built-in over
    // the repository's own task.
    retired_shadowable("demo", true),
    shadowable("distribution", Some("Validate distribution metadata, glibc floors, and release packaging surfaces"), 11, false, None, None),
    shadowable("bundle", Some("Inspect and sync bundle sources"), 1, true, None, Some(HelpTopic::Bundle)),
    shadowable("changelog", Some("Inspect and extract changelog release notes"), 2, true, None, Some(HelpTopic::Changelog)),
    shadowable("fmt", None, usize::MAX, true, Some("fmt-rust"), Some(HelpTopic::Fmt)),
    // The escape spelling for a shadowed `fmt` is itself an ordinary built-in,
    // so a repository task named `fmt-rust` shadows it in turn.
    shadowable("fmt-rust", None, usize::MAX, true, None, Some(HelpTopic::Fmt)),
    shadowable("deploy", Some("Inspect, plan, apply, and export deployment flows"), 7, true, None, Some(HelpTopic::Deploy)),
    shadowable("deps", Some("Inspect dependency state, manage machine-local links, and author committed Bun pins"), 8, true, None, Some(HelpTopic::Deps)),
    shadowable("secrets", Some("Inspect and manage local secret and encrypted vault surfaces"), 18, true, None, Some(HelpTopic::Secrets)),
    shadowable("defer", Some("Run the configured `[defer]` fallback explicitly instead of relying on selector miss routing"), 6, true, None, Some(HelpTopic::Defer)),
    shadowable("exec", Some("Run typed shell and container execution surfaces"), 13, true, None, Some(HelpTopic::Exec)),
    shadowable("state", Some("Plan, apply, and capture state stacks, and inspect, stage, or capture data artifacts"), 20, true, None, Some(HelpTopic::State)),
    shadowable("workspace", Some("Run workspace command surfaces"), 23, true, None, Some(HelpTopic::Workspace)),
    shadowable("gateway", Some("Run internal gateway service surfaces"), 14, true, None, Some(HelpTopic::Gateway)),
    shadowable("service", Some("Run typed service command surfaces"), 19, true, None, Some(HelpTopic::Service)),
    shadowable("rhai", None, usize::MAX, true, None, Some(HelpTopic::Rhai)),
    shadowable("skill", None, usize::MAX, true, None, Some(HelpTopic::Skill)),
    shadowable("docs", Some("Run documentation checks and related QA surfaces"), 12, true, None, Some(HelpTopic::Docs)),
    shadowable("contracts", Some("Validate JSON contracts and print selection sets"), 5, true, None, Some(HelpTopic::Contracts)),
    shadowable("container", Some("Operate manifest-defined local container environments"), 4, true, None, Some(HelpTopic::Container)),
    shadowable("bootstrap", Some("Download and start a project: clone or use this checkout, prepare pinned sources and submodules, run its setup selectors, then its start selector"), 0, true, None, Some(HelpTopic::Bootstrap)),
    shadowable("release", Some("Inspect, gate, prepare, execute, and verify releases"), 16, true, None, Some(HelpTopic::Release)),
    shadowable("doctor", Some("Built-in remedial health checks for environment, manifests, and task references"), 10, true, None, Some(HelpTopic::Doctor)),
    shadowable("tasks", Some("List effective catalogs and available tasks"), 21, true, None, Some(HelpTopic::Tasks)),
    shadowable("run", None, usize::MAX, true, None, None),
    shadowable("runs", None, usize::MAX, true, None, Some(HelpTopic::Runs)),
    shadowable("uninstall", None, usize::MAX, true, None, Some(HelpTopic::Uninstall)),
    // Reached through ordinary task routing, so a declared task wins without
    // any parse-time deferral.
    shadowable("config", Some("Show supported project effigy.toml configuration keys/examples and machine-level config helpers"), 3, false, None, None),
    shadowable("init", Some("Initialize baseline effigy.toml scaffold with dry-run/force controls"), 15, false, None, Some(HelpTopic::Init)),
    shadowable("scan", Some("Run the built-in repository scanners `god-files` and `duplicate-blocks`"), 17, false, None, None),
    // Reserved: contract 038 owns `test`; `help` and `version` are how an
    // operator asks Effigy about itself.
    reserved("test", Some("Built-in test runner detection, supports <catalog>/test fallback, optional --plan"), 22, HelpTopic::Test),
    reserved("help", Some("Show general help (same as --help)"), 9, HelpTopic::General),
    reserved("version", None, usize::MAX, HelpTopic::General),
];

/// The classification of a built-in top-level command name.
pub fn builtin_command_name(name: &str) -> Option<&'static BuiltinCommandName> {
    BUILTIN_COMMAND_NAMES
        .iter()
        .find(|entry| entry.name == name)
}

/// Whether a repository task named `name` wins over the built-in at parse time.
///
/// Only words the parser would otherwise claim need this; every other
/// shadowable built-in is already resolved through task routing.
pub fn is_deferred_builtin_command(name: &str) -> bool {
    builtin_command_name(name).is_some_and(|entry| {
        entry.authority == BuiltinNameAuthority::Shadowable && entry.deferred_at_parse
    })
}

/// Every live built-in top-level command, in the order the table declares.
///
/// The single owner of "which commands exist" for completion menus and
/// candidate discovery: a retired word keeps its classification but is not
/// offered as a command to run.
pub fn live_builtin_command_names() -> impl Iterator<Item = &'static str> {
    BUILTIN_COMMAND_NAMES
        .iter()
        .filter(|entry| !entry.retired)
        .map(|entry| entry.name)
}

/// Every built-in top-level command a repository task may shadow.
pub fn shadowable_builtin_names() -> impl Iterator<Item = &'static str> {
    BUILTIN_COMMAND_NAMES
        .iter()
        .filter(|entry| entry.authority == BuiltinNameAuthority::Shadowable)
        .map(|entry| entry.name)
}

/// The explicit spelling that reaches `name`'s built-in after a repository task
/// has taken the word.
pub fn builtin_escape_spelling(name: &str) -> Option<&'static str> {
    builtin_command_name(name).and_then(|entry| entry.escape)
}

/// The public task listing, in its established display order.
pub fn builtin_tasks() -> impl Iterator<Item = (&'static str, &'static str)> {
    let mut entries: Vec<_> = BUILTIN_COMMAND_NAMES
        .iter()
        .filter_map(|entry| {
            entry
                .description
                .map(|description| (entry.listing_order, entry.name, description))
        })
        .collect();
    entries.sort_by_key(|entry| entry.0);
    entries
        .into_iter()
        .map(|(_, name, description)| (name, description))
}

pub fn is_builtin_task_name(name: &str) -> bool {
    builtin_command_name(name).is_some_and(|entry| entry.description.is_some())
}
