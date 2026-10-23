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
}

const fn shadowable(
    name: &'static str,
    deferred_at_parse: bool,
    escape: Option<&'static str>,
) -> BuiltinCommandName {
    BuiltinCommandName {
        name,
        authority: BuiltinNameAuthority::Shadowable,
        deferred_at_parse,
        escape,
    }
}

const fn reserved(name: &'static str) -> BuiltinCommandName {
    BuiltinCommandName {
        name,
        authority: BuiltinNameAuthority::Reserved,
        deferred_at_parse: false,
        escape: None,
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
    shadowable("demo", true, None),
    shadowable("distribution", false, None),
    shadowable("bundle", true, None),
    shadowable("changelog", true, None),
    shadowable("fmt", true, Some("fmt-rust")),
    // The escape spelling for a shadowed `fmt` is itself an ordinary built-in,
    // so a repository task named `fmt-rust` shadows it in turn.
    shadowable("fmt-rust", true, None),
    shadowable("deploy", true, None),
    shadowable("deps", true, None),
    shadowable("secrets", true, None),
    shadowable("defer", true, None),
    shadowable("exec", true, None),
    shadowable("state", true, None),
    shadowable("workspace", true, None),
    shadowable("gateway", true, None),
    shadowable("service", true, None),
    shadowable("rhai", true, None),
    shadowable("skill", true, None),
    shadowable("docs", true, None),
    shadowable("contracts", true, None),
    shadowable("container", true, None),
    shadowable("bootstrap", true, None),
    shadowable("release", true, None),
    shadowable("doctor", true, None),
    shadowable("tasks", true, None),
    shadowable("run", true, None),
    shadowable("runs", true, None),
    shadowable("uninstall", true, None),
    // Reached through ordinary task routing, so a declared task wins without
    // any parse-time deferral.
    shadowable("config", false, None),
    shadowable("init", false, None),
    shadowable("scan", false, None),
    // Reserved: contract 038 owns `test`; `help` and `version` are how an
    // operator asks Effigy about itself.
    reserved("test"),
    reserved("help"),
    reserved("version"),
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
