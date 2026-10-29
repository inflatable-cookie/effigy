//! The published wait of a managed session entry (effigy#458, Q-017).
//!
//! A wait that happens in another process cannot be re-derived by a front end:
//! the environment's health counts are observed by the `up` child, and a host
//! process's dependency wait happens before its own supervisor exists. Each
//! owner therefore publishes one record per entry under the checkout's runtime
//! directory, and every front end — the TUI tab, `--headless` text and
//! `status --json` — reads that record instead of inventing a label.
//!
//! Records are a projection, never authority: the process that owns the wait
//! writes it, and the journal stays the record of what ran. A record left by a
//! finished session is cleared when the next one starts.

use std::path::{Path, PathBuf};

use effigy_tui::waits::{EntryPhase, EntryState, ENTRY_WAIT_SCHEMA};
use serde::{Deserialize, Serialize};

use crate::runner::error::RunnerError;
use crate::runner::host_process::write_private_json;

/// Every managed wait record lives under this checkout-relative directory.
const WAITS_DIR: &str = ".effigy/runtime/waits";
/// The one record naming the environment a scope's waits belong to. It is
/// written before the environment phase can block, so `status` can find the
/// environment's own records while it is still coming up.
const SESSION_RECORD: &str = "session.json";
/// Who currently publishes one environment scope's entries. The run's owner
/// publishes the host graph before its environment phase can block; the host
/// planner takes the same scope over when it starts the plan, so the two never
/// write the same entries at once (effigy#458).
const OWNER_RECORD: &str = "owner.json";
/// The environment phase's own record. It belongs to the phase, not to either
/// publisher, so it is read whichever one currently owns the entries.
const ENVIRONMENT_ENTRY: &str = "environment";

/// The environment identity a scope publishes before its phase blocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionRecord {
    schema: String,
    scope: String,
    environment: String,
}

/// Who publishes one environment scope's entries right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::runner) enum WaitOwner {
    /// The run's own owner, before the environment's host planner takes over.
    Run,
    /// The environment's host planner: the one process that starts hosts.
    Planner,
}

impl WaitOwner {
    /// The directory this publisher writes its entries in, and the one readers
    /// resolve while it owns the scope. Because every publisher writes only to
    /// its own directory, a write that races a handoff can never land on the
    /// entry the current owner published (effigy#458).
    fn directory(self) -> &'static str {
        match self {
            Self::Run => "run",
            Self::Planner => "planner",
        }
    }
}

/// The one record naming the current publisher of a scope.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OwnerRecord {
    schema: String,
    scope: String,
    owner: WaitOwner,
}

/// One published record: the session it belongs to and the entry's state.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct WaitRecord {
    schema: String,
    /// `<scope>`: the environment name, or `task-<task>` / `task-<task>-<profile>`.
    scope: String,
    #[serde(flatten)]
    state: EntryState,
}

/// Where the entries of one scope publish their waits.
///
/// Records live per publisher: `<scope>/run/<entry>.json` and
/// `<scope>/planner/<entry>.json`. Readers resolve the directory the current
/// owner claimed, so a publisher that has lost the scope can keep writing and
/// never change what a front end reads (effigy#458).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct WaitLedger {
    dir: PathBuf,
    scope: String,
    /// The namespace this writer publishes in. `None` reads the current
    /// owner's entries and writes the environment phase's own record.
    owner: Option<WaitOwner>,
}

impl WaitLedger {
    /// The waits of one container environment's own phase and host processes.
    pub(in crate::runner) fn environment(repo_root: &Path, environment: &str) -> Self {
        Self::new(repo_root, "environment", environment)
    }

    /// The waits of one managed task and profile's workloads.
    pub(in crate::runner) fn task(repo_root: &Path, task: &str, profile: &str) -> Self {
        Self::new(repo_root, "task", &format!("{task}-{profile}"))
    }

    fn new(repo_root: &Path, kind: &str, name: &str) -> Self {
        let scope = format!("{kind}-{}", component(name));
        Self {
            dir: repo_root.join(WAITS_DIR).join(&scope),
            scope,
            owner: None,
        }
    }

    /// This ledger's records, as one publisher writes them.
    fn owned(&self, owner: WaitOwner) -> Self {
        Self {
            dir: self.dir.clone(),
            scope: self.scope.clone(),
            owner: Some(owner),
        }
    }

    /// Forgets every record of this scope: a new session never reads the
    /// previous session's waits as its own.
    pub(in crate::runner) fn clear(&self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }

    /// Publishes one entry's state. A best-effort write: a front end that
    /// cannot read a record falls back to the entry's own process status, and
    /// the record is never the run's authority.
    pub(in crate::runner) fn publish(&self, state: &EntryState) {
        let record = WaitRecord {
            schema: ENTRY_WAIT_SCHEMA.to_owned(),
            scope: self.scope.clone(),
            state: state.clone(),
        };
        self.write(&state.entry, &record);
    }

    /// The entry no longer waits: it started, became ready or settled. The
    /// record says so rather than being deleted, so a front end that reads it
    /// later never renders a wait the entry has left behind (effigy#458).
    pub(in crate::runner) fn settle(&self, entry: &str) {
        let record = WaitRecord {
            schema: ENTRY_WAIT_SCHEMA.to_owned(),
            scope: self.scope.clone(),
            state: EntryState {
                entry: entry.to_owned(),
                phase: EntryPhase::Ready,
                waiting_on: None,
                not_started_because: None,
                detail: None,
                elapsed_ms: 0,
            },
        };
        self.write(entry, &record);
    }

    /// Claims this scope for one publisher and returns the ledger that
    /// publishes in its namespace. The claim is one atomic record replace: the
    /// host planner claims it when it starts the plan, and from that moment
    /// readers resolve the planner's entries, while the run's owner keeps
    /// writing only its own (effigy#458).
    pub(in crate::runner) fn claim(&self, owner: WaitOwner) -> Self {
        if std::fs::create_dir_all(&self.dir).is_err() {
            return self.owned(owner);
        }
        let record = OwnerRecord {
            schema: ENTRY_WAIT_SCHEMA.to_owned(),
            scope: self.scope.clone(),
            owner,
        };
        let _ = write_private_json(&self.dir.join(OWNER_RECORD), &record);
        self.owned(owner)
    }

    /// Who publishes this scope's entries now, when it recorded a claim.
    pub(in crate::runner) fn current_owner(&self) -> Option<WaitOwner> {
        let raw = std::fs::read(self.dir.join(OWNER_RECORD)).ok()?;
        let record: OwnerRecord = serde_json::from_slice(&raw).ok()?;
        (record.schema == ENTRY_WAIT_SCHEMA && record.scope == self.scope).then_some(record.owner)
    }

    /// Names the environment this scope's waits belong to. The run publishes
    /// it before its environment phase can block, so a status read during
    /// bring-up finds the environment's host waits and counts (effigy#458).
    pub(in crate::runner) fn publish_environment_identity(&self, environment: &str) {
        if std::fs::create_dir_all(&self.dir).is_err() {
            return;
        }
        let record = SessionRecord {
            schema: ENTRY_WAIT_SCHEMA.to_owned(),
            scope: self.scope.clone(),
            environment: environment.to_owned(),
        };
        let _ = write_private_json(&self.dir.join(SESSION_RECORD), &record);
    }

    /// The environment this scope's waits belong to, when it published one.
    pub(in crate::runner) fn environment_identity(&self) -> Option<String> {
        let raw = std::fs::read(self.dir.join(SESSION_RECORD)).ok()?;
        let record: SessionRecord = serde_json::from_slice(&raw).ok()?;
        (record.schema == ENTRY_WAIT_SCHEMA && record.scope == self.scope)
            .then_some(record.environment)
    }

    /// The directory this ledger's entries are written in: its own namespace
    /// when it is a publisher, else the one the current owner claimed.
    fn namespace(&self) -> PathBuf {
        let owner = self
            .owner
            .or_else(|| self.current_owner())
            .unwrap_or(WaitOwner::Run);
        self.dir.join(owner.directory())
    }

    fn write(&self, entry: &str, record: &WaitRecord) {
        if entry == ENVIRONMENT_ENTRY {
            // The phase's record belongs to the phase, not to a publisher.
            if std::fs::create_dir_all(&self.dir).is_err() {
                return;
            }
            let _ = write_private_json(&self.dir.join(format!("{ENVIRONMENT_ENTRY}.json")), record);
            return;
        }
        let namespace = self.namespace();
        if std::fs::create_dir_all(&namespace).is_err() {
            return;
        }
        // One accepted private-record writer: temp file, 0600, rename.
        let _ = write_private_json(
            &namespace.join(format!("{}.json", component(entry))),
            record,
        );
    }

    /// The entry's record, when it publishes one for this scope. The
    /// environment phase's record is read whichever publisher owns the
    /// entries; every other entry resolves the current owner's namespace.
    pub(in crate::runner) fn read(&self, entry: &str) -> Option<EntryState> {
        let path = if entry == ENVIRONMENT_ENTRY {
            self.dir.join(format!("{ENVIRONMENT_ENTRY}.json"))
        } else {
            self.namespace().join(format!("{}.json", component(entry)))
        };
        let raw = std::fs::read(path).ok()?;
        let record: WaitRecord = serde_json::from_slice(&raw).ok()?;
        (record.schema == ENTRY_WAIT_SCHEMA && record.scope == self.scope).then_some(record.state)
    }

    /// Every published record of this scope, in entry-name order: the
    /// environment phase's own record, then the current owner's entries.
    pub(in crate::runner) fn read_all(&self) -> Vec<EntryState> {
        let mut states = Vec::new();
        if let Some(phase) = self.read(ENVIRONMENT_ENTRY) {
            states.push(phase);
        }
        let Ok(entries) = std::fs::read_dir(self.namespace()) else {
            return states;
        };
        states.extend(
            entries
                .filter_map(Result::ok)
                .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
                .filter_map(|entry| {
                    let raw = std::fs::read(entry.path()).ok()?;
                    let record: WaitRecord = serde_json::from_slice(&raw).ok()?;
                    (record.schema == ENTRY_WAIT_SCHEMA && record.scope == self.scope)
                        .then_some(record.state)
                }),
        );
        states.sort_by(|left, right| left.entry.cmp(&right.entry));
        states
    }
}

/// The attribution of an entry that failed: the entry that reported it, and
/// the first line of what it reported, bounded so a diagnostic never becomes a
/// tab title.
pub(in crate::runner) fn failure_attribution(
    entry: &str,
    error: &RunnerError,
) -> effigy_tui::waits::Attribution {
    let detail = match error {
        // The listener's own readiness report is the evidence a person needs,
        // and the error's own prefix would repeat the entry's name.
        RunnerError::ManagedHostReadiness { diagnostic, .. } => diagnostic.clone(),
        other => other.to_string(),
    };
    let line = detail.lines().next().unwrap_or_default().trim();
    let bounded = line.chars().take(160).collect::<String>();
    effigy_tui::waits::Attribution::new(
        entry,
        if bounded.is_empty() {
            "failed".to_owned()
        } else {
            format!("failed: {bounded}")
        },
    )
}

fn component(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use effigy_tui::waits::{Attribution, EntryPhase, WaitOn};

    fn fixture() -> (tempfile::TempDir, WaitLedger) {
        let home = tempfile::tempdir().expect("fixture directory");
        let ledger = WaitLedger::environment(home.path(), "app/one");
        (home, ledger)
    }

    #[test]
    fn a_published_record_round_trips_with_its_waiting_on() {
        let (_home, ledger) = fixture();
        ledger.clear();
        ledger.publish(&EntryState {
            entry: "front".to_owned(),
            phase: EntryPhase::Waiting,
            waiting_on: Some(WaitOn::Entry {
                name: "api".to_owned(),
                phase: EntryPhase::Starting,
            }),
            not_started_because: None,
            detail: None,
            elapsed_ms: 4_200,
        });
        assert!(
            ledger.read("front").is_some(),
            "the record is published in the current owner's namespace"
        );
        let state = ledger.read("front").expect("the record reads back");
        assert_eq!(state.entry, "front");
        assert_eq!(state.elapsed_ms, 4_200);
        assert_eq!(
            state.waiting_on,
            Some(WaitOn::Entry {
                name: "api".to_owned(),
                phase: EntryPhase::Starting,
            })
        );
        // Another entry has no record, and a cleared scope has none at all.
        assert!(ledger.read("admin").is_none());
        ledger.clear();
        assert!(ledger.read("front").is_none());
    }

    /// The handoff is atomic because each publisher writes only its own
    /// directory and readers resolve the directory the current claim names: a
    /// publisher that lost the scope can keep writing and never changes what
    /// a front end reads (effigy#458).
    #[test]
    fn a_publisher_that_lost_the_scope_cannot_change_what_readers_see() {
        let (_home, ledger) = fixture();
        ledger.clear();
        let run = ledger.claim(WaitOwner::Run);
        run.publish(&EntryState {
            entry: "api".to_owned(),
            phase: EntryPhase::Waiting,
            waiting_on: Some(WaitOn::Environment {
                healthy: Some(3),
                total: Some(4),
            }),
            not_started_because: None,
            detail: None,
            elapsed_ms: 0,
        });
        assert_eq!(
            ledger.read("api").and_then(|state| state.waiting_on),
            Some(WaitOn::Environment {
                healthy: Some(3),
                total: Some(4)
            })
        );

        // The planner takes the scope over before it starts any listener.
        let planner = ledger.claim(WaitOwner::Planner);
        planner.publish(&EntryState {
            entry: "api".to_owned(),
            phase: EntryPhase::Starting,
            waiting_on: Some(WaitOn::Probe {
                probe: "GET / -> 200".to_owned(),
            }),
            not_started_because: None,
            detail: None,
            elapsed_ms: 0,
        });
        assert_eq!(ledger.current_owner(), Some(WaitOwner::Planner));

        // The run's owner keeps publishing, as its progress ticks do: its
        // writes land in its own namespace and are not what a reader resolves.
        run.publish(&EntryState {
            entry: "api".to_owned(),
            phase: EntryPhase::Waiting,
            waiting_on: Some(WaitOn::Environment {
                healthy: Some(3),
                total: Some(4),
            }),
            not_started_because: None,
            detail: None,
            elapsed_ms: 0,
        });
        run.settle("api");
        assert_eq!(
            ledger.read("api").and_then(|state| state.waiting_on),
            Some(WaitOn::Probe {
                probe: "GET / -> 200".to_owned()
            }),
            "the planner's record is the one read"
        );

        // The same holds for a terminal failure: the run's generic attribution
        // never replaces the planner's.
        planner.publish(&EntryState {
            entry: "front".to_owned(),
            phase: EntryPhase::NotStarted,
            waiting_on: None,
            not_started_because: Some(effigy_tui::waits::Attribution::new(
                "api",
                "failed: readiness probe GET / returned 500",
            )),
            detail: None,
            elapsed_ms: 0,
        });
        run.publish(&EntryState {
            entry: "front".to_owned(),
            phase: EntryPhase::NotStarted,
            waiting_on: None,
            not_started_because: Some(effigy_tui::waits::Attribution::new(
                "environment",
                "failed: `up` exited 1",
            )),
            detail: None,
            elapsed_ms: 0,
        });
        assert_eq!(
            ledger
                .read("front")
                .and_then(|state| state.not_started_because)
                .map(|cause| cause.name),
            Some("api".to_owned())
        );
    }

    #[test]
    fn an_attributed_failure_survives_the_session_that_published_it() {
        let (_home, ledger) = fixture();
        ledger.publish(&EntryState {
            entry: "front".to_owned(),
            phase: EntryPhase::NotStarted,
            waiting_on: None,
            not_started_because: Some(Attribution::new("api", "failed (status 500)")),
            detail: None,
            elapsed_ms: 0,
        });
        let state = ledger.read("front").expect("the record reads back");
        assert_eq!(state.status_line(), "not started — api failed (status 500)");
    }

    #[test]
    fn every_published_record_is_listed_once_in_entry_order() {
        let (_home, ledger) = fixture();
        for (entry, phase) in [
            ("front", EntryPhase::NotStarted),
            ("api", EntryPhase::Starting),
        ] {
            ledger.publish(&EntryState {
                entry: entry.to_owned(),
                phase,
                waiting_on: None,
                not_started_because: None,
                detail: None,
                elapsed_ms: 0,
            });
        }
        let states = ledger.read_all();
        assert_eq!(
            states
                .iter()
                .map(|state| state.entry.as_str())
                .collect::<Vec<_>>(),
            vec!["api", "front"]
        );
    }
}
