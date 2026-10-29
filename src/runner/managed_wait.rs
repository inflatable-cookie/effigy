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
/// The environment phase's own record. It is a reserved storage key, never an
/// entry name: [`path_component`] escapes `@`, so a host process that an
/// operator names `environment` keeps its own record beside the phase's
/// (effigy#458).
const PHASE_RECORD: &str = "@environment.json";
/// Where an unclaimed scope's entries live. A task scope never claims one.
const DEFAULT_NAMESPACE: &str = "entries";

/// The environment identity a scope publishes before its phase blocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionRecord {
    schema: String,
    scope: String,
    environment: String,
}

/// Who publishes one environment scope's entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::runner) enum WaitOwner {
    /// The run's own owner, before the environment's host planner takes over.
    Run,
    /// The environment's host planner: the one process that starts hosts.
    Planner,
}

impl WaitOwner {
    fn name(self) -> &'static str {
        match self {
            Self::Run => "run",
            Self::Planner => "planner",
        }
    }
}

/// The one record naming the current publisher of a scope: the directory its
/// complete snapshot lives in. Switching it is one atomic rename, so a reader
/// always resolves a whole snapshot — the previous publisher's until the new
/// one is complete, and the new one's afterwards (effigy#458).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OwnerRecord {
    schema: String,
    scope: String,
    directory: String,
}

/// One published record: the session it belongs to and the entry's state.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct WaitRecord {
    schema: String,
    /// `<scope>`: the environment name, or the task and profile.
    scope: String,
    #[serde(flatten)]
    state: EntryState,
}

/// Where the entries of one scope publish their waits.
///
/// Records live in one directory per published snapshot. A publisher stages a
/// complete snapshot in a fresh directory and then commits it by replacing
/// `owner.json`, so readers resolve the whole previous snapshot until the new
/// one is complete. A publisher that has lost the scope keeps writing its own
/// directory, which nothing reads (effigy#458).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct WaitLedger {
    dir: PathBuf,
    scope: String,
    /// The snapshot this ledger publishes in, when it is a publisher.
    namespace: Option<PathBuf>,
}

impl WaitLedger {
    /// The waits of one container environment's own phase and host processes.
    pub(in crate::runner) fn environment(repo_root: &Path, environment: &str) -> Self {
        Self::new(repo_root, "environment", &path_component(environment))
    }

    /// The waits of one managed task and profile's workloads. The two are
    /// separate components, so `a-b`/`c` and `a`/`b-c` never share a scope.
    pub(in crate::runner) fn task(repo_root: &Path, task: &str, profile: &str) -> Self {
        Self::new(
            repo_root,
            "task",
            &format!("{}+{}", path_component(task), path_component(profile)),
        )
    }

    /// `name` is already encoded by [`path_component`].
    fn new(repo_root: &Path, kind: &str, name: &str) -> Self {
        let scope = format!("{kind}-{name}");
        Self {
            dir: repo_root.join(WAITS_DIR).join(&scope),
            scope,
            namespace: None,
        }
    }

    /// Forgets every record of this scope: a new session never reads the
    /// previous session's waits as its own.
    pub(in crate::runner) fn clear(&self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }

    /// Starts a fresh, not yet visible snapshot for `owner`. The caller
    /// publishes the complete snapshot into it and only then commits it, so a
    /// reader never resolves a half-written set of records (effigy#458).
    pub(in crate::runner) fn stage(&self, owner: WaitOwner) -> Self {
        static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let directory = format!(
            "{}-{}-{}",
            owner.name(),
            std::process::id(),
            SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        let namespace = self.dir.join(&directory);
        let _ = std::fs::create_dir_all(&namespace);
        Self {
            dir: self.dir.clone(),
            scope: self.scope.clone(),
            namespace: Some(namespace),
        }
    }

    /// Makes a staged snapshot the one readers resolve. The switch is one
    /// atomic record replace; the snapshot it replaces is dropped afterwards.
    pub(in crate::runner) fn commit(&self, staged: &Self) {
        let Some(namespace) = staged.namespace.as_ref() else {
            return;
        };
        let Some(directory) = namespace.file_name().and_then(|name| name.to_str()) else {
            return;
        };
        let previous = self.committed_namespace();
        let record = OwnerRecord {
            schema: ENTRY_WAIT_SCHEMA.to_owned(),
            scope: self.scope.clone(),
            directory: directory.to_owned(),
        };
        if write_private_json(&self.dir.join(OWNER_RECORD), &record).is_err() {
            return;
        }
        if previous != *namespace {
            // The snapshot readers resolved until now is no longer reachable.
            let _ = std::fs::remove_dir_all(previous);
        }
    }

    /// Whether this publisher's snapshot is the one readers resolve now.
    pub(in crate::runner) fn is_visible(&self) -> bool {
        self.namespace
            .as_ref()
            .is_some_and(|namespace| self.committed_namespace() == *namespace)
    }

    /// The directory readers resolve: the committed snapshot, or the default
    /// one for a scope that never claimed a publisher.
    fn committed_namespace(&self) -> PathBuf {
        let raw = std::fs::read(self.dir.join(OWNER_RECORD)).ok();
        let directory = raw
            .and_then(|raw| serde_json::from_slice::<OwnerRecord>(&raw).ok())
            .filter(|record| record.schema == ENTRY_WAIT_SCHEMA && record.scope == self.scope)
            .map(|record| record.directory);
        match directory {
            Some(directory) => self.dir.join(directory),
            None => self.dir.join(DEFAULT_NAMESPACE),
        }
    }

    /// The directory this ledger writes entry records in.
    fn namespace(&self) -> PathBuf {
        self.namespace
            .clone()
            .unwrap_or_else(|| self.committed_namespace())
    }

    /// Publishes one entry's state. A best-effort write: a front end that
    /// cannot read a record falls back to the entry's own process status, and
    /// the record is never the run's authority.
    pub(in crate::runner) fn publish(&self, state: &EntryState) {
        self.write_entry(
            &state.entry,
            WaitRecord {
                schema: ENTRY_WAIT_SCHEMA.to_owned(),
                scope: self.scope.clone(),
                state: state.clone(),
            },
        );
    }

    /// The entry no longer waits: it started, became ready or settled. The
    /// record says so rather than being deleted, so a front end that reads it
    /// later never renders a wait the entry has left behind (effigy#458).
    pub(in crate::runner) fn settle(&self, entry: &str) {
        self.write_entry(
            entry,
            WaitRecord {
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
            },
        );
    }

    /// The environment phase's own wait, under its reserved storage key.
    pub(in crate::runner) fn publish_phase(&self, state: &EntryState) {
        self.write_phase(WaitRecord {
            schema: ENTRY_WAIT_SCHEMA.to_owned(),
            scope: self.scope.clone(),
            state: state.clone(),
        });
    }

    /// The environment phase is over.
    pub(in crate::runner) fn settle_phase(&self) {
        self.write_phase(WaitRecord {
            schema: ENTRY_WAIT_SCHEMA.to_owned(),
            scope: self.scope.clone(),
            state: EntryState {
                entry: "environment".to_owned(),
                phase: EntryPhase::Ready,
                waiting_on: None,
                not_started_because: None,
                detail: None,
                elapsed_ms: 0,
            },
        });
    }

    /// The containers the environment phase reported healthy, from its own
    /// record. It is the one handoff the owner polls while the environment
    /// comes up, and the same counts every entry that waits for the phase
    /// names (effigy#458).
    pub(in crate::runner) fn read_progress(&self) -> Option<(u32, u32)> {
        match self.read_phase()?.waiting_on? {
            effigy_tui::waits::WaitOn::Environment {
                healthy: Some(healthy),
                total: Some(total),
            } => Some((healthy, total)),
            _ => None,
        }
    }

    /// The environment phase's wait, whichever publisher owns the entries.
    pub(in crate::runner) fn read_phase(&self) -> Option<EntryState> {
        let raw = std::fs::read(self.dir.join(PHASE_RECORD)).ok()?;
        let record: WaitRecord = serde_json::from_slice(&raw).ok()?;
        (record.schema == ENTRY_WAIT_SCHEMA && record.scope == self.scope).then_some(record.state)
    }

    /// Names the environment this scope's waits belong to. The run publishes
    /// it before its environment phase can block, so a status read during
    /// bring-up finds the environment's own host waits and counts (effigy#458).
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

    fn write_entry(&self, entry: &str, record: WaitRecord) {
        let namespace = self.namespace();
        if std::fs::create_dir_all(&namespace).is_err() {
            return;
        }
        // One accepted private-record writer: temp file, 0600, rename.
        let _ = write_private_json(
            &namespace.join(format!("{}.json", path_component(entry))),
            &record,
        );
    }

    fn write_phase(&self, record: WaitRecord) {
        if std::fs::create_dir_all(&self.dir).is_err() {
            return;
        }
        let _ = write_private_json(&self.dir.join(PHASE_RECORD), &record);
    }

    /// The entry's record from the snapshot readers resolve now. A commit that
    /// retires the snapshot this read resolved re-resolves it, so an in-flight
    /// reader never observes a broken snapshot (effigy#458).
    pub(in crate::runner) fn read(&self, entry: &str) -> Option<EntryState> {
        read_stable(
            || self.committed_namespace(),
            |namespace| {
                let raw = std::fs::read(namespace.join(format!("{}.json", path_component(entry))))
                    .ok()?;
                let record: WaitRecord = serde_json::from_slice(&raw).ok()?;
                (record.schema == ENTRY_WAIT_SCHEMA && record.scope == self.scope)
                    .then_some(record.state)
            },
        )
    }

    /// Every entry record of the snapshot readers resolve now, in entry-name
    /// order. The environment phase's own record is read with
    /// [`Self::read_phase`], and a snapshot retired mid-read is re-resolved.
    pub(in crate::runner) fn read_all(&self) -> Vec<EntryState> {
        read_stable(
            || self.committed_namespace(),
            |namespace| {
                let entries = std::fs::read_dir(namespace).ok()?;
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
                Some(states)
            },
        )
        .unwrap_or_default()
    }
}

/// Reads through a snapshot handoff. `resolve` names the snapshot readers
/// resolve now and `load` reads one; a snapshot that a commit retired between
/// the two is re-resolved, so an in-flight reader observes a complete snapshot
/// either side of the switch rather than a missing one (effigy#458).
fn read_stable<T>(
    mut resolve: impl FnMut() -> PathBuf,
    mut load: impl FnMut(&Path) -> Option<T>,
) -> Option<T> {
    for _ in 0..4 {
        let namespace = resolve();
        if let Some(value) = load(&namespace) {
            return Some(value);
        }
        if resolve() == namespace {
            // The snapshot did not change under us: the record is really absent.
            return None;
        }
    }
    None
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

/// The one injective encoding for a record path component (K1): `+` separates
/// components and is never left bare, and every byte outside `[A-Za-z0-9_-]` is
/// escaped as `%XX`. Two distinct accepted entry or scope names therefore never
/// share a record — a lossy mapping would let one entry's wait replace
/// another's, so `status` could omit an entry and a late TUI attach could read
/// the wrong one (effigy#458). The exec markers use the same rule.
pub(in crate::runner) fn path_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
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

    /// A commit retires the previous snapshot so it cannot accumulate, and the
    /// read path re-resolves when the snapshot it resolved was the retired one:
    /// an in-flight reader still observes a complete snapshot (effigy#458).
    #[test]
    fn a_read_that_races_a_commit_resolves_the_visible_snapshot() {
        let (_home, ledger) = fixture();
        ledger.clear();
        let run = ledger.stage(WaitOwner::Run);
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
        ledger.commit(&run);
        // What an in-flight reader resolved just before the handoff.
        let resolved_before_commit = ledger.committed_namespace();

        let planner = ledger.stage(WaitOwner::Planner);
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
        ledger.commit(&planner);
        assert!(
            !resolved_before_commit.exists(),
            "the retired snapshot is cleaned up"
        );

        // The production read path resolves the visible snapshot.
        assert_eq!(
            ledger.read("api").and_then(|state| state.waiting_on),
            Some(WaitOn::Probe {
                probe: "GET / -> 200".to_owned()
            }),
            "the committed snapshot is what a read resolves"
        );

        // A reader that resolved the retired snapshot re-resolves it, so it
        // still observes a complete snapshot instead of a missing one.
        let mut attempts = 0;
        let recovered = read_stable(
            || {
                attempts += 1;
                if attempts == 1 {
                    resolved_before_commit.clone()
                } else {
                    ledger.committed_namespace()
                }
            },
            |namespace| {
                let raw = std::fs::read(namespace.join(format!("{}.json", path_component("api"))))
                    .ok()?;
                let record: WaitRecord = serde_json::from_slice(&raw).ok()?;
                Some(record.state)
            },
        )
        .expect("a read that raced the commit still resolves the visible snapshot");
        assert_eq!(
            recovered.waiting_on,
            Some(WaitOn::Probe {
                probe: "GET / -> 200".to_owned()
            })
        );
        assert!(attempts >= 2, "the reader re-resolved the snapshot");
    }

    /// A host process an operator names `environment` is a host, not the
    /// environment phase: the phase has its own storage key, so the two never
    /// overwrite each other (effigy#458).
    #[test]
    fn a_host_named_environment_keeps_its_own_record() {
        let (_home, ledger) = fixture();
        ledger.clear();
        ledger.publish_phase(&EntryState {
            entry: "environment".to_owned(),
            phase: EntryPhase::Waiting,
            waiting_on: Some(WaitOn::Environment {
                healthy: Some(3),
                total: Some(4),
            }),
            not_started_because: None,
            detail: None,
            elapsed_ms: 0,
        });
        ledger.publish(&EntryState {
            entry: "environment".to_owned(),
            phase: EntryPhase::Starting,
            waiting_on: Some(WaitOn::Probe {
                probe: "GET / -> 200".to_owned(),
            }),
            not_started_because: None,
            detail: None,
            elapsed_ms: 12_000,
        });

        let phase = ledger.read_phase().expect("the phase record");
        assert_eq!(
            phase.waiting_on,
            Some(WaitOn::Environment {
                healthy: Some(3),
                total: Some(4)
            })
        );
        let host = ledger.read("environment").expect("the host's own record");
        assert_eq!(
            host.waiting_on,
            Some(WaitOn::Probe {
                probe: "GET / -> 200".to_owned()
            }),
            "the host's wait is its own, not the phase's"
        );
        assert_eq!(
            ledger
                .read_all()
                .iter()
                .map(|state| state.entry.as_str())
                .collect::<Vec<_>>(),
            vec!["environment"],
            "the phase is read on its own key, never as an entry"
        );

        ledger.settle_phase();
        assert!(!ledger
            .read_phase()
            .is_some_and(|phase| phase.holds_a_wait()));
        assert!(
            ledger.read("environment").is_some(),
            "the host record stays"
        );
    }

    /// Two accepted entry names are two records. A lossy file name would let
    /// one entry's wait replace another's, so `status` could omit an entry and
    /// a late TUI attach could read the wrong one (effigy#458).
    #[test]
    fn distinct_accepted_names_never_share_a_record() {
        let (_home, ledger) = fixture();
        ledger.clear();
        let states = [
            (
                "api-v1",
                WaitOn::Delay {
                    remaining_ms: 1_200,
                },
            ),
            (
                "api v1",
                WaitOn::Entry {
                    name: "api".to_owned(),
                    phase: EntryPhase::Starting,
                },
            ),
            (
                "api/v1",
                WaitOn::Probe {
                    probe: "GET / -> 200".to_owned(),
                },
            ),
            (
                "api_v1",
                WaitOn::Environment {
                    healthy: Some(3),
                    total: Some(4),
                },
            ),
        ];
        for (entry, waiting_on) in &states {
            ledger.publish(&EntryState {
                entry: (*entry).to_owned(),
                phase: EntryPhase::Waiting,
                waiting_on: Some(waiting_on.clone()),
                not_started_because: None,
                detail: None,
                elapsed_ms: 0,
            });
        }
        for (entry, waiting_on) in &states {
            let state = ledger
                .read(entry)
                .unwrap_or_else(|| panic!("{entry} keeps its own record"));
            assert_eq!(&state.entry, entry);
            assert_eq!(
                state.waiting_on.as_ref(),
                Some(waiting_on),
                "{entry} reads its own wait"
            );
        }
        let published = ledger.read_all();
        assert_eq!(
            published
                .iter()
                .map(|state| state.entry.as_str())
                .collect::<Vec<_>>(),
            vec!["api v1", "api-v1", "api/v1", "api_v1"],
            "every distinct name is published once"
        );
    }

    /// A task and a profile are separate scope components, so two different
    /// task/profile pairs never share a scope.
    #[test]
    fn task_scopes_keep_the_task_and_profile_apart() {
        let home = tempfile::tempdir().expect("fixture directory");
        let left = WaitLedger::task(home.path(), "a-b", "c");
        let right = WaitLedger::task(home.path(), "a", "b-c");
        left.clear();
        right.clear();
        left.publish(&EntryState {
            entry: "api".to_owned(),
            phase: EntryPhase::Waiting,
            waiting_on: Some(WaitOn::Delay { remaining_ms: 1 }),
            not_started_because: None,
            detail: None,
            elapsed_ms: 0,
        });
        right.publish(&EntryState {
            entry: "api".to_owned(),
            phase: EntryPhase::NotStarted,
            waiting_on: None,
            not_started_because: Some(Attribution::new("api", "failed (status 500)")),
            detail: None,
            elapsed_ms: 0,
        });
        assert_eq!(
            left.read("api").map(|state| state.phase),
            Some(EntryPhase::Waiting)
        );
        assert_eq!(
            right.read("api").map(|state| state.phase),
            Some(EntryPhase::NotStarted)
        );
    }

    /// The handoff is atomic because each publisher writes only its own
    /// directory and readers resolve the directory the current claim names: a
    /// publisher that lost the scope can keep writing and never changes what
    /// a front end reads (effigy#458).
    #[test]
    fn a_publisher_that_lost_the_scope_cannot_change_what_readers_see() {
        let (_home, ledger) = fixture();
        ledger.clear();
        let run = ledger.stage(WaitOwner::Run);
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
        ledger.commit(&run);
        assert_eq!(
            ledger.read("api").and_then(|state| state.waiting_on),
            Some(WaitOn::Environment {
                healthy: Some(3),
                total: Some(4)
            })
        );

        // The planner stages its complete snapshot before it starts any
        // listener, and commits it afterwards. A reader paused in between still
        // resolves the run's whole snapshot, never an empty one.
        let planner = ledger.stage(WaitOwner::Planner);
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
        assert_eq!(
            ledger.read("api").and_then(|state| state.waiting_on),
            Some(WaitOn::Environment {
                healthy: Some(3),
                total: Some(4)
            }),
            "a reader between staging and commit sees the previous complete snapshot"
        );
        assert!(
            !planner.is_visible(),
            "the staged planner snapshot is not visible before its commit"
        );
        assert!(
            run.is_visible(),
            "the run's snapshot stays visible until the planner commits"
        );
        ledger.commit(&planner);
        assert!(planner.is_visible());
        assert_eq!(
            ledger.read("api").and_then(|state| state.waiting_on),
            Some(WaitOn::Probe {
                probe: "GET / -> 200".to_owned()
            })
        );

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
