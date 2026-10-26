//! What the oracles read: a read-only snapshot of one fault HOME after
//! recovery, plus the audit rows and fake logs written while it ran.
use super::*;
use crate::custody::ProcessReceipt;
use crate::fault::Event;
use crate::gate::GateState;
use crate::id::RunId;
use crate::ident::{self, ProcessIdentity, ProcessState};
use crate::root::StateRoot;
use crate::run::{Effect, RunStatus};
use crate::store::Store;
use std::collections::{BTreeMap, BTreeSet};
use std::os::unix::fs::MetadataExt;

/// Audit triggers: instruments, never authority. They fire inside the kernel's
/// own transactions, so a crash cannot erase evidence of a double release or
/// an admission beside an unresolved predecessor. No primary keys: a second
/// release is recorded, never refused.
const AUDITS: &str = "
CREATE TABLE IF NOT EXISTS audit_releases(environment TEXT, run_id TEXT);
CREATE TRIGGER IF NOT EXISTS audit_release AFTER UPDATE OF state ON attachments
  WHEN NEW.state='released' AND OLD.state<>'released'
  BEGIN INSERT INTO audit_releases VALUES (NEW.environment, NEW.run_id); END;
CREATE TABLE IF NOT EXISTS audit_claim_releases(resource TEXT, token TEXT);
CREATE TRIGGER IF NOT EXISTS audit_claim_release AFTER UPDATE OF state ON claims
  WHEN NEW.state IN ('released','quarantined') AND OLD.state IN ('reserved','live')
  BEGIN INSERT INTO audit_claim_releases VALUES (NEW.resource, NEW.reservation_token); END;
CREATE TABLE IF NOT EXISTS audit_admissions(run_id TEXT, owner TEXT, unresolved INTEGER, open INTEGER);
CREATE TRIGGER IF NOT EXISTS audit_admission BEFORE INSERT ON runs
  BEGIN INSERT INTO audit_admissions SELECT NEW.run_id, NEW.owner,
    (SELECT count(*) FROM runs p WHERE p.owner=NEW.owner AND (p.phase<>'closed'
       OR ((p.cleanup_outcome IS NULL OR p.cleanup_outcome<>'complete') AND p.residue_ack IS NULL))),
    (SELECT count(*) FROM runs p WHERE p.owner=NEW.owner AND p.phase<>'closed'); END;
";
pub(super) fn install_audits(home: &Path) {
    let cx = mutate();
    let root = StateRoot::effects(&cx, home).expect("root");
    let store = Store::open(&cx, &root).expect("store");
    store.conn().execute_batch(AUDITS).expect("audits");
}

#[derive(Debug, Clone)]
pub(super) struct RunRow {
    pub id: RunId,
    pub owner: String,
    pub phase: String,
    pub cleanup: Option<String>,
    pub acknowledged: bool,
    pub sentinel: Option<ProcessIdentity>,
}
#[derive(Debug, Clone)]
pub(super) struct GateRead {
    pub file: String,
    pub observed: Observed<GateState>,
}
#[derive(Debug, Clone)]
pub(super) struct Custodied {
    pub run: RunId,
    pub sequence: i64,
    pub receipt: Option<ProcessReceipt>,
}
/// A process the scenario declared, observed fresh at capture.
#[derive(Debug, Clone)]
pub(super) struct Proc {
    pub pid: u32,
    pub alive: bool,
    pub nonce: Option<String>,
}
#[derive(Debug, Clone)]
pub(super) struct Admission {
    pub run: String,
    pub owner: String,
    pub unresolved: i64,
    pub open: i64,
}
/// An `atomic_replace` target: its name, allowed bodies and actual body.
pub(super) type Replaced = (String, Vec<Vec<u8>>, Option<Vec<u8>>);
#[derive(Debug, Default)]
pub(super) struct World {
    pub runs: Vec<RunRow>,
    pub effects: BTreeMap<String, Vec<Effect>>,
    pub statuses: BTreeMap<String, Observed<RunStatus>>,
    /// Successor admission without acknowledgement, attempted per incomplete run.
    pub successor_blocked: BTreeMap<String, bool>,
    pub gates: Vec<GateRead>,
    pub scopes_held: Vec<String>,
    pub attachments: Vec<(String, String, String)>,
    pub releases: Vec<(String, String)>,
    pub claims: Vec<(String, String, Option<String>)>,
    pub claim_releases: Vec<(String, Option<String>)>,
    pub admissions: Vec<Admission>,
    pub integrity: Option<Result<(), String>>,
    pub custody: Vec<Custodied>,
    /// Workload processes that announced themselves in `<home>/pids`.
    pub workloads: Vec<Proc>,
    /// Probe subprocesses that announced themselves (O14).
    pub probes: Vec<Proc>,
    /// Foreign fixtures: alive, and every caught signal they recorded (O12).
    pub foreign: Vec<(Proc, Vec<String>)>,
    pub events: Vec<Event>,
    /// The durable strict-fake call log.
    pub calls: Vec<String>,
    pub replaced: Vec<Replaced>,
    /// Observability faults: (label, baseline exit, faulted exit) (O9).
    pub outcomes: Vec<(String, i32, i32)>,
    /// Static or Observe steps: (label, filesystem diff) (O10).
    pub diffs: Vec<(String, Vec<String>)>,
    /// Probe processes measured against their evidence bound (O14).
    pub probe_bound: Option<Duration>,
    /// Holders the harness declares live at capture (a stalled child).
    pub live_holders: BTreeSet<u32>,
}

pub(super) fn identity(text: &str) -> Option<ProcessIdentity> {
    serde_json::from_str(text).ok()
}
fn alive(pid: u32) -> bool {
    !matches!(
        ident::process_state(pid),
        Observed::Absent(_) | Observed::Verified(ProcessState::Zombie, _)
    )
}
pub(super) fn identity_alive(id: &ProcessIdentity) -> bool {
    alive(id.pid) && ident::process_identity(id.pid).verified() == Some(id)
}
pub(super) fn proc(pid: u32, key: &[u8]) -> Proc {
    let alive = alive(pid);
    let nonce = alive
        .then(|| {
            crate::custody::platform::read_nonce_key(pid, key)
                .verified()
                .cloned()
        })
        .flatten();
    // Re-read: a process that exited during the nonce read is not alive.
    let alive = alive && self::alive(pid);
    Proc { pid, alive, nonce }
}
pub(super) fn pids(file: &Path) -> Vec<u32> {
    std::fs::read_to_string(file)
        .unwrap_or_default()
        .split_whitespace()
        .filter_map(|pid| pid.parse().ok())
        .collect()
}

impl World {
    /// Just the run rows, for recovery and quiescence.
    pub(super) fn capture_runs(home: &Path) -> Vec<RunRow> {
        let cx = observe();
        let Observed::Verified(root, _) = StateRoot::observe(&cx, home) else {
            return vec![];
        };
        match Store::read(&cx, &root) {
            Observed::Verified(reader, _) => read_runs(reader.conn()),
            _ => vec![],
        }
    }
    /// Reads `home` without repairing anything. The caller has already run
    /// recovery and waited for recorded sentinels to finish.
    pub(super) fn capture(home: &Path) -> Self {
        let mut world = World {
            events: crate::fault::events(),
            calls: std::fs::read_to_string(home.join("calls"))
                .unwrap_or_default()
                .lines()
                .map(str::to_owned)
                .collect(),
            workloads: pids(&home.join("pids"))
                .into_iter()
                .map(|pid| proc(pid, b"EFFIGY_CUSTODY_NONCE="))
                .collect(),
            ..World::default()
        };
        world.read_gates(home);
        let cx = observe();
        let Observed::Verified(root, _) = StateRoot::observe(&cx, home) else {
            return world;
        };
        let Observed::Verified(reader, _) = Store::read(&cx, &root) else {
            return world;
        };
        let conn = reader.conn();
        world.runs = read_runs(conn);
        for run in &world.runs {
            let effects = crate::run::journal_effects(conn, run.id).expect("effects");
            for effect in &effects {
                if effect.spec.kind == crate::custody::KIND {
                    world.custody.push(Custodied {
                        run: run.id,
                        sequence: effect.sequence,
                        receipt: effect
                            .receipt
                            .clone()
                            .and_then(|r| serde_json::from_value(r).ok()),
                    });
                }
            }
            world.effects.insert(run.id.to_string(), effects);
            world.statuses.insert(
                run.id.to_string(),
                crate::run::status(&cx, &root, &reader, run.id, next()),
            );
        }
        let rows = |sql: &str| -> Vec<Vec<Option<String>>> {
            let Ok(mut stmt) = conn.prepare(sql) else {
                return vec![];
            };
            let width = stmt.column_count();
            stmt.query_map([], |r| {
                (0..width)
                    .map(|i| r.get::<_, Option<String>>(i))
                    .collect::<Result<Vec<_>, _>>()
            })
            .expect("audit rows")
            .map(|row| row.expect("audit row"))
            .collect()
        };
        let text = |value: &Option<String>| value.clone().unwrap_or_default();
        world.attachments = rows("SELECT environment,run_id,state FROM attachments")
            .iter()
            .map(|r| (text(&r[0]), text(&r[1]), text(&r[2])))
            .collect();
        world.releases = rows("SELECT environment,run_id FROM audit_releases")
            .iter()
            .map(|r| (text(&r[0]), text(&r[1])))
            .collect();
        world.claims = rows("SELECT resource,state,reservation_token FROM claims")
            .iter()
            .map(|r| (text(&r[0]), text(&r[1]), r[2].clone()))
            .collect();
        world.claim_releases = rows("SELECT resource,token FROM audit_claim_releases")
            .iter()
            .map(|r| (text(&r[0]), r[1].clone()))
            .collect();
        world.admissions = rows(
            "SELECT run_id,owner,cast(unresolved AS TEXT),cast(open AS TEXT) FROM audit_admissions",
        )
        .iter()
        .map(|r| Admission {
            run: text(&r[0]),
            owner: text(&r[1]),
            unresolved: text(&r[2]).parse().unwrap_or(-1),
            open: text(&r[3]).parse().unwrap_or(-1),
        })
        .collect();
        world
    }
    /// Every gate inode on disk, probed; every scope gate, tested exclusively.
    fn read_gates(&mut self, home: &Path) {
        let gates = home.join(".effigy/state/gates");
        let Ok(scopes) = std::fs::read_dir(&gates) else {
            return;
        };
        let cx = observe();
        let Observed::Verified(root, _) = StateRoot::observe(&cx, home) else {
            return;
        };
        for entry in scopes.map(|e| e.expect("gate entry")) {
            let name = entry.file_name().into_string().expect("gate name");
            if let Some(scope) = name.strip_suffix(".scope") {
                let file = std::fs::File::open(entry.path()).expect("scope gate");
                if !crate::fs::try_flock(&file, libc::LOCK_EX).unwrap_or(false) {
                    self.scopes_held.push(scope.into());
                }
                continue;
            }
            if !entry.metadata().expect("scope dir").is_dir() {
                continue;
            }
            for gate in std::fs::read_dir(entry.path()).expect("scope dir") {
                let gate = gate.expect("gate");
                let file = gate.file_name().into_string().expect("gate name");
                let Some(rid) = file.strip_suffix(".gate") else {
                    continue;
                };
                self.gates.push(GateRead {
                    file: format!("{name}/{file}"),
                    observed: crate::gate::probe_names(&cx, &root, &name, rid),
                });
            }
        }
    }
}

fn read_runs(conn: &rusqlite::Connection) -> Vec<RunRow> {
    let mut stmt = conn
        .prepare("SELECT run_id,owner,phase,cleanup_outcome,residue_ack IS NOT NULL,sentinel_identity FROM runs ORDER BY generation, created_at")
        .expect("runs");
    stmt.query_map([], |r| {
        Ok(RunRow {
            id: r.get::<_, String>(0)?.parse().expect("run id"),
            owner: r.get(1)?,
            phase: r.get(2)?,
            cleanup: r.get(3)?,
            acknowledged: r.get(4)?,
            sentinel: r.get::<_, Option<String>>(5)?.as_deref().and_then(identity),
        })
    })
    .expect("runs")
    .map(|row| row.expect("run row"))
    .collect()
}
/// A content snapshot of every entry under `dir`, for O10 diffs.
pub(super) fn tree(dir: &Path) -> BTreeMap<String, String> {
    fn walk(base: &Path, dir: &Path, out: &mut BTreeMap<String, String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.map(|e| e.expect("entry")) {
            let path = entry.path();
            let meta = std::fs::symlink_metadata(&path).expect("metadata");
            let key = path
                .strip_prefix(base)
                .expect("under base")
                .display()
                .to_string();
            // SQLite's WAL index is shared memory: a reader's read mark is not
            // application state, and the volume's mapping timing must not turn
            // it into a reported write.
            if key.ends_with("-shm") {
                continue;
            }
            let body = if meta.is_file() {
                let bytes = std::fs::read(&path).unwrap_or_default();
                format!("{:?}", crate::id::Digest::of(&bytes))
            } else {
                String::new()
            };
            out.insert(
                key,
                format!(
                    "{:o} {} {} {} {body}",
                    meta.mode(),
                    meta.ino(),
                    meta.len(),
                    meta.mtime_nsec() + meta.mtime() * 1_000_000_000
                ),
            );
            if meta.is_dir() {
                walk(base, &path, out);
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}
pub(super) fn diff(
    before: &BTreeMap<String, String>,
    after: &BTreeMap<String, String>,
) -> Vec<String> {
    let mut changes = vec![];
    for (key, value) in after {
        match before.get(key) {
            None => changes.push(format!("created {key}")),
            Some(old) if old != value => changes.push(format!("changed {key}")),
            _ => {}
        }
    }
    for key in before.keys().filter(|k| !after.contains_key(*k)) {
        changes.push(format!("removed {key}"));
    }
    changes
}
