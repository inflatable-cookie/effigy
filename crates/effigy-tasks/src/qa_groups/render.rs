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
        EffigyTasksError::message(format!("failed to encode qa-group plan: {error}"))
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
                    "heavy (members: {}); one host-wide lease for the whole run",
                    plan.admission.member_ids.join(", ")
                )
            } else {
                "standard; no heavy lease needed".to_owned()
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
        "hard_timeout_ms and stop are unavailable: owned-run supervision (contract 052) has not landed",
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
  "admission": { "required": false, "member_ids": [], "model": "one host-wide heavy lease per group run; serial members reserve the maximum serial requirement" },
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
}
