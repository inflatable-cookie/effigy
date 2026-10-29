//! Bounded, redacted caller provenance for gateway operator events.
//!
//! One operator action appends one event to the gateway log, and that event
//! already named the invoking pid, argv, cwd and cause. This module adds the
//! bounded parent chain, the session it belongs to and the allowlisted
//! agent/task/run markers that make the caller attributable.
//!
//! Everything here is diagnostic evidence. It never authorizes, refuses or
//! delays an operator action, and a fact that cannot be read becomes a bounded
//! unknown reason rather than a guess or a whole-environment dump.
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::Path;

/// Most ancestors recorded above the invoking process. A deeper chain ends with
/// one bounded truncation reason rather than an unbounded process-table walk.
pub const CHAIN_LIMIT: usize = 8;
/// Longest text kept for one command argument, identity or reason.
pub const TEXT_LIMIT: usize = 96;
/// Most arguments kept for one process, own or ancestor.
pub const ARGV_LIMIT: usize = 32;
/// Most unknown reasons kept per event.
pub const UNKNOWN_LIMIT: usize = 8;
/// Written in place of a value that could carry a credential.
pub const REDACTED: &str = "<redacted>";

/// The only environment variables ever read, by name. A marker is never an
/// arbitrary variable and its value is never recorded until it validates.
pub const MARKERS: &[&str] = &[
    "PASEO_AGENT_ID",
    "PASEO_TASK_ID",
    "PASEO_RUN_ID",
    "PASEO_WORKSPACE_ID",
    "PASEO_HOST_ID",
    "NS_AGENT_ID",
    "NS_TASK_ID",
    "NS_RUN_ID",
    "NS_WORKSPACE_ID",
    "NS_HOST_ID",
    "EFFIGY_RUN_AGENT_ID",
    "EFFIGY_RUN_TASK_ID",
    "EFFIGY_RUN_ID",
];
/// Identity values carry one host, agent, task or run name and nothing else.
const MARKER_VALUE_LIMIT: usize = 64;

/// Which operator action an event records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatorAction {
    Up,
    Down,
    Setup,
}
impl OperatorAction {
    /// The existing event name for a stop is kept; `up` and `setup` follow it.
    pub fn event(self) -> &'static str {
        match self {
            Self::Up => "gateway_up",
            Self::Down => "gateway_stop",
            Self::Setup => "gateway_setup",
        }
    }
}

/// Read-only process facts one chain step needs. `argv` is `None` when the
/// platform refused the read, which is common for a root-owned ancestor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facts {
    pub parent: u32,
    pub session: Option<u32>,
    /// Start identity text: two processes that held one pid are distinguishable.
    pub start: String,
    pub argv: Option<Vec<String>>,
}
/// The process table, as much of it as provenance needs.
pub trait Tree {
    fn process(&self, pid: u32) -> Result<Facts, String>;
}

/// One ancestor of the invoking process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Link {
    pub pid: u32,
    pub start: String,
    pub session: Option<u32>,
    /// Redacted argument boundaries; `null` when they were unreadable.
    pub command: Option<Vec<String>>,
}
/// Bounded provenance for the process that asked for the gateway action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Caller {
    pub pid: u32,
    pub start: Option<String>,
    pub session: Option<u32>,
    pub terminal: Option<String>,
    pub parents: Vec<Link>,
    pub markers: BTreeMap<String, String>,
    pub unknown: Vec<String>,
}

/// Walk from `pid`'s parent to its session leader, bounded and redacted.
/// `terminal` is the invoking process's own controlling terminal, and `marker`
/// answers only for a name in [`MARKERS`].
pub fn caller(
    tree: &impl Tree,
    pid: u32,
    terminal: Option<String>,
    marker: impl Fn(&str) -> Option<OsString>,
) -> Caller {
    let mut unknown = Vec::new();
    let mut markers = BTreeMap::new();
    for name in MARKERS {
        let Some(value) = marker(name) else {
            continue;
        };
        match identity(&value) {
            Ok(value) => {
                markers.insert((*name).to_owned(), value);
            }
            Err(reason) => unknown.push(bounded(&format!("{name}: {reason}"), TEXT_LIMIT)),
        }
    }
    let own = tree.process(pid);
    let (start, session, mut next) = match &own {
        Ok(facts) => (Some(facts.start.clone()), facts.session, facts.parent),
        Err(reason) => {
            unknown.push(bounded(&format!("caller {pid}: {reason}"), TEXT_LIMIT));
            (None, None, 0)
        }
    };
    // A caller that leads its own session is already the chain's far end: its
    // parent belongs to a different session and is not part of this record.
    let own_leader = session == Some(pid);
    let mut parents = Vec::new();
    let mut seen = BTreeSet::from([pid]);
    while own.is_ok() && !own_leader && next != 0 {
        if parents.len() == CHAIN_LIMIT {
            unknown.push(format!(
                "parent chain truncated after {CHAIN_LIMIT} entries before its session leader"
            ));
            break;
        }
        let facts = match tree.process(next) {
            Ok(facts) => facts,
            Err(reason) => {
                unknown.push(bounded(&format!("parent {next}: {reason}"), TEXT_LIMIT));
                break;
            }
        };
        if !seen.insert(next) {
            unknown.push(format!("parent chain repeats pid {next}; stopped"));
            break;
        }
        // A pid can be reused between two reads. The start identity is the only
        // proof that every fact in this link belongs to one process; when it
        // moves, the link is unknown rather than a mixture of two processes.
        if tree
            .process(next)
            .is_ok_and(|again| again.start != facts.start)
        {
            unknown.push(format!("parent {next}: identity changed while it was read"));
            break;
        }
        let command = match &facts.argv {
            Some(argv) => Some(ancestor_command(argv)),
            None => {
                unknown.push(format!("parent {next}: arguments are unreadable"));
                None
            }
        };
        // A process that leads its own session is the chain's far end: nothing
        // above it belongs to the session that invoked the action.
        let leader = facts.session == Some(next) || facts.parent == next;
        parents.push(Link {
            pid: next,
            start: facts.start.clone(),
            session: facts.session,
            command,
        });
        if leader {
            break;
        }
        next = facts.parent;
    }
    unknown.truncate(UNKNOWN_LIMIT);
    Caller {
        pid,
        start,
        session,
        terminal: terminal.map(|path| bounded(&path, TEXT_LIMIT)),
        parents,
        markers,
        unknown,
    }
}

/// The one bounded operator event. `argv` and `cwd` are the invoking process's
/// own, and `caller` carries the chain that explains it.
pub fn event(
    action: OperatorAction,
    cause: &str,
    pid: u32,
    argv: &[String],
    cwd: Option<&Path>,
    caller: &Caller,
) -> Value {
    serde_json::json!({
        "event": action.event(),
        "action": action,
        "pid": pid,
        "argv": command(argv),
        "cwd": cwd,
        "cause": bounded(cause, TEXT_LIMIT),
        "caller": caller,
    })
}

/// The platform's process table, through the kernel's read-only identity facts.
#[derive(Debug, Default)]
pub struct Platform;
impl Tree for Platform {
    fn process(&self, pid: u32) -> Result<Facts, String> {
        use crate::ident;
        use crate::observe::Observed;
        let start = match ident::process_identity(pid) {
            Observed::Verified(identity, _) => identity.start.text(),
            Observed::Absent(_) => return Err("the process is gone".into()),
            Observed::Unknown(reason) => return Err(reason.to_string()),
        };
        let lineage = ident::process_lineage(pid).map_err(|error| error.to_string())?;
        Ok(Facts {
            parent: lineage.parent,
            session: lineage.session,
            start,
            // One unreadable command never hides the rest of the chain.
            argv: ident::process_arguments(pid).ok().map(|argv| {
                argv.into_iter()
                    .map(|argument| argument.to_string_lossy().into_owned())
                    .collect()
            }),
        })
    }
}

/// Redacted, bounded argument boundaries for the invoking process. Effigy's own
/// command grammar is known, so a recognised credential name keeps its flag and
/// loses only the value beside it.
fn command(argv: &[String]) -> Vec<String> {
    argv.iter()
        .take(ARGV_LIMIT)
        .enumerate()
        .map(|(index, argument)| {
            redact(
                argument,
                index
                    .checked_sub(1)
                    .and_then(|previous| argv.get(previous))
                    .map(String::as_str),
            )
        })
        .collect()
}

/// One argument with any credential value replaced, never the whole argv.
fn redact(argument: &str, previous: Option<&str>) -> String {
    if previous.is_some_and(secret_flag) || secret_value(argument) {
        return REDACTED.into();
    }
    if let Some(redacted) = redacted_url(argument) {
        return redacted;
    }
    // `KEY=value`: the name says whether the value is a credential.
    if let Some((key, value)) = argument.split_once('=') {
        if secret_name(key) || secret_value(value) {
            return bounded(
                &format!("{}={REDACTED}", bounded(key, TEXT_LIMIT / 2)),
                TEXT_LIMIT,
            );
        }
    }
    // `Authorization: Bearer …` in one argument, as a client sends it.
    if let Some((header, value)) = argument.split_once(':') {
        if secret_name(header) || secret_value(value.trim()) {
            return bounded(
                &format!("{}: {REDACTED}", bounded(header, TEXT_LIMIT / 2)),
                TEXT_LIMIT,
            );
        }
    }
    bounded(argument, TEXT_LIMIT)
}

/// Bounded, redacted argument boundaries for an *ancestor's* command. An
/// ancestor's command line is arbitrary third-party text, so no credential name
/// is trusted to describe its values: only recognised flag names and plain
/// word/path shapes are recorded, and anything else is redacted.
fn ancestor_command(argv: &[String]) -> Vec<String> {
    let mut rendered = Vec::new();
    let mut previous: Option<&str> = None;
    for argument in argv.iter().take(ARGV_LIMIT) {
        let recorded = match previous {
            // A shell string is a command line of its own: read it token by
            // token so `sh -c 'psql --password …'` loses that value too.
            Some(flag) if shell_flag(flag) => shell_command(argument),
            Some(flag) if flag_shaped(flag) => REDACTED.into(),
            _ => ancestor_token(argument),
        };
        rendered.push(recorded);
        previous = Some(argument);
    }
    rendered
}

/// Token-wise redaction of one shell command string.
fn shell_command(script: &str) -> String {
    let mut rendered = Vec::new();
    let mut previous: Option<&str> = None;
    for token in script.split_whitespace().take(ARGV_LIMIT) {
        let recorded = match previous {
            Some(flag) if flag_shaped(flag) => REDACTED.into(),
            _ => ancestor_token(token),
        };
        rendered.push(recorded);
        previous = Some(token);
    }
    bounded(&rendered.join(" "), TEXT_LIMIT)
}

/// One ancestor argument kept only when it is a recognised flag name or a
/// plain word/path. A flag token that is not a recognised name may carry an
/// attached value (`-pHunter2`), so the whole token is redacted.
fn ancestor_token(argument: &str) -> String {
    let argument = argument.trim();
    if flag_shaped(argument) {
        return if flag_name(argument) {
            bounded(argument, TEXT_LIMIT)
        } else {
            REDACTED.into()
        };
    }
    if word(argument) && !secret(argument) {
        return bounded(argument, TEXT_LIMIT);
    }
    REDACTED.into()
}

/// A recognised flag name: `--words` or one of the short flags above. A token
/// that carries an attached or `=`-joined value is not a flag name.
fn flag_name(argument: &str) -> bool {
    if let Some(long) = argument.strip_prefix("--") {
        return !long.is_empty()
            && long.len() <= 32
            && long
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    }
    SHORT_FLAGS.contains(&argument)
}

/// Whether a token looks like a flag, whether or not its name is recognised:
/// the argument after it is its value and is never recorded as it is.
fn flag_shaped(argument: &str) -> bool {
    argument.starts_with('-') && argument.len() > 1
}

/// A plain word or path, with no separator that could carry a value of its own.
fn word(argument: &str) -> bool {
    !argument.is_empty()
        && !argument.starts_with('-')
        && argument.len() <= MAX_TOKEN
        && argument
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '+' | '-' | '/'))
        && argument.chars().any(|c| c.is_ascii_alphanumeric())
}

/// A value that is a credential however it is spelled.
fn secret(value: &str) -> bool {
    secret_value(value) || opaque(value)
}

/// A long unseparated run of letters and digits is a token, not a word.
fn opaque(value: &str) -> bool {
    value.len() >= 24
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/'))
        && value.chars().any(|c| c.is_ascii_digit())
        && value.chars().any(|c| c.is_ascii_alphabetic())
}

fn shell_flag(argument: &str) -> bool {
    SHELL_FLAGS.contains(&argument.trim())
}

/// `scheme://user:password@host` keeps its shape and loses the userinfo.
fn redacted_url(argument: &str) -> Option<String> {
    let (scheme, rest) = argument.split_once("://")?;
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let (_, host) = authority.rsplit_once('@')?;
    Some(bounded(
        &format!("{scheme}://{REDACTED}@{host}/{path}"),
        TEXT_LIMIT,
    ))
}

/// Names after `--` or before `=` that mark the following value as a credential.
const SECRET_NAMES: &[&str] = &[
    "TOKEN",
    "SECRET",
    "PASSWORD",
    "PASSWD",
    "APIKEY",
    "API_KEY",
    "ACCESS_KEY",
    "PRIVATE_KEY",
    "CREDENTIAL",
    "COOKIE",
    "AUTHORIZATION",
    "AUTH_TOKEN",
    "SIGNATURE",
    "BEARER",
];
/// Flags whose *next* argument is the credential.
const SECRET_FLAGS: &[&str] = &[
    "--token",
    "--password",
    "--passwd",
    "--secret",
    "--api-key",
    "--apikey",
    "--access-key",
    "--private-key",
    "--auth",
    "--authorization",
    "--bearer",
];
/// Flags that carry a command line as their value; it is read token by token.
const SHELL_FLAGS: &[&str] = &["-c", "-lc", "-ec", "-ic", "--command"];
/// Short flag names recorded as they are. A short token that is not one of
/// these may carry an attached value (`-pHunter2`) and is redacted whole.
const SHORT_FLAGS: &[&str] = &[
    "-c", "-lc", "-ec", "-ic", "-il", "-li", "-i", "-l", "-e", "-x", "-n", "-v", "-q", "-h", "-s",
    "-u", "-p", "-a", "-d", "-f", "-o", "-H", "-m", "-r", "-t", "-w", "-k", "-j",
];
/// Longest token shape considered at all; the recorded text is bounded apart.
const MAX_TOKEN: usize = 4096;
/// Shapes that are a credential whatever their name says.
const SECRET_PREFIXES: &[&str] = &[
    "sk-",
    "sk_live_",
    "sk_test_",
    "rk_live_",
    "ghp_",
    "gho_",
    "ghs_",
    "ghu_",
    "github_pat_",
    "glpat-",
    "xoxa-",
    "xoxb-",
    "xoxp-",
    "xoxr-",
    "xoxs-",
    "npm_",
    "pypi-",
    "AIza",
    "AKIA",
    "ASIA",
    "eyJ",
    "Bearer ",
    "Basic ",
];

fn secret_flag(argument: &str) -> bool {
    SECRET_FLAGS
        .iter()
        .any(|flag| argument.trim().eq_ignore_ascii_case(flag))
}

fn secret_name(name: &str) -> bool {
    let name = name
        .trim_start_matches('-')
        .replace('-', "_")
        .to_ascii_uppercase();
    SECRET_NAMES.iter().any(|marker| name.contains(marker))
}

fn secret_value(value: &str) -> bool {
    let value = value.trim();
    SECRET_PREFIXES
        .iter()
        .any(|prefix| value.starts_with(prefix))
}

/// A marker value is one bounded identity, never a path, a URL or a blob.
fn identity(value: &OsString) -> Result<String, &'static str> {
    let Some(value) = value.to_str() else {
        return Err("value is not UTF-8");
    };
    if value.is_empty() {
        return Err("value is empty");
    }
    if value.len() > MARKER_VALUE_LIMIT {
        return Err("value is too long for an identity");
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'))
        || !value.chars().any(|c| c.is_ascii_alphanumeric())
    {
        return Err("value is not an identity");
    }
    if secret_value(value) {
        return Err("value is secret-shaped");
    }
    Ok(value.to_owned())
}

/// Keep one diagnostic text field inside its bound at a character boundary.
fn bounded(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        return value.to_owned();
    }
    let mut kept: String = value.chars().take(limit.saturating_sub(1)).collect();
    kept.push('…');
    kept
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// A process table the tests state outright: a pid answers from a stated
    /// sequence (the last answer repeats), and an absent pid is unreadable.
    #[derive(Default)]
    struct Fake {
        answers: BTreeMap<u32, Vec<Result<Facts, String>>>,
        reads: std::cell::RefCell<BTreeMap<u32, usize>>,
    }
    impl Fake {
        fn facts(
            mut self,
            pid: u32,
            parent: u32,
            session: u32,
            start: &str,
            argv: &[&str],
        ) -> Self {
            self.answers.insert(
                pid,
                vec![Ok(Facts {
                    parent,
                    session: Some(session),
                    start: start.into(),
                    argv: Some(argv.iter().map(|arg| (*arg).to_owned()).collect()),
                })],
            );
            self
        }
        fn unreadable(mut self, pid: u32, reason: &str) -> Self {
            self.answers.insert(pid, vec![Err(reason.into())]);
            self
        }
        /// The same pid answering differently on successive reads: a reused pid.
        fn reused(mut self, pid: u32, first: &str, second: &str) -> Self {
            let answer = |start: &str| {
                Ok(Facts {
                    parent: 1,
                    session: Some(pid),
                    start: start.into(),
                    argv: Some(vec!["/bin/sh".into()]),
                })
            };
            self.answers
                .insert(pid, vec![answer(first), answer(second)]);
            self
        }
    }
    impl Tree for Fake {
        fn process(&self, pid: u32) -> Result<Facts, String> {
            let Some(answers) = self.answers.get(&pid) else {
                return Err("the process is gone".into());
            };
            let mut reads = self.reads.borrow_mut();
            let read = reads.entry(pid).or_insert(0);
            let answer = answers
                .get(*read)
                .or_else(|| answers.last())
                .cloned()
                .unwrap_or_else(|| Err("the process is gone".into()));
            *read += 1;
            answer
        }
    }
    /// The fake answers repeated reads from its next entry, so a chain walk that
    /// re-reads an identity sees the reuse.
    fn caller_of(fake: &Fake, pid: u32) -> Caller {
        caller(fake, pid, None, |_| None)
    }
    fn markers(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        let mut map = BTreeMap::new();
        for (name, value) in pairs {
            map.insert((*name).to_owned(), (*value).to_owned());
        }
        map
    }

    /// Every operator event keeps the existing fields and names its action, so
    /// a stop stays `gateway_stop` and `up`/`setup` are attributable the same way.
    #[test]
    fn gateway_operator_events_keep_their_fields_and_share_one_caller_record() {
        let fake =
            Fake::default().facts(50, 1, 50, "macos:1.000001", &["effigy", "gateway", "down"]);
        let record = caller_of(&fake, 50);
        // This caller leads its own session, so nothing above it belongs to the
        // chain and no outer pid is read at all.
        assert_eq!(
            (record.start.as_deref(), record.parents.len()),
            (Some("macos:1.000001"), 0)
        );
        assert!(record.unknown.is_empty(), "{:?}", record.unknown);
        for (action, name) in [
            (OperatorAction::Down, "gateway_stop"),
            (OperatorAction::Up, "gateway_up"),
            (OperatorAction::Setup, "gateway_setup"),
        ] {
            let event = event(
                action,
                "gateway command",
                50,
                &["/opt/effigy".into(), "gateway".into(), "down".into()],
                Some(Path::new("/src/effigy")),
                &caller_of(&Fake::default().facts(50, 1, 50, "macos:1.000001", &[]), 50),
            );
            assert_eq!(event["event"], name);
            assert_eq!(event["cause"], "gateway command");
            assert_eq!(event["pid"], 50);
            assert_eq!(event["cwd"], "/src/effigy");
            assert_eq!(event["argv"][1], "gateway");
            assert_eq!(event["caller"]["pid"], 50);
            assert!(event["caller"]["parents"].is_array(), "{event}");
        }
    }

    /// The chain stops at the session leader and records every parent between.
    #[test]
    fn gateway_caller_chain_walks_to_the_session_leader_and_stops() {
        let fake = Fake::default()
            .facts(50, 40, 30, "macos:1.000050", &["effigy", "gateway", "up"])
            .facts(
                40,
                30,
                30,
                "macos:1.000040",
                &["zsh", "-c", "effigy gateway up"],
            )
            .facts(30, 10, 30, "macos:1.000030", &["/bin/zsh"])
            // Above the leader: never read, even though it would answer.
            .facts(10, 1, 10, "macos:1.000010", &["/bin/other"]);
        let record = caller_of(&fake, 50);
        assert_eq!(
            record
                .parents
                .iter()
                .map(|link| link.pid)
                .collect::<Vec<_>>(),
            [40, 30]
        );
        assert_eq!(
            record.parents[1].command.as_deref(),
            Some(&["/bin/zsh".into()][..])
        );
        assert_eq!(record.session, Some(30));
        assert!(record.unknown.is_empty(), "{:?}", record.unknown);
    }

    /// A deeper chain ends with one bounded truncation reason.
    #[test]
    fn gateway_caller_chain_truncates_at_its_bound() {
        let mut fake = Fake::default().facts(100, 99, 1, "macos:1.000100", &[]);
        for pid in (2_u32..=99).rev() {
            fake = fake.facts(pid, pid - 1, 1, &format!("macos:1.{pid:06}"), &[]);
        }
        fake = fake.facts(1, 0, 1, "macos:1.000001", &[]);
        let record = caller_of(&fake, 100);
        assert_eq!(record.parents.len(), CHAIN_LIMIT);
        assert!(
            record
                .unknown
                .iter()
                .any(|reason| reason.contains("truncated")),
            "{:?}",
            record.unknown
        );
        assert!(record.parents.iter().all(|link| link.pid > 1));
    }

    /// An absent, unreadable or reused parent is a bounded reason, never a guess
    /// and never a shorter chain that looks complete.
    #[test]
    fn gateway_caller_chain_records_unknown_parents_and_pid_reuse() {
        let absent = caller_of(
            &Fake::default().facts(50, 40, 40, "macos:1.000050", &[]),
            50,
        );
        assert!(absent.parents.is_empty());
        assert!(
            absent
                .unknown
                .iter()
                .any(|reason| reason.contains("parent 40")),
            "{:?}",
            absent.unknown
        );

        let unreadable = caller_of(
            &Fake::default()
                .facts(50, 40, 30, "macos:1.000050", &[])
                .facts(40, 30, 30, "macos:1.000040", &[])
                .unreadable(30, "permission denied"),
            50,
        );
        assert_eq!(unreadable.parents.len(), 1);
        assert!(
            unreadable
                .unknown
                .iter()
                .any(|reason| reason.contains("parent 30: permission denied")),
            "{:?}",
            unreadable.unknown
        );

        // The pid answered twice with two start identities: it was reused while
        // it was read, so no link claims it.
        let reused = caller_of(
            &Fake::default()
                .facts(50, 40, 40, "macos:1.000050", &[])
                .reused(40, "macos:1.000040", "macos:2.000040"),
            50,
        );
        assert!(reused.parents.is_empty(), "{reused:?}");
        assert!(
            reused
                .unknown
                .iter()
                .any(|reason| reason.contains("parent 40") && reason.contains("identity changed")),
            "{:?}",
            reused.unknown
        );

        // Two chains that saw different processes at one pid are distinguishable
        // by that identity alone.
        let before = caller_of(
            &Fake::default().facts(50, 40, 40, "macos:1.000050", &[]),
            50,
        );
        let after = caller_of(
            &Fake::default().facts(50, 40, 40, "macos:2.000050", &[]),
            50,
        );
        assert_ne!(before.start, after.start);
    }

    /// An unreadable ancestor command keeps the link and says the argv is unknown.
    #[test]
    fn gateway_caller_chain_keeps_a_link_with_unreadable_arguments() {
        let mut fake = Fake::default()
            .facts(50, 1, 40, "macos:1.000050", &[])
            .facts(1, 0, 1, "macos:1.000001", &[]);
        fake.answers.insert(
            1,
            vec![Ok(Facts {
                parent: 0,
                session: Some(1),
                start: "macos:1.000001".into(),
                argv: None,
            })],
        );
        let record = caller_of(&fake, 50);
        assert_eq!(record.parents.len(), 1);
        assert_eq!(record.parents[0].command, None);
        assert!(
            record
                .unknown
                .iter()
                .any(|reason| reason.contains("arguments are unreadable")),
            "{:?}",
            record.unknown
        );
    }

    /// Only allowlisted markers are read, and only a valid identity is kept.
    #[test]
    fn gateway_caller_markers_are_allowlisted_and_validated() {
        let fake = Fake::default().facts(50, 1, 50, "macos:1.000050", &[]);
        let values = [
            ("PASEO_AGENT_ID", "agent-7"),
            ("NS_RUN_ID", "run:2026-10-28"),
            ("EFFIGY_RUN_TASK_ID", "effigy_450"),
            // Presence is read by name, so an unlisted name is never consulted.
            ("PASEO_HOME", "/Users/someone/.paseo"),
            ("OPENAI_API_KEY", "sk-not-a-marker"),
            ("NS_TOKEN", "not-read"),
        ];
        let record = caller(&fake, 50, None, |name| {
            values
                .iter()
                .find(|(candidate, _)| *candidate == name)
                .map(|(_, value)| OsString::from(*value))
        });
        assert_eq!(
            record.markers,
            markers(&[
                ("PASEO_AGENT_ID", "agent-7"),
                ("NS_RUN_ID", "run:2026-10-28"),
                ("EFFIGY_RUN_TASK_ID", "effigy_450"),
            ])
        );
        assert!(record.unknown.is_empty(), "{:?}", record.unknown);

        let malformed = caller(&fake, 50, None, |name| match name {
            "NS_RUN_ID" => Some(OsString::from("run id with spaces")),
            "PASEO_TASK_ID" => Some(OsString::from("x".repeat(MARKER_VALUE_LIMIT + 1))),
            "EFFIGY_RUN_ID" => Some(OsString::from("sk_live_0123456789")),
            _ => None,
        });
        assert!(malformed.markers.is_empty(), "{:?}", malformed.markers);
        assert_eq!(malformed.unknown.len(), 3);
        assert!(
            malformed
                .unknown
                .iter()
                .any(|reason| reason == "NS_RUN_ID: value is not an identity"),
            "{:?}",
            malformed.unknown
        );
        assert!(
            malformed
                .unknown
                .iter()
                .all(|reason| !reason.contains("spaces")),
            "{:?}",
            malformed.unknown
        );
    }

    /// No secret survives in an argument, a marker-like variable or an
    /// unlisted environment value.
    #[test]
    fn gateway_caller_record_never_exposes_a_secret() {
        const SECRET: &str = "hunter2-not-a-real-secret";
        let fake = Fake::default()
            .facts(50, 40, 40, "macos:1.000050", &["/bin/sh", "-c", "deploy"])
            .facts(
                40,
                30,
                40,
                "macos:1.000040",
                &[
                    "effigy",
                    "--token",
                    SECRET,
                    "--api-key=abc123",
                    "https://user:hunter2@example.test/hook",
                    "Authorization: Bearer abcdef",
                    "--password",
                    "letmein",
                    "sk_live_abcdef",
                    "--author",
                    "someone",
                    "build",
                ],
            );
        let record = caller(&fake, 50, Some("/dev/ttys003".into()), |name| match name {
            // A marker-shaped credential is refused, not recorded.
            "PASEO_RUN_ID" => Some(OsString::from("Bearer not-a-run")),
            _ => None,
        });
        let parents = serde_json::to_string(&record.parents).unwrap();
        let unknown = record.unknown.join("\n");
        let rendered = serde_json::to_string(&event(
            OperatorAction::Down,
            "gateway down command",
            50,
            &[
                "effigy".into(),
                "gateway".into(),
                "down".into(),
                "--token=hunter2-not-a-real-secret".into(),
                "sk-live-abcdef".into(),
            ],
            Some(Path::new("/src/effigy")),
            &record,
        ))
        .unwrap();
        for exposed in [
            SECRET,
            "letmein",
            "abc123",
            "abcdef",
            "hunter2",
            "not-a-run",
        ] {
            assert!(!parents.contains(exposed), "{parents}");
            assert!(!rendered.contains(exposed), "{rendered}");
        }
        assert!(!unknown.contains("hunter2"), "{unknown}");
        // Attribution survives redaction: the command, flag and plain arguments stay.
        assert!(parents.contains("--token"), "{parents}");
        assert!(parents.contains("--author"), "{parents}");
        assert!(parents.contains("build"), "{parents}");
        assert!(parents.contains("<redacted>"), "{parents}");
        assert!(rendered.contains("<redacted>"), "{rendered}");
        // A plain marker and the terminal are still recorded.
        assert!(
            rendered.contains("\"terminal\":\"/dev/ttys003\""),
            "{rendered}"
        );
    }

    /// An ancestor's command is arbitrary third-party text, so only a
    /// recognised flag name and a plain word/path are recorded. A shell string
    /// is a command line of its own and loses its flag values the same way.
    #[test]
    fn gateway_ancestor_commands_lose_every_credential_shaped_value() {
        let script = Fake::default()
            .facts(50, 40, 40, "macos:1.000050", &["effigy"])
            .facts(
                40,
                30,
                40,
                "macos:1.000040",
                &["/bin/zsh", "-lc", "psql --password hunter2 --host db"],
            );
        let parent = caller_of(&script, 50).parents[0].command.clone().unwrap();
        let rendered = parent.join(" ");
        for exposed in ["hunter2", " db"] {
            assert!(!rendered.contains(exposed), "{rendered}");
        }
        assert_eq!(parent[0], "/bin/zsh", "{rendered}");
        assert_eq!(parent[1], "-lc", "{rendered}");
        for kept in ["psql", "--password", "--host"] {
            assert!(rendered.contains(kept), "{kept} missing from {rendered}");
        }
        assert!(rendered.contains("<redacted>"), "{rendered}");

        // A short flag with an attached value, URL userinfo and a value after a
        // flag are all redacted; the flag names and the program stay.
        let plain = Fake::default()
            .facts(50, 40, 40, "macos:1.000050", &["effigy"])
            .facts(
                40,
                30,
                40,
                "macos:1.000040",
                &[
                    "curl",
                    "-u",
                    "admin:hunter2",
                    "https://api.test/?token=abc123",
                    "--data",
                    r#"{"password":"hunter2"}"#,
                    "https://user:pass@example.test/hook",
                    "-pS3cret",
                ],
            );
        let parent = caller_of(&plain, 50).parents[0].command.clone().unwrap();
        let rendered = parent.join(" ");
        for exposed in ["hunter2", "abc123", "admin", "S3cret", "pass@", "?token"] {
            assert!(!rendered.contains(exposed), "{rendered}");
        }
        for kept in ["curl", "-u", "--data"] {
            assert!(rendered.contains(kept), "{kept} missing from {rendered}");
        }
    }

    /// A path and a separated identity stay attributable; a long unseparated
    /// token does not.
    #[test]
    fn gateway_ancestor_tokens_keep_only_a_plain_command_shape() {
        let token = "a8f5f167f44f4964e6c998dee827110cf2d4c1f7";
        let fake = Fake::default()
            .facts(50, 40, 40, "macos:1.000050", &[])
            .facts(
                40,
                30,
                40,
                "macos:1.000040",
                &[
                    "node",
                    "/opt/paseo/bin/paseo",
                    "run",
                    "f593c9f2-075b-4243-9e1b-176d303ec799",
                    token,
                    "-m",
                    "wip",
                ],
            );
        let parent = caller_of(&fake, 50).parents[0].command.clone().unwrap();
        let rendered = parent.join(" ");
        for exposed in [token, "wip"] {
            assert!(!rendered.contains(exposed), "{rendered}");
        }
        for kept in [
            "node",
            "/opt/paseo/bin/paseo",
            "run",
            "f593c9f2-075b-4243-9e1b-176d303ec799",
            "-m",
        ] {
            assert!(rendered.contains(kept), "{kept} missing from {rendered}");
        }
    }

    /// Long values are bounded, and the bound itself is visible.
    #[test]
    fn gateway_caller_record_is_bounded() {
        let long = "a".repeat(TEXT_LIMIT * 3);
        let fake = Fake::default()
            .facts(50, 40, 30, "macos:1.000050", &[])
            .facts(40, 30, 30, "macos:1.000040", &[long.as_str()]);
        let record = caller(&fake, 50, Some(long.clone()), |_| None);
        let rendered = serde_json::to_string(&record).unwrap();
        assert!(!rendered.contains(&long), "{rendered}");
        let argument = record.parents[0].command.as_ref().unwrap()[0].clone();
        assert_eq!(argument.chars().count(), TEXT_LIMIT);
        assert!(argument.ends_with('…'));
        assert_eq!(
            record
                .terminal
                .as_deref()
                .map(str::chars)
                .map(Iterator::count),
            Some(TEXT_LIMIT)
        );
        assert!(record.unknown.len() <= UNKNOWN_LIMIT);
    }
}
