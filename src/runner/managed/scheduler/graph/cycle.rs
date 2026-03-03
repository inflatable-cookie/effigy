pub(super) fn detect_dependency_cycle(
    dependencies: &[Vec<usize>],
    display_names: &[String],
) -> Option<Vec<String>> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum VisitState {
        Visiting,
        Visited,
    }

    fn visit(
        node: usize,
        dependencies: &[Vec<usize>],
        display_names: &[String],
        state: &mut Vec<Option<VisitState>>,
        stack: &mut Vec<usize>,
    ) -> Option<Vec<String>> {
        match state[node] {
            Some(VisitState::Visited) => return None,
            Some(VisitState::Visiting) => {
                if let Some(cycle_start) = stack.iter().position(|item| *item == node) {
                    let mut cycle = stack[cycle_start..]
                        .iter()
                        .map(|index| display_names[*index].clone())
                        .collect::<Vec<String>>();
                    cycle.push(display_names[node].clone());
                    return Some(cycle);
                }
                return Some(vec![
                    display_names[node].clone(),
                    display_names[node].clone(),
                ]);
            }
            None => {}
        }

        state[node] = Some(VisitState::Visiting);
        stack.push(node);

        for dependency in &dependencies[node] {
            if let Some(cycle) = visit(*dependency, dependencies, display_names, state, stack) {
                return Some(cycle);
            }
        }

        stack.pop();
        state[node] = Some(VisitState::Visited);
        None
    }

    let mut state = vec![None; dependencies.len()];
    let mut stack = Vec::<usize>::new();
    for node in 0..dependencies.len() {
        if let Some(cycle) = visit(node, dependencies, display_names, &mut state, &mut stack) {
            return Some(cycle);
        }
    }
    None
}
