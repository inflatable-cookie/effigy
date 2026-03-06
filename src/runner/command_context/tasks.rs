const TASK_SELECTION_PRECEDENCE: [&str; 4] = [
    "explicit catalog alias prefix",
    "relative/absolute catalog path prefix",
    "unprefixed nearest in-scope catalog by cwd",
    "unprefixed shallowest catalog from workspace root",
];

pub(in crate::runner) fn task_selection_precedence_notes() -> Vec<String> {
    TASK_SELECTION_PRECEDENCE
        .into_iter()
        .map(str::to_owned)
        .collect()
}
