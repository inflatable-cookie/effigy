use std::collections::BTreeSet;

use effigy_tasks::normalize_builtin_test_suite;

use super::planning::BuiltinTestRunnable;

#[derive(Debug, Clone)]
pub(super) struct BuiltinSuiteSelection {
    pub(super) runnable: Vec<BuiltinTestRunnable>,
    pub(super) requested_suite: Option<String>,
    pub(super) passthrough: Vec<String>,
}

#[derive(Debug, Clone)]
pub(super) struct BuiltinSuiteSelectionError {
    pub(super) message: String,
    pub(super) available_runners: BTreeSet<String>,
}

pub(super) fn select_builtin_test_suite(
    mut runnable: Vec<BuiltinTestRunnable>,
    mut passthrough: Vec<String>,
) -> Result<BuiltinSuiteSelection, BuiltinSuiteSelectionError> {
    let available_runners = runnable
        .iter()
        .map(|plan| plan.runner.clone())
        .collect::<BTreeSet<String>>();
    let requested_suite_raw = passthrough.first().cloned();
    let requested_suite = passthrough.first().and_then(|candidate| {
        normalize_builtin_test_suite(candidate)
            .map(str::to_owned)
            .or_else(|| {
                if available_runners.contains(candidate) {
                    Some(candidate.clone())
                } else {
                    None
                }
            })
    });

    if let Some(selected) = requested_suite.as_ref() {
        passthrough.remove(0);
        runnable.retain(|entry| &entry.runner == selected);
        if runnable.is_empty() {
            let available = render_available_suites(&available_runners);
            let forwarded = passthrough.join(" ");
            let suggested = available_runners
                .iter()
                .map(|suite| {
                    if forwarded.is_empty() {
                        format!("effigy test {suite}")
                    } else {
                        format!("effigy test {suite} {forwarded}")
                    }
                })
                .collect::<Vec<String>>()
                .join(" | ");
            return Err(BuiltinSuiteSelectionError {
                message: format!(
                    "built-in `test` runner `{selected}` is not available in this target (available: {available}). Try one of: {suggested}. Use `effigy test --plan <args>` to preview suite routing before execution."
                ),
                available_runners,
            });
        }
    } else if !passthrough.is_empty() && available_runners.len() > 1 {
        let first = requested_suite_raw.unwrap_or_else(|| passthrough[0].clone());
        if let Some(suggested_suite) = suggest_suite_name(&first, &available_runners) {
            let remainder = passthrough.iter().skip(1).cloned().collect::<Vec<String>>();
            let suggested = if remainder.is_empty() {
                format!("effigy test {suggested_suite}")
            } else {
                format!("effigy test {suggested_suite} {}", remainder.join(" "))
            };
            let available = render_available_suites(&available_runners);
            return Err(BuiltinSuiteSelectionError {
                message: format!(
                    "built-in `test` runner `{first}` is not available in this target (available: {available}). Did you mean `{suggested_suite}`? Try: {suggested}. Use `effigy test --plan <args>` to preview suite routing before execution.",
                ),
                available_runners,
            });
        }
        let available = render_available_suites(&available_runners);
        let user_args = passthrough.join(" ");
        let suggested = available_runners
            .iter()
            .map(|suite| format!("effigy test {suite} {user_args}"))
            .collect::<Vec<String>>()
            .join(" | ");
        return Err(BuiltinSuiteSelectionError {
            message: format!(
                "built-in `test` is ambiguous for arguments `{user_args}` because multiple suites are available ({available}); specify a suite first. Try one of: {suggested}. Use `effigy test --plan <args>` to preview suite routing before execution.",
            ),
            available_runners,
        });
    }

    Ok(BuiltinSuiteSelection {
        runnable,
        requested_suite,
        passthrough,
    })
}

pub(super) fn render_available_suites(available_runners: &BTreeSet<String>) -> String {
    if available_runners.is_empty() {
        "<none>".to_owned()
    } else {
        available_runners
            .iter()
            .cloned()
            .collect::<Vec<String>>()
            .join(", ")
    }
}

fn suggest_suite_name(raw: &str, available_runners: &BTreeSet<String>) -> Option<String> {
    let candidate = raw.to_lowercase();
    let aliases = available_runners
        .iter()
        .flat_map(|suite| {
            if suite == "cargo-nextest" {
                vec!["cargo-nextest".to_owned(), "nextest".to_owned()]
            } else {
                vec![suite.clone()]
            }
        })
        .collect::<BTreeSet<String>>();

    aliases
        .into_iter()
        .map(|name| {
            let dist = edit_distance(&candidate, &name);
            (name, dist)
        })
        .filter(|(_, dist)| *dist <= 2)
        .min_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)))
        .map(|(name, _)| name)
}

fn edit_distance(a: &str, b: &str) -> usize {
    if a == b {
        return 0;
    }
    if a.is_empty() {
        return b.chars().count();
    }
    if b.is_empty() {
        return a.chars().count();
    }
    let a_chars = a.chars().collect::<Vec<char>>();
    let b_chars = b.chars().collect::<Vec<char>>();
    let mut prev = (0..=b_chars.len()).collect::<Vec<usize>>();
    let mut curr = vec![0usize; b_chars.len() + 1];
    for (i, a_char) in a_chars.iter().enumerate() {
        curr[0] = i + 1;
        for (j, b_char) in b_chars.iter().enumerate() {
            let cost = if a_char == b_char { 0 } else { 1 };
            curr[j + 1] =
                std::cmp::min(std::cmp::min(curr[j] + 1, prev[j + 1] + 1), prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b_chars.len()]
}
