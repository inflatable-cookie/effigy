//! Explicit allocation removal under the gateway Store's owner gate.
//! The raw pre-edit document is retained as a journaled backup; removal and
//! endpoint quarantine commit in one publication, preserving unknown fields.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::{PortAllocation, PortRegistry, DEFAULT_BASE, DEFAULT_RANGE, MAX_PORT_EXCLUSIVE};
use crate::endpoint_store::EndpointStore;
use crate::error::GatewayError;

/// Free-range capacity for the default generated-container range size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PortCapacity {
    /// Ports per allocation (the generated-container default).
    pub range: u16,
    /// Registered allocations, including ones whose project is gone.
    pub allocations: usize,
    /// Whole ranges that a new allocation could take, packed greedily.
    pub allocatable_ranges: usize,
    /// Base port the next allocation would receive, if any range is free.
    pub next_base: Option<u16>,
}

impl PortRegistry {
    /// Count how many default-size ranges remain free above the default base.
    ///
    /// Counting uses the same greedy packing as allocation: each free gap
    /// yields `gap / range` ranges, so the count matches what repeated
    /// `allocate` calls would succeed in taking.
    pub fn capacity(&self) -> PortCapacity {
        let range = u32::from(DEFAULT_RANGE);
        let mut spans = self
            .allocations
            .values()
            .map(|alloc| {
                (
                    u32::from(alloc.base),
                    alloc.end_u32().min(MAX_PORT_EXCLUSIVE),
                )
            })
            .collect::<Vec<_>>();
        spans.sort_by_key(|&(base, _)| base);

        let mut cursor = u32::from(DEFAULT_BASE);
        let mut allocatable_ranges = 0usize;
        let mut next_base = None;
        for (start, end) in spans {
            if start > cursor {
                let gap = start - cursor;
                allocatable_ranges += (gap / range) as usize;
                if gap >= range && next_base.is_none() {
                    next_base = u16::try_from(cursor).ok();
                }
            }
            cursor = cursor.max(end);
        }
        if MAX_PORT_EXCLUSIVE > cursor {
            let gap = MAX_PORT_EXCLUSIVE - cursor;
            allocatable_ranges += (gap / range) as usize;
            if gap >= range && next_base.is_none() {
                next_base = u16::try_from(cursor).ok();
            }
        }

        PortCapacity {
            range: DEFAULT_RANGE,
            allocations: self.allocations.len(),
            allocatable_ranges,
            next_base,
        }
    }
}

/// What a locked decision wants removed, plus the value it reports.
#[derive(Debug)]
pub struct PortRegistryEdit<T> {
    /// Value returned to the caller, computed under the lock.
    pub value: T,
    /// Allocation names to remove. Empty means no write happens.
    pub remove: Vec<String>,
}

/// Result of a locked edit, including the evidence needed to audit a removal.
#[derive(Debug)]
pub struct PortRegistryEditOutcome<T> {
    pub value: T,
    /// Copy of the pre-edit file, present only when an entry was removed.
    pub backup: Option<PathBuf>,
    /// SHA-256 of the registry file before the edit, if it existed.
    pub sha256_before: Option<String>,
    /// SHA-256 of the registry file after the edit, if a write happened.
    pub sha256_after: Option<String>,
}

/// Run `decide` under the registry lock, then remove the entries it names.
///
/// The decision sees the registry as it is under the lock, so every check it
/// makes is revalidated against the same snapshot that gets written back.
pub fn edit_locked<T, E: From<GatewayError>>(
    path: &Path,
    decide: impl FnOnce(&PortRegistry) -> Result<PortRegistryEdit<T>, E>,
) -> Result<PortRegistryEditOutcome<T>, E> {
    EndpointStore::for_ports(path).transaction_with_backup(None, |snapshot| {
        let mut content =
            serde_json::to_string_pretty(snapshot.document("ports")?).map_err(|error| {
                GatewayError::EndpointStore {
                    reason: error.to_string(),
                }
            })?;
        content.push('\n');
        let registry = snapshot.ports()?;
        let edit = decide(&registry)?;
        let sha256_before = Some(sha256_hex(content.as_bytes()));
        if edit.remove.is_empty() {
            return Ok((
                PortRegistryEditOutcome {
                    value: edit.value,
                    backup: None,
                    sha256_before,
                    sha256_after: None,
                },
                None,
            ));
        }
        let backup = backup_path(path);
        let rewritten = remove_entries(path, &content, &registry, &edit.remove)?;
        let ports: PortRegistry =
            serde_json::from_str(&rewritten).map_err(|error| GatewayError::EndpointStore {
                reason: error.to_string(),
            })?;
        snapshot.set_ports(&ports)?;
        Ok((
            PortRegistryEditOutcome {
                value: edit.value,
                backup: Some(backup.clone()),
                sha256_before,
                sha256_after: Some(sha256_hex(rewritten.as_bytes())),
            },
            Some((backup, content.into_bytes())),
        ))
    })
}

fn remove_entries(
    path: &Path,
    content: &str,
    registry: &PortRegistry,
    names: &[String],
) -> Result<String, GatewayError> {
    let parse_error = |reason: String| GatewayError::RouteTableParseError {
        path: path.to_path_buf(),
        reason: format!("port registry: {reason}"),
    };
    let mut document: Value =
        serde_json::from_str(content).map_err(|error| parse_error(error.to_string()))?;
    let allocations = document
        .get_mut("allocations")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| parse_error("`allocations` object is missing".to_owned()))?;

    for name in names {
        let expected = registry
            .get(name)
            .ok_or_else(|| parse_error(format!("allocation `{name}` vanished under lock")))?;
        let raw = allocations
            .get(name)
            .ok_or_else(|| parse_error(format!("allocation `{name}` vanished under lock")))?;
        let decoded: PortAllocation = serde_json::from_value(raw.clone())
            .map_err(|error| parse_error(format!("allocation `{name}`: {error}")))?;
        if &decoded != expected {
            return Err(parse_error(format!(
                "allocation `{name}` changed under lock; refusing to remove it"
            )));
        }
    }
    for name in names {
        allocations.remove(name);
    }

    let mut rewritten = serde_json::to_string_pretty(&document)
        .map_err(|error| parse_error(format!("serialize: {error}")))?;
    rewritten.push('\n');
    Ok(rewritten)
}

fn backup_path(path: &Path) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or_default();
    let filename = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("ports.json");
    path.with_file_name(format!("{filename}.reclaim-backup-{nanos}"))
}

/// Lowercase hex SHA-256 digest, used for registry and plan evidence.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}
