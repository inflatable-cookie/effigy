use super::detect::RepoSetupContext;
use super::model::{
    SetupApplicability, SetupCategory, SetupExecutionKind, SetupJob, SetupSafetyClass,
};
use crate::init::agent::AgentCheck;

pub(super) fn baseline_jobs(checks: &[AgentCheck]) -> Vec<SetupJob> {
    checks
        .iter()
        .map(|check| SetupJob {
            id: check.id().to_owned(),
            category: SetupCategory::Baseline,
            execution_kind: SetupExecutionKind::Apply,
            safety_class: SetupSafetyClass::SafeApply,
            applicability: if check.needs_change() {
                SetupApplicability::Applicable
            } else {
                SetupApplicability::AlreadySatisfied
            },
            summary: check.action_description(),
            reason: String::new(),
            recommended_command: Some("effigy init --apply".to_owned()),
            can_run_noninteractive: true,
        })
        .collect()
}

pub(super) fn task_jobs(context: &RepoSetupContext) -> Vec<SetupJob> {
    vec![
        contextual_job(
            "task_surface.scan",
            SetupCategory::Tasks,
            SetupExecutionKind::Inspect,
            SetupSafetyClass::SafeCheck,
            true,
            "Inspect the current task surface.".to_owned(),
            "Effigy task helpers and migration paths depend on the repo's existing task wrappers."
                .to_owned(),
            Some("effigy tasks".to_owned()),
            true,
        ),
        contextual_job(
            "task_migration.package_json",
            SetupCategory::Tasks,
            SetupExecutionKind::Apply,
            SetupSafetyClass::ContextualApply,
            context.has_package_json,
            "Import `package.json` scripts into `[tasks]`.".to_owned(),
            "package.json detected".to_owned(),
            Some("effigy tasks migrate".to_owned()),
            true,
        ),
        contextual_job(
            "task_migration.makefile",
            SetupCategory::Tasks,
            SetupExecutionKind::Guidance,
            SetupSafetyClass::ContextualApply,
            context.has_makefile,
            "Review Makefile-backed task migration candidates.".to_owned(),
            "Makefile detected; no init-owned migration adapter exists yet".to_owned(),
            None,
            false,
        ),
        contextual_job(
            "task_migration.cargo_alias",
            SetupCategory::Tasks,
            SetupExecutionKind::Guidance,
            SetupSafetyClass::ContextualApply,
            context.has_cargo_aliases,
            "Review Cargo alias migration candidates.".to_owned(),
            "Cargo alias config detected; no init-owned migration adapter exists yet".to_owned(),
            None,
            false,
        ),
    ]
}

pub(super) fn health_jobs() -> Vec<SetupJob> {
    vec![
        guidance_job(
            "doctor.run",
            SetupCategory::Health,
            SetupExecutionKind::Inspect,
            SetupSafetyClass::SafeCheck,
            "Run repo health checks.".to_owned(),
            "doctor gives the structural front-door view before broader setup work".to_owned(),
            Some("effigy doctor".to_owned()),
            true,
        ),
        guidance_job(
            "tasks.inspect",
            SetupCategory::Health,
            SetupExecutionKind::Inspect,
            SetupSafetyClass::SafeCheck,
            "List available task selectors.".to_owned(),
            "task discovery should happen before wrapper migration or validation planning"
                .to_owned(),
            Some("effigy tasks".to_owned()),
            true,
        ),
        guidance_job(
            "test_plan.inspect",
            SetupCategory::Health,
            SetupExecutionKind::Inspect,
            SetupSafetyClass::SafeCheck,
            "Inspect the test execution plan.".to_owned(),
            "test planning is the safest way to understand what validation will do".to_owned(),
            Some("effigy test --plan".to_owned()),
            true,
        ),
    ]
}

pub(super) fn graph_jobs(context: &RepoSetupContext) -> Vec<SetupJob> {
    vec![
        guidance_job(
            "graph_status.inspect",
            SetupCategory::Graph,
            SetupExecutionKind::Inspect,
            SetupSafetyClass::SafeCheck,
            "Inspect graph freshness.".to_owned(),
            "graph status shows whether the repo already has a usable local index".to_owned(),
            Some("effigy graph status --json".to_owned()),
            true,
        ),
        contextual_job(
            "graph_index.build",
            SetupCategory::Graph,
            SetupExecutionKind::Apply,
            SetupSafetyClass::SafeApply,
            !context.has_graph_index,
            "Build the local graph index.".to_owned(),
            "no local graph index found under `.effigy/graph/graph.db`".to_owned(),
            Some("effigy graph index --json".to_owned()),
            true,
        ),
        contextual_job(
            "graph_watch.guidance",
            SetupCategory::Graph,
            SetupExecutionKind::Guidance,
            SetupSafetyClass::SafeCheck,
            context.has_graph_index,
            "Keep the graph warm during longer sessions.".to_owned(),
            "graph watch is useful once the repo already has an index".to_owned(),
            Some("effigy graph watch --json".to_owned()),
            false,
        ),
    ]
}

pub(super) fn secrets_jobs(context: &RepoSetupContext) -> Vec<SetupJob> {
    vec![
        contextual_job(
            "secrets_surface.inspect",
            SetupCategory::Secrets,
            SetupExecutionKind::Inspect,
            SetupSafetyClass::SafeCheck,
            context.has_secrets,
            "Inspect declared secrets surfaces.".to_owned(),
            "`[secrets]` is declared in the manifest".to_owned(),
            Some("effigy secrets doctor".to_owned()),
            true,
        ),
        contextual_job(
            "secrets_vault.init",
            SetupCategory::Secrets,
            SetupExecutionKind::Apply,
            SetupSafetyClass::ContextualApply,
            context.has_secrets,
            "Initialize the local secrets vault.".to_owned(),
            "vault setup is only relevant when the repo declares secrets".to_owned(),
            Some("effigy secrets init".to_owned()),
            true,
        ),
    ]
}

pub(super) fn runtime_jobs(context: &RepoSetupContext) -> Vec<SetupJob> {
    vec![contextual_job(
        "containers_surface.inspect",
        SetupCategory::Runtime,
        SetupExecutionKind::Guidance,
        SetupSafetyClass::ContextualApply,
        context.has_containers,
        "Review local runtime bring-up.".to_owned(),
        "container or workspace runtime sections are declared; init does not start them implicitly"
            .to_owned(),
        Some("effigy container up".to_owned()),
        false,
    )]
}

pub(super) fn bundle_jobs(context: &RepoSetupContext) -> Vec<SetupJob> {
    vec![
        contextual_job(
            "bundle_surface.inspect",
            SetupCategory::Bundles,
            SetupExecutionKind::Inspect,
            SetupSafetyClass::SafeCheck,
            context.has_bundle,
            "Inspect the active bundle source.".to_owned(),
            "`[bundle]` is declared in the manifest".to_owned(),
            Some("effigy bundle inspect".to_owned()),
            true,
        ),
        contextual_job(
            "bundle_sync.run",
            SetupCategory::Bundles,
            SetupExecutionKind::Apply,
            SetupSafetyClass::ContextualApply,
            context.has_bundle,
            "Refresh remote bundle sources.".to_owned(),
            "bundle sync is only relevant when the repo declares a bundle source".to_owned(),
            Some("effigy bundle sync".to_owned()),
            true,
        ),
    ]
}

pub(super) fn validation_jobs(context: &RepoSetupContext) -> Vec<SetupJob> {
    let (summary, command, reason) = if context.has_qa_task {
        (
            "Run the repo QA surface.".to_owned(),
            Some("effigy qa".to_owned()),
            "repo declares a `qa` task".to_owned(),
        )
    } else if context.has_validate_task {
        (
            "Run the repo validation surface.".to_owned(),
            Some("effigy validate".to_owned()),
            "repo declares a `validate` task".to_owned(),
        )
    } else {
        (
            "Run the baseline test surface.".to_owned(),
            Some("effigy test".to_owned()),
            "no repo-specific QA task was detected".to_owned(),
        )
    };
    vec![guidance_job(
        "validation_command.recommend",
        SetupCategory::Validation,
        SetupExecutionKind::Guidance,
        SetupSafetyClass::SafeCheck,
        summary,
        reason,
        command,
        true,
    )]
}

pub(super) fn advanced_jobs(context: &RepoSetupContext) -> Vec<SetupJob> {
    vec![
        contextual_job(
            "state_surface.inspect",
            SetupCategory::Advanced,
            SetupExecutionKind::Inspect,
            SetupSafetyClass::SafeCheck,
            context.has_state,
            "Inspect declared state stacks.".to_owned(),
            "`[state]` is declared; init does not apply state".to_owned(),
            Some("effigy state plan".to_owned()),
            false,
        ),
        contextual_job(
            "deploy_surface.inspect",
            SetupCategory::Advanced,
            SetupExecutionKind::Inspect,
            SetupSafetyClass::SafeCheck,
            context.has_deploy,
            "Inspect declared deployment environments.".to_owned(),
            "`[deploy]` is declared; init does not mutate deployments".to_owned(),
            Some("effigy deploy plan <env>".to_owned()),
            false,
        ),
        contextual_job(
            "distribution_surface.inspect",
            SetupCategory::Advanced,
            SetupExecutionKind::Inspect,
            SetupSafetyClass::SafeCheck,
            context.has_distribution,
            "Inspect distribution preflight surfaces.".to_owned(),
            "`[distribution]` is declared; init does not publish artifacts".to_owned(),
            Some("effigy distribution preflight".to_owned()),
            false,
        ),
        contextual_job(
            "release_surface.inspect",
            SetupCategory::Advanced,
            SetupExecutionKind::Inspect,
            SetupSafetyClass::SafeCheck,
            context.has_release,
            "Inspect release readiness.".to_owned(),
            "`[release]` is declared; init does not execute release mutations".to_owned(),
            Some("effigy release status --check-gates".to_owned()),
            false,
        ),
    ]
}

fn contextual_job(
    id: &str,
    category: SetupCategory,
    execution_kind: SetupExecutionKind,
    safety_class: SetupSafetyClass,
    applicable: bool,
    summary: String,
    reason: String,
    recommended_command: Option<String>,
    can_run_noninteractive: bool,
) -> SetupJob {
    SetupJob {
        id: id.to_owned(),
        category,
        execution_kind,
        safety_class,
        applicability: if applicable {
            SetupApplicability::Applicable
        } else {
            SetupApplicability::NotApplicable
        },
        summary,
        reason,
        recommended_command,
        can_run_noninteractive,
    }
}

fn guidance_job(
    id: &str,
    category: SetupCategory,
    execution_kind: SetupExecutionKind,
    safety_class: SetupSafetyClass,
    summary: String,
    reason: String,
    recommended_command: Option<String>,
    can_run_noninteractive: bool,
) -> SetupJob {
    SetupJob {
        id: id.to_owned(),
        category,
        execution_kind,
        safety_class,
        applicability: SetupApplicability::Applicable,
        summary,
        reason,
        recommended_command,
        can_run_noninteractive,
    }
}
