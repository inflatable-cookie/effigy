#[path = "finding_templates/health.rs"]
mod health;
#[path = "finding_templates/manifest_parse.rs"]
mod manifest_parse;
#[path = "finding_templates/workflow.rs"]
mod workflow;

pub(super) use health::HealthFinding;
pub(super) use manifest_parse::ManifestParseFinding;
pub(super) use workflow::WorkflowFinding;

#[cfg(test)]
mod tests;
