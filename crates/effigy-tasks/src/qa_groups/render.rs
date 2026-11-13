//! Text and versioned-JSON rendering for QA-group inventory and plans.

use effigy_core::widgets::{KeyValue, NoticeLevel};
use effigy_ui::{encode_json, plain_renderer, render_utf8, text_color_enabled, Renderer};

use super::inventory::{ListQaGroupsResult, QA_GROUPS_SCHEMA};
use super::plan::{QaGroupPlan, ScopeAssessment};
use crate::EffigyTasksError;

/// Render the inventory payload as versioned JSON.
pub fn render_qa_groups_json(
    result: &ListQaGroupsResult,
    pretty_json: bool,
) -> Result<String, EffigyTasksError> {
    let payload = serde_json::json!({
        "schema": QA_GROUPS_SCHEMA,
        "schema_version": 1,
        "count": result.count(),
        "filter": result.filter,
        "qa_groups": result.rows,
        "file": result.file,
    });
    encode_json(&payload, pretty_json).map_err(EffigyTasksError::from)
}

/// Render the inventory as reviewable text.
pub fn render_qa_groups_text(result: &ListQaGroupsResult) -> Result<String, EffigyTasksError> {
    let color_enabled = text_color_enabled();
    let mut renderer = plain_renderer(color_enabled);

    match result.filter.as_ref() {
        Some(filter) => renderer.section(&format!("QA Group Matches: {filter}"))?,
        None => renderer.section("QA Groups")?,
    }
    renderer.key_values(&[KeyValue::new("count", result.count().to_string())])?;
    if result.file.is_none() {
        renderer
            .text("pass --file to include one temporary definition; there is no directory scan")?;
    }
    renderer.text("")?;

    if result.rows.is_empty() {
        renderer.notice(NoticeLevel::Info, "none")?;
        return render_utf8(renderer.into_inner()).map_err(EffigyTasksError::from);
    }

    for row in &result.rows {
        let expiry = if row.expired { " expired" } else { "" };
        renderer.text(&format!(
            "- {} : {} · {}{} · {} member(s)",
            row.selector, row.lifecycle, row.scope_policy, expiry, row.member_count
        ))?;
        renderer.text(&format!("      purpose: {}", row.purpose))?;
        renderer.text(&format!("      source: {}", row.source))?;
        match row.expected_wall_ms {
            Some(expected) => renderer.text(&format!("      expected wall time: {expected} ms"))?,
            None => renderer.text("      expected wall time: unknown")?,
        }
        for notice in &row.notices {
            renderer.notice(NoticeLevel::Warning, notice)?;
        }
    }

    if let Some(file) = &result.file {
        renderer.text(&format!(
            "selected file: {} ({}, {})",
            file.path, file.tracking, file.definition_sha256
        ))?;
        for notice in &file.notices {
            renderer.notice(NoticeLevel::Warning, notice)?;
        }
    }
    render_utf8(renderer.into_inner()).map_err(EffigyTasksError::from)
}

/// Render the plan payload as versioned JSON.
pub fn render_qa_group_plan_json(
    plan: &QaGroupPlan,
    pretty_json: bool,
) -> Result<String, EffigyTasksError> {
    let value = serde_json::to_value(plan).map_err(|error| {
        EffigyTasksError::from(effigy_ui::UiError::Encoding(format!(
            "failed to encode qa-group plan: {error}"
        )))
    })?;
    encode_json(&value, pretty_json).map_err(EffigyTasksError::from)
}

/// Render the plan as reviewable text: identity, provenance, scope verdict,
/// and each member's resolved route and declarations.
pub fn render_qa_group_plan_text(plan: &QaGroupPlan) -> Result<String, EffigyTasksError> {
    let color_enabled = text_color_enabled();
    let mut renderer = plain_renderer(color_enabled);

    renderer.section(&format!(
        "QA Group Plan: {} ({}, catalog {})",
        plan.group.name, plan.group.lifecycle, plan.group.catalog
    ))?;
    renderer.key_values(&[
        KeyValue::new("source", plan.group.source.clone()),
        KeyValue::new(
            "definition digest",
            plan.group
                .definition_sha256
                .clone()
                .unwrap_or_else(|| "n/a (composed manifest)".to_owned()),
        ),
        KeyValue::new(
            "expected wall time",
            plan.group
                .expected_wall_ms
                .map(|ms| format!("{ms} ms"))
                .unwrap_or_else(|| "unknown".to_owned()),
        ),
        KeyValue::new(
            "scope assessment",
            match plan.scope_assessment {
                ScopeAssessment::DeclaredMatch => "declared_match".to_owned(),
                ScopeAssessment::NotRequested => "not_requested".to_owned(),
                ScopeAssessment::NeedsPlanner => "needs_planner".to_owned(),
            },
        ),
        KeyValue::new(
            "admission",
            if plan.admission.required {
                format!(
                    "heavy (members: {}); one admission for the whole group when EFFIGY_ADMISSION_HOOK is set",
                    plan.admission.member_ids.join(", ")
                )
            } else {
                "standard; no admission needed".to_owned()
            },
        ),
    ])?;
    if let Some(tracking) = &plan.group.file_tracking {
        renderer.key_values(&[KeyValue::new("file tracking", tracking.clone())])?;
    }
    if let Some(basis) = &plan.group.expectation_basis {
        renderer.text(&format!("expectation basis: {basis}"))?;
    }
    for limit in &plan.group.proof_limits {
        renderer.text(&format!("proof limit: {limit}"))?;
    }
    for gap in &plan.group.coverage_gaps {
        renderer.text(&format!("coverage gap: {} — {}", gap.input, gap.reason))?;
    }
    if plan.group.expired {
        renderer.notice(
            NoticeLevel::Warning,
            "expired: advisory only; the definition stays runnable when selected explicitly",
        )?;
    }
    for input in &plan.scope_inputs {
        renderer.text(&format!("scope input: {input}"))?;
    }
    for reason in &plan.needs_planner_reasons {
        renderer.notice(
            NoticeLevel::Warning,
            &format!(
                "needs_planner [{}] {}: {}",
                reason.kind,
                reason.token.as_deref().unwrap_or("(scope)"),
                reason.reason
            ),
        )?;
    }
    renderer.text(&format!("scope: {}", plan.coverage_disclaimer))?;
    renderer.text("")?;

    for member in &plan.members {
        renderer.text(&format!(
            "- {} : {} · {} · {}/{} · args {:?} · admission {}",
            member.id,
            member.kind,
            member.surface,
            member.catalog,
            member.task,
            member.args,
            member.admission
        ))?;
        renderer.text(&format!("      targets: {}", member.targets.join(", ")))?;
        if !member.covers.is_empty() {
            renderer.text(&format!("      covers: {}", member.covers.join(", ")))?;
        }
        if !member.companions.is_empty() {
            renderer.text(&format!(
                "      companions: {}",
                member.companions.join(", ")
            ))?;
        }
        for limit in &member.limits {
            renderer.text(&format!("      limit: {limit}"))?;
        }
    }
    renderer.text("")?;
    renderer.notice(
        NoticeLevel::Info,
        "hard_timeout_ms and stop are unavailable: QA-group controls are not integrated with owned-run supervision (contract 052)",
    )?;
    if !plan.executable {
        renderer.notice(
            NoticeLevel::Warning,
            "needs_planner: no run was created; resolve the reasons above with the planner",
        )?;
    }
    render_utf8(renderer.into_inner()).map_err(EffigyTasksError::from)
}

#[cfg(test)]
mod tests {
    use super::{
        render_qa_group_plan_json, render_qa_group_plan_text, render_qa_groups_json,
        render_qa_groups_text,
    };
    use crate::qa_groups::inventory::{ListQaGroupsResult, QaGroupListRow, TemporaryFileSummary};
    use crate::qa_groups::plan::{QaGroupPlan, QA_GROUP_PLAN_SCHEMA};

    fn minimal_plan() -> QaGroupPlan {
        serde_json::from_str(
            r#"
{
  "schema": "effigy.qa-group-plan.v1",
  "schema_version": 1,
  "executable": false,
  "group": {
    "surface": "maintained",
    "name": "sample",
    "catalog": "root",
    "catalog_root": "/repo",
    "source": "/repo/effigy.toml",
    "lifecycle": "maintained",
    "expired": false,
    "scope_policy": "required",
    "coverage_gaps": [],
    "proof_limits": ["bounded"],
    "expected_wall_ms": null
  },
  "head": { "commit": null, "worktree": "unknown" },
  "scope_inputs": [],
  "scope_assessment": "needs_planner",
  "scope_matches": [],
  "unmatched_scope_inputs": [],
  "needs_planner_reasons": [{ "token": null, "kind": "required-scope-missing", "reason": "supply scope" }],
  "coverage_disclaimer": "Declared mappings do not establish map truth or caller scope completeness",
  "admission": { "required": false, "member_ids": [], "model": "one admission per group run through EFFIGY_ADMISSION_HOOK, when set; members run inside it" },
  "members": [],
  "capabilities": { "hard_timeout": false, "stop": false, "prerequisite": "owned-run supervision (contract 052)" }
}
"#,
        )
        .expect("plan fixture")
    }

    #[test]
    fn plan_json_round_trips_the_schema_fields() {
        let plan = minimal_plan();
        assert_eq!(plan.schema, QA_GROUP_PLAN_SCHEMA);
        let rendered = render_qa_group_plan_json(&plan, false).expect("render");
        let value: serde_json::Value = serde_json::from_str(&rendered).expect("json");
        assert_eq!(value["schema"], "effigy.qa-group-plan.v1");
        assert_eq!(value["scope_assessment"], "needs_planner");
        assert_eq!(value["capabilities"]["hard_timeout"], false);
    }

    #[test]
    fn plan_text_names_the_prerequisite_and_planner_verdict() {
        let rendered = render_qa_group_plan_text(&minimal_plan()).expect("render");
        assert!(rendered.contains("needs_planner"), "{rendered}");
        assert!(rendered.contains("contract 052"), "{rendered}");
        assert!(rendered.contains("no run was created"), "{rendered}");
        assert!(rendered.contains("• warn: needs_planner:"), "{rendered}");
        assert!(
            !rendered.contains("running"),
            "a plan is not a run: {rendered}"
        );
    }

    #[test]
    fn inventory_json_carries_the_schema_id() {
        let result = crate::qa_groups::inventory::ListQaGroupsResult {
            rows: Vec::new(),
            file: None,
            filter: None,
        };
        let rendered = render_qa_groups_json(&result, false).expect("render");
        let value: serde_json::Value = serde_json::from_str(&rendered).expect("json");
        assert_eq!(value["schema"], "effigy.qa-groups.v1");
        assert_eq!(value["count"], 0);
        let text = render_qa_groups_text(&result).expect("render");
        assert!(text.contains("QA Groups"), "{text}");
    }

    fn inventory_row(selector: &str, expired: bool, notices: Vec<String>) -> QaGroupListRow {
        QaGroupListRow {
            selector: selector.to_owned(),
            name: "build".to_owned(),
            lifecycle: "maintained".to_owned(),
            expired,
            catalog: "acme".to_owned(),
            catalog_root: "/repo".to_owned(),
            source: "effigy.toml".to_owned(),
            definition_sha256: Some("sha256:definition".to_owned()),
            file_tracking: None,
            purpose: "build the workspace".to_owned(),
            scope_policy: "required".to_owned(),
            expected_wall_ms: Some(2_500),
            member_count: 2,
            notices,
        }
    }

    /// The inventory text names the filter, each row's lifecycle, expiry,
    /// member count, purpose, source, wall-time evidence and notices, plus the
    /// selected temporary file — every field a caller would otherwise read out
    /// of the JSON payload.
    #[test]
    fn inventory_text_renders_the_filter_rows_and_the_selected_file() {
        let result = ListQaGroupsResult {
            rows: vec![inventory_row(
                "acme/build",
                true,
                vec!["expired on 2026-01-01".to_owned()],
            )],
            file: Some(TemporaryFileSummary {
                path: "qa/temp.toml".to_owned(),
                definition_sha256: "sha256:temporary".to_owned(),
                tracking: "untracked".to_owned(),
                notices: vec!["untracked definition".to_owned()],
            }),
            filter: Some("acme/build".to_owned()),
        };

        let text = render_qa_groups_text(&result).expect("render");
        assert!(text.contains("QA Group Matches: acme/build"), "{text}");
        assert!(text.contains("count: 1"), "{text}");
        assert!(
            text.contains("- acme/build : maintained · required expired · 2 member(s)"),
            "{text}"
        );
        assert!(text.contains("purpose: build the workspace"), "{text}");
        assert!(text.contains("source: effigy.toml"), "{text}");
        assert!(text.contains("expected wall time: 2500 ms"), "{text}");
        assert!(text.contains("expired on 2026-01-01"), "{text}");
        assert!(
            text.contains("selected file: qa/temp.toml (untracked, sha256:temporary)"),
            "{text}"
        );
        assert!(text.contains("untracked definition"), "{text}");
        assert!(
            !text.contains("pass --file"),
            "the hint is only for a listing with no file: {text}"
        );
    }

    /// An empty listing keeps the no-directory-scan contract and an unknown
    /// wall time distinct from a zero budget.
    #[test]
    fn inventory_text_marks_an_empty_listing_and_an_unknown_wall_time() {
        let result = ListQaGroupsResult {
            rows: Vec::new(),
            file: None,
            filter: None,
        };
        let text = render_qa_groups_text(&result).expect("render");
        assert!(text.contains("QA Groups"), "{text}");
        assert!(text.contains("count: 0"), "{text}");
        assert!(
            text.contains("pass --file to include one temporary definition"),
            "{text}"
        );
        assert!(text.contains("none"), "{text}");

        let mut unknown = inventory_row("acme/build", false, Vec::new());
        unknown.expected_wall_ms = None;
        let text = render_qa_groups_text(&ListQaGroupsResult {
            rows: vec![unknown],
            file: None,
            filter: None,
        })
        .expect("render");
        assert!(text.contains("expected wall time: unknown"), "{text}");
    }

    /// The inventory JSON carries the rows, the count and the filter, so the
    /// text and the payload are projections of one result.
    #[test]
    fn inventory_json_carries_the_rows_and_the_filter() {
        let result = ListQaGroupsResult {
            rows: vec![inventory_row("acme/build", false, Vec::new())],
            file: None,
            filter: Some("build".to_owned()),
        };
        let rendered = render_qa_groups_json(&result, false).expect("render");
        let value: serde_json::Value = serde_json::from_str(&rendered).expect("json");
        assert_eq!(value["count"], 1);
        assert_eq!(value["filter"], "build");
        assert_eq!(value["qa_groups"][0]["selector"], "acme/build");
        assert_eq!(value["qa_groups"][0]["expected_wall_ms"], 2_500);
    }

    /// A plan with members renders each member's resolved route, targets,
    /// covers, companions and limits, plus the group's proof limits, coverage
    /// gaps, expiry notice and heavy-admission sentence.
    fn plan_with_members() -> QaGroupPlan {
        let mut plan = minimal_plan();
        plan.executable = true;
        plan.scope_assessment = crate::qa_groups::plan::ScopeAssessment::DeclaredMatch;
        plan.group.expired = true;
        plan.group.expected_wall_ms = Some(90_000);
        plan.group.expectation_basis = Some("declared".to_owned());
        plan.group.proof_limits = vec!["bounded".to_owned()];
        plan.group.coverage_gaps = vec![crate::qa_groups::plan::QaCoverageGapEntry {
            input: "src/api".to_owned(),
            reason: "no member declares it".to_owned(),
        }];
        plan.scope_inputs = vec!["src/api".to_owned()];
        plan.admission = crate::qa_groups::plan::QaGroupAdmissionPlan {
            required: true,
            member_ids: vec!["lint".to_owned()],
            model: "one admission per group run",
        };
        plan.needs_planner_reasons = Vec::new();
        plan.members = vec![crate::qa_groups::plan::QaGroupPlanMember {
            id: "lint".to_owned(),
            kind: "task".to_owned(),
            surface: "published".to_owned(),
            catalog: "acme".to_owned(),
            resolved_selector: "acme/lint".to_owned(),
            task: "lint".to_owned(),
            args: vec!["--fix".to_owned()],
            targets: vec!["src/api".to_owned()],
            covers: vec!["src/api".to_owned()],
            companions: vec!["fmt".to_owned()],
            limits: vec!["no native proof".to_owned()],
            admission: "heavy".to_owned(),
            heavy_reasons: vec!["native".to_owned()],
            declared_run_in: "host".to_owned(),
        }];
        plan
    }

    #[test]
    fn plan_text_renders_members_limits_and_coverage_gaps() {
        let text = render_qa_group_plan_text(&plan_with_members()).expect("render");
        assert!(text.contains("QA Group Plan: sample (maintained"), "{text}");
        assert!(text.contains("expected wall time: 90000 ms"), "{text}");
        assert!(text.contains("scope assessment: declared_match"), "{text}");
        assert!(text.contains("expectation basis: declared"), "{text}");
        assert!(text.contains("proof limit: bounded"), "{text}");
        assert!(
            text.contains("coverage gap: src/api — no member declares it"),
            "{text}"
        );
        assert!(text.contains("advisory only"), "{text}");
        assert!(text.contains("scope input: src/api"), "{text}");
        assert!(
            text.contains("heavy (members: lint); one admission for the whole group"),
            "{text}"
        );
        assert!(
            text.contains("- lint : task · published · acme/lint"),
            "{text}"
        );
        assert!(text.contains("args [\"--fix\"]"), "{text}");
        assert!(text.contains("targets: src/api"), "{text}");
        assert!(text.contains("covers: src/api"), "{text}");
        assert!(text.contains("companions: fmt"), "{text}");
        assert!(text.contains("limit: no native proof"), "{text}");
        assert!(
            !text.contains("no run was created"),
            "an executable plan is not a refused plan: {text}"
        );
    }

    #[test]
    fn plan_json_carries_the_member_and_admission_shape() {
        let rendered = render_qa_group_plan_json(&plan_with_members(), false).expect("render");
        let value: serde_json::Value = serde_json::from_str(&rendered).expect("json");
        assert_eq!(value["executable"], true);
        assert_eq!(value["members"][0]["resolved_selector"], "acme/lint");
        assert_eq!(value["members"][0]["args"][0], "--fix");
        assert_eq!(value["admission"]["required"], true);
        assert_eq!(value["admission"]["member_ids"][0], "lint");
        assert_eq!(value["group"]["expected_wall_ms"], 90_000);
    }
}
