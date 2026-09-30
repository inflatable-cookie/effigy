//! Durable runtime-scope inventory stored outside the worktree.
//!
//! Git's private worktree directory is removed when a checkout is retired.
//! This record keeps the generation token and owned-resource inventory under
//! `~/.effigy/runtime-scopes/` so cleanup can retry after a crash or a
//! deleted tree.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::policy::hosts::EffectiveHostMap;
use crate::policy::model::{EffectiveComposeSource, EffectiveContainerPolicy};
use crate::policy_support::effigy_home_dir;
use effigy_gateway::loopback::project_loopback_identity;
use effigy_gateway::routes::RouteTableLock;

pub const SCOPE_LABEL: &str = "com.effigy.scope";
pub const MANAGED_LABEL: &str = "com.effigy.managed";
pub const PROJECT_LABEL: &str = "com.effigy.project";
pub const PERSIST_LABEL: &str = "com.effigy.persist";
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
        let retain_volumes = policy
            .managed_volumes
            .iter()
            .filter(|volume| volume.persist)
            .map(|volume| volume.name.clone())
            .collect::<Vec<_>>();
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

pub fn records_dir() -> io::Result<PathBuf> {
    let home = effigy_home_dir().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "cannot resolve Effigy home for runtime-scope records",
        )
    })?;
    Ok(home.join("runtime-scopes"))
}

pub fn record_path(token: &str) -> io::Result<PathBuf> {
    Ok(records_dir()?.join(format!("{token}.json")))
}

pub fn upsert(record: &ScopeRecord) -> io::Result<PathBuf> {
    let dir = records_dir()?;
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.json", record.token));
    let _lock = lock_record(&path)?;
    let merged = match read_record_file(&path)? {
        Some(existing) if existing.checkout == record.checkout => existing.merged_with(record),
        Some(_) => record.clone(),
        None => record.clone(),
    };
    let payload = serde_json::to_vec_pretty(&merged).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("cannot serialize runtime-scope record: {error}"),
        )
    })?;
    atomic_write(&path, &payload)?;
    Ok(path)
}

fn lock_record(path: &Path) -> io::Result<RouteTableLock> {
    RouteTableLock::acquire(path).map_err(|error| {
        io::Error::other(format!(
            "cannot lock runtime-scope record {}: {error}",
            path.display()
        ))
    })
}

fn atomic_write(path: &Path, payload: &[u8]) -> io::Result<()> {
    let tmp = path.with_file_name(format!(
        ".{}.{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("scope.json"),
        std::process::id(),
        unix_now()
    ));
    fs::write(&tmp, payload)?;
    fs::rename(&tmp, path).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })?;
    Ok(())
}

fn read_record_file(path: &Path) -> io::Result<Option<ScopeRecord>> {
    match fs::read_to_string(path) {
        Ok(source) => {
            let record = serde_json::from_str::<ScopeRecord>(&source).map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "invalid runtime-scope record at {}: {error}",
                        path.display()
                    ),
                )
            })?;
            Ok(Some(record))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

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

pub fn load(token: &str) -> io::Result<Option<ScopeRecord>> {
    read_record_file(&record_path(token)?)
}

pub fn load_for_checkout(checkout: &Path) -> io::Result<Vec<ScopeRecord>> {
    let dir = records_dir()?;
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let wanted = checkout.display().to_string();
    let mut records = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        if let Some(record) = read_record_file(&path)? {
            if record.checkout == wanted {
                records.push(record);
            }
        }
    }
    records.sort_by(|left, right| left.token.cmp(&right.token));
    Ok(records)
}

pub fn remove(token: &str) -> io::Result<()> {
    let path = record_path(token)?;
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
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
    use crate::policy_support::with_test_effigy_home;
    use crate::{EffectiveComposeSource, EffectiveContainerPolicy, EffectiveDnsRoute};
    use effigy_manifest::{
        ManifestContainerDriver, ManifestContainerOnTaskExit, ManifestContainerSecretDelivery,
        ManifestContainerShutdownMode, ManifestContainerStartup,
    };

    fn policy(root: &Path) -> EffectiveContainerPolicy {
        EffectiveContainerPolicy {
            repo_root: root.to_path_buf(),
            name: "web".to_owned(),
            driver: ManifestContainerDriver::Colima,
            startup: ManifestContainerStartup::Detached,
            profile: "effigy".to_owned(),
            compose_source: EffectiveComposeSource::Generated,
            compose_files: vec![],
            compose_file_display: "generated".to_owned(),
            managed_volumes: vec![],
            shared_services: vec![],
            project_name: "app-dev-wt-abcdef012345".to_owned(),
            primary_service: "app".to_owned(),
            dns_domain: Some("app-wabcdef01.test".to_owned()),
            dns_tls: true,
            dns_port: Some(80),
            dns_routes: vec![EffectiveDnsRoute {
                domain: "app-wabcdef01.test".to_owned(),
                declared_domain: "app.test".to_owned(),
                tls: true,
                port: Some(80),
                service: Some("web".to_owned()),
                target_host: None,
            }],
            service_aliases: vec![],
            declared_ports: vec![],
            ports_declared_explicitly: false,
            declared_mounts: vec![],
            declared_media_mounts: vec![],
            pull_production_hook: None,
            health_check: None,
            health_timeout_secs: 60,
            secret_delivery: ManifestContainerSecretDelivery::ComposeEnv,
            secret_runtime_dir: None,
            source_secret_runtime_for_deferrals: false,
            workspace_user: None,
            workspace_home: None,
            on_task_exit: ManifestContainerOnTaskExit::Stop,
            shutdown: ManifestContainerShutdownMode::Graceful,
            detach_timeout_secs: 10,
            host_processes: Vec::new(),
        }
    }

    #[test]
    fn record_survives_missing_checkout() {
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
            let loaded = load("abcdef0123456789abcdef0123456789")
                .unwrap()
                .expect("record");
            assert_eq!(loaded.checkout, checkout.display().to_string());
            assert_eq!(loaded.project_names, vec!["app-dev-wt-abcdef012345"]);
            let by_path = load_for_checkout(&checkout).unwrap();
            assert_eq!(by_path.len(), 1);
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
            let web_map = build_host_map(&web.dns_routes, &[], &[], Some(token), false);
            let worker_map = build_host_map(&worker.dns_routes, &[], &[], Some(token), false);
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
            let loaded = load(token).unwrap().expect("record");
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
            let web_map = build_host_map(&web.dns_routes, &[], &[], Some(token), false);
            let jobs_map = build_host_map(&jobs.dns_routes, &[], &[], Some(token), false);
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
            let loaded = load(token).unwrap().expect("record");
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
            std::fs::create_dir_all(records_dir().unwrap()).unwrap();
            std::fs::write(
                record_path("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap(),
                "{not-json",
            )
            .unwrap();
            let error = load_for_checkout(&checkout).expect_err("corrupt");
            assert_eq!(error.kind(), io::ErrorKind::InvalidData);
            let error = load("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").expect_err("corrupt");
            assert_eq!(error.kind(), io::ErrorKind::InvalidData);
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
