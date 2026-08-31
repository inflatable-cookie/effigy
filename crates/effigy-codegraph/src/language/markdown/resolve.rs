use std::collections::BTreeSet;
use std::path::{Component, Path};

use super::paths::slugify;
use crate::error::CodeGraphError;
use crate::extractor::file_graph_id;
use crate::model::{EdgeRecord, ReferenceRecord};
use crate::storage::GraphStore;
use crate::GraphId;

const DOC_REL_KIND: &str = "doc-rel";

pub(crate) fn demote_typed_relations(store: &GraphStore) -> Result<bool, CodeGraphError> {
    let mut changed = false;
    for mut edge in store.list_edges()? {
        if edge.kind != DOC_REL_KIND || edge.to_id.is_none() {
            continue;
        }
        let Some(dest) = typed_edge_dest(&edge) else {
            continue;
        };
        edge.to_id = None;
        edge.unresolved_target = Some(dest);
        store.save_edge(&edge)?;
        changed = true;
    }
    for mut reference in store.list_references()? {
        if reference.kind != DOC_REL_KIND || reference.target_id.is_none() {
            continue;
        }
        let Some(dest) = typed_reference_dest(&reference) else {
            continue;
        };
        reference.target_id = None;
        reference.unresolved_target = Some(dest);
        store.save_reference(&reference)?;
        changed = true;
    }
    Ok(changed)
}

pub(crate) fn resolve_typed_relations(store: &GraphStore) -> Result<bool, CodeGraphError> {
    let mut live_ids = BTreeSet::new();
    for file in store.list_files()? {
        live_ids.insert(file.id.to_string());
    }
    for symbol in store.list_symbols()? {
        live_ids.insert(symbol.id.to_string());
    }
    let mut changed = false;
    for mut edge in store.list_edges()? {
        if edge.kind != DOC_REL_KIND {
            continue;
        }
        let Some(dest) = typed_edge_dest(&edge) else {
            continue;
        };
        let (to_id, unresolved_target) =
            resolve_dest(&edge.provenance.source_path, &dest, &live_ids)?;
        if edge.to_id != to_id || edge.unresolved_target != unresolved_target {
            edge.to_id = to_id;
            edge.unresolved_target = unresolved_target;
            store.save_edge(&edge)?;
            changed = true;
        }
    }
    for mut reference in store.list_references()? {
        if reference.kind != DOC_REL_KIND {
            continue;
        }
        let Some(dest) = typed_reference_dest(&reference) else {
            continue;
        };
        let (target_id, unresolved_target) =
            resolve_dest(&reference.provenance.source_path, &dest, &live_ids)?;
        if reference.target_id != target_id || reference.unresolved_target != unresolved_target {
            reference.target_id = target_id;
            reference.unresolved_target = unresolved_target;
            store.save_reference(&reference)?;
            changed = true;
        }
    }
    Ok(changed)
}

/// Recover the destination exactly as the source document declared it.
///
/// A resolved edge stores the graph identity in `to_id`, so the declared link
/// text survives only in the record id. Retrieval reports source provenance, so
/// it needs the declared text, not the normalized identity.
pub(crate) fn typed_edge_dest(edge: &EdgeRecord) -> Option<String> {
    if let Some(dest) = &edge.unresolved_target {
        return Some(dest.clone());
    }
    let token = edge.provenance.detail.as_deref()?;
    let prefix = format!("edge:doc-rel:{}:{}:", edge.provenance.source_path, token);
    edge.id.as_str().strip_prefix(&prefix).map(str::to_owned)
}

/// Reference-side counterpart of [`typed_edge_dest`].
pub(crate) fn typed_reference_dest(reference: &ReferenceRecord) -> Option<String> {
    if let Some(dest) = &reference.unresolved_target {
        return Some(dest.clone());
    }
    let token = reference.provenance.detail.as_deref()?;
    let prefix = format!(
        "ref:doc-rel:{}:{}:",
        reference.provenance.source_path, token
    );
    let rest = reference.id.as_str().strip_prefix(&prefix)?;
    rest.split_once(':')
        .map(|(_, dest)| dest.to_owned())
        .filter(|dest| !dest.is_empty())
}

fn resolve_dest(
    source_path: &str,
    dest: &str,
    live_ids: &BTreeSet<String>,
) -> Result<(Option<GraphId>, Option<String>), CodeGraphError> {
    if dest.contains("://") {
        return Ok((None, Some(dest.to_owned())));
    }
    let (path_part, fragment) = match dest.split_once('#') {
        Some((path, fragment)) => (path, Some(fragment)),
        None => (dest, None),
    };
    let Some(path) = join_source_dest(source_path, path_part) else {
        return Ok((None, Some(dest.to_owned())));
    };
    match fragment {
        Some(fragment) => {
            let anchor = slugify(fragment);
            if anchor.is_empty() {
                return Ok((None, Some(dest.to_owned())));
            }
            let id = GraphId::new(format!("symbol:doc:{path}:#{anchor}"))?;
            if live_ids.contains(id.as_str()) {
                Ok((Some(id), None))
            } else {
                Ok((None, Some(dest.to_owned())))
            }
        }
        None => {
            let id = file_graph_id(&path)?;
            if live_ids.contains(id.as_str()) {
                Ok((Some(id), None))
            } else {
                Ok((None, Some(dest.to_owned())))
            }
        }
    }
}

fn join_source_dest(source_path: &str, dest_path: &str) -> Option<String> {
    let dest_path = dest_path.trim();
    if dest_path.is_empty() {
        return Some(source_path.to_owned());
    }
    let mut parts: Vec<String> = if dest_path.starts_with('/') {
        Vec::new()
    } else {
        Path::new(source_path)
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .components()
            .filter_map(|component| match component {
                Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect()
    };
    for component in Path::new(dest_path.trim_start_matches('/')).components() {
        match component {
            Component::CurDir | Component::RootDir => {}
            Component::Prefix(_) => return None,
            Component::ParentDir => {
                parts.pop()?;
            }
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
        }
    }
    Some(parts.join("/"))
}
