use effigy_gateway::loopback::LoopbackRegistry;
use effigy_gateway::routes::{RouteSource, RouteTable};

use crate::exec::{RunningComposeContainer, RunningComposeContainerInventory};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct LoopbackPruneOutcome {
    pub changed: bool,
    pub skipped_reason: Option<String>,
}

pub fn prune_loopback_assignments(
    registry: &mut LoopbackRegistry,
    route_table: &RouteTable,
    inventory: &RunningComposeContainerInventory,
) -> LoopbackPruneOutcome {
    if let Some(failures) = inventory.failure_summary() {
        return LoopbackPruneOutcome {
            changed: false,
            skipped_reason: Some(format!(
                "skipped stale loopback reclamation because runtime inventory is incomplete; all uncertain assignments were preserved ({failures})"
            )),
        };
    }
    let rows = inventory
        .rows
        .iter()
        .map(|row| row.row.clone())
        .collect::<Vec<_>>();
    LoopbackPruneOutcome {
        changed: prune_loopback_assignments_with_rows(registry, route_table, &rows),
        skipped_reason: None,
    }
}

pub fn prune_loopback_assignments_with_rows(
    registry: &mut LoopbackRegistry,
    route_table: &RouteTable,
    rows: &[RunningComposeContainer],
) -> bool {
    let active_identities = rows
        .iter()
        .flat_map(|row| {
            let mut identities = Vec::new();
            if let Some(project_name) = row.project_name.as_deref() {
                identities.push(project_name.to_owned());
                identities.push(format!("shared:{project_name}"));
                if let Some(working_dir) = row.working_dir.as_deref() {
                    identities.push(format!("project:{project_name}:{working_dir}"));
                    identities.push(format!("shared:{project_name}:{working_dir}"));
                }
            }
            identities
        })
        .collect::<std::collections::BTreeSet<_>>();
    let active_projects = rows
        .iter()
        .filter_map(|row| row.working_dir.as_deref())
        .collect::<std::collections::BTreeSet<_>>();
    let active_names_without_working_dir = rows
        .iter()
        .filter(|row| row.working_dir.is_none())
        .filter_map(|row| row.project_name.as_deref())
        .collect::<std::collections::BTreeSet<_>>();
    let active_ips = route_table
        .all_routes()
        .into_iter()
        .filter(|route| route.source == RouteSource::Container)
        .filter_map(|route| {
            route
                .dns_ip
                .filter(|_| active_projects.contains(route.project.as_str()))
        })
        .collect::<std::collections::BTreeSet<_>>();
    let stale = registry
        .assignments
        .iter()
        .filter(|(identity, assignment)| {
            let project_name_is_ambiguous = active_names_without_working_dir
                .iter()
                .any(|name| identity.starts_with(&format!("project:{name}:")));
            !active_identities.contains(identity.as_str())
                && !active_ips.contains(&assignment.ip)
                && !identity.starts_with("shared:")
                && !project_name_is_ambiguous
        })
        .map(|(identity, _)| identity.clone())
        .collect::<Vec<_>>();
    let changed = !stale.is_empty();
    for identity in stale {
        registry.deallocate(&identity);
    }
    changed
}

/// Report a skipped stale-loopback reclamation at most once per distinct
/// reason in this process.
///
/// One launch can allocate several loopback identities and rediscover the same
/// incomplete inventory each time. The first warning already names every failed
/// backend, profile and error, so an identical repeat adds no diagnostic value.
/// Distinct failures still print.
pub fn warn_skipped_reclamation(reason: &str) {
    static EMITTED: std::sync::OnceLock<std::sync::Mutex<std::collections::BTreeSet<String>>> =
        std::sync::OnceLock::new();
    let mut emitted = EMITTED
        .get_or_init(|| std::sync::Mutex::new(std::collections::BTreeSet::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if record_reclamation_warning(&mut emitted, reason) {
        eprintln!("[warn] {reason}");
    }
}

fn record_reclamation_warning(
    emitted: &mut std::collections::BTreeSet<String>,
    reason: &str,
) -> bool {
    emitted.insert(reason.to_owned())
}

#[cfg(test)]
mod tests {
    use super::record_reclamation_warning;
    use super::{prune_loopback_assignments, RunningComposeContainerInventory};
    use crate::exec::RuntimeInventoryFailure;
    use effigy_gateway::loopback::LoopbackRegistry;
    use effigy_gateway::routes::RouteTable;

    #[test]
    fn identical_skipped_reclamation_reasons_are_emitted_once() {
        let mut emitted = std::collections::BTreeSet::new();

        assert!(record_reclamation_warning(&mut emitted, "same reason"));
        assert!(!record_reclamation_warning(&mut emitted, "same reason"));
        assert!(record_reclamation_warning(&mut emitted, "different reason"));
    }

    #[test]
    fn unknown_inventory_preserves_live_and_shared_assignments() {
        let mut registry = LoopbackRegistry::new();
        registry
            .allocate("project:live:/tmp/live", "/tmp/live")
            .expect("allocate live project");
        registry
            .allocate("shared:shared:/tmp/live", "/tmp/live")
            .expect("allocate shared identity");
        registry
            .allocate("project:stale:/tmp/stale", "/tmp/stale")
            .expect("allocate stale project");
        let before = registry.assignments.clone();
        let inventory = RunningComposeContainerInventory {
            rows: Vec::new(),
            failures: vec![RuntimeInventoryFailure {
                backend: "docker".to_owned(),
                profile: "default".to_owned(),
                error: "non-Unicode DOCKER_HOST is not a resolvable endpoint".to_owned(),
            }],
        };

        let outcome = prune_loopback_assignments(&mut registry, &RouteTable::new(), &inventory);

        assert!(!outcome.changed);
        assert_eq!(registry.assignments, before);
        assert!(registry.get("project:live:/tmp/live").is_some());
        assert!(registry.get("shared:shared:/tmp/live").is_some());
        assert!(registry.get("project:stale:/tmp/stale").is_some());
        let reason = outcome.skipped_reason.expect("skipped-prune reason");
        assert!(reason.contains("docker profile `default`"));
        assert!(reason.contains("non-Unicode DOCKER_HOST"));
    }
}
