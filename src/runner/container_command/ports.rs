//! `effigy container ports`: machine-wide generated-container port registry
//! status, dry-run reclaim, and digest-confirmed apply.

use effigy_cli::ContainerPortsSubcommand;
use effigy_containers::{
    port_reclaim_apply, port_reclaim_dry_run, port_report_json, port_status, Decision,
    PortRecoveryReport, PortRecoveryRow,
};

use super::RunnerError;

pub(super) fn run_container_ports(
    subcommand: &ContainerPortsSubcommand,
    output_json: bool,
) -> Result<String, RunnerError> {
    let report = match subcommand {
        ContainerPortsSubcommand::Status => port_status(),
        ContainerPortsSubcommand::Reclaim { apply: false, .. } => port_reclaim_dry_run(),
        ContainerPortsSubcommand::Reclaim {
            apply: true,
            expect_plan: Some(digest),
        } => port_reclaim_apply(digest),
        ContainerPortsSubcommand::Reclaim {
            apply: true,
            expect_plan: None,
        } => Err(effigy_containers::ContainerPolicyError::TaskInvocation(
            "`--apply` requires `--expect-plan <DIGEST>`".to_owned(),
        )),
    }
    .map_err(|error| RunnerError::task_invocation(error.to_string()))?;

    if output_json {
        Ok(port_report_json(&report).to_string())
    } else {
        Ok(render_text(&report))
    }
}

const RECLAIM_ROWS_SHOWN: usize = 10;

fn format_row(row: &PortRecoveryRow) -> String {
    format!(
        "  {} {}..{} {} {}",
        row.decision.label(),
        row.base,
        row.base.saturating_add(row.range),
        row.name,
        row.reason
    )
}

fn render_text(report: &PortRecoveryReport) -> String {
    let plan = &report.plan;
    let mut lines = vec![
        format!("ports.mode: {}", report.mode),
        format!(
            "registry: {} ({})",
            report.registry_path.display(),
            if report.registry_present {
                "present"
            } else {
                "absent"
            }
        ),
        format!(
            "capacity: {} free {}-port ranges; next base {}; {} allocations",
            report.capacity_now.allocatable_ranges,
            report.capacity_now.range,
            report
                .capacity_now
                .next_base
                .map_or_else(|| "none".to_owned(), |base| base.to_string()),
            report.capacity_now.allocations,
        ),
    ];
    if let Some(hold) = &plan.global_hold {
        lines.push(format!("global hold: {hold}"));
    }
    // Holds are actionable and always listed. Reclaim rows can number in the
    // hundreds, so the text view shows a bounded sample; `--json` has them all.
    let mut retained = 0usize;
    let mut reclaim_shown = 0usize;
    let reclaim_total = plan.reclaim_names().len();
    for row in &plan.rows {
        match row.decision {
            Decision::Retain => retained += 1,
            Decision::Reclaim if reclaim_shown >= RECLAIM_ROWS_SHOWN => {}
            Decision::Reclaim => {
                reclaim_shown += 1;
                lines.push(format_row(row));
            }
            Decision::Hold => lines.push(format_row(row)),
        }
    }
    if reclaim_total > reclaim_shown {
        lines.push(format!(
            "  ... {} more reclaim entries; use --json for the full list",
            reclaim_total - reclaim_shown
        ));
    }
    if retained > 0 {
        lines.push(format!("  {retained} retained entries"));
    }
    if report.mode == "apply" {
        lines.push(format!("removed: {}", report.removed.len()));
        if let Some(backup) = &report.backup {
            lines.push(format!("backup: {}", backup.display()));
        }
        lines.push(format!(
            "capacity after apply: {} free ranges; next base {}",
            report.capacity_now.allocatable_ranges,
            report
                .capacity_now
                .next_base
                .map_or_else(|| "none".to_owned(), |base| base.to_string()),
        ));
    } else {
        lines.push(format!(
            "planned reclaim: {} entries; after plan: {} free ranges",
            plan.reclaim_names().len(),
            plan.capacity_after.allocatable_ranges
        ));
        lines.push(format!("plan digest: {}", plan.digest));
    }
    lines.push(String::new());
    lines.join("\n")
}
