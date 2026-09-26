//! Pure retirement plan for one runtime scope.
//!
//! Deletion decisions use recorded identity plus backend labels. The planner
//! never matches by name prefix and never marks success while owned mutable
//! resources remain.

use super::scope::{ScopeComposeKind, ScopeRecord};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservedKind {
    Container,
    Volume,
    Network,
    Route,
    Port,
    Loopback,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedResource {
    pub kind: ObservedKind,
    pub name: String,
    pub scope_label: Option<String>,
    pub project_label: Option<String>,
    pub persist: bool,
    pub external: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetirementPlan {
    pub compose_kind: ScopeComposeKind,
    pub skip_reason: Option<&'static str>,
    pub delete: Vec<ObservedResource>,
    pub retain: Vec<ObservedResource>,
    pub mismatch: Vec<ObservedResource>,
}

pub fn plan_retirement(record: &ScopeRecord, observed: &[ObservedResource]) -> RetirementPlan {
    if record.compose_kind == ScopeComposeKind::SharedIdentity {
        return RetirementPlan {
            compose_kind: record.compose_kind,
            skip_reason: Some("shared_runtime_identity"),
            delete: Vec::new(),
            retain: observed.to_vec(),
            mismatch: Vec::new(),
        };
    }

    let mut delete = Vec::new();
    let mut retain = Vec::new();
    let mut mismatch = Vec::new();
    for resource in observed {
        match classify(record, resource) {
            ResourceClass::Delete => delete.push(resource.clone()),
            ResourceClass::Retain => retain.push(resource.clone()),
            ResourceClass::Mismatch => mismatch.push(resource.clone()),
        }
    }
    RetirementPlan {
        compose_kind: record.compose_kind,
        skip_reason: None,
        delete,
        retain,
        mismatch,
    }
}

/// After attempted deletion, resources that still exist and should have been
/// removed. Empty remaining plus empty mismatch is the success condition.
pub fn remaining_after(
    record: &ScopeRecord,
    still_present: &[ObservedResource],
) -> Vec<ObservedResource> {
    still_present
        .iter()
        .filter(|resource| classify(record, resource) == ResourceClass::Delete)
        .cloned()
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResourceClass {
    Delete,
    Retain,
    Mismatch,
}

fn classify(record: &ScopeRecord, resource: &ObservedResource) -> ResourceClass {
    if resource.external {
        return ResourceClass::Retain;
    }
    if resource.project_label.as_deref().is_some_and(|project| {
        record
            .retain_project_names
            .iter()
            .any(|name| name == project)
    }) {
        return ResourceClass::Retain;
    }
    if let Some(scope) = resource.scope_label.as_deref() {
        if scope != record.token {
            return ResourceClass::Mismatch;
        }
        return ResourceClass::Delete;
    }
    if resource
        .project_label
        .as_deref()
        .is_some_and(|project| record.project_names.iter().any(|name| name == project))
    {
        return ResourceClass::Delete;
    }
    if resource.kind == ObservedKind::Route
        && record.routes.iter().any(|route| route == &resource.name)
    {
        return ResourceClass::Delete;
    }
    if resource.kind == ObservedKind::Port
        && resource
            .project_label
            .as_deref()
            .is_some_and(|project| record.project_names.iter().any(|name| name == project))
    {
        return ResourceClass::Delete;
    }
    if resource.kind == ObservedKind::Loopback
        && record
            .loopback_identities
            .iter()
            .any(|identity| identity == &resource.name)
    {
        return ResourceClass::Delete;
    }
    ResourceClass::Mismatch
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> ScopeRecord {
        ScopeRecord {
            schema: "effigy.runtime-scope.v1".to_owned(),
            schema_version: 1,
            token: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
            host_key: "aaaaaaaa".to_owned(),
            checkout: "/tmp/worker".to_owned(),
            updated_unix: 1,
            compose_kind: ScopeComposeKind::Generated,
            profile: "effigy".to_owned(),
            project_names: vec!["app-dev-wt-aaaaaaaaaaaa".to_owned()],
            retain_project_names: vec!["shared-redis".to_owned()],
            owned_volumes: vec!["app-dev-wt-aaaaaaaaaaaa-db-data".to_owned()],
            retain_volumes: vec!["shared-redis".to_owned()],
            routes: vec!["app-waaaaaaaa.test".to_owned()],
            loopback_identities: vec!["project:app-dev-wt-aaaaaaaaaaaa:/tmp/worker".to_owned()],
        }
    }

    fn volume(
        name: &str,
        scope: Option<&str>,
        project: Option<&str>,
        persist: bool,
        external: bool,
    ) -> ObservedResource {
        ObservedResource {
            kind: ObservedKind::Volume,
            name: name.to_owned(),
            scope_label: scope.map(str::to_owned),
            project_label: project.map(str::to_owned),
            persist,
            external,
        }
    }

    #[test]
    fn deletes_owned_persist_volume_and_keeps_shared_and_foreign() {
        let plan = plan_retirement(
            &record(),
            &[
                volume(
                    "app-dev-wt-aaaaaaaaaaaa-db-data",
                    Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
                    Some("app-dev-wt-aaaaaaaaaaaa"),
                    true,
                    false,
                ),
                volume("shared-redis-data", None, Some("shared-redis"), true, false),
                volume(
                    "other-db",
                    Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
                    Some("other-dev"),
                    true,
                    false,
                ),
                volume("external-data", None, None, true, true),
            ],
        );
        assert_eq!(plan.delete.len(), 1);
        assert_eq!(plan.delete[0].name, "app-dev-wt-aaaaaaaaaaaa-db-data");
        assert_eq!(plan.retain.len(), 2);
        assert_eq!(plan.mismatch.len(), 1);
        assert_eq!(plan.mismatch[0].name, "other-db");
    }

    #[test]
    fn idempotent_when_nothing_remains() {
        let record = record();
        let remaining = remaining_after(&record, &[]);
        assert!(remaining.is_empty());
        let again = plan_retirement(&record, &[]);
        assert!(again.delete.is_empty());
        assert!(again.mismatch.is_empty());
    }

    #[test]
    fn interrupted_cleanup_reports_remaining_owned() {
        let record = record();
        let leftover = volume(
            "app-dev-wt-aaaaaaaaaaaa-db-data",
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            Some("app-dev-wt-aaaaaaaaaaaa"),
            true,
            false,
        );
        let remaining = remaining_after(&record, std::slice::from_ref(&leftover));
        assert_eq!(remaining, vec![leftover]);
    }

    #[test]
    fn shared_identity_skips_deletion() {
        let mut record = record();
        record.compose_kind = ScopeComposeKind::SharedIdentity;
        let plan = plan_retirement(
            &record,
            &[volume(
                "app-dev-db-data",
                None,
                Some("app-dev"),
                true,
                false,
            )],
        );
        assert_eq!(plan.skip_reason, Some("shared_runtime_identity"));
        assert!(plan.delete.is_empty());
    }

    #[test]
    fn does_not_delete_by_name_prefix_without_proof() {
        let plan = plan_retirement(
            &record(),
            &[volume(
                "app-dev-wt-aaaaaaaaaaaa-lookalike",
                None,
                None,
                false,
                false,
            )],
        );
        assert!(plan.delete.is_empty());
        assert_eq!(plan.mismatch.len(), 1);
    }
}
