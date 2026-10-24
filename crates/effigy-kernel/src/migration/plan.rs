//! Select the complete relational closure before creating any destination.
use super::*;
use crate::id::{CheckoutId, CheckoutKind, ResourceKind, Scope};
use crate::store::StoreReader;
use rusqlite::OptionalExtension;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Serialize)]
pub struct Partition {
    pub checkout: Option<CheckoutId>,
    pub checkout_aliases: Vec<CheckoutId>,
    pub path_hint: Option<String>,
    pub git_dir_hint: Option<String>,
    pub resources: BTreeSet<String>,
    pub runs: BTreeSet<String>,
    pub objects: BTreeSet<String>,
    pub release_owners: BTreeSet<String>,
    pub unresolved_runs: usize,
    /// Holds current authority: an unreleased claim's resource or holder, or a
    /// run reachable from an open run, a claim's run or an unreleased
    /// attachment. A partition that is not live is completed history (Q-010:
    /// "Completed history may be dropped"), so an unresolvable checkout that
    /// owns only history is dropped rather than blocking the cutover.
    pub live: bool,
}
#[derive(Debug, Serialize)]
pub struct ImportPlan {
    pub preflight: Preflight,
    pub partitions: Vec<Partition>,
}

/// Plans only a checkpointed source with no observed live custodian. This
/// still grants no import authority: the importer must fence the source and
/// revalidate every destination identity before staging.
pub fn plan(cx: &ObserveContext, root: &StateRoot) -> Result<ImportPlan, MigrationError> {
    let legacy = root.legacy_source();
    let root = &legacy;
    let preflight = inspect(cx, root)?;
    if !preflight.ready() {
        return Err(MigrationError::Blocked(preflight));
    }
    let reader = match Store::read_sealed(cx, root) {
        Observed::Verified(reader, _) => reader,
        Observed::Absent(_) => {
            return Ok(ImportPlan {
                preflight,
                partitions: Vec::new(),
            })
        }
        Observed::Unknown(reason) => return Err(inconsistent(reason).into()),
    };
    reader
        .conn()
        .execute_batch("BEGIN")
        .map_err(StoreError::from)?;
    let partitions = partitions(cx, &reader)?;
    Ok(ImportPlan {
        preflight,
        partitions,
    })
}

pub(super) fn partitions(
    cx: &ObserveContext,
    reader: &StoreReader,
) -> Result<Vec<Partition>, StoreError> {
    let conn = reader.conn();
    let resources = inspect_resources(cx, conn)?;
    let mut owners: BTreeMap<String, String> = BTreeMap::new();
    let mut partitions: BTreeMap<String, Partition> = BTreeMap::new();
    // Checkout coordinates are authoritative; path hints only help the engine
    // locate and verify that identity. No deleted checkout is redirected.
    for (name, id) in &resources {
        let key = match id.scope() {
            Scope::Checkout(checkout) => {
                let key = crate::store::token(&checkout);
                if !partitions.contains_key(&key) {
                    let record: Option<(u8, Option<String>, Option<String>)> = conn
                        .query_row(
                            "SELECT kind,path_hint,git_dir_hint FROM checkouts WHERE token=?1",
                            [&key],
                            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                        )
                        .optional()?;
                    let (kind, path_hint, git_dir_hint) = record.ok_or_else(|| {
                        StoreError::Corrupt(format!("missing legacy checkout {key}"))
                    })?;
                    if kind != checkout.kind() as u8
                        || matches!(
                            checkout.kind(),
                            CheckoutKind::Machine | CheckoutKind::Filesystem
                        )
                    {
                        return Err(StoreError::Corrupt(format!(
                            "legacy checkout {key} kind mismatch"
                        )));
                    }
                    partitions.insert(
                        key.clone(),
                        Partition {
                            checkout: Some(checkout),
                            checkout_aliases: Vec::new(),
                            path_hint,
                            git_dir_hint,
                            resources: BTreeSet::new(),
                            runs: BTreeSet::new(),
                            objects: BTreeSet::new(),
                            release_owners: BTreeSet::new(),
                            unresolved_runs: 0,
                            live: false,
                        },
                    );
                }
                key
            }
            Scope::Machine if id.is_machine_fact() => {
                partitions
                    .entry("machine".into())
                    .or_insert_with(|| Partition {
                        checkout: None,
                        checkout_aliases: Vec::new(),
                        path_hint: None,
                        git_dir_hint: None,
                        resources: BTreeSet::new(),
                        runs: BTreeSet::new(),
                        objects: BTreeSet::new(),
                        release_owners: BTreeSet::new(),
                        unresolved_runs: 0,
                        live: false,
                    });
                "machine".into()
            }
            Scope::Machine => continue,
        };
        owners.insert(name.clone(), key);
    }
    let mut query =
        conn.prepare("SELECT resource,holder,state,run_id FROM claims WHERE state<>'released'")?;
    let claims = query
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })?
        .take(ENTRY_LIMIT + 1)
        .collect::<Result<Vec<_>, _>>()?;
    if claims.len() > ENTRY_LIMIT {
        return Err(StoreError::Corrupt(
            "legacy claim planning limit exceeded".into(),
        ));
    }
    // Data claims belong to the environment that holds them, including a
    // quarantined reservation's exact holder. A claim without a locatable
    // authority is a refusal, not a new machine-wide data registry.
    for (resource, holder, _, _) in &claims {
        let id = resources
            .get(resource)
            .ok_or_else(|| StoreError::Corrupt("claim has no resource".into()))?;
        if id.kind() == ResourceKind::Data {
            let key = holder
                .as_ref()
                .and_then(|holder| owners.get(holder))
                .ok_or_else(|| {
                    StoreError::Corrupt(format!(
                        "legacy data claim {resource} has no locatable checkout holder"
                    ))
                })?
                .clone();
            if key == "machine" {
                return Err(StoreError::Corrupt(
                    "machine-owned data claim is outside the migration allowlist".into(),
                ));
            }
            owners.insert(resource.clone(), key);
        }
    }
    let dropped_releases =
        super::release::assign(reader, &resources, &mut owners, &mut partitions)?;
    let mut seeds = BTreeSet::new();
    // Opaque engine payloads and immutable object pointers are product state,
    // even when the operation which published them completed long ago.
    let engine = if has_column(conn, "resources", "engine")? {
        "engine"
    } else {
        "NULL"
    };
    let backend = if has_column(conn, "resources", "backend")? {
        "backend"
    } else {
        "NULL"
    };
    let mut query = conn.prepare(&format!("SELECT name FROM resources WHERE desired IS NOT NULL OR last_applied IS NOT NULL OR state IS NOT NULL OR {engine} IS NOT NULL OR {backend} IS NOT NULL OR kind IN (3,5,7)"))?;
    for name in query.query_map([], |r| r.get::<_, String>(0))? {
        seeds.insert(name?);
    }
    for (resource, holder, _, _) in &claims {
        seeds.insert(resource.clone());
        seeds.extend(holder.clone());
    }
    let mut runs = BTreeMap::new();
    // A run whose every recorded process identity belongs to an earlier boot,
    // or which recorded none and predates this boot, has no process left that
    // could hold authority. It is still imported when its checkout resolves;
    // it just never makes an unlocatable partition live.
    let current_boot = prior_boot_reference();
    let column = |name: &'static str| -> rusqlite::Result<&'static str> {
        Ok(if has_column(conn, "runs", name)? {
            name
        } else {
            "NULL"
        })
    };
    let (owner_identity, supervisor_identity, sentinel_identity) = (
        column("owner_identity")?,
        column("supervisor_identity")?,
        column("sentinel_identity")?,
    );
    let mut dead_runs = BTreeSet::new();
    let mut query = conn.prepare(&format!("SELECT run_id,owner,parent_run,phase<>'closed' OR cleanup_outcome IS NULL OR cleanup_outcome<>'complete',{owner_identity},{supervisor_identity},{sentinel_identity},created_at FROM runs"))?;
    let mut rows = query.query([])?;
    while let Some(row) = rows.next()? {
        cx.check("legacy closure planning")
            .map_err(StoreError::Interrupted)?;
        if runs.len() == ENTRY_LIMIT {
            return Err(StoreError::Corrupt(
                "legacy run planning limit exceeded".into(),
            ));
        }
        let id: String = row.get(0)?;
        id.parse::<crate::id::RunId>()?;
        if let Some((boot, boot_wall_ms)) = current_boot.as_ref() {
            let identities = [
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
            ];
            if from_prior_boot(&identities, row.get::<_, i64>(7)?, boot, *boot_wall_ms) {
                dead_runs.insert(id.clone());
            }
        }
        runs.insert(
            id,
            (
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, bool>(3)?,
            ),
        );
    }
    let mut kept_runs: BTreeSet<String> = runs
        .iter()
        .filter(|(_, (_, _, open))| *open)
        .map(|(id, _)| id.clone())
        .collect();
    // Current authority, before history and release ownership widen the set.
    let mut live_runs: BTreeSet<String> = kept_runs.difference(&dead_runs).cloned().collect();
    let mut live_resources = BTreeSet::new();
    for (resource, holder, _, run) in &claims {
        live_resources.insert(resource.clone());
        live_resources.extend(holder.clone());
        live_runs.extend(run.clone());
    }
    kept_runs.extend(
        partitions
            .values()
            .flat_map(|part| part.runs.iter().cloned()),
    );
    // Current claim provenance and live attachments cannot dangle. Keep only
    // their required run closure, rather than unrelated completed history.
    for (_, _, _, run) in &claims {
        kept_runs.extend(run.clone());
    }
    let mut query = conn.prepare("SELECT run_id FROM attachments WHERE state<>'released'")?;
    for id in query.query_map([], |r| r.get::<_, String>(0))? {
        let id = id?;
        live_runs.insert(id.clone());
        kept_runs.insert(id);
    }
    close_runs(cx, &runs, &mut live_runs)?;
    close_runs(cx, &runs, &mut kept_runs)?;
    for id in &kept_runs {
        let (owner, _, _) = runs.get(id).ok_or_else(|| {
            StoreError::Corrupt(format!("legacy closure references missing run {id}"))
        })?;
        seeds.insert(owner.clone());
    }
    // Object references may be the only publication pointer in an older
    // additive schema. Their resources must survive independently of history.
    let mut query = conn.prepare("SELECT resource FROM object_refs")?;
    for name in query.query_map([], |r| r.get::<_, String>(0))? {
        seeds.insert(name?);
    }
    for name in seeds {
        if dropped_releases.contains(&name) {
            continue;
        }
        let key = owners.get(&name).ok_or_else(|| {
            StoreError::Corrupt(format!(
                "legacy live resource {name} has no permitted destination"
            ))
        })?;
        partitions
            .get_mut(key)
            .expect("owner partition was inserted")
            .resources
            .insert(name);
    }
    for id in kept_runs {
        let (owner, _, unresolved) = &runs[&id];
        let key = owners.get(owner).ok_or_else(|| {
            StoreError::Corrupt(format!(
                "legacy run {id} owner has no permitted destination"
            ))
        })?;
        let part = partitions
            .get_mut(key)
            .expect("owner partition was inserted");
        part.runs.insert(id);
        part.unresolved_runs += usize::from(*unresolved);
    }
    let mut query = conn.prepare("SELECT digest,resource FROM object_refs")?;
    for row in query.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (digest, resource) = row?;
        crate::objects::validate_digest(&digest)?;
        let key = owners
            .get(&resource)
            .ok_or_else(|| StoreError::Corrupt("object resource has no destination".into()))?;
        partitions
            .get_mut(key)
            .expect("owner partition was inserted")
            .objects
            .insert(digest);
    }
    for part in partitions.values_mut() {
        part.live = !part.release_owners.is_empty()
            || part.runs.iter().any(|run| live_runs.contains(run))
            || part
                .resources
                .iter()
                .any(|resource| live_resources.contains(resource));
    }
    Ok(partitions
        .into_values()
        .filter(|p| !p.resources.is_empty() || !p.runs.is_empty())
        .collect())
}

type RunRows = BTreeMap<String, (String, Option<String>, bool)>;

/// This boot's identity and its wall-clock start, or `None` when either is
/// unknown (then no run is treated as dead).
fn prior_boot_reference() -> Option<(String, i64)> {
    let crate::observe::Observed::Verified(clock, _) = crate::ident::boot_clock() else {
        return None;
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis();
    let boot_wall_ms = now.checked_sub(clock.elapsed.as_millis())?;
    Some((
        clock.boot.as_str().to_owned(),
        i64::try_from(boot_wall_ms).ok()?,
    ))
}

/// Margin for clock adjustment between a run's wall-clock creation time and
/// this boot's derived start.
const BOOT_MARGIN_MS: i64 = 60_000;

pub(super) fn from_prior_boot(
    identities: &[Option<String>],
    created_at_ms: i64,
    current_boot: &str,
    boot_wall_ms: i64,
) -> bool {
    let boots: Vec<Option<String>> = identities
        .iter()
        .flatten()
        .map(|text| {
            serde_json::from_str::<serde_json::Value>(text)
                .ok()
                .and_then(|value| value.get("boot")?.as_str().map(str::to_owned))
        })
        .collect();
    if boots.is_empty() {
        return created_at_ms < boot_wall_ms - BOOT_MARGIN_MS;
    }
    // Any unreadable identity, or any from this boot, keeps the run live.
    boots
        .iter()
        .all(|boot| boot.as_deref().is_some_and(|boot| boot != current_boot))
}

/// Close a run set over parent and child links, so no kept run dangles.
fn close_runs(
    cx: &ObserveContext,
    runs: &RunRows,
    kept: &mut BTreeSet<String>,
) -> Result<(), StoreError> {
    loop {
        let before = kept.len();
        for (id, (_, parent, _)) in runs {
            if kept.contains(id) {
                kept.extend(parent.clone());
            }
            if parent.as_ref().is_some_and(|parent| kept.contains(parent)) {
                kept.insert(id.clone());
            }
        }
        if kept.len() == before {
            return Ok(());
        }
        cx.check("legacy child closure planning")
            .map_err(StoreError::Interrupted)?;
    }
}

pub(super) fn has_column(conn: &Connection, table: &str, column: &str) -> rusqlite::Result<bool> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info(?1) WHERE name=?2)",
        [table, column],
        |row| row.get(0),
    )
}

#[cfg(test)]
mod tests {
    use super::from_prior_boot;

    const BOOT: &str = "20a888e4-10fc-4798-a139-fe8e08758c34";
    const OLD: &str = "bd26e49d-3769-45f6-a60e-c19276638748";
    const BOOT_WALL: i64 = 1_000_000_000;

    fn identity(boot: &str) -> Option<String> {
        Some(format!(
            r#"{{"boot":"{boot}","pid":7,"start":{{"MacOs":{{"seconds":1,"microseconds":2}}}}}}"#
        ))
    }

    #[test]
    fn a_run_is_from_a_prior_boot_only_when_every_identity_says_so() {
        assert!(from_prior_boot(
            &[identity(OLD), None, identity(OLD)],
            BOOT_WALL,
            BOOT,
            BOOT_WALL
        ));
        assert!(!from_prior_boot(
            &[identity(OLD), identity(BOOT)],
            0,
            BOOT,
            BOOT_WALL
        ));
        assert!(!from_prior_boot(
            &[Some("not json".into())],
            0,
            BOOT,
            BOOT_WALL
        ));
    }

    #[test]
    fn a_run_without_identities_is_prior_only_when_created_before_this_boot() {
        assert!(from_prior_boot(
            &[None, None, None],
            BOOT_WALL - 120_000,
            BOOT,
            BOOT_WALL
        ));
        // Within the clock margin, or after boot, it may be this boot's run.
        assert!(!from_prior_boot(
            &[None, None, None],
            BOOT_WALL - 30_000,
            BOOT,
            BOOT_WALL
        ));
        assert!(!from_prior_boot(&[], BOOT_WALL + 1, BOOT, BOOT_WALL));
    }
}
