use super::super::super::util::{
    parse_task_reference_invocation, render_task_selector, shell_quote,
};
use super::super::super::{RunnerError, TaskSelector, BUILTIN_TASKS};

pub(super) struct ParsedTaskRef {
    pub(super) selector: TaskSelector,
    pub(super) selector_rendered: String,
    pub(super) args_rendered: String,
}

pub(super) fn parse_task_ref(task_ref: &str) -> Result<ParsedTaskRef, RunnerError> {
    let (selector, args) = parse_task_reference_invocation(task_ref)?;
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

pub(super) fn merge_args_rendered(ref_args_rendered: &str, args_rendered: &str) -> String {
    match (ref_args_rendered.is_empty(), args_rendered.is_empty()) {
        (true, true) => String::new(),
        (false, true) => ref_args_rendered.to_owned(),
        (true, false) => args_rendered.to_owned(),
        (false, false) => format!("{ref_args_rendered} {args_rendered}"),
    }
}

pub(super) fn is_builtin_task_selector(selector: &TaskSelector) -> bool {
    BUILTIN_TASKS
        .iter()
        .any(|(name, _)| *name == selector.task_name.as_str())
}
