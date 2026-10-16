//! `data_from` volume sharing: handover planning and borrowed-volume
//! preparation.
//!
//! A container environment with `data_from = "<owner>"` mounts the owner
//! environment's named volumes for every service the two environments
//! declare with the same name and catalog entry. Everything here is pure
//! planning over an [`EffectiveContainerPolicy`]; the runner executes the
//! returned command and compose plans through its manager adapters.

use effigy_catalog::volumes::DockerCommand;

use crate::policy::hosts::scope_token;
use crate::policy::model::{ContainerPolicyError, EffectiveContainerPolicy};
use crate::runtime::scope::{
    MANAGED_LABEL, MOUNT_TARGET_LABEL, PERSIST_LABEL, PROJECT_LABEL, REPO_ROOT_LABEL, SCOPE_LABEL,
    SERVICE_LABEL, VOLUME_NAME_LABEL,
};

/// One peer environment whose services must stop before this environment's
/// data services start, so both never hold the shared volumes at once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolumeDataHandover {
    /// One human-readable line naming what is handed over and to whom.
    pub note: String,
    /// The peer environment's manifest name.
    pub environment: String,
    /// The peer environment's Compose project name in this checkout.
    pub project_name: String,
    /// The peer's services holding the shared volumes, sorted.
    pub services: Vec<String>,
}

/// Plan the handovers this environment must perform before its data
/// services start. Empty when the environment has no `data_from` links.
pub fn plan_volume_data_handover(policy: &EffectiveContainerPolicy) -> Vec<VolumeDataHandover> {
    policy
        .data_peers
        .iter()
        .filter(|peer| !peer.services.is_empty())
        .map(|peer| {
            let handed = human_service_list(&peer.services);
            let note = if policy.data_from.as_deref() == Some(peer.environment.as_str()) {
                format!(
                    "[data_from] handing over {handed} data volume{} from `{owner}` to `{name}`",
                    if peer.services.len() == 1 { "" } else { "s" },
                    owner = peer.environment,
                    name = policy.name,
                )
            } else {
                format!(
                    "[data_from] taking back {handed} data volume{} from `{peer}` to `{name}`",
                    if peer.services.len() == 1 { "" } else { "s" },
                    peer = peer.environment,
                    name = policy.name,
                )
            };
            VolumeDataHandover {
                note,
                environment: peer.environment.clone(),
                project_name: peer.project_name.clone(),
                services: peer.services.clone(),
            }
        })
        .collect()
}

fn human_service_list(services: &[String]) -> String {
    services.join(", ")
}

/// `docker volume create` command specs that make every borrowed volume
/// exist before the borrowing environment's Compose up resolves its
/// external volume entries. Creation is idempotent: an existing volume is
/// returned unchanged, so the owner's data is never overwritten and the
/// labels below only land on volumes this environment creates first.
pub fn borrowed_volume_ensure_commands(
    policy: &EffectiveContainerPolicy,
) -> Result<Vec<DockerCommand>, ContainerPolicyError> {
    if policy.borrowed_volumes.is_empty() {
        return Ok(Vec::new());
    }
    let token = scope_token(&policy.repo_root, policy_share_runtime_identity())?;
    Ok(policy
        .borrowed_volumes
        .iter()
        .map(|volume| {
            let mut args = vec![
                "volume".to_owned(),
                "create".to_owned(),
                "--label".to_owned(),
                format!("{MANAGED_LABEL}=true"),
                "--label".to_owned(),
                format!("{PROJECT_LABEL}={}", peer_project_name(policy)),
                "--label".to_owned(),
                format!("{SERVICE_LABEL}={}", volume.service),
                "--label".to_owned(),
                format!("{VOLUME_NAME_LABEL}={}", volume.name),
                "--label".to_owned(),
                format!(
                    "{PERSIST_LABEL}={}",
                    if volume.persist { "true" } else { "false" }
                ),
            ];
            if let Some(token) = token.as_deref() {
                args.push("--label".to_owned());
                args.push(format!("{SCOPE_LABEL}={token}"));
            }
            args.push("--label".to_owned());
            args.push(format!("{REPO_ROOT_LABEL}={}", policy.repo_root.display()));
            if let Some(target) = volume.mount_target.as_deref() {
                args.push("--label".to_owned());
                args.push(format!("{MOUNT_TARGET_LABEL}={target}"));
            }
            args.push(volume.name.clone());
            DockerCommand {
                program: "docker".to_owned(),
                args,
                description: format!("ensure borrowed volume `{}` exists", volume.name),
            }
        })
        .collect())
}

fn policy_share_runtime_identity() -> bool {
    // A borrowing environment never shares its own runtime identity; the
    // manifest refuses that combination. Pass false so the scope token is
    // always the checkout's generation token.
    false
}

fn peer_project_name(policy: &EffectiveContainerPolicy) -> String {
    policy
        .data_from
        .as_deref()
        .and_then(|owner| {
            policy
                .data_peers
                .iter()
                .find(|peer| peer.environment == owner)
        })
        .map(|peer| peer.project_name.clone())
        .unwrap_or_else(|| policy.project_name.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::model::{test_policy, EffectiveDataPeer};
    use effigy_catalog::volumes::ManagedVolume;

    fn policy(
        data_from: Option<&str>,
        peers: Vec<EffectiveDataPeer>,
    ) -> (tempfile::TempDir, EffectiveContainerPolicy) {
        let root = tempfile::tempdir().expect("temporary fixture root");
        let mut policy = test_policy(root.path());
        policy.data_from = data_from.map(str::to_owned);
        policy.data_peers = peers;
        policy.borrowed_volumes = vec![ManagedVolume {
            name: "acowtancy-dev-db-data".to_owned(),
            service: "db".to_owned(),
            persist: true,
            size_bytes: None,
            mount_point: None,
            mount_target: Some("/var/lib/postgresql/data".to_owned()),
        }];
        (root, policy)
    }

    fn peer(environment: &str, project: &str, services: &[&str]) -> EffectiveDataPeer {
        EffectiveDataPeer {
            environment: environment.to_owned(),
            project_name: project.to_owned(),
            services: services.iter().map(|name| (*name).to_owned()).collect(),
        }
    }

    #[test]
    fn handover_plan_names_owner_direction() {
        let (_root, policy) = policy(
            Some("dev"),
            vec![peer("dev", "acowtancy-dev", &["db", "s3"])],
        );
        let plan = plan_volume_data_handover(&policy);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].environment, "dev");
        assert_eq!(plan[0].project_name, "acowtancy-dev");
        assert_eq!(plan[0].services, vec!["db".to_owned(), "s3".to_owned()]);
        assert!(
            plan[0]
                .note
                .contains("handing over db, s3 data volumes from `dev` to `hybrid`"),
            "{}",
            plan[0].note
        );
    }

    #[test]
    fn handover_plan_names_take_back_direction() {
        let (_root, policy) = policy(None, vec![peer("hybrid", "acowtancy-dev-hybrid", &["db"])]);
        let plan = plan_volume_data_handover(&policy);
        assert_eq!(plan.len(), 1);
        assert!(
            plan[0]
                .note
                .contains("taking back db data volume from `hybrid`"),
            "{}",
            plan[0].note
        );
    }

    #[test]
    fn handover_plan_is_empty_without_peers() {
        let (_root, policy) = policy(None, vec![]);
        assert!(plan_volume_data_handover(&policy).is_empty());
    }

    #[test]
    fn borrowed_volume_ensure_commands_are_idempotent_creates_with_owner_labels() {
        let (_root, policy) = policy(Some("dev"), vec![peer("dev", "acowtancy-dev", &["db"])]);
        let commands = borrowed_volume_ensure_commands(&policy).expect("ensure commands");
        assert_eq!(commands.len(), 1);
        let command = &commands[0];
        assert_eq!(command.program, "docker");
        assert_eq!(command.args.first().map(String::as_str), Some("volume"));
        assert_eq!(command.args.get(1).map(String::as_str), Some("create"));
        assert!(
            command
                .args
                .windows(2)
                .any(|pair| pair[0] == "--label"
                    && pair[1] == format!("{PROJECT_LABEL}=acowtancy-dev")),
            "args: {:?}",
            command.args
        );
        assert_eq!(
            command.args.last().map(String::as_str),
            Some("acowtancy-dev-db-data")
        );
    }
}
