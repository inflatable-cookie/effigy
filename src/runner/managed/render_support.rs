use crate::process_manager::ProcessSpec;
use crate::ui::{KeyValue, NoticeLevel, Renderer};

use super::super::{ManagedProcessSpec, ManagedTaskPlan, RunnerError};

pub(super) fn managed_process_specs<I>(processes: I) -> Vec<ProcessSpec>
where
    I: IntoIterator<Item = ManagedProcessSpec>,
{
    processes
        .into_iter()
        .map(|process| ProcessSpec {
            name: process.name,
            run: process.run,
            cwd: process.cwd,
            start_after_ms: process.start_after_ms,
            pty: true,
        })
        .collect()
}

pub(super) fn write_managed_overview(
    renderer: &mut impl Renderer,
    title: &str,
    task_name: &str,
    plan: &ManagedTaskPlan,
    leading_fields: Vec<KeyValue>,
    trailing_fields: Vec<KeyValue>,
    notices: &[&str],
) -> Result<(), RunnerError> {
    let mut items = vec![
        KeyValue::new("task", task_name.to_owned()),
        KeyValue::new("mode", plan.mode.clone()),
        KeyValue::new("profile", plan.profile.clone()),
    ];
    items.extend(leading_fields);
    items.push(KeyValue::new("processes", plan.processes.len().to_string()));
    items.extend(trailing_fields);
    items.push(KeyValue::new(
        "fail-on-non-zero",
        managed_fail_on_non_zero_label(plan.fail_on_non_zero),
    ));

    renderer.section(title)?;
    renderer.key_values(&items)?;
    renderer.text("")?;
    for notice in notices {
        renderer.notice(NoticeLevel::Info, notice)?;
    }
    renderer.text("")?;
    Ok(())
}

fn managed_fail_on_non_zero_label(enabled: bool) -> &'static str {
    if enabled {
        "enabled"
    } else {
        "disabled"
    }
}
