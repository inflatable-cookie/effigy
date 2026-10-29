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
    /// The executable path the operating system reports for the process, never
    /// its `argv[0]`, which `execve` lets a caller choose.
    pub executable: Option<String>,
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
    // Marker and chain failures are collected apart: the chain explains a
    // missing or short record, so it keeps its place in a bounded list.
    let mut marker_failures = Vec::new();
    let mut chain_failures = Vec::new();
    let mut markers = BTreeMap::new();
    for name in MARKERS {
        let Some(value) = marker(name) else {
            continue;
        };
        match identity(name, &value) {
            Ok(form) => {
                markers.insert((*name).to_owned(), fingerprint(form, &value));
            }
            Err(reason) => marker_failures.push(bounded(&format!("{name}: {reason}"), TEXT_LIMIT)),
        }
    }
    let own = tree.process(pid);
    let (start, session, mut next) = match &own {
        Ok(facts) => (Some(facts.start.clone()), facts.session, facts.parent),
        Err(reason) => {
            chain_failures.push(bounded(&format!("caller {pid}: {reason}"), TEXT_LIMIT));
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
            chain_failures.push(format!(
                "parent chain truncated after {CHAIN_LIMIT} entries before its session leader"
            ));
            break;
        }
        let facts = match tree.process(next) {
            Ok(facts) => facts,
            Err(reason) => {
                chain_failures.push(bounded(&format!("parent {next}: {reason}"), TEXT_LIMIT));
                break;
            }
        };
        if !seen.insert(next) {
            chain_failures.push(format!("parent chain repeats pid {next}; stopped"));
            break;
        }
        // A pid can be reused between two reads. The start identity is the only
        // proof that every fact in this link belongs to one process: a second
        // read must succeed and agree, or the link would mix one process's start
        // identity with another's command and ancestry.
        match tree.process(next) {
            Ok(again) if again.start == facts.start => {}
            Ok(_) => {
                chain_failures.push(format!("parent {next}: identity changed while it was read"));
                break;
            }
            Err(reason) => {
                chain_failures.push(bounded(
                    &format!("parent {next}: identity is unconfirmed ({reason})"),
                    TEXT_LIMIT,
                ));
                break;
            }
        }
        // The chain is the invoking session's chain. A parent in another session
        // means the walk has already left it — a reparented child, or a leader we
        // cannot reach — so name that instead of attributing another session.
        if let (Some(invoking), Some(parent_session)) = (session, facts.session) {
            if parent_session != invoking {
                chain_failures.push(bounded(
                    &format!(
                        "parent {next}: session {parent_session} is outside the invoking \
                         session {invoking}"
                    ),
                    TEXT_LIMIT,
                ));
                break;
            }
        }
        let command = match &facts.argv {
            Some(argv) => Some(ancestor_command(argv, facts.executable.as_deref())),
            None => {
                chain_failures.push(format!("parent {next}: arguments are unreadable"));
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
    Caller {
        pid,
        start,
        session,
        terminal: terminal.map(|path| bounded(&path, TEXT_LIMIT)),
        parents,
        markers,
        unknown: bounded_reasons(chain_failures, marker_failures),
    }
}

/// The one bounded operator event. `argv` is the invoking process's own command
/// line, published through the same vocabulary as an ancestor's, and `cwd` is
/// its directory; `caller` carries the chain that explains it.
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
        "argv": command_text(argv, argv.first().map(String::as_str), true),
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
            executable: ident::process_executable(pid)
                .ok()
                .map(|path| path.to_string_lossy().into_owned()),
            // One unreadable command never hides the rest of the chain.
            argv: ident::process_arguments(pid).ok().map(|argv| {
                argv.into_iter()
                    .map(|argument| argument.to_string_lossy().into_owned())
                    .collect()
            }),
        })
    }
}

/// The program names an event may publish: Effigy itself and the launchers,
/// runtimes and tools an operator or agent wraps it in. Nothing else about a
/// command line is a command word — a positional token in an arbitrary program
/// or shell script can be a value, so a known spelling never establishes its
/// role (effigy#450 review). Only the program slot uses this list.
const PROGRAM_NAMES: &[&str] = &[
    "effigy",
    "sh",
    "bash",
    "zsh",
    "dash",
    "fish",
    "env",
    "exec",
    "command",
    "sudo",
    "doas",
    "nice",
    "timeout",
    "xargs",
    "open",
    "launchctl",
    "systemctl",
    "osascript",
    "node",
    "bun",
    "deno",
    "python",
    "python3",
    "uv",
    "cargo",
    "git",
    "make",
    "just",
    "paseo",
    "claude",
];
/// Flags and short options Effigy defines or that only select a behaviour. A
/// flag's value is never published: only its name can be.
const FLAGS: &[&str] = &[
    "--json",
    "--help",
    "--version",
    "--verbose",
    "--quiet",
    "--force",
    "--yes",
    "--all",
    "--plan",
    "--detach",
    "--private",
    "--suffix",
    "--private-state-root",
    "--dns-addr",
    "--proxy-addr",
    "--https-addr",
    "--listener-handoff",
    "--repo",
    "--lock-wait-ms",
    "--env-schema",
    "-h",
    "-v",
    "-q",
    "-y",
];
/// The flags above that never take a value. Every other published flag is
/// treated as taking one, so the token after it is never published.
const BOOLEAN_FLAGS: &[&str] = &[
    "--json",
    "--help",
    "--version",
    "--verbose",
    "--quiet",
    "--force",
    "--yes",
    "--all",
    "--plan",
    "--detach",
    "--private",
    "--listener-handoff",
    "-h",
    "-v",
    "-q",
    "-y",
];
/// Flags whose value is a command line of its own; that value is read token by
/// token so the words inside it are judged the same way.
const SHELL_FLAGS: &[&str] = &["-c", "-lc", "-ec", "-ic", "--command"];

/// One bounded command line, published by role: the program's file name and one
/// of Effigy's own flag names. No positional token is ever published — an
/// arbitrary program or shell script can put a password in any position, and a
/// known spelling proves nothing about a token's role — and neither is a flag's
/// value, in `--flag value` or `--flag=value` form.
fn command_text(argv: &[String], program: Option<&str>, trusted: bool) -> Vec<String> {
    let mut rendered = Vec::new();
    let mut previous: Option<&str> = None;
    for (index, argument) in argv.iter().take(ARGV_LIMIT).enumerate() {
        let recorded = if index == 0 {
            program.map_or_else(|| REDACTED.into(), program_text)
        } else if !trusted {
            // Another program's options are its own: Effigy's flag vocabulary
            // says nothing about what `--json` means here, and a token that
            // merely matches one can be that program's value.
            REDACTED.into()
        } else {
            match previous {
                Some(flag) if shell_flag(flag) => shell_text(argument),
                Some(flag) if flag_shaped(flag) && !takes_no_value(flag) => REDACTED.into(),
                _ => positional(argument),
            }
        };
        rendered.push(recorded);
        previous = Some(argument);
    }
    rendered
}

/// An ancestor's command line. Its program slot names the executable the
/// operating system reports rather than the caller-chosen `argv[0]`.
fn ancestor_command(argv: &[String], executable: Option<&str>) -> Vec<String> {
    command_text(argv, executable, false)
}

/// The file name of the program, published only when it is a word this record
/// may publish. A program name is caller-controlled text like any other.
fn program_text(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path).trim();
    if PROGRAM_NAMES.contains(&name) {
        return bounded(name, TEXT_LIMIT);
    }
    REDACTED.into()
}

/// A shell command string is a command line of its own: the same role rules
/// apply token by token, no program slot is claimed for it, and no positional
/// token in it is published.
fn shell_text(script: &str) -> String {
    let rendered = script
        .split_whitespace()
        .take(ARGV_LIMIT)
        .map(|_| REDACTED)
        .collect::<Vec<_>>()
        .join(" ");
    bounded(&rendered, TEXT_LIMIT)
}

/// The published form of one token that is not itself in a value position: one
/// of Effigy's own flag names, or the name half of a `--flag=value` pair whose
/// value is then suppressed. Everything else — including a positional token
/// whose spelling is a command word — is a placeholder, because in an arbitrary
/// program or shell script only the flag position is ours to trust.
fn positional(argument: &str) -> String {
    let argument = argument.trim();
    if let Some((name, _)) = argument.split_once('=') {
        return if published_flag(name) {
            bounded(&format!("{name}={REDACTED}"), TEXT_LIMIT)
        } else {
            REDACTED.into()
        };
    }
    if published_flag(argument) {
        return bounded(argument, TEXT_LIMIT);
    }
    REDACTED.into()
}

fn published_flag(argument: &str) -> bool {
    FLAGS.contains(&argument) || SHELL_FLAGS.contains(&argument)
}

/// A flag that never takes a value leaves the next token in command position;
/// any other flag (and any flag this record does not know) owns it.
fn takes_no_value(argument: &str) -> bool {
    BOOLEAN_FLAGS.contains(&argument.trim())
}

fn flag_shaped(argument: &str) -> bool {
    argument.starts_with('-') && argument.len() > 1
}

fn shell_flag(argument: &str) -> bool {
    SHELL_FLAGS.contains(&argument.trim())
}

/// A marker value must be one of the identity forms Effigy and its agents
/// actually use, so arbitrary text — a password pasted into a marker variable —
/// is never recorded as an identity (effigy#450 review). The reason never quotes
/// the rejected value.
fn identity(name: &str, value: &OsString) -> Result<IdentityForm, &'static str> {
    let Some(value) = value.to_str() else {
        return Err("value is not UTF-8");
    };
    if value.is_empty() {
        return Err("value is empty");
    }
    if value.len() > MARKER_VALUE_LIMIT {
        return Err("value is too long for an identity");
    }
    if uuid(value) {
        return Ok(IdentityForm::Uuid);
    }
    if decimal(value) {
        return Ok(IdentityForm::Decimal);
    }
    // Effigy's own run id is a 26-character base32 encoding of 16 bytes.
    if name.starts_with("EFFIGY_RUN_") && effigy_run_id(value) {
        return Ok(IdentityForm::Run);
    }
    Err("value is not a supported identity form")
}

/// The recognised shapes a marker value can have. Only the shape is recorded;
/// the value itself never is, because an ID-shaped value can still be a secret
/// (effigy#450 review).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IdentityForm {
    Uuid,
    Decimal,
    Run,
}
impl IdentityForm {
    fn name(self) -> &'static str {
        match self {
            Self::Uuid => "uuid",
            Self::Decimal => "decimal",
            Self::Run => "run",
        }
    }
}

/// A marker is recorded as its recognised form and a one-way fingerprint. Two
/// events from the same agent, task or run carry the same fingerprint, which is
/// the attribution an operator needs, and no raw marker value is ever written.
fn fingerprint(form: IdentityForm, value: &OsString) -> String {
    use sha2::{Digest as _, Sha256};
    let digest = Sha256::digest(value.to_string_lossy().as_bytes());
    let hex = digest
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("{}:{hex}", form.name())
}

/// `8-4-4-4-12` hexadecimal, the form Paseo and Queue identity values use.
fn uuid(value: &str) -> bool {
    let groups = [8, 4, 4, 4, 12];
    if value.len() != 36 {
        return false;
    }
    let mut parts = value.split('-');
    groups.iter().all(|length| {
        parts.next().is_some_and(|group| {
            group.len() == *length && group.chars().all(|c| c.is_ascii_hexdigit())
        })
    }) && parts.next().is_none()
}

/// A small decimal identifier such as a process or port ordinal.
fn decimal(value: &str) -> bool {
    !value.is_empty() && value.len() <= 20 && value.chars().all(|c| c.is_ascii_digit())
}

/// The kernel's run identity encoding: 26 characters of `a-z2-7`.
fn effigy_run_id(value: &str) -> bool {
    value.len() == 26
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || matches!(c, '2'..='7'))
}

/// A bounded reason list that always says when it was cut: chain failures come
/// first, because they explain a missing or unconfirmed part of the record, and
/// the last slot names how many reasons were omitted.
fn bounded_reasons(chain: Vec<String>, markers: Vec<String>) -> Vec<String> {
    let total = chain.len() + markers.len();
    let mut reasons = chain
        .into_iter()
        .chain(markers)
        .take(UNKNOWN_LIMIT)
        .collect::<Vec<_>>();
    if total > UNKNOWN_LIMIT {
        reasons.truncate(UNKNOWN_LIMIT - 1);
        reasons.push(format!(
            "{} further reasons omitted",
            total - (UNKNOWN_LIMIT - 1)
        ));
    }
    reasons
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
                    // The operating system reports the executable separately
                    // from the caller-chosen argument vector.
                    executable: argv.first().map(|program| (*program).to_owned()),
                    argv: Some(argv.iter().map(|arg| (*arg).to_owned()).collect()),
                })],
            );
            self
        }
        /// A process whose `argv[0]` is not its executable, as `exec -a` sets it.
        fn renamed(
            mut self,
            pid: u32,
            parent: u32,
            session: u32,
            start: &str,
            argv: &[&str],
            executable: Option<&str>,
        ) -> Self {
            self.answers.insert(
                pid,
                vec![Ok(Facts {
                    parent,
                    session: Some(session),
                    start: start.into(),
                    executable: executable.map(str::to_owned),
                    argv: Some(argv.iter().map(|arg| (*arg).to_owned()).collect()),
                })],
            );
            self
        }
        fn unreadable(mut self, pid: u32, reason: &str) -> Self {
            self.answers.insert(pid, vec![Err(reason.into())]);
            self
        }
        /// A pid whose first read succeeds and whose second read fails: it may
        /// have been replaced between its identity read and its other facts.
        fn unconfirmed(
            mut self,
            pid: u32,
            parent: u32,
            session: u32,
            start: &str,
            reason: &str,
        ) -> Self {
            self.answers.insert(
                pid,
                vec![
                    Ok(Facts {
                        parent,
                        session: Some(session),
                        start: start.into(),
                        executable: Some("/bin/zsh".into()),
                        argv: Some(vec!["/bin/zsh".into()]),
                    }),
                    Err(reason.into()),
                ],
            );
            self
        }
        /// The same pid answering differently on successive reads: a reused pid.
        fn reused(mut self, pid: u32, first: &str, second: &str) -> Self {
            let answer = |start: &str| {
                Ok(Facts {
                    parent: 1,
                    session: Some(pid),
                    start: start.into(),
                    executable: Some("/bin/sh".into()),
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
            // The program is published; positional arguments are not.
            assert_eq!(event["argv"][0], "effigy");
            assert_eq!(event["argv"][1], REDACTED);
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
            Some(&["zsh".into()][..])
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
                session: Some(40),
                start: "macos:1.000001".into(),
                executable: Some("/bin/zsh".into()),
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

    /// A parent in another session is not part of the invoking chain: a
    /// reparented child gets a bounded reason, never another session's leader.
    #[test]
    fn gateway_caller_chain_stops_at_a_parent_outside_the_session() {
        let fake = Fake::default()
            .facts(50, 1, 40, "macos:1.000050", &[])
            .facts(1, 0, 1, "macos:1.000001", &["/sbin/launchd"]);
        let record = caller_of(&fake, 50);
        assert!(record.parents.is_empty(), "{record:?}");
        assert!(
            record.unknown.iter().any(|reason| {
                reason.contains("parent 1") && reason.contains("outside the invoking session 40")
            }),
            "{:?}",
            record.unknown
        );
    }

    /// A parent whose second identity read fails is unknown: the record must not
    /// mix one process's start identity with another's command and ancestry.
    #[test]
    fn gateway_caller_chain_refuses_a_parent_whose_identity_is_unconfirmed() {
        let fake = Fake::default()
            .facts(50, 40, 30, "macos:1.000050", &[])
            .unconfirmed(40, 1, 30, "macos:1.000040", "the process is gone");
        let record = caller_of(&fake, 50);
        assert!(record.parents.is_empty(), "{record:?}");
        assert!(
            record
                .unknown
                .iter()
                .any(|reason| reason.contains("parent 40") && reason.contains("unconfirmed")),
            "{:?}",
            record.unknown
        );
    }

    /// Only allowlisted markers are read, and a marker is recorded as its
    /// recognised form and a fingerprint, never as its value: an ID-shaped value
    /// can still be a secret (effigy#450 review).
    #[test]
    fn gateway_caller_markers_are_allowlisted_validated_and_fingerprinted() {
        let fake = Fake::default().facts(50, 1, 50, "macos:1.000050", &[]);
        let agent = "3f2b8c1a-9d4e-4f6a-8b2c-7e5d1a9c0b34";
        let run = "1b3dafc2-a108-48af-8807-ec552d605e8f";
        let effigy_run = "abcdefghijklmnopqrstuvwxyz";
        let values = [
            ("PASEO_AGENT_ID", agent),
            ("NS_RUN_ID", run),
            ("EFFIGY_RUN_ID", effigy_run),
            ("NS_HOST_ID", "42"),
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
        assert_eq!(record.markers.len(), 4, "{:?}", record.markers);
        assert_eq!(
            record.markers.keys().collect::<Vec<_>>(),
            ["EFFIGY_RUN_ID", "NS_HOST_ID", "NS_RUN_ID", "PASEO_AGENT_ID"]
        );
        assert_eq!(
            record.markers["PASEO_AGENT_ID"].split(':').next(),
            Some("uuid")
        );
        assert_eq!(
            record.markers["NS_HOST_ID"].split(':').next(),
            Some("decimal")
        );
        assert_eq!(
            record.markers["EFFIGY_RUN_ID"].split(':').next(),
            Some("run")
        );
        let rendered = serde_json::to_string(&record).unwrap();
        for raw in [agent, run, effigy_run, "42"] {
            assert!(!rendered.contains(&format!("\"{raw}\"")), "{rendered}");
        }
        assert!(record.unknown.is_empty(), "{:?}", record.unknown);

        // The same value carries the same fingerprint, and a different one does
        // not: correlation survives, the value does not.
        let fingerprint = |value: &str| {
            caller(&fake, 50, None, |name| {
                (name == "PASEO_AGENT_ID").then(|| OsString::from(value))
            })
            .markers["PASEO_AGENT_ID"]
                .clone()
        };
        assert_eq!(fingerprint(agent), fingerprint(agent));
        assert_ne!(fingerprint(agent), fingerprint(run));

        // An ID-shaped secret is recorded as a fingerprint too, not as text.
        let id_shaped = caller(&fake, 50, None, |name| match name {
            "NS_RUN_ID" => Some(OsString::from("123456")),
            "PASEO_AGENT_ID" => Some(OsString::from(agent)),
            _ => None,
        });
        let rendered = serde_json::to_string(&id_shaped).unwrap();
        for raw in ["123456", agent] {
            assert!(!rendered.contains(&format!("\"{raw}\"")), "{rendered}");
        }
        assert_eq!(
            id_shaped.markers["NS_RUN_ID"].split(':').next(),
            Some("decimal")
        );

        // A syntactically plain password, a value that is too long and a
        // secret-shaped value are all refused without being quoted.
        let malformed = caller(&fake, 50, None, |name| match name {
            "PASEO_RUN_ID" => Some(OsString::from("hunter2")),
            "PASEO_TASK_ID" => Some(OsString::from("x".repeat(MARKER_VALUE_LIMIT + 1))),
            "EFFIGY_RUN_ID" => Some(OsString::from("sk_live_0123456789")),
            _ => None,
        });
        assert!(malformed.markers.is_empty(), "{:?}", malformed.markers);
        assert_eq!(malformed.unknown.len(), 3);
        assert!(
            malformed.unknown.iter().all(|reason| reason
                .contains("value is not a supported identity form")
                || reason.contains("value is too long")),
            "{:?}",
            malformed.unknown
        );
        assert!(
            malformed
                .unknown
                .iter()
                .all(|reason| !reason.contains("hunter2") && !reason.contains("sk_live")),
            "{:?}",
            malformed.unknown
        );
    }

    /// No secret survives in an argument, a marker-like variable or an
    /// unlisted environment value: a command line publishes only Effigy's own
    /// vocabulary, so an arbitrary flag name or value is never written.
    #[test]
    fn gateway_caller_record_never_exposes_a_secret() {
        const SECRET: &str = "hunter2-not-a-real-secret";
        let fake = Fake::default()
            .facts(50, 40, 40, "macos:1.000050", &["effigy"])
            .facts(
                40,
                30,
                40,
                "macos:1.000040",
                &[
                    "effigy",
                    "--token",
                    SECRET,
                    "--suffix",
                    SECRET,
                    "--hunter2",
                    "https://user:hunter2@example.test/hook",
                    "gateway",
                    "setup",
                ],
            );
        let record = caller(&fake, 50, Some("/dev/ttys003".into()), |name| match name {
            // A marker-shaped credential is refused, not recorded.
            "PASEO_RUN_ID" => Some(OsString::from("Bearer not-a-run")),
            _ => None,
        });
        let parents = serde_json::to_string(&record.parents).unwrap();
        let unknown = record.unknown.join("\n");
        // The invoking command is caller-controlled too: a value supplied to an
        // option the CLI accepts is not published.
        let rendered = serde_json::to_string(&event(
            OperatorAction::Setup,
            "gateway setup command",
            50,
            &[
                "/opt/effigy".into(),
                "gateway".into(),
                "setup".into(),
                "--suffix".into(),
                SECRET.into(),
            ],
            Some(Path::new("/src/effigy")),
            &record,
        ))
        .unwrap();
        for exposed in [SECRET, "hunter2", "not-a-run", "abc123"] {
            assert!(!parents.contains(exposed), "{parents}");
            assert!(!rendered.contains(exposed), "{rendered}");
        }
        assert!(!unknown.contains("hunter2"), "{unknown}");
        // A vocabulary word supplied as an option value is still a value: the
        // CLI accepts `--suffix down`, and the record must not publish it.
        let word_value = serde_json::to_string(&event(
            OperatorAction::Setup,
            "gateway setup command",
            50,
            &[
                "effigy".into(),
                "gateway".into(),
                "setup".into(),
                "--suffix".into(),
                "down".into(),
            ],
            Some(Path::new("/src/effigy")),
            &record,
        ))
        .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&word_value).unwrap();
        assert_eq!(parsed["argv"][3], "--suffix", "{word_value}");
        assert_eq!(parsed["argv"][4], REDACTED, "{word_value}");
        // The same value in `--flag=value` form keeps the flag and loses the value.
        let assigned = serde_json::to_string(&event(
            OperatorAction::Setup,
            "gateway setup command",
            50,
            &[
                "effigy".into(),
                "gateway".into(),
                "setup".into(),
                "--suffix=down".into(),
            ],
            Some(Path::new("/src/effigy")),
            &record,
        ))
        .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&assigned).unwrap();
        assert_eq!(
            parsed["argv"][3],
            format!("--suffix={REDACTED}"),
            "{assigned}"
        );
        // Attribution survives: the program and Effigy's own flag names stay,
        // while the action itself is named by the event's `cause` and `action`.
        assert!(parents.contains("effigy"), "{parents}");
        // No option of another program is published, not even one that spells a
        // flag this record defines.
        assert!(!parents.contains("--suffix"), "{parents}");
        assert!(!parents.contains("--token"), "{parents}");
        assert!(rendered.contains("--suffix"), "{rendered}");
        assert!(parents.contains("<redacted>"), "{parents}");
        // A plain marker and the terminal are still recorded.
        assert!(
            rendered.contains("\"terminal\":\"/dev/ttys003\""),
            "{rendered}"
        );
    }

    /// An ancestor's command publishes its program and nothing else. Option
    /// tokens are that program's own, so Effigy's flag vocabulary says nothing
    /// about them, and a token that merely matches a flag can be its value — in a
    /// direct command line and inside a shell string alike (effigy#450 review).
    #[test]
    fn gateway_ancestor_commands_publish_only_the_program() {
        let direct = Fake::default()
            .facts(50, 40, 40, "macos:1.000050", &["effigy"])
            .facts(
                40,
                30,
                40,
                "macos:1.000040",
                &[
                    "/opt/helper/bin/helper",
                    "/opt/helper/bin/x",
                    "login",
                    "hunter2",
                    "hunter2/abc",
                    "--verbose",
                ],
            );
        let parent = caller_of(&direct, 50).parents[0].command.clone().unwrap();
        assert_eq!(
            parent,
            vec![
                REDACTED.to_owned(),
                REDACTED.to_owned(),
                REDACTED.to_owned(),
                REDACTED.to_owned(),
                REDACTED.to_owned(),
                REDACTED.to_owned(),
            ]
        );

        // A value that matches one of Effigy's own flags is another program's
        // token, and a known spelling does not make it ours.
        let foreign_option = Fake::default()
            .facts(50, 40, 40, "macos:1.000050", &["effigy"])
            .facts(
                40,
                30,
                40,
                "macos:1.000040",
                &["git", "--suffix", "--json", "--verbose"],
            );
        let parent = caller_of(&foreign_option, 50).parents[0]
            .command
            .clone()
            .unwrap();
        assert_eq!(
            parent,
            vec![
                "git".to_owned(),
                REDACTED.to_owned(),
                REDACTED.to_owned(),
                REDACTED.to_owned(),
            ]
        );
        let foreign_shell = Fake::default()
            .facts(50, 40, 40, "macos:1.000050", &["effigy"])
            .facts(
                40,
                30,
                40,
                "macos:1.000040",
                &["/bin/sh", "-c", "git --suffix --json"],
            );
        let parent = caller_of(&foreign_shell, 50).parents[0]
            .command
            .clone()
            .unwrap();
        assert_eq!(
            parent,
            vec!["sh".to_owned(), REDACTED.to_owned(), REDACTED.to_owned()]
        );

        // A wrapper around Effigy keeps only its own program name: the shell
        // option and the script are that shell's text.
        let wrapper = Fake::default()
            .facts(50, 40, 40, "macos:1.000050", &["effigy"])
            .facts(
                40,
                30,
                40,
                "macos:1.000040",
                &["/bin/zsh", "-lc", "effigy gateway down --json"],
            );
        let parent = caller_of(&wrapper, 50).parents[0].command.clone().unwrap();
        assert_eq!(
            parent,
            vec!["zsh".to_owned(), REDACTED.to_owned(), REDACTED.to_owned()]
        );

        // Positional words, paths and unknown flags are values like any other.
        let arbitrary = Fake::default()
            .facts(50, 40, 40, "macos:1.000050", &["effigy"])
            .facts(
                40,
                30,
                40,
                "macos:1.000040",
                &["helper", "--hunter2", "gateway"],
            );
        let parent = caller_of(&arbitrary, 50).parents[0]
            .command
            .clone()
            .unwrap();
        assert_eq!(
            parent,
            vec![
                REDACTED.to_owned(),
                REDACTED.to_owned(),
                REDACTED.to_owned(),
            ]
        );

        // The same inside a shell string, including a path-shaped secret.
        let shell = Fake::default()
            .facts(50, 40, 40, "macos:1.000050", &["effigy"])
            .facts(
                40,
                30,
                40,
                "macos:1.000040",
                &[
                    "/bin/sh",
                    "-c",
                    "psql --password hunter2 /etc/passwd --verbose",
                ],
            );
        let parent = caller_of(&shell, 50).parents[0].command.clone().unwrap();
        let rendered = parent.join(" ");
        assert_eq!(parent[0], "sh", "{rendered}");
        for exposed in ["psql", "password", "hunter2", "passwd", "etc", "--verbose"] {
            assert!(!rendered.contains(exposed), "{rendered}");
        }
    }

    /// The program slot names the executable the operating system reports rather
    /// than the caller-chosen `argv[0]`, and only when it is a name this record
    /// may publish.
    #[test]
    fn gateway_ancestor_program_comes_from_the_executable_not_argv0() {
        let renamed = Fake::default()
            .facts(50, 40, 40, "macos:1.000050", &["effigy"])
            .renamed(
                40,
                30,
                40,
                "macos:1.000040",
                &["hunter2", "login"],
                Some("/bin/zsh"),
            );
        let parent = caller_of(&renamed, 50).parents[0].command.clone().unwrap();
        assert_eq!(parent[0], "zsh", "{parent:?}");
        assert!(!parent.join(" ").contains("hunter2"), "{parent:?}");

        // An executable whose own file name is not our vocabulary is not
        // published either, and neither is the caller-chosen `argv[0]`.
        let foreign = Fake::default()
            .facts(50, 40, 40, "macos:1.000050", &["effigy"])
            .renamed(
                40,
                30,
                40,
                "macos:1.000040",
                &["hunter2", "login"],
                Some("/usr/bin/hunter2"),
            );
        let parent = caller_of(&foreign, 50).parents[0].command.clone().unwrap();
        assert_eq!(parent[0], REDACTED, "{parent:?}");
        assert!(!parent.join(" ").contains("hunter2"), "{parent:?}");

        // With no executable fact, no program name is claimed either.
        let unknown = Fake::default()
            .facts(50, 40, 40, "macos:1.000050", &["effigy"])
            .renamed(40, 30, 40, "macos:1.000040", &["hunter2", "login"], None);
        let parent = caller_of(&unknown, 50).parents[0].command.clone().unwrap();
        assert_eq!(parent[0], REDACTED, "{parent:?}");
        assert!(!parent.join(" ").contains("hunter2"), "{parent:?}");
    }

    /// A bounded reason list keeps the chain's own failure and says how much it
    /// omitted, so an invalid-marker flood never hides a missing chain.
    #[test]
    fn gateway_caller_reports_when_reasons_are_bounded() {
        let fake = Fake::default()
            .facts(50, 40, 30, "macos:1.000050", &[])
            .unreadable(40, "permission denied");
        // Every allowlisted marker carries an invalid value.
        let record = caller(&fake, 50, None, |_| Some(OsString::from("hunter2")));
        assert!(record.markers.is_empty(), "{:?}", record.markers);
        assert!(
            record.unknown.len() <= UNKNOWN_LIMIT,
            "{:?}",
            record.unknown
        );
        assert!(
            record
                .unknown
                .iter()
                .any(|reason| reason.contains("parent 40") && reason.contains("permission denied")),
            "{:?}",
            record.unknown
        );
        assert!(
            record
                .unknown
                .last()
                .is_some_and(|reason| reason.contains("further reasons omitted")),
            "{:?}",
            record.unknown
        );
        assert!(!record.unknown.join("\n").contains("hunter2"));
    }

    /// A vocabulary word is published; a positional identity, an opaque token,
    /// an unknown short flag and its value are not.
    #[test]
    fn gateway_ancestor_tokens_publish_only_vocabulary_words() {
        let token = "a8f5f167f44f4964e6c998dee827110cf2d4c1f7";
        let fake = Fake::default()
            .facts(50, 40, 40, "macos:1.000050", &[])
            .facts(
                40,
                30,
                40,
                "macos:1.000040",
                &[
                    "/opt/paseo/bin/paseo",
                    "run",
                    "f593c9f2-075b-4243-9e1b-176d303ec799",
                    token,
                    "-m",
                    "wip",
                ],
            );
        let parent = caller_of(&fake, 50).parents[0].command.clone().unwrap();
        assert_eq!(
            parent,
            vec![
                "paseo".to_owned(),
                REDACTED.to_owned(),
                REDACTED.to_owned(),
                REDACTED.to_owned(),
                REDACTED.to_owned(),
                REDACTED.to_owned(),
            ]
        );
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
        // A long positional argument is a value, so it is not recorded at all.
        assert_eq!(
            record.parents[0].command.as_ref().unwrap()[0],
            REDACTED,
            "{rendered}"
        );
        assert_eq!(
            record
                .terminal
                .as_deref()
                .map(str::chars)
                .map(Iterator::count),
            Some(TEXT_LIMIT)
        );
        // A long token outside the published vocabulary is a value, so it is
        // never written; the cause and the terminal are truncated visibly.
        let event = event(
            OperatorAction::Up,
            &long,
            50,
            std::slice::from_ref(&long),
            Some(Path::new("/src/effigy")),
            &record,
        );
        assert_eq!(event["argv"][0], REDACTED, "{event}");
        let cause = event["cause"].as_str().unwrap();
        assert_eq!(cause.chars().count(), TEXT_LIMIT);
        assert!(cause.ends_with('…'), "{cause}");
        assert!(record.unknown.len() <= UNKNOWN_LIMIT);
    }
}
