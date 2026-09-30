//! Non-executing QA-group plan resolution: members, admission shape, and
//! typed scope comparison.
//!
//! Resolution is side-effect free: it never acquires capacity, locks,
//! containers, or environments, and it never consults Git state, a diff, or
//! graph data. A `needs_planner` result carries exact tokens and reasons and
//! has no run ID.

use effigy_manifest::{
    LoadedCatalog, ManifestManagedRun, ManifestManagedRunStep, ManifestQaGroup,
    ManifestQaGroupMember, ManifestQaLifecycle, ManifestQaMemberSurface, ManifestQaScopePolicy,
    ManifestTask, QaScopeToken,
};
use std::collections::BTreeSet;

use super::{QaGroupDefinitionSource, SelectedQaGroup};
use crate::EffigyTasksError;

pub const QA_GROUP_PLAN_SCHEMA: &str = "effigy.qa-group-plan.v1";
pub const SCOPE_COVERAGE_DISCLAIMER: &str =
    "Declared mappings do not establish map truth or caller scope completeness";

/// Repository/worktree context attached by the runner; the pure resolver
/// leaves it unknown.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct QaGroupHeadContext {
    pub commit: Option<String>,
    pub worktree: String,
}

impl Default for QaGroupHeadContext {
    fn default() -> Self {
        Self {
            commit: None,
            worktree: "unknown".to_owned(),
        }
    }
}

/// Aggregated admission shape for the whole group.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct QaGroupAdmissionPlan {
    /// True when at least one member or nested task reference is heavy; the
    /// group then holds one host-wide lease across all members.
    pub required: bool,
    /// Members whose route classification is heavy.
    pub member_ids: Vec<String>,
    /// Reservation model note: members run serially, so the single lease
    /// reserves the maximum serial requirement (uniform host reservations
    /// today).
    pub model: &'static str,
}

/// Documented capability limits that require the unlanded supervision work.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct QaGroupCapabilityLimits {
    pub hard_timeout: bool,
    pub stop: bool,
    pub prerequisite_lead: &'static str,
}

/// One scope token's comparison result.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct QaScopeMatch {
    pub input: String,
    pub member_ids: Vec<String>,
    pub gap_reasons: Vec<String>,
}

/// One precise `needs_planner` reason.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct QaPlannerReason {
    /// The offending token, when the reason is about one token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    pub kind: String,
    pub reason: String,
}

/// Scope comparison outcome for the plan.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeAssessment {
    DeclaredMatch,
    NotRequested,
    NeedsPlanner,
}

/// Group identity and provenance in a plan payload.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct QaGroupPlanGroup {
    pub surface: String,
    pub name: String,
    pub catalog: String,
    pub catalog_root: String,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_tracking: Option<String>,
    pub lifecycle: String,
    pub expired: bool,
    pub scope_policy: String,
    pub coverage_gaps: Vec<QaCoverageGapEntry>,
    pub proof_limits: Vec<String>,
    pub expected_wall_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expectation_basis: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct QaCoverageGapEntry {
    pub input: String,
    pub reason: String,
}

/// One resolved member in a plan payload.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct QaGroupPlanMember {
    pub id: String,
    pub kind: String,
    pub surface: String,
    pub catalog: String,
    pub resolved_selector: String,
    pub task: String,
    pub args: Vec<String>,
    pub targets: Vec<String>,
    pub covers: Vec<String>,
    pub companions: Vec<String>,
    pub limits: Vec<String>,
    pub admission: String,
    pub heavy_reasons: Vec<String>,
    pub declared_run_in: String,
}

/// The complete non-executing plan. When `scope_assessment` is
/// `needs_planner`, `executable` is false and the runner must not create a
/// run.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct QaGroupPlan {
    pub schema: &'static str,
    pub schema_version: u8,
    pub executable: bool,
    pub group: QaGroupPlanGroup,
    pub head: QaGroupHeadContext,
    pub scope_inputs: Vec<String>,
    pub scope_assessment: ScopeAssessment,
    pub scope_matches: Vec<QaScopeMatch>,
    pub unmatched_scope_inputs: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub needs_planner_reasons: Vec<QaPlannerReason>,
    pub coverage_disclaimer: &'static str,
    pub admission: QaGroupAdmissionPlan,
    pub members: Vec<QaGroupPlanMember>,
    pub capabilities: QaGroupCapabilityLimits,
}

pub struct QaGroupPlanRequest<'a> {
    pub selected: &'a SelectedQaGroup,
    pub catalogs: &'a [LoadedCatalog],
    /// Raw caller tokens, parsed and compared in order.
    pub scope_tokens: &'a [String],
    /// Today's date for advisory expiry evidence.
    pub today: effigy_manifest::ManifestDraftDate,
}

/// Build the full plan for a selected group. Fails closed on unresolved
/// member selectors or unresolvable nested admission shapes, before any run
/// could exist.
pub fn build_qa_group_plan(
    request: QaGroupPlanRequest<'_>,
) -> Result<QaGroupPlan, EffigyTasksError> {
    let selected = request.selected;
    let group: &ManifestQaGroup = &selected.group;
    let owning_alias = match &selected.source {
        QaGroupDefinitionSource::Maintained { catalog_alias, .. } => catalog_alias.clone(),
        QaGroupDefinitionSource::Temporary { .. } => group
            .catalog
            .clone()
            .ok_or_else(|| EffigyTasksError::message("temporary group must declare `catalog`"))?,
    };

    let mut members = Vec::new();
    let mut heavy_member_ids = Vec::new();
    for member in &group.members {
        let resolved = resolve_member(member, &owning_alias, request.catalogs)?;
        if resolved.admission_heavy {
            heavy_member_ids.push(member.id.clone());
        }
        members.push(QaGroupPlanMember {
            id: member.id.clone(),
            kind: member.kind.as_str().to_owned(),
            surface: member.surface.as_str().to_owned(),
            catalog: resolved.catalog_alias.clone(),
            resolved_selector: format!("{}/{}", resolved.catalog_alias, member.task),
            task: member.task.clone(),
            args: member.args.clone(),
            targets: member.targets.iter().map(QaScopeToken::as_token).collect(),
            covers: member.covers.iter().map(QaScopeToken::as_token).collect(),
            companions: member.companions.clone(),
            limits: member.limits.clone(),
            admission: if resolved.admission_heavy {
                "heavy".to_owned()
            } else {
                "standard".to_owned()
            },
            heavy_reasons: resolved.heavy_reasons.clone(),
            declared_run_in: resolved.declared_run_in.clone(),
        });
    }

    let scope = compare_scope(group, request.scope_tokens)?;

    let expired = group.is_expired_on(request.today.to_naive_date());
    let (file_tracking, definition_sha256) = match &selected.source {
        QaGroupDefinitionSource::Temporary {
            tracking,
            definition_sha256,
            ..
        } => (
            Some(tracking.as_str().to_owned()),
            Some(definition_sha256.clone()),
        ),
        QaGroupDefinitionSource::Maintained { .. } => (None, None),
    };
    let catalog_root = selected_catalog_root(&selected.source, request.catalogs, &owning_alias);

    Ok(QaGroupPlan {
        schema: QA_GROUP_PLAN_SCHEMA,
        schema_version: 1,
        executable: scope.assessment != ScopeAssessment::NeedsPlanner,
        group: QaGroupPlanGroup {
            surface: match group.lifecycle {
                ManifestQaLifecycle::Maintained => "maintained".to_owned(),
                ManifestQaLifecycle::Temporary => "temporary".to_owned(),
            },
            name: group.name.clone(),
            catalog: owning_alias.clone(),
            catalog_root,
            source: selected.source.display(),
            definition_sha256,
            file_tracking,
            lifecycle: group.lifecycle.as_str().to_owned(),
            expired,
            scope_policy: group.scope_policy.as_str().to_owned(),
            coverage_gaps: group
                .coverage_gaps
                .iter()
                .map(|gap| QaCoverageGapEntry {
                    input: gap.input.as_token(),
                    reason: gap.reason.clone(),
                })
                .collect(),
            proof_limits: group.proof_limits.clone(),
            expected_wall_ms: group.expected_wall_ms,
            expectation_basis: group.expectation_basis.clone(),
        },
        head: QaGroupHeadContext::default(),
        scope_inputs: scope.inputs,
        scope_assessment: scope.assessment,
        scope_matches: scope.matches,
        unmatched_scope_inputs: scope.unmatched,
        needs_planner_reasons: scope.reasons,
        coverage_disclaimer: SCOPE_COVERAGE_DISCLAIMER,
        admission: QaGroupAdmissionPlan {
            required: !heavy_member_ids.is_empty(),
            member_ids: heavy_member_ids,
            model: "one host-wide heavy lease per group run; serial members reserve the maximum serial requirement",
        },
        members,
        capabilities: QaGroupCapabilityLimits {
            hard_timeout: false,
            stop: false,
            prerequisite_lead: "29e5f6f7",
        },
    })
}

fn selected_catalog_root(
    source: &QaGroupDefinitionSource,
    catalogs: &[LoadedCatalog],
    owning_alias: &str,
) -> String {
    match source {
        QaGroupDefinitionSource::Maintained { catalog_root, .. } => {
            catalog_root.display().to_string()
        }
        QaGroupDefinitionSource::Temporary { .. } => catalogs
            .iter()
            .find(|catalog| catalog.alias == owning_alias)
            .map(|catalog| catalog.catalog_root.display().to_string())
            .unwrap_or_default(),
    }
}

struct ResolvedMember {
    catalog_alias: String,
    admission_heavy: bool,
    heavy_reasons: Vec<String>,
    declared_run_in: String,
}

/// Resolve one member to its exact catalog + surface and classify admission.
///
/// Lookup defaults to the owning group catalog; an explicit member `catalog`
/// pins cross-catalog lookup. One surface never falls through to the other,
/// and cwd/shallowest precedence never applies to members.
fn resolve_member(
    member: &ManifestQaGroupMember,
    owning_alias: &str,
    catalogs: &[LoadedCatalog],
) -> Result<ResolvedMember, EffigyTasksError> {
    let alias = member.catalog.as_deref().unwrap_or(owning_alias);
    let catalog = catalogs
        .iter()
        .find(|catalog| catalog.alias == alias)
        .ok_or_else(|| {
            EffigyTasksError::message(format!(
                "member `{}` pins catalog `{alias}`, which is not in the effective catalog set",
                member.id
            ))
        })?;
    let (task, surface) = match member.surface {
        ManifestQaMemberSurface::Published => (
            catalog.manifest.tasks.get(&member.task),
            crate::TaskSurface::Published,
        ),
        ManifestQaMemberSurface::Draft => (
            catalog
                .manifest
                .drafts
                .get(&member.task)
                .map(|draft| &draft.task),
            crate::TaskSurface::Draft,
        ),
    };
    let Some(task) = task else {
        return Err(EffigyTasksError::message(format!(
            "member `{}` selector `{}` does not resolve as a {} task in catalog `{alias}`; unresolved members fail before any run",
            member.id,
            member.task,
            member.surface.as_str()
        )));
    };

    let mut heavy_reasons = Vec::new();
    let mut visited = BTreeSet::new();
    if is_directly_heavy(task) {
        heavy_reasons.push(format!(
            "task `{}` declares `admission = \"heavy\"`",
            member.task
        ));
    }
    if is_admission_reserved_name(&member.task) {
        heavy_reasons.push(format!(
            "selector `{}` routes through the canonical heavy admission gate",
            member.task
        ));
    }
    classify_task_admission(
        task,
        surface,
        &member.task,
        catalogs,
        &mut visited,
        &mut heavy_reasons,
    )
    .map_err(|detail| {
        EffigyTasksError::message(format!(
            "member `{}` admission shape cannot be resolved: {detail}",
            member.id
        ))
    })?;

    Ok(ResolvedMember {
        catalog_alias: alias.to_owned(),
        admission_heavy: !heavy_reasons.is_empty(),
        heavy_reasons,
        declared_run_in: task.run_in().as_str().to_owned(),
    })
}

/// Task-selector names the admission gate treats as heavy even without
/// metadata (mirrors the canonical pipeline's `qa`/`ci` handling).
fn is_directly_heavy(task: &ManifestTask) -> bool {
    task.admission.is_some()
}

/// Walk one task's known nested task references, classifying heavy shapes.
///
/// `missing` collects names for the fail-closed diagnostic; a reference that
/// cannot be resolved is a plan error, not a silent assumption.
fn classify_task_admission(
    task: &ManifestTask,
    surface: crate::TaskSurface,
    label: &str,
    catalogs: &[LoadedCatalog],
    visited: &mut BTreeSet<String>,
    heavy_reasons: &mut Vec<String>,
) -> Result<(), String> {
    if !visited.insert(label.to_owned()) {
        return Ok(());
    }
    if visited.len() > 64 {
        return Err(format!(
            "nested task references from `{label}` exceed the classification depth limit"
        ));
    }
    let mut references: Vec<(crate::TaskSurface, String)> = Vec::new();
    collect_task_references(task, surface, &mut references);
    for (reference_surface, reference) in references {
        let (alias, name) = match reference.rsplit_once('/') {
            Some((prefix, name)) => (Some(prefix), name),
            None => (None, reference.as_str()),
        };
        // Qualified references pin the catalog; unprefixed references resolve
        // on the exact surface across the effective set, and a miss fails
        // closed before any run could exist.
        let catalog = match alias {
            Some(prefix) => catalogs
                .iter()
                .find(|catalog| catalog.alias == prefix)
                .ok_or_else(|| {
                    format!("nested reference `{reference}` pins unknown catalog prefix `{prefix}`")
                })?,
            None => catalogs
                .iter()
                .find(|catalog| task_on_surface(catalog, reference_surface, name).is_some())
                .ok_or_else(|| {
                    format!(
                        "nested {} reference `{name}` does not resolve in any effective catalog",
                        reference_surface.as_str()
                    )
                })?,
        };
        let nested = task_on_surface(catalog, reference_surface, name).ok_or_else(|| {
            format!(
                "nested {} reference `{name}` does not resolve in catalog `{}`",
                reference_surface.as_str(),
                catalog.alias
            )
        })?;
        if is_directly_heavy(nested) || is_admission_reserved_name(name) {
            heavy_reasons.push(format!(
                "nested {} task `{}` in catalog `{}` is heavy",
                reference_surface.as_str(),
                name,
                catalog.alias
            ));
        }
        classify_task_admission(
            nested,
            reference_surface,
            &format!("{}::{}", catalog.alias, name),
            catalogs,
            visited,
            heavy_reasons,
        )?;
    }
    Ok(())
}

fn task_on_surface<'a>(
    catalog: &'a LoadedCatalog,
    surface: crate::TaskSurface,
    name: &str,
) -> Option<&'a ManifestTask> {
    match surface {
        crate::TaskSurface::Published => catalog.manifest.tasks.get(name),
        crate::TaskSurface::Draft => catalog.manifest.drafts.get(name).map(|draft| &draft.task),
    }
}

fn is_admission_reserved_name(name: &str) -> bool {
    matches!(name, "qa" | "ci" | "ci:fresh")
}

/// Collect a task's direct nested task references in declaration order.
fn collect_task_references(
    task: &ManifestTask,
    owner_surface: crate::TaskSurface,
    out: &mut Vec<(crate::TaskSurface, String)>,
) {
    if let Some(run) = task.run.as_ref() {
        collect_run_references(run, owner_surface, out);
    }
    for entry in &task.concurrent {
        if let Some(reference) = entry.task.as_deref() {
            push_reference(owner_surface, reference, out);
        }
        for step in &entry.setup {
            collect_step_references(step, owner_surface, out);
        }
    }
    for profile in task.profiles.values() {
        for entry in &profile.concurrent {
            if let Some(reference) = entry.task.as_deref() {
                push_reference(owner_surface, reference, out);
            }
            for step in &entry.setup {
                collect_step_references(step, owner_surface, out);
            }
        }
    }
}

fn collect_run_references(
    run: &ManifestManagedRun,
    owner_surface: crate::TaskSurface,
    out: &mut Vec<(crate::TaskSurface, String)>,
) {
    match run {
        ManifestManagedRun::Command(command) => {
            if let Some(reference) = command.strip_prefix("task:").map(str::trim) {
                let name = reference.split_whitespace().next().unwrap_or_default();
                if !name.is_empty() {
                    push_reference(crate::TaskSurface::Published, name, out);
                }
            }
        }
        ManifestManagedRun::Sequence(steps) => {
            for step in steps {
                collect_step_references(step, owner_surface, out);
            }
        }
    }
}

fn collect_step_references(
    step: &ManifestManagedRunStep,
    owner_surface: crate::TaskSurface,
    out: &mut Vec<(crate::TaskSurface, String)>,
) {
    match step {
        ManifestManagedRunStep::Command(command) => {
            collect_run_references(
                &ManifestManagedRun::Command(command.clone()),
                owner_surface,
                out,
            );
        }
        ManifestManagedRunStep::Step(table) => {
            if let Some(reference) = table.task.as_deref() {
                push_reference(crate::TaskSurface::Published, reference, out);
            }
            if let Some(reference) = table.draft.as_deref() {
                push_reference(crate::TaskSurface::Draft, reference, out);
            }
            if let Some(run) = table.run.as_deref() {
                collect_run_references(
                    &ManifestManagedRun::Command(run.to_owned()),
                    owner_surface,
                    out,
                );
            }
        }
    }
}

fn push_reference(
    default_surface: crate::TaskSurface,
    raw: &str,
    out: &mut Vec<(crate::TaskSurface, String)>,
) {
    let reference = raw.trim();
    if reference.is_empty() {
        return;
    }
    // `{ task = ... }`/`{ draft = ... }` keys already fix the surface; a
    // `task:` command reference always resolves published.
    let surface = match default_surface {
        crate::TaskSurface::Draft => default_surface,
        crate::TaskSurface::Published => crate::TaskSurface::Published,
    };
    out.push((surface, reference.to_owned()));
}

struct ScopeComparison {
    inputs: Vec<String>,
    assessment: ScopeAssessment,
    matches: Vec<QaScopeMatch>,
    unmatched: Vec<String>,
    reasons: Vec<QaPlannerReason>,
}

/// Compare caller scope tokens with the group's declared coverage.
///
/// Any unmatched token, any token matching a known gap, a required-but-absent
/// scope, or requested scope against an empty coverage map returns
/// `needs_planner` with exact tokens and reasons. A fully mapped scope is
/// `declared_match`; no scope on an advisory group is `not_requested`. Scope
/// never filters members.
fn compare_scope(
    group: &ManifestQaGroup,
    raw_tokens: &[String],
) -> Result<ScopeComparison, EffigyTasksError> {
    let mut inputs = Vec::new();
    let mut tokens = Vec::new();
    for raw in raw_tokens {
        let token = QaScopeToken::parse(raw, false).map_err(EffigyTasksError::message)?;
        inputs.push(token.as_token());
        tokens.push(token);
    }

    let has_any_coverage = group.members.iter().any(|member| !member.covers.is_empty())
        || !group.coverage_gaps.is_empty();

    if tokens.is_empty() {
        if group.scope_policy == ManifestQaScopePolicy::Required {
            return Ok(ScopeComparison {
                inputs,
                assessment: ScopeAssessment::NeedsPlanner,
                matches: Vec::new(),
                unmatched: Vec::new(),
                reasons: vec![QaPlannerReason {
                    token: None,
                    kind: "required-scope-missing".to_owned(),
                    reason: format!(
                        "group `{}` has `scope_policy = \"required\"`; supply at least one `--scope` token naming the affected inputs",
                        group.name
                    ),
                }],
            });
        }
        return Ok(ScopeComparison {
            inputs,
            assessment: ScopeAssessment::NotRequested,
            matches: Vec::new(),
            unmatched: Vec::new(),
            reasons: Vec::new(),
        });
    }

    let mut matches = Vec::new();
    let mut unmatched = Vec::new();
    let mut reasons = Vec::new();
    for token in &tokens {
        let member_ids: Vec<String> = group
            .members
            .iter()
            .filter(|member| member.covers.iter().any(|pattern| pattern.matches(token)))
            .map(|member| member.id.clone())
            .collect();
        let gap_reasons: Vec<String> = group
            .coverage_gaps
            .iter()
            .filter(|gap| gap.input.matches(token))
            .map(|gap| gap.reason.clone())
            .collect();
        if let Some(reason) = gap_reasons.first() {
            reasons.push(QaPlannerReason {
                token: Some(token.as_token()),
                kind: "known-gap".to_owned(),
                reason: reason.clone(),
            });
        }
        if member_ids.is_empty() && gap_reasons.is_empty() {
            unmatched.push(token.as_token());
            reasons.push(QaPlannerReason {
                token: Some(token.as_token()),
                kind: "unmatched-token".to_owned(),
                reason: if has_any_coverage {
                    "no member `covers` pattern maps this input; ask the planner which proof covers it"
                        .to_owned()
                } else {
                    "the group declares no coverage map at all; ask the planner which proof covers this input"
                        .to_owned()
                },
            });
        }
        matches.push(QaScopeMatch {
            input: token.as_token(),
            member_ids,
            gap_reasons,
        });
    }

    if reasons.is_empty() {
        Ok(ScopeComparison {
            inputs,
            assessment: ScopeAssessment::DeclaredMatch,
            matches,
            unmatched,
            reasons,
        })
    } else {
        Ok(ScopeComparison {
            inputs,
            assessment: ScopeAssessment::NeedsPlanner,
            matches,
            unmatched,
            reasons,
        })
    }
}

/// Compare expected wall time with observed execution wall time.
///
/// `over_budget` preserves the check outcome; unknown measurements stay
/// unknown rather than zero.
pub fn budget_state(expected_wall_ms: Option<u64>, observed_wall_ms: Option<u64>) -> &'static str {
    match (expected_wall_ms, observed_wall_ms) {
        (Some(expected), Some(observed)) if observed > expected => "over_budget",
        (Some(_), Some(_)) => "within_budget",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::{budget_state, compare_scope};
    use effigy_manifest::{ManifestQaGroup, ManifestQaScopePolicy};

    fn group(scope_policy: ManifestQaScopePolicy) -> ManifestQaGroup {
        let table: effigy_manifest::ManifestQaGroupTable = toml::from_str(
            r#"
lifecycle = "maintained"
purpose = "sample"
scope_policy = "required"
proof_limits = ["bounded"]
coverage_gaps = [{ input = "input:bindings-generator-transitive-compile-dependencies", reason = "Generator compile closure is not enumerated" }]
members = [
  { id = "tests", kind = "test", task = "t", targets = ["cargo-package:cli"], covers = ["cargo-package:cli", "path:crates/cli/**"], limits = ["nothing"] },
  { id = "docs", kind = "docs", task = "d", targets = ["path:docs/**"], covers = ["path:docs/**"], limits = ["nothing"] },
]
"#,
        )
        .expect("parse fixture");
        let mut group = table
            .into_manifest_group(Some("sample"))
            .expect("valid group");
        group.scope_policy = scope_policy;
        group
    }

    #[test]
    fn required_scope_missing_yields_planner() {
        let group = group(ManifestQaScopePolicy::Required);
        let comparison = compare_scope(&group, &[]).expect("compare");
        assert_eq!(comparison.assessment, super::ScopeAssessment::NeedsPlanner);
        assert_eq!(comparison.reasons[0].kind, "required-scope-missing");
    }

    #[test]
    fn advisory_scope_omitted_is_not_requested() {
        let group = group(ManifestQaScopePolicy::Advisory);
        let comparison = compare_scope(&group, &[]).expect("compare");
        assert_eq!(comparison.assessment, super::ScopeAssessment::NotRequested);
    }

    #[test]
    fn mapped_scope_matches_every_claiming_member() {
        let group = group(ManifestQaScopePolicy::Required);
        let comparison = compare_scope(
            &group,
            &[
                "cargo-package:cli".to_owned(),
                "path:crates/cli/src/main.rs".to_owned(),
            ],
        )
        .expect("compare");
        assert_eq!(comparison.assessment, super::ScopeAssessment::DeclaredMatch);
        assert_eq!(comparison.matches[0].member_ids, vec!["tests".to_owned()]);
        assert_eq!(comparison.unmatched.len(), 0);
    }

    #[test]
    fn known_gap_fails_even_when_a_member_also_claims_the_input() {
        let group = group(ManifestQaScopePolicy::Required);
        let comparison = compare_scope(
            &group,
            &["input:bindings-generator-transitive-compile-dependencies".to_owned()],
        )
        .expect("compare");
        assert_eq!(comparison.assessment, super::ScopeAssessment::NeedsPlanner);
        assert_eq!(comparison.reasons[0].kind, "known-gap");
    }

    #[test]
    fn unmatched_token_yields_planner_with_the_token_named() {
        let group = group(ManifestQaScopePolicy::Advisory);
        let comparison =
            compare_scope(&group, &["cargo-package:other".to_owned()]).expect("compare");
        assert_eq!(comparison.assessment, super::ScopeAssessment::NeedsPlanner);
        assert_eq!(comparison.unmatched, vec!["cargo-package:other".to_owned()]);
    }

    #[test]
    fn path_patterns_map_multiple_members() {
        let group = group(ManifestQaScopePolicy::Advisory);
        let comparison =
            compare_scope(&group, &["path:docs/guide.md".to_owned()]).expect("compare");
        assert_eq!(comparison.matches[0].member_ids, vec!["docs".to_owned()]);
    }

    #[test]
    fn budget_state_compares_only_known_values() {
        assert_eq!(budget_state(Some(100), Some(99)), "within_budget");
        assert_eq!(budget_state(Some(100), Some(101)), "over_budget");
        assert_eq!(budget_state(None, Some(50)), "unknown");
        assert_eq!(budget_state(Some(50), None), "unknown");
        assert_eq!(budget_state(None, None), "unknown");
    }
}
