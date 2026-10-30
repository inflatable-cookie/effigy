//! The runtime restarts a stopped service, applied after `up` started it.
//!
//! A rendered Compose file that declared `restart: unless-stopped` let the
//! runtime's restart manager act on a container nerdctl compose had created but
//! not yet started, and Compose then failed the start with `cannot start a
//! container that has stopped` (effigy#383). Effigy asks the runtime for the
//! policy with its own `update` once `up` has started the stack: the rendered
//! file and its Compose config-hash name no restart policy at all, so a re-up
//! never recreates a service for one, and the runtime still brings a service
//! back after a reboot, a Colima restart or a crash. A service that declares a
//! policy keeps it (effigy#371).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::exec::{ContainerExecError, RunningComposeContainer};
use crate::EffectiveContainerPolicy;

/// The restart policy a service gets when its stack declares none.
pub const RESTART_POLICY: &str = "unless-stopped";

/// The owned-binding budget for each listed container when the caller shares
/// no deadline. It matches the bring-up's own owned-binding window.
const OWNED_UPDATE_VERIFICATION_BUDGET: Duration = Duration::from_secs(15);

/// One stage of the post-start restart-policy apply, in the order it runs.
///
/// The apply reads the Compose declarations, reads the runtime's container
/// inventory, and only then may ask the runtime to update anything. A caller
/// that shows progress names the stage now in flight, so a stalled read never
/// claims an update was already sent (effigy#460, Q-017).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartPolicyStage {
    /// Reading the Compose files for the services that declare no policy.
    ComposeDeclaration,
    /// Reading the running containers of those services from the runtime.
    RuntimeInventory,
    /// Asking the runtime to keep those containers running.
    Apply,
}

impl RestartPolicyStage {
    /// The stage in the operator's words, for the stack or service `subject`.
    pub fn description(self, subject: &str) -> String {
        match self {
            Self::ComposeDeclaration => format!(
                "reading the Compose declaration of {subject} for services without a restart policy"
            ),
            Self::RuntimeInventory => {
                format!("reading the running containers of {subject} from the runtime")
            }
            Self::Apply => format!(
                "asking the runtime to keep the containers of {subject} on `restart: {RESTART_POLICY}`"
            ),
        }
    }
}

/// Give every container `up` started the restart policy its stack did not
/// declare. A file or container list that cannot be read, and an update the
/// runtime refuses, are each named and the activation proceeds: the stack is
/// running either way, and the next `up` applies the policy again.
///
/// Each container is update-eligible only after its full ID, project, declared
/// service, canonical checkout and scope verify against the current policy, and
/// the update targets that verified ID. A foreign, replaced or unknown container
/// — a same-name impostor or a sibling of another owner — receives no update and
/// is named as refused.
///
/// `deadline` bounds the inventory read that names those containers. A
/// bring-up passes the one deadline it shares with its readiness wait, so a
/// runtime that stops answering spends that budget once (effigy#411).
pub fn apply_after_start(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    project_name: &str,
    compose_files: &[PathBuf],
    deadline: Option<Instant>,
) {
    apply_after_start_observed(
        repo_root,
        policy,
        project_name,
        compose_files,
        deadline,
        &mut |_| {},
    );
}

/// The same apply, reporting each stage as it starts.
pub fn apply_after_start_observed(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    project_name: &str,
    compose_files: &[PathBuf],
    deadline: Option<Instant>,
    stage: &mut dyn FnMut(RestartPolicyStage),
) {
    stage(RestartPolicyStage::ComposeDeclaration);
    let services = match undeclared_services(compose_files) {
        Ok(services) => services,
        Err(error) => {
            eprintln!("[warn] cannot read the restart policies of `{project_name}`: {error}");
            return;
        }
    };
    if services.is_empty() {
        return;
    }
    stage(RestartPolicyStage::RuntimeInventory);
    let rows = match crate::exec::list_running_compose_containers_for_project_with_deadline(
        repo_root,
        policy,
        project_name,
        deadline,
    ) {
        Ok(rows) => rows
            .into_iter()
            .filter(|row| {
                row.service
                    .as_deref()
                    .is_some_and(|service| services.contains(service))
            })
            .collect::<Vec<_>>(),
        Err(error) => {
            eprintln!("[warn] cannot list the running `{project_name}` containers: {error}");
            return;
        }
    };
    if rows.is_empty() {
        return;
    }
    stage(RestartPolicyStage::Apply);
    let deadline = deadline.unwrap_or_else(|| Instant::now() + OWNED_UPDATE_VERIFICATION_BUDGET);
    for row in &rows {
        if let Err(error) = update_verified_container(repo_root, policy, row, deadline) {
            eprintln!(
                "[warn] cannot ask the runtime to keep `{}` running with `restart: {RESTART_POLICY}`: {error}",
                row.container_name
            );
        }
    }
}

/// Send the restart-policy update to one listed container, and only after its
/// owned binding is proven against the current policy. The update names the
/// verified full ID, never the listed name, and a second strict inspection runs
/// right before it so a replacement or ownership change in between sends
/// nothing. A row that fails either check is refused with its reason and
/// receives no update; the caller's own owned-binding check still decides
/// whether the selected service is usable.
fn update_verified_container(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    row: &RunningComposeContainer,
    deadline: Instant,
) -> Result<(), ContainerExecError> {
    let binding = crate::exec::verify_owned_running_compose_identities(
        repo_root,
        policy,
        std::slice::from_ref(row),
        deadline,
    )?
    .pop()
    .ok_or_else(|| ContainerExecError::Failure {
        command: "inspect owned binding".to_owned(),
        code: None,
        stdout: String::new(),
        stderr: "binding unavailable: the listed container is not an owned service container"
            .to_owned(),
    })?;
    binding.reverify(repo_root, policy, deadline)?;
    crate::exec::set_restart_policy(
        repo_root,
        policy,
        &[binding.id().to_owned()],
        RESTART_POLICY,
    )
}

/// The services these Compose files define without declaring `restart:`,
/// merged the way Compose merges them: a policy any file declares wins.
fn undeclared_services(compose_files: &[PathBuf]) -> Result<BTreeSet<String>, String> {
    let mut defined = BTreeSet::new();
    let mut declared = BTreeSet::new();
    for file in compose_files {
        let body = std::fs::read_to_string(file)
            .map_err(|error| format!("{}: {error}", file.display()))?;
        let document: serde_yaml::Value =
            serde_yaml::from_str(&body).map_err(|error| format!("{}: {error}", file.display()))?;
        let Some(services) = document
            .get("services")
            .and_then(|value| value.as_mapping())
        else {
            continue;
        };
        for (name, service) in services {
            let Some(name) = name.as_str() else {
                continue;
            };
            match service.get("restart") {
                Some(_) => declared.insert(name.to_owned()),
                None => defined.insert(name.to_owned()),
            };
        }
    }
    defined.retain(|name| !declared.contains(name));
    Ok(defined)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(body: &[&str]) -> (tempfile::TempDir, Vec<PathBuf>) {
        let directory = tempfile::tempdir().expect("tempdir");
        let files = body
            .iter()
            .enumerate()
            .map(|(index, body)| {
                let path = directory.path().join(format!("{index}.compose.yml"));
                std::fs::write(&path, body).expect("write compose input");
                path
            })
            .collect();
        (directory, files)
    }

    #[test]
    fn every_service_that_declares_no_policy_is_named() {
        let (_directory, files) = files(&[
            "services:\n  app:\n    image: one\n  db:\n    image: two\n    restart: \"no\"\n",
        ]);
        let services = undeclared_services(&files).expect("read");
        assert_eq!(services.into_iter().collect::<Vec<_>>(), vec!["app"]);
    }

    #[test]
    fn a_policy_an_overlay_declares_wins() {
        let (_directory, files) = files(&[
            "services:\n  app:\n    image: one\n  db:\n    image: two\n",
            "services:\n  app:\n    restart: always\n",
        ]);
        let services = undeclared_services(&files).expect("read");
        assert_eq!(services.into_iter().collect::<Vec<_>>(), vec!["db"]);
    }

    #[test]
    fn an_unreadable_file_is_an_error_named_with_its_path() {
        let (directory, files) = files(&["services:\n  app:\n    image: one\n"]);
        let missing = directory.path().join("gone.compose.yml");
        let error = undeclared_services(&[files[0].clone(), missing]).expect_err("unreadable");
        assert!(error.contains("gone.compose.yml"), "{error}");
    }
}
