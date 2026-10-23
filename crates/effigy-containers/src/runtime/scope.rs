//! Checkout-local scope inventory. Backend labels provide discovery after a
//! checkout is deleted; an inventory record alone never grants cleanup authority.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::policy::hosts::EffectiveHostMap;
use crate::policy::model::{EffectiveComposeSource, EffectiveContainerPolicy};
use effigy_gateway::loopback::project_loopback_identity;

pub const SCOPE_LABEL: &str = "com.effigy.scope";
pub const MANAGED_LABEL: &str = "com.effigy.managed";
pub const PROJECT_LABEL: &str = "com.effigy.project";
pub const PERSIST_LABEL: &str = "com.effigy.persist";
pub const SERVICE_LABEL: &str = "com.effigy.service";
pub const VOLUME_NAME_LABEL: &str = "com.effigy.volume-name";
pub const MOUNT_TARGET_LABEL: &str = "com.effigy.mount-target";
pub const REPO_ROOT_LABEL: &str = "com.effigy.repo-root";
/// Canonical checkout path that rendered a generated Compose service. nerdctl
/// compose omits Docker's `com.docker.compose.project.working_dir`, so owned
/// binding checks fall back to this label.
pub const CHECKOUT_LABEL: &str = "com.effigy.checkout";
pub const CHECKOUT_FENCE_LABEL: &str = "com.effigy.checkout-fence";
pub const CHECKOUT_GENERATION_LABEL: &str = "com.effigy.checkout-generation";
pub const COMPOSE_PROJECT_LABEL: &str = "com.docker.compose.project";

const RECORD_SCHEMA: &str = "effigy.runtime-scope.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeComposeKind {
    Generated,
    RepoOwned,
    SharedIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopeRecord {
    pub schema: String,
    pub schema_version: u32,
    pub token: String,
    pub host_key: String,
    pub checkout: String,
    pub updated_unix: u64,
    pub compose_kind: ScopeComposeKind,
    pub profile: String,
    #[serde(default)]
    pub profiles: Vec<String>,
    pub project_names: Vec<String>,
    pub retain_project_names: Vec<String>,
    #[serde(default)]
    pub repo_owned_projects: Vec<String>,
    pub owned_volumes: Vec<String>,
    pub retain_volumes: Vec<String>,
    pub routes: Vec<String>,
    #[serde(default)]
    pub retain_routes: Vec<String>,
    pub loopback_identities: Vec<String>,
    #[serde(default)]
    pub retain_loopback_identities: Vec<String>,
    #[serde(default)]
    pub pending_tls_certs: Vec<String>,
    #[serde(default)]
    pub checkout_fence: Option<effigy_kernel::gate::CheckoutFenceIdentity>,
    #[serde(default)]
    pub checkout_generation: Option<effigy_kernel::fs::DirectoryIdentity>,
}

impl ScopeRecord {
    pub fn from_policy(
        policy: &EffectiveContainerPolicy,
        token: &str,
        host_key: &str,
        host_map: &EffectiveHostMap,
        share_runtime_identity: bool,
    ) -> Self {
        let compose_kind = if share_runtime_identity {
            ScopeComposeKind::SharedIdentity
        } else {
            match policy.compose_source {
                EffectiveComposeSource::Generated => ScopeComposeKind::Generated,
                EffectiveComposeSource::Direct => ScopeComposeKind::RepoOwned,
            }
        };
        let mut retain_project_names = policy
            .shared_services
            .iter()
            .map(|service| service.project_name.clone())
            .collect::<Vec<_>>();
        let owned_volumes = policy
            .managed_volumes
            .iter()
            .map(|volume| volume.name.clone())
            .collect::<Vec<_>>();
        // Volumes borrowed through `data_from` belong to the owner
        // environment's project. This record retains them so no retirement
        // path deletes or claims them; they are never owned from a
        // borrowing environment.
        let mut retain_volumes = policy
            .managed_volumes
            .iter()
            .filter(|volume| volume.persist)
            .map(|volume| volume.name.clone())
            .collect::<Vec<_>>();
        retain_volumes.extend(
            policy
                .borrowed_volumes
                .iter()
                .map(|volume| volume.name.clone()),
        );
        let host_routes = host_map
            .routes
            .iter()
            .map(|route| route.effective.clone())
            .collect::<Vec<_>>();
        let loopback = project_loopback_identity(&policy.project_name, &policy.repo_root);
        let mut routes = Vec::new();
        let mut retain_routes = Vec::new();
        let mut loopback_identities = Vec::new();
        let mut retain_loopback_identities = Vec::new();
        let mut repo_owned_projects = Vec::new();
        let project_names = if share_runtime_identity {
            retain_project_names.push(policy.project_name.clone());
            retain_routes = host_routes;
            retain_loopback_identities.push(loopback);
            Vec::new()
        } else {
            routes = host_routes;
            loopback_identities.push(loopback);
            if matches!(policy.compose_source, EffectiveComposeSource::Direct) {
                repo_owned_projects.push(policy.project_name.clone());
            }
            vec![policy.project_name.clone()]
        };
        Self {
            schema: RECORD_SCHEMA.to_owned(),
            schema_version: 1,
            token: token.to_owned(),
            host_key: host_key.to_owned(),
            checkout: policy.repo_root.display().to_string(),
            updated_unix: unix_now(),
            compose_kind,
            profile: policy.profile.clone(),
            profiles: vec![policy.profile.clone()],
            project_names,
            retain_project_names,
            repo_owned_projects,
            owned_volumes,
            retain_volumes,
            routes,
            retain_routes,
            loopback_identities,
            retain_loopback_identities,
            pending_tls_certs: Vec::new(),
            checkout_fence: None,
            checkout_generation: None,
        }
    }

    pub fn observation_profiles(&self) -> Vec<String> {
        if self.profiles.is_empty() {
            vec![self.profile.clone()]
        } else {
            self.profiles.clone()
        }
    }

    pub fn volume_is_persistent(&self, name: &str, labels: &BTreeMap<String, String>) -> bool {
        if let Some(value) = labels.get(PERSIST_LABEL) {
            return value.eq_ignore_ascii_case("true");
        }
        if self.retain_volumes.iter().any(|retained| retained == name) {
            return true;
        }
        let project = labels
            .get(PROJECT_LABEL)
            .or_else(|| labels.get(COMPOSE_PROJECT_LABEL));
        project.is_some_and(|project| {
            self.repo_owned_projects
                .iter()
                .any(|owned| owned == project)
        })
    }

    fn merged_with(&self, other: &Self) -> Self {
        let mut merged = self.clone();
        union_sorted(&mut merged.project_names, &other.project_names);
        union_sorted(
            &mut merged.retain_project_names,
            &other.retain_project_names,
        );
        union_sorted(&mut merged.repo_owned_projects, &other.repo_owned_projects);
        union_sorted(&mut merged.owned_volumes, &other.owned_volumes);
        union_sorted(&mut merged.retain_volumes, &other.retain_volumes);
        union_sorted(&mut merged.routes, &other.routes);
        union_sorted(&mut merged.retain_routes, &other.retain_routes);
        union_sorted(&mut merged.loopback_identities, &other.loopback_identities);
        union_sorted(
            &mut merged.retain_loopback_identities,
            &other.retain_loopback_identities,
        );
        union_sorted(&mut merged.pending_tls_certs, &other.pending_tls_certs);
        union_sorted(&mut merged.profiles, &other.profiles);
        merged.compose_kind = merge_compose_kind(merged.compose_kind, other.compose_kind);
        if other.checkout_fence.is_some() {
            merged.checkout_fence = other.checkout_fence;
        }
        if other.checkout_generation.is_some() {
            merged.checkout_generation = other.checkout_generation;
        }
        merged.host_key = other.host_key.clone();
        merged.profile = other.profile.clone();
        merged.updated_unix = unix_now();
        merged
    }
}

pub fn volume_has_ownership_proof(labels: &BTreeMap<String, String>) -> bool {
    labels.contains_key(SCOPE_LABEL)
        || labels.contains_key(PROJECT_LABEL)
        || labels.contains_key(COMPOSE_PROJECT_LABEL)
}

mod journal;
pub use journal::{
    load, load_for_checkout, load_local_records, remove, upsert, upsert_at, with_retirement,
    ScopeJournalError,
};

fn union_sorted(target: &mut Vec<String>, extra: &[String]) {
    for value in extra {
        if !target.iter().any(|existing| existing == value) {
            target.push(value.clone());
        }
    }
    target.sort();
    target.dedup();
}

fn merge_compose_kind(left: ScopeComposeKind, right: ScopeComposeKind) -> ScopeComposeKind {
    match (left, right) {
        (ScopeComposeKind::Generated, _) | (_, ScopeComposeKind::Generated) => {
            ScopeComposeKind::Generated
        }
        (ScopeComposeKind::RepoOwned, _) | (_, ScopeComposeKind::RepoOwned) => {
            ScopeComposeKind::RepoOwned
        }
        (ScopeComposeKind::SharedIdentity, ScopeComposeKind::SharedIdentity) => {
            ScopeComposeKind::SharedIdentity
        }
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::hosts::build_host_map;
    use crate::policy::model::test_policy;
    use crate::policy_support::with_test_effigy_home;
    use crate::EffectiveContainerPolicy;

    fn policy(root: &Path) -> EffectiveContainerPolicy {
        std::fs::create_dir_all(root).unwrap();
        test_policy(root)
    }

    #[test]
    fn legacy_scope_is_named_read_only_input_and_tombstone_prevents_resurrection() {
        let home = tempfile::tempdir().unwrap();
        with_test_effigy_home(home.path(), || {
            let checkout = home.path().join("checkout");
            let policy = policy(&checkout);
            let token = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
            let record = ScopeRecord::from_policy(
                &policy,
                token,
                "host",
                &build_host_map(&policy.dns_routes, &[], &[], Some(token), false, None),
                false,
            );
            let legacy = home.path().join("runtime-scopes");
            std::fs::create_dir_all(&legacy).unwrap();
            let file = legacy.join(format!("{token}.json"));
            let bytes = serde_json::to_vec(&record).unwrap();
            std::fs::write(&file, &bytes).unwrap();
            assert_eq!(load(&checkout, token).unwrap(), Some(record.clone()));
            assert!(!checkout.join(".effigy").exists());
            remove(&checkout, token).unwrap();
            assert!(load(&checkout, token).unwrap().is_none());
            assert_eq!(std::fs::read(&file).unwrap(), bytes);
            let other = home.path().join("other");
            std::fs::create_dir(&other).unwrap();
            assert!(load(&other, token).is_err());
            assert!(load(&checkout, "../escape").is_err());
        });
    }

    #[test]
    fn deleted_checkout_loses_inventory_and_is_never_recreated() {
        let home = tempfile::tempdir().unwrap();
        with_test_effigy_home(home.path(), || {
            let checkout = home.path().join("worker");
            let policy = policy(&checkout);
            let host_map = build_host_map(
                &policy.dns_routes,
                &[],
                &[],
                Some("abcdef0123456789abcdef0123456789"),
                false,
                None,
            );
            let record = ScopeRecord::from_policy(
                &policy,
                "abcdef0123456789abcdef0123456789",
                "abcdef01",
                &host_map,
                false,
            );
            upsert(&record).unwrap();
            std::fs::remove_dir_all(&checkout).ok();
            assert!(load(&checkout, "abcdef0123456789abcdef0123456789")
                .unwrap()
                .is_none());
            assert!(load_for_checkout(&checkout).unwrap().is_empty());
            assert!(!checkout.exists());
            assert!(!home.path().join("runtime-scopes").exists());
        });
    }

    #[test]
    fn upsert_merges_sibling_environments_for_one_scope() {
        let home = tempfile::tempdir().unwrap();
        with_test_effigy_home(home.path(), || {
            let checkout = home.path().join("worker");
            let mut web = policy(&checkout);
            web.name = "web".to_owned();
            web.project_name = "app-web-wt-abcdef012345".to_owned();
            let mut worker = policy(&checkout);
            worker.name = "worker".to_owned();
            worker.project_name = "app-worker-wt-abcdef012345".to_owned();
            worker.dns_routes[0].domain = "jobs.app-wabcdef01.test".to_owned();
            worker.dns_routes[0].declared_domain = "jobs.app.test".to_owned();
            let token = "abcdef0123456789abcdef0123456789";
            let web_map = build_host_map(&web.dns_routes, &[], &[], Some(token), false, None);
            let worker_map = build_host_map(&worker.dns_routes, &[], &[], Some(token), false, None);
            upsert(&ScopeRecord::from_policy(
                &web, token, "abcdef01", &web_map, false,
            ))
            .unwrap();
            upsert(&ScopeRecord::from_policy(
                &worker,
                token,
                "abcdef01",
                &worker_map,
                false,
            ))
            .unwrap();
            let loaded = load(&checkout, token).unwrap().expect("record");
            assert_eq!(
                loaded.project_names,
                vec![
                    "app-web-wt-abcdef012345".to_owned(),
                    "app-worker-wt-abcdef012345".to_owned()
                ]
            );
            assert!(loaded
                .routes
                .iter()
                .any(|route| route == "app-wabcdef01.test"));
            assert!(loaded
                .routes
                .iter()
                .any(|route| route == "jobs.app-wabcdef01.test"));
            assert_eq!(loaded.profiles, vec!["effigy".to_owned()]);
        });
    }

    #[test]
    fn from_policy_keeps_shared_hosts_on_the_retain_list() {
        let home = tempfile::tempdir().unwrap();
        let policy = policy(home.path());
        let host_map = build_host_map(
            &policy.dns_routes,
            &[],
            &[],
            Some("abcdef0123456789abcdef0123456789"),
            true,
            None,
        );
        let record = ScopeRecord::from_policy(
            &policy,
            "abcdef0123456789abcdef0123456789",
            "abcdef01",
            &host_map,
            true,
        );
        assert!(record.project_names.is_empty());
        assert!(record.routes.is_empty());
        assert!(record.loopback_identities.is_empty());
        assert!(record.retain_project_names.contains(&policy.project_name));
        assert!(record.retain_routes.iter().any(|route| route == "app.test"));
        assert_eq!(record.compose_kind, ScopeComposeKind::SharedIdentity);
    }

    #[test]
    fn from_policy_records_repo_owned_compose_projects() {
        let home = tempfile::tempdir().unwrap();
        let mut policy = policy(home.path());
        policy.compose_source = EffectiveComposeSource::Direct;
        let host_map = build_host_map(
            &policy.dns_routes,
            &[],
            &[],
            Some("abcdef0123456789abcdef0123456789"),
            false,
            None,
        );
        let record = ScopeRecord::from_policy(
            &policy,
            "abcdef0123456789abcdef0123456789",
            "abcdef01",
            &host_map,
            false,
        );
        assert_eq!(record.compose_kind, ScopeComposeKind::RepoOwned);
        assert_eq!(
            record.repo_owned_projects,
            vec![policy.project_name.clone()]
        );
        assert_eq!(record.project_names, vec![policy.project_name]);
    }

    #[test]
    fn upsert_unions_profiles_and_locks_concurrent_writes() {
        let home = tempfile::tempdir().unwrap();
        with_test_effigy_home(home.path(), || {
            let checkout = home.path().join("worker");
            let mut web = policy(&checkout);
            web.profile = "effigy".to_owned();
            web.project_name = "app-web-wt-abcdef012345".to_owned();
            let mut jobs = policy(&checkout);
            jobs.name = "jobs".to_owned();
            jobs.profile = "jobs-profile".to_owned();
            jobs.project_name = "app-jobs-wt-abcdef012345".to_owned();
            jobs.dns_routes[0].domain = "jobs.app-wabcdef01.test".to_owned();
            let token = "abcdef0123456789abcdef0123456789";
            let web_map = build_host_map(&web.dns_routes, &[], &[], Some(token), false, None);
            let jobs_map = build_host_map(&jobs.dns_routes, &[], &[], Some(token), false, None);
            let web_record = ScopeRecord::from_policy(&web, token, "abcdef01", &web_map, false);
            let jobs_record = ScopeRecord::from_policy(&jobs, token, "abcdef01", &jobs_map, false);
            std::thread::scope(|scope| {
                scope.spawn(|| {
                    with_test_effigy_home(home.path(), || upsert(&web_record).unwrap());
                });
                scope.spawn(|| {
                    with_test_effigy_home(home.path(), || upsert(&jobs_record).unwrap());
                });
            });
            let loaded = load(&checkout, token).unwrap().expect("record");
            assert_eq!(
                loaded.project_names,
                vec![
                    "app-jobs-wt-abcdef012345".to_owned(),
                    "app-web-wt-abcdef012345".to_owned()
                ]
            );
            assert_eq!(
                loaded.profiles,
                vec!["effigy".to_owned(), "jobs-profile".to_owned()]
            );
            assert_eq!(loaded.observation_profiles(), loaded.profiles);
        });
    }

    #[test]
    fn load_for_checkout_fails_closed_on_corrupt_record() {
        let home = tempfile::tempdir().unwrap();
        with_test_effigy_home(home.path(), || {
            let checkout = home.path().join("worker");
            std::fs::create_dir_all(checkout.join(".effigy/state/runtime-scopes")).unwrap();
            std::fs::write(
                checkout.join(".effigy/state/runtime-scopes/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.json"),
                "{not-json",
            )
            .unwrap();
            let error = load_for_checkout(&checkout).expect_err("corrupt");
            assert!(error.to_string().contains("scope"));
            let error = load(&checkout, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").expect_err("corrupt");
            assert!(error.to_string().contains("scope"));
        });
    }

    #[test]
    fn repo_owned_volumes_persist_without_an_explicit_label() {
        let home = tempfile::tempdir().unwrap();
        let mut policy = policy(home.path());
        policy.compose_source = EffectiveComposeSource::Direct;
        let host_map = build_host_map(
            &policy.dns_routes,
            &[],
            &[],
            Some("abcdef0123456789abcdef0123456789"),
            false,
            None,
        );
        let record = ScopeRecord::from_policy(
            &policy,
            "abcdef0123456789abcdef0123456789",
            "abcdef01",
            &host_map,
            false,
        );
        let mut labels = BTreeMap::new();
        labels.insert(
            COMPOSE_PROJECT_LABEL.to_owned(),
            policy.project_name.clone(),
        );
        assert!(record.volume_is_persistent("app-dev-db-data", &labels));
        labels.insert(PERSIST_LABEL.to_owned(), "false".to_owned());
        assert!(!record.volume_is_persistent("app-dev-db-data", &labels));
        assert!(volume_has_ownership_proof(&labels));
        assert!(!volume_has_ownership_proof(&BTreeMap::new()));
    }
}
