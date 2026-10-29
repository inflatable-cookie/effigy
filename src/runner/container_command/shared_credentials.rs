//! Private credential transport for one shared data exec (effigy#477).
//!
//! A declared data target's client credential travels by variable name in the
//! Compose invocation. The value lives in a private, content-addressed file
//! under the Effigy home that the Compose client sources before it runs: the
//! host environment for a host backend, and inside the Colima VM through
//! `colima ssh` for the VM-hosted backend (the same transport managed task
//! secrets use). No value reaches a command line or a report. Like the render's
//! own private Compose inputs, the file is 0600 inside a 0700 directory, named
//! by the render digest, and not pruned.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Output;

use effigy_containers::{
    BackendId, ContainerComposeInvocationPlan, EffectiveContainerPolicy, SharedServiceBinding,
};
use effigy_core::shell::shell_quote;

use super::RunnerError;

/// One private file holding `export NAME=value` lines for a shared exec.
struct SharedCredentialFile {
    path: PathBuf,
    names: Vec<String>,
}

impl SharedCredentialFile {
    fn write(
        service: &SharedServiceBinding,
        env: &BTreeMap<String, String>,
    ) -> Result<Self, RunnerError> {
        for name in env.keys() {
            if !valid_env_name(name) {
                return Err(RunnerError::task_invocation(format!(
                    "shared {} data command declares `{name}`, which is not a portable environment name",
                    service.service_name
                )));
            }
        }
        let home = std::env::var_os("HOME").map(PathBuf::from).ok_or_else(|| {
            RunnerError::task_invocation(
                "`HOME` is not set; cannot place the private shared data credential file",
            )
        })?;
        let root = home.join(".effigy").join("data-credentials");
        private_directory(&root)?;
        // Content-addressed by the render digest alone, so no manifest name can
        // steer this path outside the private directory.
        let directory = root.join(&service.render_digest);
        private_directory(&directory)?;
        let path = directory.join("env.delivery");
        let mut body = String::from("set +xv\n");
        for (name, value) in env {
            body.push_str(&format!("export {name}={}\n", shell_quote(value)));
        }
        write_private_file(&path, body.as_bytes())?;
        Ok(Self {
            path,
            names: env.keys().cloned().collect(),
        })
    }
}

fn valid_env_name(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|first| first == '_' || first.is_ascii_alphabetic())
        && characters.all(|rest| rest == '_' || rest.is_ascii_alphanumeric())
}

/// A private directory this user owns, or a refusal. An existing path is
/// checked rather than relaxed.
fn private_directory(path: &Path) -> Result<(), RunnerError> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
    match std::fs::DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let metadata = std::fs::symlink_metadata(path).map_err(|error| {
                RunnerError::task_invocation(format!(
                    "cannot inspect the private shared data credential directory {}: {error}",
                    path.display()
                ))
            })?;
            let owner = unsafe { libc::geteuid() };
            if metadata.is_dir()
                && metadata.uid() == owner
                && metadata.permissions().mode() & 0o777 == 0o700
            {
                Ok(())
            } else {
                Err(RunnerError::task_invocation(format!(
                    "the private shared data credential directory {} is not a 0700 directory this user owns; refusing to place a credential there",
                    path.display()
                )))
            }
        }
        Err(error) => Err(RunnerError::task_invocation(format!(
            "cannot create the private shared data credential directory {}: {error}",
            path.display()
        ))),
    }
}

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), RunnerError> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "cannot write the private shared data credential file {}: {error}",
                path.display()
            ))
        })?;
    file.write_all(bytes).map_err(|error| {
        RunnerError::task_invocation(format!(
            "cannot write the private shared data credential file {}: {error}",
            path.display()
        ))
    })?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(|error| {
        RunnerError::task_invocation(format!(
            "cannot set the private shared data credential file {} to 0600: {error}",
            path.display()
        ))
    })
}

/// Source the private credential file and exec the Compose command with the
/// values in the client's own environment. A VM-hosted backend runs the same
/// bootstrap inside the VM, which reads the file through the shared home.
pub(super) fn run_shared_credential_exec(
    cwd: &Path,
    policy: &EffectiveContainerPolicy,
    plan: &ContainerComposeInvocationPlan,
    service: &SharedServiceBinding,
    env: &BTreeMap<String, String>,
    stdin_file: Option<&Path>,
) -> Result<Output, RunnerError> {
    let credential = SharedCredentialFile::write(service, env)?;
    // Mirrors the managed-secret bootstrap: source the file, remove nothing,
    // and fail closed when it cannot be read.
    let bootstrap = ". \"$1\"; code=$?; [ \"$code\" = 0 ] || { echo 'effigy: cannot read the private shared data credential source' >&2; exit 125; }; ";
    let label = plan.label.clone();
    let (program, args) = if plan.backend_id == BackendId::colima_nerdctl() {
        // `colima nerdctl --profile P -- <args>`: the Compose client runs in
        // the VM, so the value travels in the VM-readable home and is preserved
        // into nerdctl by the same transport managed task secrets use.
        let (Some(prefix), Some(inside)) = (plan.args.get(..4), plan.args.get(4..)) else {
            return Err(RunnerError::task_invocation(
                "shared data exec has no nerdctl arguments to run in the VM",
            ));
        };
        if prefix[0] != "nerdctl" || prefix[1] != "--profile" || prefix[3] != "--" {
            return Err(RunnerError::task_invocation(
                "shared data exec did not resolve a nerdctl VM invocation",
            ));
        }
        let command = format!(
            "exec sudo --preserve-env={names} nerdctl {args}",
            names = credential.names.join(","),
            args = inside
                .iter()
                .map(|arg| shell_quote(&arg.to_string_lossy()))
                .collect::<Vec<_>>()
                .join(" ")
        );
        (
            OsString::from("colima"),
            vec![
                OsString::from("ssh"),
                OsString::from("--profile"),
                OsString::from(policy.profile.as_str()),
                OsString::from("--"),
                OsString::from("sh"),
                OsString::from("-c"),
                OsString::from(format!("{bootstrap}{command}")),
                OsString::from("sh"),
                credential.path.as_os_str().to_os_string(),
            ],
        )
    } else {
        let command = format!(
            "exec {} {}",
            shell_quote(&plan.program.to_string_lossy()),
            plan.args
                .iter()
                .map(|arg| shell_quote(&arg.to_string_lossy()))
                .collect::<Vec<_>>()
                .join(" ")
        );
        (
            OsString::from("sh"),
            vec![
                OsString::from("-c"),
                OsString::from(format!("{bootstrap}{command}")),
                OsString::from("sh"),
                credential.path.as_os_str().to_os_string(),
            ],
        )
    };
    crate::runner::exec_command::run_command_capture_until(cwd, &program, &args, stdin_file, None)
        .map_err(|error| match error {
            RunnerError::TaskInvocation(message) => RunnerError::task_invocation(format!(
                "{label} could not run with its private credential: {message}"
            )),
            other => other,
        })
}
