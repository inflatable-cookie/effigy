//! Pure retirement plan for one runtime scope.
//!
//! Deletion decisions use recorded identity plus backend labels. The planner
//! never matches by name prefix and never marks success while owned mutable
//! resources remain.

use super::scope::{ScopeComposeKind, ScopeRecord};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ObservedKind {
    Container,
    Volume,
    Network,
    Route,
    Port,
    Loopback,
    TlsCert,
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
        skip_reason: shared_identity_skip_reason(record),
        delete,
        retain,
        mismatch,
    }
}

fn shared_identity_skip_reason(record: &ScopeRecord) -> Option<&'static str> {
    if record.compose_kind == ScopeComposeKind::SharedIdentity
        && record.project_names.is_empty()
        && record.routes.is_empty()
        && record.loopback_identities.is_empty()
        && record.pending_tls_certs.is_empty()
    {
        Some("shared_runtime_identity")
    } else {
        None
    }
}

/// After attempted deletion, resources that still exist and should have been
/// removed. Retained persistent, shared, or external resources are omitted.
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
    if resource.kind == ObservedKind::Route
        && record
            .retain_routes
            .iter()
            .any(|route| route == &resource.name)
    {
        return ResourceClass::Retain;
    }
    if resource.kind == ObservedKind::Loopback
        && record
            .retain_loopback_identities
            .iter()
            .any(|identity| identity == &resource.name)
    {
        return ResourceClass::Retain;
    }
    if resource.kind == ObservedKind::TlsCert
        && record
            .retain_routes
            .iter()
            .any(|route| route == &resource.name)
    {
        return ResourceClass::Retain;
    }
    if resource
        .scope_label
        .as_deref()
        .is_some_and(|scope| scope != record.token)
    {
        return ResourceClass::Mismatch;
    }
    if resource.kind == ObservedKind::Volume
        && (resource.persist
            || record
                .retain_volumes
                .iter()
                .any(|name| name == &resource.name))
    {
        return ResourceClass::Retain;
    }
    if resource
        .scope_label
        .as_deref()
        .is_some_and(|scope| scope == record.token)
    {
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
    if resource.kind == ObservedKind::TlsCert
        && record
            .pending_tls_certs
            .iter()
            .any(|domain| domain == &resource.name)
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
            profiles: vec!["effigy".to_owned()],
            project_names: vec!["app-dev-wt-aaaaaaaaaaaa".to_owned()],
            retain_project_names: vec!["shared-redis".to_owned()],
            repo_owned_projects: vec![],
            owned_volumes: vec![
                "app-dev-wt-aaaaaaaaaaaa-db-data".to_owned(),
                "app-dev-wt-aaaaaaaaaaaa-target".to_owned(),
            ],
            retain_volumes: vec!["app-dev-wt-aaaaaaaaaaaa-db-data".to_owned()],
            routes: vec!["app-waaaaaaaa.test".to_owned()],
            retain_routes: vec![],
            loopback_identities: vec!["project:app-dev-wt-aaaaaaaaaaaa:/tmp/worker".to_owned()],
            retain_loopback_identities: vec![],
            pending_tls_certs: vec![],
        }
    }

    fn route(name: &str, token: &str) -> ObservedResource {
        ObservedResource {
            kind: ObservedKind::Route,
            name: name.to_owned(),
            scope_label: Some(token.to_owned()),
            project_label: Some("/tmp/worker".to_owned()),
            persist: false,
            external: false,
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
    fn retains_persistent_shared_and_external_volumes_and_deletes_mutable_owned() {
        let mut record = record();
        record.retain_volumes = vec!["app-dev-wt-aaaaaaaaaaaa-db-data".to_owned()];
        let plan = plan_retirement(
            &record,
            &[
                volume(
                    "app-dev-wt-aaaaaaaaaaaa-db-data",
                    Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
                    Some("app-dev-wt-aaaaaaaaaaaa"),
                    true,
                    false,
                ),
                volume(
                    "app-dev-wt-aaaaaaaaaaaa-target",
                    Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
                    Some("app-dev-wt-aaaaaaaaaaaa"),
                    false,
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
        assert_eq!(plan.delete[0].name, "app-dev-wt-aaaaaaaaaaaa-target");
        assert_eq!(plan.retain.len(), 3);
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
            "app-dev-wt-aaaaaaaaaaaa-target",
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            Some("app-dev-wt-aaaaaaaaaaaa"),
            false,
            false,
        );
        let remaining = remaining_after(&record, std::slice::from_ref(&leftover));
        assert_eq!(remaining, vec![leftover]);
    }

    #[test]
    fn shared_identity_skips_deletion() {
        let mut record = record();
        record.compose_kind = ScopeComposeKind::SharedIdentity;
        record.project_names.clear();
        record.routes.clear();
        record.loopback_identities.clear();
        record.retain_project_names.push("app-dev".to_owned());
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
        assert_eq!(plan.retain.len(), 1);
    }

    #[test]
    fn mixed_shared_and_isolated_retains_shared_routes() {
        let mut record = record();
        record.retain_routes = vec!["app.test".to_owned()];
        record.retain_loopback_identities = vec!["project:app-dev:/tmp/worker".to_owned()];
        let token = record.token.clone();
        let plan = plan_retirement(
            &record,
            &[
                route("app-waaaaaaaa.test", &token),
                route("app.test", &token),
                ObservedResource {
                    kind: ObservedKind::Loopback,
                    name: "project:app-dev:/tmp/worker".to_owned(),
                    scope_label: Some(token.clone()),
                    project_label: Some("app-dev".to_owned()),
                    persist: false,
                    external: false,
                },
            ],
        );
        assert!(plan.skip_reason.is_none());
        assert_eq!(plan.delete.len(), 1);
        assert_eq!(plan.delete[0].name, "app-waaaaaaaa.test");
        assert_eq!(plan.retain.len(), 2);
        assert!(plan
            .retain
            .iter()
            .any(|resource| resource.name == "app.test"));
    }

    #[test]
    fn pending_tls_cert_is_owned_until_the_files_are_gone() {
        let mut record = record();
        record.pending_tls_certs = vec!["app-waaaaaaaa.test".to_owned()];
        let leftover = ObservedResource {
            kind: ObservedKind::TlsCert,
            name: "app-waaaaaaaa.test".to_owned(),
            scope_label: Some(record.token.clone()),
            project_label: None,
            persist: false,
            external: false,
        };
        let remaining = remaining_after(&record, std::slice::from_ref(&leftover));
        assert_eq!(remaining, vec![leftover]);
    }

    #[test]
    fn repo_owned_persistent_volume_is_retained() {
        let mut record = record();
        record.compose_kind = ScopeComposeKind::RepoOwned;
        record.repo_owned_projects = vec!["app-dev-wt-aaaaaaaaaaaa".to_owned()];
        let plan = plan_retirement(
            &record,
            &[volume(
                "app-dev-wt-aaaaaaaaaaaa-db-data",
                Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
                Some("app-dev-wt-aaaaaaaaaaaa"),
                true,
                false,
            )],
        );
        assert!(plan.delete.is_empty());
        assert_eq!(plan.retain.len(), 1);
    }

    #[test]
    fn persist_flag_retains_volume_even_without_recorded_name() {
        let plan = plan_retirement(
            &record(),
            &[volume(
                "unrecorded-persist",
                Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
                Some("app-dev-wt-aaaaaaaaaaaa"),
                true,
                false,
            )],
        );
        assert!(plan.delete.is_empty());
        assert_eq!(plan.retain.len(), 1);
    }

    #[test]
    fn deletes_owned_network_by_project_label() {
        let plan = plan_retirement(
            &record(),
            &[ObservedResource {
                kind: ObservedKind::Network,
                name: "app-dev-wt-aaaaaaaaaaaa_default".to_owned(),
                scope_label: Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned()),
                project_label: Some("app-dev-wt-aaaaaaaaaaaa".to_owned()),
                persist: false,
                external: false,
            }],
        );
        assert_eq!(plan.delete.len(), 1);
        assert_eq!(plan.delete[0].name, "app-dev-wt-aaaaaaaaaaaa_default");
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
