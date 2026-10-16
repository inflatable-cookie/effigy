//! Transfer only the selected, verified service pack before guest policy loads.
//! Project overrides remain mounted project data; user overrides are not copied.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use effigy_catalog::pack::{
    content::copy_tree, select_pack, verify_installed_pack, InstalledPackRecord, PackSelection,
    PackStore,
};
use effigy_containers::EffectiveContainerPolicy;
use effigy_core::shell::shell_quote;
use serde_json::Value;

use crate::runner::container_command::{bounded_failure_error, bounded_phase_error};
use crate::runner::exec_command::{
    copy_file_into_service, copy_file_into_service_bounded, BoundedCapture, BoundedCommandBudget,
    BoundedCommandPhase, BoundedScopedCaptureFailure,
};
use crate::runner::service_command::effigy_version;

use super::RunnerError;

/// Selection is pinned before binary preparation, including its early returns.
pub(super) struct HostCatalog {
    selection: PackSelection,
}

impl HostCatalog {
    pub(super) fn pin() -> Result<Self, &'static str> {
        let selection = select_pack(effigy_version());
        if selection.reason.is_fallback() {
            return Err("host selected pack is unhealthy or incompatible; inspect `effigy service pack status` and repair the host selection before retrying");
        }
        // Selection's baseline fallback treats a non-file state path as absent.
        // That is not sufficient evidence for cross-user preparation.
        if let Some(store) = PackStore::user() {
            match std::fs::symlink_metadata(store.state_path()) {
                Ok(metadata) if !metadata.is_file() || metadata.file_type().is_symlink() => {
                    return Err("host catalog state is unsupported; inspect `effigy service pack status` before retrying");
                }
                Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                    return Err("host catalog state is unavailable; inspect `effigy service pack status` before retrying");
                }
                _ => {}
            }
        }
        Ok(Self { selection })
    }

    fn unchanged(&self) -> Result<(), &'static str> {
        if Self::pin()?.selection != self.selection {
            return Err("host catalog selection or retained content changed during workspace preparation; inspect `effigy service pack status` and retry");
        }
        Ok(())
    }

    fn retained_root(&self, record: &InstalledPackRecord) -> Result<PathBuf, &'static str> {
        let store_root = self
            .selection
            .store_root
            .as_ref()
            .ok_or("host pack store is unavailable")?;
        // A state file is not authority to copy arbitrary host paths.
        let mut components = Path::new(&record.install_id).components();
        if !matches!(
            (components.next(), components.next()),
            (Some(std::path::Component::Normal(_)), None)
        ) {
            return Err("host pack install identity is not confined to the retained store");
        }
        Ok(store_root.join("installs").join(&record.install_id))
    }

    pub(super) fn prepare(
        &self,
        repo_root: &Path,
        policy: &EffectiveContainerPolicy,
    ) -> Result<(), RunnerError> {
        self.prepare_with(CatalogTransport {
            repo_root,
            policy,
            budget: None,
            failure: None,
            verified_target: None,
            target: None,
        })
    }

    /// Bounded preparation for the verified workspace container `target_id`.
    pub(super) fn prepare_bounded(
        &self,
        repo_root: &Path,
        policy: &EffectiveContainerPolicy,
        target_id: &str,
        budget: &mut BoundedCommandBudget,
    ) -> Result<(), RunnerError> {
        self.prepare_with(CatalogTransport {
            repo_root,
            policy,
            budget: Some(budget),
            failure: None,
            verified_target: Some(target_id),
            target: None,
        })
    }

    fn prepare_with(&self, mut transport: CatalogTransport<'_>) -> Result<(), RunnerError> {
        self.prepare_using(&mut transport).map_err(|reason| {
            let identity = self
                .selection
                .active
                .as_ref()
                .map(|record| format!(" Required host pack content: {}.", record.content_id))
                .unwrap_or_default();
            if let Some(mut error) = transport.failure.take() {
                if let RunnerError::BoundedExecFailure { message, .. } = &mut error {
                    message.push_str(&identity);
                    message.push_str(" Inspect `effigy service pack status` inside the workspace before retrying.");
                }
                return error;
            }
            transport.refusal(&format!("{reason}{identity}"))
        })
    }

    fn prepare_using(&self, transport: &mut CatalogTransport<'_>) -> Result<(), &'static str> {
        self.unchanged()?;
        let Some(record) = self.selection.active.as_ref() else {
            return Ok(());
        };
        let guard = identity_guard(transport.policy)?;
        let selected = transport.selected_target()?;
        if transport
            .verified_target
            .is_some_and(|verified| verified != selected)
        {
            return Err("selected workspace changed during catalog preparation; no task was started; retry against the activated workspace");
        }
        transport.target = Some(selected);
        let status = transport.run(&format!(
            "{guard}; exec /usr/local/bin/effigy service pack status --json"
        ))?;
        match guest_selection(&status, transport.policy, record)? {
            GuestSelection::Matching => return self.unchanged(),
            GuestSelection::Empty => {}
        }
        // Snapshot the retained install, never its original source checkout.
        // The owned tempdir stays alive until every copy/install/probe completes.
        // Stage inside the checkout: VM-backed runtimes such as Colima mount the
        // checkout but not the host's system temp directory.
        let staging_parent = transport.repo_root.join(".effigy/tmp");
        std::fs::create_dir_all(&staging_parent)
            .map_err(|_| "could not create private host catalog staging")?;
        let snapshot = tempfile::Builder::new()
            .prefix("effigy-workspace-catalog-")
            .tempdir_in(&staging_parent)
            .map_err(|_| "could not create private host catalog staging")?;
        let payload = snapshot.path().join("pack");
        copy_tree(&self.retained_root(record)?, &payload)
            .map_err(|_| "could not snapshot the retained host catalog pack")?;
        verify_installed_pack(&payload, record, effigy_version())
            .map_err(|_| "host catalog content changed while snapshotting; inspect `effigy service pack status` and retry")?;
        self.unchanged()?;
        // No guessed shared path: mkdir must prove this invocation created it.
        // On any uncertain remote effect the new directory is retained; no
        // cleanup command is dispatched after a refusal or expired budget.
        let staging = format!(
            "/tmp/effigy-catalog-{}",
            snapshot
                .path()
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or("catalog staging name is unavailable")?
        );
        transport.run(&format!(
            "{guard}; umask 077; mkdir -m 700 {}",
            shell_quote(&staging)
        ))?;
        transport.copy(&payload, &format!("{staging}/pack"))?;
        // Runtime `cp` creates the copied tree as root; hand it to the workspace
        // user so its install and the cleanup below run without root.
        let workspace_user = transport
            .policy
            .workspace_user
            .clone()
            .ok_or("workspace catalog user is unavailable")?;
        transport.run_as_root(&["chown", "-R", &workspace_user, &format!("{staging}/pack")])?;
        verify_installed_pack(&payload, record, effigy_version()).map_err(|_| {
            "host catalog snapshot changed during transport; retry workspace preparation"
        })?;
        self.unchanged()?;
        let install = format!(
            "{guard}; exec /usr/local/bin/effigy service pack install --path {} --prepare-content {} --json",
            shell_quote(&format!("{staging}/pack")), shell_quote(&record.content_id),
        );
        let installed = transport.run(&install)?;
        if !matches!(
            guest_selection(&installed, transport.policy, record)?,
            GuestSelection::Matching
        ) {
            return Err("guest catalog installation did not select the pinned content; inspect `effigy service pack status` inside the workspace");
        }
        self.unchanged()?;
        // Installation is verified before cleaning only our new staging.
        // The final selection probe below also detects changes during cleanup.
        transport.run(&format!("{guard}; rm -rf -- {}", shell_quote(&staging)))?;
        // Re-read through real guest selection; exit zero alone is not proof.
        let status = transport.run(&format!(
            "{guard}; exec /usr/local/bin/effigy service pack status --json"
        ))?;
        if !matches!(
            guest_selection(&status, transport.policy, record)?,
            GuestSelection::Matching
        ) {
            return Err("guest catalog verification did not select the pinned content; inspect `effigy service pack status` inside the workspace");
        }
        self.unchanged()
    }
}

fn identity_guard(policy: &EffectiveContainerPolicy) -> Result<String, &'static str> {
    let user = policy.workspace_user.as_deref().ok_or("selected host catalog requires a declared workspace_user; configure the workspace identity before retrying")?;
    let home = policy.workspace_home.as_deref().filter(|home| {
        home.starts_with('/') && *home != "/" && !Path::new(home).components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    }).ok_or("selected host catalog requires an absolute workspace_home; configure the workspace identity before retrying")?;
    let user = user
        .split(':')
        .next()
        .filter(|user| !user.is_empty())
        .ok_or("workspace catalog user is unavailable")?;
    let uid = if user.bytes().all(|byte| byte.is_ascii_digit()) {
        shell_quote(user)
    } else {
        format!("\"$(id -u {})\"", shell_quote(user))
    };
    Ok(format!(
        "test \"$HOME\" = {} && test -d \"$HOME\" && test ! -L \"$HOME\" && test \"$(id -u)\" = {uid} || exit 73",
        shell_quote(home)
    ))
}

enum GuestSelection {
    Empty,
    Matching,
}

fn guest_selection(
    raw: &[u8],
    policy: &EffectiveContainerPolicy,
    expected: &InstalledPackRecord,
) -> Result<GuestSelection, &'static str> {
    let envelope: Value = serde_json::from_slice(raw).map_err(|_| "guest catalog response is malformed; inspect `effigy service pack status` inside the workspace")?;
    if envelope["schema"] != "effigy.command.v1"
        || envelope["schema_version"] != 1
        || envelope["ok"] != true
        || envelope["command"]["kind"] != "service"
        || envelope["command"]["name"] != "service"
        || !envelope.get("error").is_some_and(Value::is_null)
    {
        return Err("guest catalog command response is unverified; inspect `effigy service pack status` inside the workspace");
    }
    let value = envelope
        .get("result")
        .ok_or("guest catalog response has no result")?;
    let home = policy
        .workspace_home
        .as_deref()
        .ok_or("workspace catalog home is unavailable")?;
    let expected_store = Path::new(home).join(".effigy/catalog-packs/v1");
    if value["schema"] != "effigy.service.pack.status.v1"
        || value["schema_version"] != 1
        || value["ok"] != true
        || value["store_root"].as_str() != expected_store.to_str()
        || value["effigy_version"].as_str() != Some(effigy_version())
        || value["selection"]["fallback"] != false
    {
        return Err("guest catalog identity, home, version or health is unverified; inspect `effigy service pack status` as the configured workspace user");
    }
    if value["selection"]["reason"] == "no-store"
        && value["selection"]["layer"] == "compiled-baseline"
        && value["selection"]["pack_id"].is_null()
        && value["selection"]["pack_version"].is_null()
        && value["selection"]["install_id"].is_null()
        && value.get("active").is_some_and(Value::is_null)
        && value.get("previous").is_some_and(Value::is_null)
        && value["installs"].as_array().is_some_and(Vec::is_empty)
    {
        return Ok(GuestSelection::Empty);
    }
    let active = &value["active"];
    if value["selection"]["reason"] == "active-pack"
        && value["selection"]["layer"] == "installed-pack"
        && active["install_id"] == expected.install_id
        && active["pack_id"] == expected.pack_id
        && active["pack_version"] == expected.pack_version
        && active["content_id"] == expected.content_id
        && active["manifest_schema_version"] == expected.manifest_schema_version
        && active["requires_effigy"] == expected.requires_effigy
        && active["compatible"] == true
        && value["selection"]["pack_id"] == expected.pack_id
        && value["selection"]["pack_version"] == expected.pack_version
        && value["selection"]["install_id"] == active["install_id"]
    {
        return Ok(GuestSelection::Matching);
    }
    Err("existing guest catalog selection differs or is unknown and was preserved; inspect `effigy service pack status` inside the workspace, then explicitly use `effigy service pack install --path <retained-pack>` for the intended host content; a matching binary does not supply optional fragments")
}

struct CatalogTransport<'a> {
    repo_root: &'a Path,
    policy: &'a EffectiveContainerPolicy,
    budget: Option<&'a mut BoundedCommandBudget>,
    failure: Option<RunnerError>,
    /// Container the bounded caller already proved it owns.
    verified_target: Option<&'a str>,
    target: Option<String>,
}

impl CatalogTransport<'_> {
    fn refusal(&self, reason: &str) -> RunnerError {
        catalog_refusal(self.policy, self.budget.as_deref(), reason)
    }

    fn selected_target(&mut self) -> Result<String, &'static str> {
        // nerdctl's `compose ps -q <service>` ignores the service and lists the
        // whole project, so select the primary by its Compose labels instead.
        let args = [
            "ps".to_owned(),
            "-q".to_owned(),
            "--no-trunc".to_owned(),
            "--filter".to_owned(),
            format!(
                "label=com.docker.compose.project={}",
                self.policy.project_name
            ),
            "--filter".to_owned(),
            format!(
                "label=com.docker.compose.service={}",
                self.policy.primary_service
            ),
        ]
        .map(OsString::from);
        let raw = self.capture_runtime(&args)?;
        let text = std::str::from_utf8(&raw)
            .map_err(|_| "workspace catalog target identity is malformed")?;
        let mut lines = text.lines().filter(|line| !line.trim().is_empty());
        let target = lines
            .next()
            .ok_or("workspace catalog target is unavailable; retry after workspace activation")?
            .trim();
        if lines.next().is_some()
            || !(12..=64).contains(&target.len())
            || !target.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(
                "workspace catalog target identity is unknown; retry after workspace activation",
            );
        }
        Ok(target.to_owned())
    }

    fn check_target(&mut self) -> Result<(), &'static str> {
        let current = self.selected_target()?;
        if self.target.as_deref() != Some(current.as_str()) {
            return Err("selected workspace changed during catalog preparation; no task was started; retry against the activated workspace");
        }
        Ok(())
    }

    fn run(&mut self, command: &str) -> Result<Vec<u8>, &'static str> {
        let mut args = vec![OsString::from("exec"), OsString::from("-T")];
        args.extend([
            "-u".into(),
            self.policy
                .workspace_user
                .as_deref()
                .ok_or("workspace catalog user is unavailable")?
                .into(),
            "-e".into(),
            format!(
                "HOME={}",
                self.policy
                    .workspace_home
                    .as_deref()
                    .ok_or("workspace catalog home is unavailable")?
            )
            .into(),
            self.policy.primary_service.as_str().into(),
            "sh".into(),
            "-c".into(),
            OsString::from(command),
        ]);
        self.check_target()?;
        let output = self.capture_exec(args)?;
        self.check_target()?;
        Ok(output)
    }

    /// Run one fixed argv as root in the selected workspace, without a shell.
    fn run_as_root(&mut self, argv: &[&str]) -> Result<Vec<u8>, &'static str> {
        let mut args = [
            "exec",
            "-T",
            "-u",
            "0",
            self.policy.primary_service.as_str(),
        ]
        .map(OsString::from)
        .to_vec();
        args.extend(argv.iter().map(OsString::from));
        self.check_target()?;
        let output = self.capture_exec(args)?;
        self.check_target()?;
        Ok(output)
    }

    /// Pass the exec tail through the existing manager/transport normalizer;
    /// it alone supplies the Compose file/project prefix in either mode.
    /// Keep the bounded budget when one governs this preparation.
    fn capture_exec(&mut self, args: Vec<OsString>) -> Result<Vec<u8>, &'static str> {
        if let Some(budget) = self.budget.as_deref_mut() {
            let target = self
                .target
                .clone()
                .ok_or("workspace catalog target is unavailable")?;
            let plan = effigy_runtime::container_manager::compose_invocation_plan_from_args(
                self.repo_root,
                self.policy,
                args,
                effigy_containers::ContainerAction::Exec,
                "workspace catalog preparation",
            )
            .map_err(|_| "could not resolve workspace catalog preparation")?;
            // Bounded preparation targets the container whose ownership the
            // caller already verified; the Compose service alias can name a
            // replacement that this preparation must not touch.
            let exec_args = crate::runner::exec_command::verified_target_exec_args(&plan, &target)
                .map_err(|_| "could not resolve workspace catalog preparation")?;
            let plan = effigy_runtime::container_manager::runtime_invocation_plan(
                self.repo_root,
                self.policy,
                "docker",
                &exec_args,
                effigy_containers::ContainerAction::Exec,
                "workspace catalog preparation",
            )
            .map_err(|_| "could not resolve workspace catalog preparation")?;
            let result = budget.run_command(
                &plan.repo_root,
                plan.program.as_os_str(),
                &plan.args,
                None,
                &[],
                BoundedCommandPhase::WorkspacePreparation,
            );
            return self.finish_bounded(result);
        }
        let output = crate::runner::exec_command::run_compose_exec(
            self.repo_root,
            self.policy,
            &args,
            true,
            "workspace catalog preparation",
        )
        .map_err(|_| {
            "workspace catalog transport failed; inspect the configured workspace user and service"
        })?;
        if !output.status.success() {
            return Err("guest catalog preparation failed; selection was not certified; inspect `effigy service pack status` as the configured workspace user");
        }
        Ok(output.stdout)
    }

    /// Capture a runtime-level (not Compose) command, inside the bounded budget
    /// when one governs this preparation.
    fn capture_runtime(&mut self, args: &[OsString]) -> Result<Vec<u8>, &'static str> {
        let plan = effigy_runtime::container_manager::runtime_invocation_plan(
            self.repo_root,
            self.policy,
            "docker",
            args,
            effigy_containers::ContainerAction::Status,
            "workspace catalog target",
        )
        .map_err(|_| "could not resolve workspace catalog target")?;
        if let Some(budget) = self.budget.as_deref_mut() {
            let result = budget.run_command(
                &plan.repo_root,
                plan.program.as_os_str(),
                &plan.args,
                None,
                &[],
                BoundedCommandPhase::WorkspacePreparation,
            );
            return self.finish_bounded(result);
        }
        let output = crate::runner::exec_command::run_command_capture_until(
            self.repo_root,
            plan.program.as_os_str(),
            &plan.args,
            None,
            None,
        )
        .map_err(|_| "workspace catalog target lookup failed")?;
        if !output.status.success() {
            return Err("workspace catalog target lookup failed");
        }
        Ok(output.stdout)
    }

    fn finish_bounded(
        &mut self,
        result: Result<BoundedCapture, BoundedScopedCaptureFailure>,
    ) -> Result<Vec<u8>, &'static str> {
        let budget = self
            .budget
            .as_deref()
            .ok_or("workspace catalog transport lost its bounded budget")?;
        let output = match result {
            Ok(output) => output,
            Err(failure) => {
                // Preserve the original failure accounting at the caller.
                self.failure = Some(bounded_failure_error(
                    self.policy,
                    &self.policy.primary_service,
                    budget,
                    failure,
                ));
                return Err("workspace catalog transport exceeded its original bounds");
            }
        };
        if !output.status.success() {
            self.failure = Some(bounded_phase_error(
                self.policy, &self.policy.primary_service, budget,
                BoundedCommandPhase::WorkspacePreparation, "workspace_catalog_mismatch",
                "guest catalog preparation failed; selection was not certified; inspect `effigy service pack status` as the configured workspace user",
                output.status.code(),
            ));
            return Err("guest catalog preparation failed");
        }
        Ok(output.stdout)
    }

    fn copy(&mut self, source: &Path, destination: &str) -> Result<(), &'static str> {
        self.check_target()?;
        if let Some(budget) = self.budget.as_deref_mut() {
            let target = self
                .target
                .as_deref()
                .ok_or("workspace catalog target is unavailable")?;
            copy_file_into_service_bounded(
                self.repo_root,
                self.policy,
                &self.policy.primary_service,
                target,
                source,
                destination,
                budget,
            )
            .map_err(|error| {
                self.failure = Some(error);
                "workspace catalog copy failed or exceeded its original bounds"
            })
        } else {
            copy_file_into_service(
                self.repo_root,
                self.policy,
                &self.policy.primary_service,
                source,
                destination,
            )
            .map_err(|error| {
                self.failure = Some(error);
                "workspace catalog copy failed; new staging is retained and no task was started"
            })
        }?;
        self.check_target()
    }
}

pub(super) fn catalog_refusal(
    policy: &EffectiveContainerPolicy,
    budget: Option<&BoundedCommandBudget>,
    reason: &str,
) -> RunnerError {
    if let Some(budget) = budget {
        let mut error = bounded_phase_error(
            policy,
            &policy.primary_service,
            budget,
            BoundedCommandPhase::WorkspacePreparation,
            "workspace_catalog_mismatch",
            "workspace service catalog preparation refused",
            None,
        );
        if let RunnerError::BoundedExecFailure { message, .. } = &mut error {
            *message = format!("workspace service catalog preparation refused: {reason}");
        }
        error
    } else {
        RunnerError::task_invocation(format!(
            "workspace service catalog preparation refused: {reason}"
        ))
    }
}

#[cfg(test)]
mod tests;
