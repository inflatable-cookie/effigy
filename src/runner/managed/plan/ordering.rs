use std::collections::HashSet;

use super::super::super::ManagedProcessSpec;

pub(super) fn build_tab_order(
    resolved: &[super::ConcurrentResolvedProcess],
    processes: &[ManagedProcessSpec],
) -> Vec<String> {
    let mut tab_entries = resolved
        .iter()
        .map(|entry| (entry.spec.name.clone(), entry.tab_rank, entry.index))
        .collect::<Vec<(String, usize, usize)>>();
    tab_entries.sort_by(|a, b| {
        a.1.cmp(&b.1)
            .then_with(|| a.2.cmp(&b.2))
            .then_with(|| a.0.cmp(&b.0))
    });
    let mut tab_order = tab_entries
        .into_iter()
        .map(|(name, _, _)| name)
        .collect::<Vec<String>>();
    let mut seen = tab_order.iter().cloned().collect::<HashSet<String>>();
    for process in processes {
        if seen.insert(process.name.clone()) {
            tab_order.push(process.name.clone());
        }
    }
    tab_order
}

pub(super) fn sort_resolved_processes(resolved: &mut [super::ConcurrentResolvedProcess]) {
    resolved.sort_by(|a, b| {
        a.start_rank
            .cmp(&b.start_rank)
            .then_with(|| a.index.cmp(&b.index))
            .then_with(|| a.spec.name.cmp(&b.spec.name))
    });
}
