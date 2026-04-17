use crate::ManagedError;

pub fn enforce_non_zero_exit_policy(
    task_name: &str,
    profile: &str,
    fail_on_non_zero: bool,
    non_zero_exits: Vec<(String, String)>,
) -> Result<(), ManagedError> {
    let processes = normalize_non_zero_exits(non_zero_exits);
    if fail_on_non_zero && !processes.is_empty() {
        return Err(ManagedError::TaskManagedNonZeroExit {
            task: task_name.to_owned(),
            profile: profile.to_owned(),
            processes,
        });
    }
    Ok(())
}

fn normalize_non_zero_exits(mut non_zero_exits: Vec<(String, String)>) -> Vec<(String, String)> {
    non_zero_exits.sort_by(|a, b| a.0.cmp(&b.0));
    non_zero_exits.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
    non_zero_exits
}

#[cfg(test)]
#[path = "policy/tests.rs"]
mod tests;
