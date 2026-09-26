//! Durable runtime-scope inventory stored outside the worktree.
//!
//! Git's private worktree directory is removed when a checkout is retired.
//! This record keeps the generation token and owned-resource inventory under
//! `~/.effigy/runtime-scopes/` so cleanup can retry after a crash or a
//! deleted tree.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::policy::hosts::EffectiveHostMap;
use crate::policy::model::{EffectiveComposeSource, EffectiveContainerPolicy};
use crate::policy_support::effigy_home_dir;

pub const SCOPE_LABEL: &str = "com.effigy.scope";
pub const MANAGED_LABEL: &str = "com.effigy.managed";
pub const PROJECT_LABEL: &str = "com.effigy.project";

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
    pub project_names: Vec<String>,
    pub retain_project_names: Vec<String>,
    pub owned_volumes: Vec<String>,
    pub retain_volumes: Vec<String>,
    pub routes: Vec<String>,
    pub loopback_identities: Vec<String>,
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
        let retain_project_names = policy
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
            .shared_services
            .iter()
            .map(|service| service.project_name.clone())
            .collect::<Vec<_>>();
        let routes = host_map
            .routes
            .iter()
            .map(|route| route.effective.clone())
            .collect::<Vec<_>>();
        let loopback_identities = vec![format!(
            "project:{}:{}",
            policy.project_name,
            policy.repo_root.display()
        )];
        Self {
            schema: RECORD_SCHEMA.to_owned(),
            schema_version: 1,
            token: token.to_owned(),
            host_key: host_key.to_owned(),
            checkout: policy.repo_root.display().to_string(),
            updated_unix: unix_now(),
            compose_kind,
            profile: policy.profile.clone(),
            project_names: vec![policy.project_name.clone()],
            retain_project_names,
            owned_volumes,
            retain_volumes,
            routes,
            loopback_identities,
        }
    }
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
    let payload = serde_json::to_vec_pretty(record).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("cannot serialize runtime-scope record: {error}"),
        )
    })?;
    fs::write(&path, payload)?;
    Ok(path)
}

pub fn load(token: &str) -> io::Result<Option<ScopeRecord>> {
    let path = record_path(token)?;
    match fs::read_to_string(&path) {
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
        let source = fs::read_to_string(&path)?;
        let Ok(record) = serde_json::from_str::<ScopeRecord>(&source) else {
            continue;
        };
        if record.checkout == wanted {
            records.push(record);
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
}
