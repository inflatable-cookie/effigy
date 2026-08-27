use std::path::Path;

use crate::contracts::{check_id, remediation};
use crate::FindingSink;

pub fn is_builtin_selector(task_name: &str) -> bool {
    matches!(
        task_name,
        "help" | "config" | "doctor" | "docs" | "test" | "tasks"
    )
}

pub fn add_invalid_reference_syntax(
    sink: &mut impl FindingSink,
    manifest_path: &Path,
    task_name: &str,
    reference: &str,
    error: &str,
) {
    sink.add_check_error(
        check_id::TASK_REFERENCES_RESOLVE,
        format!(
            "{} task `{}` has invalid task reference `{}`: {}",
            manifest_path.display(),
            task_name,
            reference,
            error
        ),
        remediation::FIX_TASK_REFERENCE_SYNTAX.to_owned(),
    );
}

pub fn add_unresolved_reference(
    sink: &mut impl FindingSink,
    manifest_path: &Path,
    task_name: &str,
    reference: &str,
    error: &str,
) {
    sink.add_check_error(
        check_id::TASK_REFERENCES_RESOLVE,
        format!(
            "{} task `{}` references `{}` but resolution failed: {}",
            manifest_path.display(),
            task_name,
            reference,
            error
        ),
        remediation::UPDATE_TASK_REFERENCE_TARGET.to_owned(),
    );
}

pub fn add_non_runnable_reference(
    sink: &mut impl FindingSink,
    manifest_path: &Path,
    task_name: &str,
    reference: &str,
) {
    sink.add_check_error(
        check_id::TASK_REFERENCES_RESOLVE,
        format!(
            "{} task `{}` references `{}` but target has no `run` command",
            manifest_path.display(),
            task_name,
            reference
        ),
        remediation::REFERENCE_RUNNABLE_TASK.to_owned(),
    );
}

#[cfg(test)]
mod tests {
    use super::is_builtin_selector;

    #[test]
    fn builtin_selector_accepts_docs_and_existing_names() {
        assert!(is_builtin_selector("docs"));
        assert!(is_builtin_selector("doctor"));
        assert!(is_builtin_selector("test"));
        assert!(is_builtin_selector("tasks"));
        assert!(!is_builtin_selector("qa"));
        assert!(!is_builtin_selector("deploy"));
    }
}
