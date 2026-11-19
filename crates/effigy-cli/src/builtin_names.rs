//! Which authority owns a built-in top-level command name.
//!
//! A repository task owns a word a built-in also uses: `effigy <name>` runs the
//! task (architecture `026` item 6, contract `043`). The one deliberate
//! exception is `test`, whose orchestration a task may never shadow (contract
//! `038`). `help` and `version` are the same kind of reserved meta route: they
//! are how an operator asks Effigy about itself, so a task cannot take them.
//!
//! Every built-in top-level command is classified here so a new built-in cannot
//! join the surface without saying whether a repository task may own its word.
//! The table is not documentation: parse-time deferral derives its shadowing
//! set from it, and the drift test in `command_surface/tests.rs` fails when a
//! built-in route is missing from it.

use crate::HelpTopic;

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
    /// Typed panel accepted by `effigy help <name>`.
    pub help_topic: Option<HelpTopic>,
}

const fn shadowable(
    name: &'static str,
    deferred_at_parse: bool,
    escape: Option<&'static str>,
    help_topic: Option<HelpTopic>,
) -> BuiltinCommandName {
    BuiltinCommandName {
        name,
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
        deferred_at_parse,
        escape: None,
        retired: true,
        help_topic: None,
    }
}

const fn reserved(name: &'static str, help_topic: HelpTopic) -> BuiltinCommandName {
    BuiltinCommandName {
        name,
        authority: BuiltinNameAuthority::Reserved,
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
    shadowable("distribution", false, None, None),
    shadowable("bundle", true, None, Some(HelpTopic::Bundle)),
    shadowable("changelog", true, None, Some(HelpTopic::Changelog)),
    shadowable("fmt", true, Some("fmt-rust"), Some(HelpTopic::Fmt)),
    // The escape spelling for a shadowed `fmt` is itself an ordinary built-in,
    // so a repository task named `fmt-rust` shadows it in turn.
    shadowable("fmt-rust", true, None, Some(HelpTopic::Fmt)),
    shadowable("deploy", true, None, Some(HelpTopic::Deploy)),
    shadowable("deps", true, None, Some(HelpTopic::Deps)),
    shadowable("secrets", true, None, Some(HelpTopic::Secrets)),
    shadowable("defer", true, None, Some(HelpTopic::Defer)),
    shadowable("exec", true, None, Some(HelpTopic::Exec)),
    shadowable("state", true, None, Some(HelpTopic::State)),
    shadowable("workspace", true, None, Some(HelpTopic::Workspace)),
    shadowable("gateway", true, None, Some(HelpTopic::Gateway)),
    shadowable("service", true, None, Some(HelpTopic::Service)),
    shadowable("rhai", true, None, Some(HelpTopic::Rhai)),
    shadowable("skill", true, None, Some(HelpTopic::Skill)),
    shadowable("docs", true, None, Some(HelpTopic::Docs)),
    shadowable("contracts", true, None, Some(HelpTopic::Contracts)),
    shadowable("container", true, None, Some(HelpTopic::Container)),
    shadowable("bootstrap", true, None, Some(HelpTopic::Bootstrap)),
    shadowable("release", true, None, Some(HelpTopic::Release)),
    shadowable("doctor", true, None, Some(HelpTopic::Doctor)),
    shadowable("tasks", true, None, Some(HelpTopic::Tasks)),
    shadowable("run", true, None, None),
    shadowable("runs", true, None, Some(HelpTopic::Runs)),
    shadowable("uninstall", true, None, Some(HelpTopic::Uninstall)),
    // Reached through ordinary task routing, so a declared task wins without
    // any parse-time deferral.
    shadowable("config", false, None, None),
    shadowable("init", false, None, Some(HelpTopic::Init)),
    shadowable("scan", false, None, None),
    // Reserved: contract 038 owns `test`; `help` and `version` are how an
    // operator asks Effigy about itself.
    reserved("test", HelpTopic::Test),
    reserved("help", HelpTopic::General),
    reserved("version", HelpTopic::General),
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
