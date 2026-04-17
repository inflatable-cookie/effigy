use std::collections::BTreeMap;
use std::path::Path;

use effigy_core::widgets::{KeyValue, NoticeLevel, SummaryCounts, TableSpec};
use effigy_process::ProcessSpec;
use effigy_ui::Renderer;

use crate::ManagedError;
use crate::{ManagedProcessSpec, ManagedTaskPlan};

pub fn managed_process_specs<I>(processes: I) -> Vec<ProcessSpec>
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
            shutdown_on_exit: process.shutdown_on_exit,
            pty: true,
            env: BTreeMap::new(),
        })
        .collect()
}

pub fn write_managed_overview(
    renderer: &mut impl Renderer,
    title: &str,
    task_name: &str,
    plan: &ManagedTaskPlan,
    leading_fields: Vec<KeyValue>,
    trailing_fields: Vec<KeyValue>,
    notices: &[&str],
) -> Result<(), ManagedError> {
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
    items.push(KeyValue::new(
        "shutdown-on-exit",
        managed_shutdown_on_exit_label(&plan.processes),
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

pub fn managed_plan_overview_fields(
    repo_root: &Path,
    manifest_path: &Path,
    plan: &ManagedTaskPlan,
) -> Vec<KeyValue> {
    vec![
        KeyValue::new("repo-root", repo_root.display().to_string()),
        KeyValue::new("manifest", manifest_path.display().to_string()),
        KeyValue::new("tab-order", plan.tab_order.join(", ")),
    ]
}

pub fn write_managed_plan_process_table(
    renderer: &mut impl Renderer,
    plan: &ManagedTaskPlan,
) -> Result<(), ManagedError> {
    renderer.table(&TableSpec::new(
        vec![
            "process".to_owned(),
            "cwd".to_owned(),
            "run".to_owned(),
            "start-after-ms".to_owned(),
            "shutdown-on-exit".to_owned(),
        ],
        managed_plan_process_rows(plan),
    ))?;
    Ok(())
}

pub fn write_managed_plan_passthrough(
    renderer: &mut impl Renderer,
    plan: &ManagedTaskPlan,
) -> Result<(), ManagedError> {
    if !plan.passthrough.is_empty() {
        renderer.text("")?;
        renderer.bullet_list("profile-args", &plan.passthrough)?;
    }
    Ok(())
}

pub fn managed_plan_summary_counts() -> SummaryCounts {
    SummaryCounts {
        ok: 1,
        warn: 1,
        err: 0,
    }
}

fn managed_plan_process_rows(plan: &ManagedTaskPlan) -> Vec<Vec<String>> {
    plan.processes
        .iter()
        .map(|process| {
            vec![
                process.name.clone(),
                process.cwd.display().to_string(),
                process.run.clone(),
                process.start_after_ms.to_string(),
                managed_process_shutdown_on_exit_label(process.shutdown_on_exit).to_owned(),
            ]
        })
        .collect::<Vec<Vec<String>>>()
}

fn managed_fail_on_non_zero_label(enabled: bool) -> &'static str {
    if enabled {
        "enabled"
    } else {
        "disabled"
    }
}

fn managed_shutdown_on_exit_label(processes: &[ManagedProcessSpec]) -> String {
    let names = processes
        .iter()
        .filter(|process| process.shutdown_on_exit)
        .map(|process| process.name.as_str())
        .collect::<Vec<&str>>();
    if names.is_empty() {
        "disabled".to_owned()
    } else {
        names.join(", ")
    }
}

fn managed_process_shutdown_on_exit_label(enabled: bool) -> &'static str {
    if enabled {
        "enabled"
    } else {
        "disabled"
    }
}
