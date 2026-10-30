//! Private credential transport for one shared data exec (effigy#477).
//!
//! A declared data target's client credential travels by variable name in the
//! Compose invocation. The value lives in a private file under the Effigy home
//! that the Compose client sources before it runs: the host environment for a
//! host backend, and inside the Colima VM through `colima ssh` for the VM-hosted
//! backend (the same transport managed task secrets use). No value reaches a
//! command line or a report.
//!
//! Each exec creates its own file (`create_new`, 0600 inside a 0700 directory,
//! never rewritten) and removes it when the call returns. Two concurrent data
//! commands therefore never share a credential path, and a partially written
//! file can never be sourced.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use effigy_containers::{
    BackendId, ContainerComposeInvocationPlan, EffectiveContainerPolicy, SharedServiceBinding,
};
use effigy_core::shell::shell_quote;

use super::RunnerError;

/// One private file holding `export NAME=value` lines for one shared exec.
#[derive(Debug)]
struct SharedCredentialFile {
    path: PathBuf,
    names: Vec<String>,
}

impl SharedCredentialFile {
    fn write(
        service: &SharedServiceBinding,
        env: &BTreeMap<String, String>,
    ) -> Result<Self, RunnerError> {
        Self::write_with(service, env, |file, body| file.write_all(body))
    }

    /// The write step is injected so a failed or partial write can be proven to
    /// remove the exact file it created (effigy#477 review).
    fn write_with(
        service: &SharedServiceBinding,
        env: &BTreeMap<String, String>,
        write_body: impl FnOnce(&mut std::fs::File, &[u8]) -> std::io::Result<()>,
    ) -> Result<Self, RunnerError> {
        static OPERATION: AtomicU64 = AtomicU64::new(0);
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
        let root = private_credential_root(&home)?;
        let mut body = String::from("set +xv\n");
        for (name, value) in env {
            body.push_str(&format!("export {name}={}\n", shell_quote(value)));
        }
        for _ in 0..64 {
            let path = root.join(format!(
                "exec-{}-{}.env",
                std::process::id(),
                OPERATION.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW)
                .open(&path)
            {
                Ok(mut file) => {
                    if let Err(error) = write_body(&mut file, body.as_bytes()) {
                        // The file is this operation's own and holds no
                        // complete credential; remove it rather than leave a
                        // partial private file behind.
                        let _ = std::fs::remove_file(&path);
                        return Err(RunnerError::task_invocation(format!(
                            "cannot write the private shared data credential file {}: {error}",
                            path.display()
                        )));
                    }
                    return Ok(Self {
                        path,
                        names: env.keys().cloned().collect(),
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(RunnerError::task_invocation(format!(
                        "cannot create the private shared data credential file {}: {error}",
                        path.display()
                    )))
                }
            }
        }
        Err(RunnerError::task_invocation(
            "cannot create a unique private shared data credential file",
        ))
    }

    fn remove(&self) -> std::io::Result<()> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }
}

fn valid_env_name(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|first| first == '_' || first.is_ascii_alphabetic())
        && characters.all(|rest| rest == '_' || rest.is_ascii_alphanumeric())
}

/// The private directory this user's shared data credentials live in, created
/// on first use. `$HOME/.effigy` may be created by a concurrent command, so a
/// lost creation race is verified rather than reported as a failure.
fn private_credential_root(home: &Path) -> Result<PathBuf, RunnerError> {
    let parent = home.join(".effigy");
    ensure_private_directory(&parent, false)?;
    let root = parent.join("data-credentials");
    ensure_private_directory(&root, true)?;
    Ok(root)
}

/// Create `path` as a private directory this user owns, or verify an existing
/// one. `exact` requires 0700; otherwise group and other write access is the
/// only refusal, as `$HOME/.effigy` is shared with Effigy's other state.
fn ensure_private_directory(path: &Path, exact: bool) -> Result<(), RunnerError> {
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
            let mode = metadata.permissions().mode() & 0o777;
            let private = if exact {
                mode == 0o700
            } else {
                mode & 0o022 == 0
            };
            if metadata.is_dir() && metadata.uid() == owner && private {
                Ok(())
            } else {
                Err(RunnerError::task_invocation(format!(
                    "the private shared data credential directory {} is not a private directory this user owns; refusing to place a credential there",
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

/// Source the private credential file and exec the Compose command with the
/// values in the client's own environment. A VM-hosted backend runs the same
/// bootstrap inside the VM, which reads the file through the shared home.
///
/// The caller owns one absolute deadline. Expiry terminates the owned process
/// group, so a hung Compose client cannot outlive the call.
pub(super) fn run_shared_credential_exec(
    cwd: &Path,
    policy: &EffectiveContainerPolicy,
    plan: &ContainerComposeInvocationPlan,
    service: &SharedServiceBinding,
    env: &BTreeMap<String, String>,
    stdin_file: Option<&Path>,
    timeout: Duration,
) -> Result<Output, RunnerError> {
    let credential = SharedCredentialFile::write(service, env)?;
    let output = run_with_credential(cwd, policy, plan, &credential, stdin_file, timeout);
    // The file is worthless after this exec; a failed removal never masks the
    // command's own result.
    if let Err(error) = credential.remove() {
        eprintln!(
            "[warn] could not remove the private shared data credential file {}: {error}",
            credential.path.display()
        );
    }
    output
}

fn run_with_credential(
    cwd: &Path,
    policy: &EffectiveContainerPolicy,
    plan: &ContainerComposeInvocationPlan,
    credential: &SharedCredentialFile,
    stdin_file: Option<&Path>,
    timeout: Duration,
) -> Result<Output, RunnerError> {
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
    let deadline = Instant::now() + timeout;
    crate::runner::exec_command::run_command_capture_until(
        cwd,
        &program,
        &args,
        stdin_file,
        Some(deadline),
    )
    .map_err(|error| match error {
        RunnerError::TaskInvocation(message) => RunnerError::task_invocation(format!(
            "{label} did not complete within {}s: {message}",
            timeout.as_secs()
        )),
        other => other,
    })
}

#[cfg(test)]
mod tests {
    use super::SharedCredentialFile;
    use crate::contract_test_support::EnvGuard;
    use effigy_containers::{
        load_container_policy, EffectiveContainerPolicy, SharedServiceBinding,
    };
    use effigy_manifest::TaskManifest;
    use effigy_test_support::Fixture;
    use std::collections::BTreeMap;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

    const MANIFEST: &str = r#"
[data.targets.acowtancy]
service = "mysql"
database = "acowtancy"

[containers]
default = "stack"

[containers.stack]
primary_service = "app"

[containers.stack.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.stack.services.mysql]
catalog = "mariadb"
version = "10.11"
database = "acowtancy"
password = "secret"
shared = true
"#;

    struct Home {
        _fixture: Fixture,
        root: PathBuf,
        home: PathBuf,
    }

    fn fixture_home(prefix: &str) -> Home {
        let fixture = Fixture::workspace(prefix).expect("fixture");
        let root = fixture.path().canonicalize().expect("canonical fixture");
        let home = root.join("home");
        fs::create_dir_all(&home).expect("mkdir home");
        Home {
            _fixture: fixture,
            root,
            home,
        }
    }

    fn binding(root: &Path) -> SharedServiceBinding {
        fs::write(root.join("effigy.toml"), MANIFEST).expect("write manifest");
        let manifest: TaskManifest =
            crate::runner::manifest::load_task_manifest(&root.join("effigy.toml"))
                .expect("load manifest");
        let policy: EffectiveContainerPolicy =
            load_container_policy(root, Some("stack")).expect("policy");
        super::super::shared_data::resolve_shared_data_binding(&policy, &manifest, "mysql")
            .expect("resolve")
            .expect("shared binding")
    }

    fn env() -> BTreeMap<String, String> {
        BTreeMap::from([("MYSQL_PWD".to_owned(), "secret".to_owned())])
    }

    /// A failed or partial write must remove the exact file it created: no
    /// partial private file survives (effigy#477 review).
    #[test]
    fn a_failed_credential_write_removes_its_partial_file() {
        let fixture = fixture_home("effigy-shared-credential-partial-");
        let binding = binding(&fixture.root);
        let _env = EnvGuard::set_many(&[("HOME", Some(fixture.home.display().to_string()))]);
        let error = match SharedCredentialFile::write_with(&binding, &env(), |file, body| {
            use std::io::Write;
            file.write_all(&body[..8])?;
            Err(std::io::Error::other("injected partial write"))
        }) {
            Ok(_) => panic!("a partial write must fail the credential handoff"),
            Err(error) => error,
        };
        assert!(
            error
                .to_string()
                .contains("cannot write the private shared data credential file"),
            "{error}"
        );
        assert!(
            !error.to_string().contains("secret"),
            "no value in the error: {error}"
        );
        let leftovers = fs::read_dir(fixture.home.join(".effigy/data-credentials"))
            .expect("credentials directory")
            .collect::<Result<Vec<_>, _>>()
            .expect("credential entries");
        assert!(
            leftovers.is_empty(),
            "a partial credential file must not survive its failed write"
        );
    }

    #[test]
    fn concurrent_credential_files_are_distinct_immutable_and_removed() {
        let fixture = fixture_home("effigy-shared-credential-race-");
        let binding = binding(&fixture.root);
        let _env = EnvGuard::set_many(&[("HOME", Some(fixture.home.display().to_string()))]);
        let env = env();
        let mut handles = Vec::new();
        for _ in 0..8 {
            let binding = binding.clone();
            let env = env.clone();
            handles.push(std::thread::spawn(move || {
                let credential = SharedCredentialFile::write(&binding, &env).expect("write");
                let path = credential.path.clone();
                let metadata = fs::symlink_metadata(&path).expect("metadata");
                assert_eq!(
                    metadata.permissions().mode() & 0o777,
                    0o600,
                    "0600 required"
                );
                let body = fs::read_to_string(&path).expect("read");
                assert_eq!(
                    body, "set +xv\nexport MYSQL_PWD='secret'\n",
                    "the file must be complete and immutable"
                );
                credential.remove().expect("remove");
                assert!(!path.exists(), "the per-operation file is removed");
                path
            }));
        }
        let paths = handles
            .into_iter()
            .map(|handle| handle.join().expect("thread"))
            .collect::<Vec<_>>();
        let unique = paths.iter().collect::<std::collections::BTreeSet<_>>();
        assert_eq!(unique.len(), paths.len(), "each exec owns its own path");
        let leftovers = fs::read_dir(fixture.home.join(".effigy/data-credentials"))
            .expect("credentials dir")
            .collect::<Result<Vec<_>, _>>()
            .expect("entries");
        assert!(leftovers.is_empty(), "no credential file survives its exec");
    }
}
