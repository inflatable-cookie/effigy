//! The argument rule for a task invocation (D1-34): Effigy's own flags come
//! before the selector, and everything after it belongs to the task.
//!
//! The CLI joins the two halves with [`TASK_ARGS_BOUNDARY`]. Invocations
//! built inside Effigy (sequence steps, deferral, built-ins) carry no
//! boundary and keep their own argument conventions.

/// Separates Effigy's flags from the task's arguments. Unit separator plus a
/// name nobody types.
pub const TASK_ARGS_BOUNDARY: &str = "\u{1f}effigy:task-args";

/// Flags still read as Effigy's when they directly follow the selector, for
/// one release; each use prints a note naming the leading spelling.
pub const COMPAT_TRAILING_FLAGS: [&str; 2] = ["--plan", "--json"];

/// Effigy flags that, after the selector, now reach the task. A use prints
/// a note so a caller learns to move it before the selector.
pub const LEADING_ONLY_FLAGS: [&str; 4] = ["--repo", "--lock-wait-ms", "--env-schema", "--verbose-root"];

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SplitTaskArgs {
    /// Effigy's flags: those before the selector, then any compatibility
    /// flags taken from directly after it.
    pub effigy: Vec<String>,
    /// The task's own arguments, in order.
    pub task: Vec<String>,
    /// Compatibility flags taken from after the selector.
    pub compat: Vec<String>,
    /// Leading-only Effigy flags that were passed to the task.
    pub passed_through: Vec<String>,
}

/// `None` when `args` carry no boundary.
pub fn split_task_args(args: &[String]) -> Option<SplitTaskArgs> {
    let index = args.iter().position(|arg| arg == TASK_ARGS_BOUNDARY)?;
    let mut effigy = args[..index].to_vec();
    let mut rest = &args[index + 1..];
    let mut compat = Vec::new();
    while let Some(first) = rest.first() {
        if !COMPAT_TRAILING_FLAGS.contains(&first.as_str()) {
            break;
        }
        compat.push(first.clone());
        effigy.push(first.clone());
        rest = &rest[1..];
    }
    let passed_through = rest
        .iter()
        .take_while(|arg| arg.as_str() != "--")
        .filter(|arg| LEADING_ONLY_FLAGS.contains(&arg.as_str()))
        .cloned()
        .collect();
    Some(SplitTaskArgs {
        effigy,
        task: rest.to_vec(),
        compat,
        passed_through,
    })
}

/// Joins Effigy flags and task arguments for a CLI task invocation.
pub fn join_task_args(effigy: Vec<String>, task: Vec<String>) -> Vec<String> {
    let mut args = effigy;
    args.push(TASK_ARGS_BOUNDARY.to_owned());
    args.extend(task);
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn everything_after_the_selector_belongs_to_the_task() {
        let args = join_task_args(
            strings(&["--repo", "/r"]),
            strings(&["--fix", "--json", "--repo", "owner/x"]),
        );
        let split = split_task_args(&args).expect("boundary");
        assert_eq!(split.effigy, strings(&["--repo", "/r"]));
        assert_eq!(split.task, strings(&["--fix", "--json", "--repo", "owner/x"]));
        assert!(split.compat.is_empty());
        assert_eq!(split.passed_through, strings(&["--repo"]));
    }

    #[test]
    fn plan_and_json_directly_after_the_selector_stay_effigy_for_one_release() {
        let args = join_task_args(Vec::new(), strings(&["--plan", "--json", "--watch"]));
        let split = split_task_args(&args).expect("boundary");
        assert_eq!(split.effigy, strings(&["--plan", "--json"]));
        assert_eq!(split.compat, strings(&["--plan", "--json"]));
        assert_eq!(split.task, strings(&["--watch"]));
    }

    #[test]
    fn internal_invocations_without_a_boundary_are_not_split() {
        assert_eq!(split_task_args(&strings(&["--json", "x"])), None);
    }
}
