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

use effigy_tui::waits::{EntryState, ENTRY_WAIT_SCHEMA};
use serde::{Deserialize, Serialize};

use crate::runner::error::RunnerError;

/// Every managed wait record lives under this checkout-relative directory.
const WAITS_DIR: &str = ".effigy/runtime/waits";

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
        let Ok(rendered) = serde_json::to_vec(&record) else {
            return;
        };
        if std::fs::create_dir_all(&self.dir).is_err() {
            return;
        }
        // The rename keeps a reader from seeing half a record.
        let path = self.path(&state.entry);
        let temp = path.with_extension("json.tmp");
        if std::fs::write(&temp, rendered).is_err() {
            return;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o600));
        }
        let _ = std::fs::rename(&temp, &path);
    }

    /// Removes one entry's record: it has started or settled, so it no longer
    /// waits.
    pub(in crate::runner) fn retire(&self, entry: &str) {
        let _ = std::fs::remove_file(self.path(entry));
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

/// The probe label of a host listener's declared readiness request:
/// `GET / -> 200`.
pub(in crate::runner) fn listener_probe_label(path: &str, status: u16) -> String {
    let path = if path.trim().is_empty() { "/" } else { path };
    format!("GET {path} -> {status}")
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
    fn the_listener_probe_names_the_declared_request() {
        assert_eq!(listener_probe_label("/", 200), "GET / -> 200");
        assert_eq!(listener_probe_label("/health", 204), "GET /health -> 204");
        assert_eq!(listener_probe_label("  ", 200), "GET / -> 200");
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
