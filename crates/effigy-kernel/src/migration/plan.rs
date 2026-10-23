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
    pub unresolved_runs: usize,
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
    let machine = |id: &ResourceId| {
        matches!(id.kind(), ResourceKind::Endpoint | ResourceKind::Gateway)
            || ["gateway-endpoints", "gateway-setup", "ports", "admission"]
                .into_iter()
                .any(|key| ResourceId::store(Scope::Machine, key).ok().as_ref() == Some(id))
    };
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
                    if kind != checkout.kind() as u8 || checkout.kind() == CheckoutKind::Machine {
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
                            unresolved_runs: 0,
                        },
                    );
                }
                key
            }
            Scope::Machine if machine(id) => {
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
                        unresolved_runs: 0,
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
    let mut query = conn.prepare("SELECT run_id,owner,parent_run,phase<>'closed' OR cleanup_outcome IS NULL OR cleanup_outcome<>'complete' FROM runs")?;
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
    // Current claim provenance and live attachments cannot dangle. Keep only
    // their required run closure, rather than unrelated completed history.
    for (_, _, _, run) in &claims {
        kept_runs.extend(run.clone());
    }
    let mut query = conn.prepare("SELECT run_id FROM attachments WHERE state<>'released'")?;
    for id in query.query_map([], |r| r.get::<_, String>(0))? {
        kept_runs.insert(id?);
    }
    loop {
        let before = kept_runs.len();
        for (id, (_, parent, _)) in &runs {
            if kept_runs.contains(id) {
                kept_runs.extend(parent.clone());
            }
            if parent
                .as_ref()
                .is_some_and(|parent| kept_runs.contains(parent))
            {
                kept_runs.insert(id.clone());
            }
        }
        if kept_runs.len() == before {
            break;
        }
        cx.check("legacy child closure planning")
            .map_err(StoreError::Interrupted)?;
    }
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
    Ok(partitions
        .into_values()
        .filter(|p| !p.resources.is_empty() || !p.runs.is_empty())
        .collect())
}

pub(super) fn has_column(conn: &Connection, table: &str, column: &str) -> rusqlite::Result<bool> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info(?1) WHERE name=?2)",
        [table, column],
        |row| row.get(0),
    )
}
