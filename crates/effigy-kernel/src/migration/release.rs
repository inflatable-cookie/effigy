//! Relocate legacy release state without changing its fenced resource identity.
use super::*;
use crate::ctx::EffectContext;
use crate::fs::DirFd;
use crate::gate::GateGuard;
use crate::id::{CheckoutId, CheckoutKind, ResourceKind, Scope};
use crate::store::StoreReader;
use rusqlite::OptionalExtension;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Component, Path};

fn invalid(message: &str) -> StoreError {
    StoreError::Corrupt(format!("legacy release: {message}"))
}
fn field<'a>(value: &'a Value, key: &str) -> Result<&'a str, StoreError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("missing text field"))
}
fn checkpoint(text: &str) -> Result<Value, StoreError> {
    if text.len() > 64 * 1024 {
        return Err(invalid("checkpoint exceeds bound"));
    }
    let value: Value = serde_json::from_str(text).map_err(|e| invalid(&e.to_string()))?;
    if field(&value, "schema")? != "effigy.release.checkpoint.v1"
        || !["paused", "awaiting-approval", "failed", "complete"]
            .contains(&field(&value, "status")?)
    {
        return Err(invalid("unsupported checkpoint"));
    }
    for key in ["release_id", "run_id"] {
        field(&value, key)?.parse::<crate::id::RunId>()?;
    }
    Ok(value)
}

pub(super) fn assign(
    reader: &StoreReader,
    resources: &BTreeMap<String, ResourceId>,
    owners: &mut BTreeMap<String, String>,
    parts: &mut BTreeMap<String, Partition>,
) -> Result<BTreeSet<String>, StoreError> {
    let mut dropped = BTreeSet::new();
    for (name, id) in resources {
        if id.kind() != ResourceKind::Store || id.scope() != Scope::Machine {
            continue;
        }
        let Some(path) = id
            .key()
            .first()
            .and_then(Option::as_deref)
            .and_then(|key| key.strip_prefix("release "))
        else {
            continue;
        };
        let engine: Option<String> =
            if super::plan::has_column(reader.conn(), "resources", "engine")? {
                reader.conn().query_row(
                    "SELECT engine FROM resources WHERE name=?1",
                    [name],
                    |r| r.get(0),
                )?
            } else {
                None
            };
        let cp = engine.as_deref().map(checkpoint).transpose()?;
        let open: bool = reader.conn().query_row("SELECT EXISTS(SELECT 1 FROM runs WHERE owner=?1 AND (phase<>'closed' OR cleanup_outcome IS NULL OR cleanup_outcome<>'complete'))", [name], |r| r.get(0))?;
        if !open && cp.as_ref().is_some_and(|cp| cp["status"] == "complete") {
            dropped.insert(name.clone());
            continue;
        }
        let record: Option<(String, u8, Option<String>)> = reader
            .conn()
            .query_row(
                "SELECT token,kind,git_dir_hint FROM checkouts WHERE path_hint=?1",
                [path],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let (token, kind, git_dir_hint) = record.ok_or_else(|| invalid("owner has no retained checkout identity; finish or drain this release with the previous Effigy before migration"))?;
        let kind = match kind {
            0 => CheckoutKind::Primary,
            1 => CheckoutKind::LinkedWorktree,
            2 => CheckoutKind::EphemeralClone,
            _ => return Err(invalid("invalid checkout kind")),
        };
        if token.len() != 32 || !token.is_ascii() {
            return Err(invalid("invalid checkout token"));
        }
        let mut bytes = [0; 16];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&token[index * 2..index * 2 + 2], 16)
                .map_err(|_| invalid("invalid checkout token"))?;
        }
        let part = parts.entry(token.clone()).or_insert_with(|| Partition {
            checkout: Some(CheckoutId::new(kind, bytes)),
            checkout_aliases: Vec::new(),
            path_hint: Some(path.into()),
            git_dir_hint,
            resources: BTreeSet::new(),
            runs: BTreeSet::new(),
            objects: BTreeSet::new(),
            release_owners: BTreeSet::new(),
            unresolved_runs: 0,
        });
        part.release_owners.insert(name.clone());
        if part.release_owners.len() > 1 {
            return Err(invalid("multiple release authorities for one checkout"));
        }
        if let Some(cp) = &cp {
            let run = field(cp, "run_id")?;
            let found: bool = reader.conn().query_row(
                "SELECT EXISTS(SELECT 1 FROM runs WHERE run_id=?1 AND owner=?2)",
                [run, name],
                |r| r.get(0),
            )?;
            if !found {
                return Err(invalid("checkpoint has no matching invocation"));
            }
            part.runs.insert(run.into());
        }
        owners.insert(name.clone(), token);
    }
    Ok(dropped)
}

fn json(dir: &DirFd, name: &str) -> Result<Option<Value>, StoreError> {
    let Some((file, stat)) =
        dir.open_file(&FileName::new(name)?, libc::O_RDONLY, None, Trust::Operator)?
    else {
        return Ok(None);
    };
    if stat.st_size < 0 || stat.st_size as u64 > 4 << 20 {
        return Err(invalid("record exceeds 4 MiB"));
    }
    let mut bytes = Vec::new();
    file.take((4 << 20) + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| crate::fs::FsError::io("release record read", dir.path(), &e))?;
    if bytes.len() as u64 != stat.st_size as u64 {
        return Err(invalid("record changed while read"));
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|e| invalid(&e.to_string()))
}
fn normal(path: &Path) -> bool {
    path.components()
        .all(|part| matches!(part, Component::Normal(_) | Component::RootDir))
}

pub(super) struct Target<'a> {
    pub stage: &'a StateRoot,
    pub final_state: &'a Path,
    pub checkout_root: &'a Path,
    pub guard: &'a GateGuard,
}
pub(super) fn copy_files(
    cx: &EffectContext,
    reader: &StoreReader,
    source: &StateRoot,
    part: &Partition,
    target: Target<'_>,
) -> Result<(), StoreError> {
    let Target {
        stage,
        final_state,
        checkout_root,
        guard,
    } = target;
    for owner in &part.release_owners {
        let engine: Option<String> = reader.conn().query_row(
            "SELECT engine FROM resources WHERE name=?1",
            [owner],
            |r| r.get(0),
        )?;
        let cp = engine.as_deref().map(checkpoint).transpose()?;
        let input = source
            .dir()
            .child("release", Trust::Operator, false)?
            .map(|dir| dir.child(owner, Trust::Operator, false))
            .transpose()?
            .flatten();
        let Some(input) = input else {
            if cp.is_some() {
                return Err(invalid("checkpoint record directory is missing"));
            }
            continue;
        };
        let Some(mut record) = json(&input, "record.json")? else {
            if cp.is_some() {
                return Err(invalid("checkpoint record is missing"));
            }
            continue;
        };
        if field(&record, "schema")? != "effigy.release.record.v1" {
            return Err(invalid("unsupported record"));
        }
        let canonical: Vec<u8> = reader.conn().query_row(
            "SELECT canonical FROM resources WHERE name=?1",
            [owner],
            |r| r.get(0),
        )?;
        let id = ResourceId::decode_canonical(&canonical)?;
        let original_root = id
            .key()
            .first()
            .and_then(Option::as_deref)
            .and_then(|key| key.strip_prefix("release "))
            .ok_or_else(|| invalid("invalid owner"))?;
        if field(&record, "root")? != original_root {
            return Err(invalid("record belongs to another checkout"));
        }
        let old_candidate = if let Some(cp) = &cp {
            for key in ["release_id", "run_id", "stage", "status", "waiting_on"] {
                if record.get(key) != cp.get(key) {
                    return Err(invalid("checkpoint and record disagree"));
                }
            }
            let path = Path::new(field(cp, "record")?);
            if !path.is_absolute()
                || !normal(path)
                || !path.ends_with(
                    Path::new(".effigy/state/release")
                        .join(owner)
                        .join("record.json"),
                )
            {
                return Err(invalid("checkpoint points outside its owned record"));
            }
            Some(path.parent().expect("record parent").join("candidate"))
        } else {
            None
        };
        let output = stage
            .dir()
            .child("release", Trust::Private, true)?
            .expect("created release family")
            .child(owner, Trust::Private, true)?
            .expect("created release owner");
        super::files::operator_tree(cx, &input, &output)?;
        let candidate = output.child("candidate", Trust::Private, false)?;
        let artifacts = record
            .get_mut("artifacts")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| invalid("missing artifacts"))?;
        for artifact in artifacts {
            let files = artifact
                .get_mut("files")
                .and_then(Value::as_array_mut)
                .ok_or_else(|| invalid("missing artifact files"))?;
            for file in files {
                cx.operation()
                    .check("release artifact migration")
                    .map_err(StoreError::Interrupted)?;
                let path = Path::new(field(file, "path")?);
                let relative = path
                    .strip_prefix(
                        old_candidate
                            .as_ref()
                            .ok_or_else(|| invalid("artifact without checkpoint"))?,
                    )
                    .map_err(|_| invalid("artifact outside candidate"))?;
                if relative.as_os_str().is_empty()
                    || !relative
                        .components()
                        .all(|p| matches!(p, Component::Normal(_)))
                {
                    return Err(invalid("invalid artifact path"));
                }
                let mut directory = candidate
                    .as_ref()
                    .ok_or_else(|| invalid("candidate directory missing"))?
                    .clone();
                for component in relative.parent().expect("relative parent").components() {
                    directory = directory
                        .child(
                            component
                                .as_os_str()
                                .to_str()
                                .ok_or_else(|| invalid("non UTF8 artifact"))?,
                            Trust::Private,
                            false,
                        )?
                        .ok_or_else(|| invalid("artifact directory missing"))?;
                }
                let name = FileName::new(
                    relative
                        .file_name()
                        .and_then(|s| s.to_str())
                        .ok_or_else(|| invalid("invalid artifact filename"))?,
                )?;
                let (mut bytes, stat) = directory
                    .open_file(&name, libc::O_RDONLY, None, Trust::Private)?
                    .ok_or_else(|| invalid("artifact missing"))?;
                let expected = file
                    .get("bytes")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| invalid("invalid artifact length"))?;
                if stat.st_size < 0 || stat.st_size as u64 != expected || expected > 4 << 30 {
                    return Err(invalid("artifact length mismatch"));
                }
                let mut hash = Sha256::new();
                let mut buffer = [0; 65536];
                let mut total = 0;
                loop {
                    cx.operation()
                        .check("release artifact digest")
                        .map_err(StoreError::Interrupted)?;
                    let n = bytes.read(&mut buffer).map_err(|e| {
                        crate::fs::FsError::io("artifact read", directory.path(), &e)
                    })?;
                    if n == 0 {
                        break;
                    }
                    total += n as u64;
                    if total > expected {
                        return Err(invalid("artifact grew"));
                    }
                    hash.update(&buffer[..n]);
                }
                if total != expected
                    || hash
                        .finalize()
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect::<String>()
                        != field(file, "sha256")?
                {
                    return Err(invalid("certified artifact digest mismatch"));
                }
                file["path"] = serde_json::json!(final_state
                    .join("release")
                    .join(owner)
                    .join("candidate")
                    .join(relative));
            }
        }
        record["root"] = serde_json::json!(checkout_root);
        let bytes = serde_json::to_vec_pretty(&record).map_err(|e| invalid(&e.to_string()))?;
        if bytes.len() > 4 << 20 {
            return Err(invalid("rebased record exceeds 4 MiB"));
        }
        crate::record::atomic_replace(
            cx,
            guard,
            &output,
            &FileName::new("record.json")?,
            &bytes,
            Default::default(),
        )?;
        output.sync()?;
    }
    Ok(())
}
