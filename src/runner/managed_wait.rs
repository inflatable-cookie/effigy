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
#[derive(Debug, Clone)]
pub(in crate::runner) struct WaitLedger {
    dir: PathBuf,
    scope: String,
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
        }
    }

    /// The path an entry's record is published at.
    pub(in crate::runner) fn path(&self, entry: &str) -> PathBuf {
        self.dir.join(format!("{}.json", component(entry)))
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

    /// Claims this scope for one publisher. The last claim wins: the host
    /// planner claims it when it starts the plan, and the run's owner stops
    /// publishing from then on (effigy#458).
    pub(in crate::runner) fn claim_owner(&self, owner: WaitOwner) {
        if std::fs::create_dir_all(&self.dir).is_err() {
            return;
        }
        let record = OwnerRecord {
            schema: ENTRY_WAIT_SCHEMA.to_owned(),
            scope: self.scope.clone(),
            owner,
        };
        let _ = write_private_json(&self.dir.join(OWNER_RECORD), &record);
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

    fn write(&self, entry: &str, record: &WaitRecord) {
        if std::fs::create_dir_all(&self.dir).is_err() {
            return;
        }
        // One accepted private-record writer: temp file, 0600, rename.
        let _ = write_private_json(&self.path(entry), record);
    }

    /// The entry's record, when it publishes one for this scope.
    pub(in crate::runner) fn read(&self, entry: &str) -> Option<EntryState> {
        let raw = std::fs::read(self.path(entry)).ok()?;
        let record: WaitRecord = serde_json::from_slice(&raw).ok()?;
        (record.schema == ENTRY_WAIT_SCHEMA && record.scope == self.scope).then_some(record.state)
    }

    /// Every published record of this scope, in entry-name order.
    pub(in crate::runner) fn read_all(&self) -> Vec<EntryState> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut states = entries
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name() != SESSION_RECORD)
            .filter(|entry| entry.file_name() != OWNER_RECORD)
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
            .filter_map(|entry| {
                let raw = std::fs::read(entry.path()).ok()?;
                let record: WaitRecord = serde_json::from_slice(&raw).ok()?;
                (record.schema == ENTRY_WAIT_SCHEMA && record.scope == self.scope)
                    .then_some(record.state)
            })
            .collect::<Vec<_>>();
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
        let (home, ledger) = fixture();
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
        assert!(ledger
            .path("front")
            .starts_with(home.path().join(WAITS_DIR)));
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
