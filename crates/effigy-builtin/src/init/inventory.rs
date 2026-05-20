#[path = "inventory/catalog.rs"]
mod catalog;
#[path = "inventory/detect.rs"]
mod detect;
#[path = "inventory/execute.rs"]
mod execute;
#[path = "inventory/model.rs"]
mod model;

#[cfg(test)]
#[path = "inventory/tests.rs"]
mod tests;

use std::path::Path;

use crate::init::agent::AgentCheck;

pub(super) use execute::execute_selected_actions;
pub(super) use model::{
    InitActionReport, SetupActionOutcome, SetupActionStatus, SetupApplicability, SetupCategory,
    SetupExecutionKind, SetupJob, SetupSafetyClass,
};

pub(super) fn build_setup_inventory(
    target_root: &Path,
    baseline_checks: &[AgentCheck],
) -> Vec<SetupJob> {
    let context = detect::inspect_repo_setup_context(target_root);
    let mut jobs = Vec::new();
    jobs.extend(catalog::baseline_jobs(baseline_checks));
    jobs.extend(catalog::task_jobs(&context));
    jobs.extend(catalog::health_jobs());
    jobs.extend(catalog::graph_jobs(&context));
    jobs.extend(catalog::secrets_jobs(&context));
    jobs.extend(catalog::runtime_jobs(&context));
    jobs.extend(catalog::bundle_jobs(&context));
    jobs.extend(catalog::validation_jobs(&context));
    jobs.extend(catalog::advanced_jobs(&context));
    jobs
}

pub(super) fn render_follow_up_jobs_excluding(
    jobs: &[SetupJob],
    excluded_ids: &std::collections::BTreeSet<String>,
) -> String {
    let mut current_category = None;
    let mut out = String::new();
    let relevant: Vec<_> = jobs
        .iter()
        .filter(|job| {
            !excluded_ids.contains(&job.id)
                && !matches!(job.category, SetupCategory::Baseline)
                && matches!(job.applicability, SetupApplicability::Applicable)
        })
        .collect();
    if relevant.is_empty() {
        return out;
    }
    out.push_str("Next steps:\n");
    for job in relevant {
        if current_category != Some(job.category) {
            current_category = Some(job.category);
            out.push_str(&format!("{}:\n", job.category.heading()));
        }
        if let Some(command) = &job.recommended_command {
            out.push_str(&format!("- `{command}` - {}", job.summary));
        } else {
            out.push_str(&format!("- {}", job.summary));
        }
        out.push('\n');
    }
    out
}
