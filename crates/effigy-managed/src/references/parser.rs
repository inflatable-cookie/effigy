use effigy_core::shell::shell_quote;
use effigy_tasks::{parse_task_reference_invocation, render_task_selector, TaskSelector};

use crate::{ManagedError, BUILTIN_TASKS};

pub struct ParsedTaskRef {
    pub selector: TaskSelector,
    pub selector_rendered: String,
    pub args_rendered: String,
}

pub fn parse_task_ref(task_ref: &str) -> Result<ParsedTaskRef, ManagedError> {
    let (selector, args) =
        parse_task_reference_invocation(task_ref).map_err(ManagedError::task_invocation)?;
    let selector_rendered = render_task_selector(&selector);
    let args_rendered = args
        .iter()
        .map(|arg| shell_quote(arg))
        .collect::<Vec<String>>()
        .join(" ");
    Ok(ParsedTaskRef {
        selector,
        selector_rendered,
        args_rendered,
    })
}

pub fn merge_args_rendered(ref_args_rendered: &str, args_rendered: &str) -> String {
    match (ref_args_rendered.is_empty(), args_rendered.is_empty()) {
        (true, true) => String::new(),
        (false, true) => ref_args_rendered.to_owned(),
        (true, false) => args_rendered.to_owned(),
        (false, false) => format!("{ref_args_rendered} {args_rendered}"),
    }
}

pub fn is_builtin_task_selector(selector: &TaskSelector) -> bool {
    BUILTIN_TASKS
        .iter()
        .any(|(name, _)| *name == selector.task_name.as_str())
}
