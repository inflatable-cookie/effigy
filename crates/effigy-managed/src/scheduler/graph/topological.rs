use std::collections::BTreeSet;

pub fn build_schedule_levels(
    step_count: usize,
    dependencies: &[Vec<usize>],
    dependents: &[Vec<usize>],
) -> Option<Vec<Vec<usize>>> {
    let mut indegree = dependencies.iter().map(Vec::len).collect::<Vec<usize>>();
    let mut ready = BTreeSet::<usize>::new();
    for (index, degree) in indegree.iter().enumerate() {
        if *degree == 0 {
            ready.insert(index);
        }
    }

    let mut levels = Vec::<Vec<usize>>::new();
    let mut processed = 0usize;
    while !ready.is_empty() {
        let current = ready.iter().copied().collect::<Vec<usize>>();
        for node in &current {
            ready.remove(node);
        }
        processed += current.len();
        for node in &current {
            for dependent in &dependents[*node] {
                indegree[*dependent] = indegree[*dependent].saturating_sub(1);
                if indegree[*dependent] == 0 {
                    ready.insert(*dependent);
                }
            }
        }
        levels.push(current);
    }

    if processed != step_count {
        return None;
    }
    Some(levels)
}
