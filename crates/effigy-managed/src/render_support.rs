use std::collections::BTreeMap;
use std::path::Path;

use effigy_core::widgets::{KeyValue, NoticeLevel, SummaryCounts, TableSpec};
use effigy_manifest::ManifestManagedRunStep;
use effigy_process::ProcessSpec;
use effigy_ui::Renderer;

use crate::ManagedError;
use crate::{ManagedProcessRole, ManagedProcessSpec, ManagedTaskPlan};

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
            pty: process.role != ManagedProcessRole::Lifecycle,
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
        KeyValue::new(
            "gateway-auto-start",
            managed_gateway_auto_start_label(plan.gateway_auto_start),
        ),
        KeyValue::new(
            "readiness-wait",
            managed_readiness_wait_label(plan.readiness.health_wait),
        ),
        KeyValue::new(
            "ready-message",
            plan.readiness
                .ready_message
                .clone()
                .unwrap_or_else(|| "disabled".to_owned()),
        ),
    ]
}

pub fn write_managed_plan_process_table(
    renderer: &mut impl Renderer,
    plan: &ManagedTaskPlan,
) -> Result<(), ManagedError> {
    renderer.table(&TableSpec::new(
        vec![
            "process".to_owned(),
            "role".to_owned(),
            "cwd".to_owned(),
            "setup".to_owned(),
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
                managed_process_role_label(process.role).to_owned(),
                process.cwd.display().to_string(),
                managed_process_setup_label(process),
                process.run.clone(),
                process.start_after_ms.to_string(),
                managed_process_shutdown_on_exit_label(process.shutdown_on_exit).to_owned(),
            ]
        })
        .collect::<Vec<Vec<String>>>()
}

fn managed_process_setup_label(process: &ManagedProcessSpec) -> String {
    if !process.setup_steps.is_empty() {
        return process
            .setup_steps
            .iter()
            .map(render_setup_step_preview)
            .collect::<Vec<_>>()
            .join(" && ");
    }
    process
        .setup
        .clone()
        .unwrap_or_else(|| "disabled".to_owned())
}

fn render_setup_step_preview(step: &ManifestManagedRunStep) -> String {
    match step {
        ManifestManagedRunStep::Command(command) => command.to_owned(),
        ManifestManagedRunStep::Step(table) => {
            let table = table.as_ref();
            match (
                table.run.as_deref(),
                table.task.as_deref(),
                table.rhai.as_deref(),
            ) {
                (Some(run), None, None) => run.to_owned(),
                (None, Some(task), None) => format!("task {task}"),
                (None, None, Some(path)) => format!("rhai {path}"),
                (None, None, None) => "env-only".to_owned(),
                _ => "invalid-step".to_owned(),
            }
        }
    }
}

fn managed_process_role_label(role: ManagedProcessRole) -> &'static str {
    match role {
        ManagedProcessRole::Standard => "standard",
        ManagedProcessRole::Lifecycle => "lifecycle",
        ManagedProcessRole::Shell => "shell",
    }
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

fn managed_readiness_wait_label(enabled: bool) -> &'static str {
    if enabled {
        "enabled"
    } else {
        "disabled"
    }
}

fn managed_gateway_auto_start_label(enabled: bool) -> &'static str {
    if enabled {
        "enabled"
    } else {
        "disabled"
    }
}

#[cfg(test)]
mod tests {
    use super::managed_process_specs;
    use crate::{ManagedProcessRole, ManagedProcessSpec};
    use std::path::PathBuf;

    #[test]
    fn managed_process_specs_only_enable_pty_for_shell_role() {
        let specs = managed_process_specs([
            ManagedProcessSpec {
                name: "lifecycle".to_owned(),
                role: ManagedProcessRole::Lifecycle,
                cwd: PathBuf::from("/tmp/repo"),
                run: "printf lifecycle".to_owned(),
                setup: None,
                setup_steps: Vec::new(),
                start_after_ms: 0,
                shutdown_on_exit: true,
                service: None,
                run_on_host: false,
            },
            ManagedProcessSpec {
                name: "api".to_owned(),
                role: ManagedProcessRole::Standard,
                cwd: PathBuf::from("/tmp/repo/api"),
                run: "printf api".to_owned(),
                setup: None,
                setup_steps: Vec::new(),
                start_after_ms: 0,
                shutdown_on_exit: false,
                service: None,
                run_on_host: false,
            },
            ManagedProcessSpec {
                name: "shell".to_owned(),
                role: ManagedProcessRole::Shell,
                cwd: PathBuf::from("/tmp/repo"),
                run: "sh".to_owned(),
                setup: None,
                setup_steps: Vec::new(),
                start_after_ms: 0,
                shutdown_on_exit: false,
                service: Some("workspace".to_owned()),
                run_on_host: false,
            },
        ]);

        assert_eq!(specs.len(), 3);
        assert!(!specs[0].pty, "lifecycle should use plain pipes");
        assert!(specs[1].pty, "standard tabs should use PTY transport");
        assert!(specs[2].pty, "shell tab should keep PTY transport");
    }
}
