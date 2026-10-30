//! Private credential transport for one database data exec (effigy#487).
//!
//! A database client's password travels to the client by variable name only
//! (`-e MYSQL_PWD` / `-e PGPASSWORD`). The value lives in a private file that
//! the exec bootstrap sources before it runs the client, so no value reaches a
//! spawned argv, a report, or a diagnostic.
//!
//! Each exec creates its own file (`create_new`, 0600 inside a 0700 directory,
//! never rewritten) and removes it when the call returns. Concurrent data
//! commands therefore never share a credential path, and a partially written
//! file can never be sourced.

use std::collections::BTreeMap;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use effigy_core::shell::shell_quote;

use super::RunnerError;

/// One private file holding `export NAME=value` lines for one data exec.
#[derive(Debug)]
pub(in crate::runner) struct DataCredentialFile {
    path: PathBuf,
    names: Vec<String>,
}

impl DataCredentialFile {
    /// Write the values of `env` into a fresh private file under `home`.
    pub(in crate::runner) fn write(
        home: &Path,
        env: &BTreeMap<String, String>,
    ) -> Result<Self, RunnerError> {
        Self::write_with(home, env, |file, body| file.write_all(body))
    }

    /// The write step is injected so a failed or partial write can be proven to
    /// remove the exact file it created.
    fn write_with(
        home: &Path,
        env: &BTreeMap<String, String>,
        write_body: impl FnOnce(&mut std::fs::File, &[u8]) -> std::io::Result<()>,
    ) -> Result<Self, RunnerError> {
        static OPERATION: AtomicU64 = AtomicU64::new(0);
        for name in env.keys() {
            if !valid_env_name(name) {
                return Err(RunnerError::task_invocation(format!(
                    "database data command declares `{name}`, which is not a portable environment name"
                )));
            }
        }
        let root = private_credential_root(home)?;
        let mut body = String::from("set +xv\n");
        for (name, value) in env {
            body.push_str(&format!("export {name}={}\n", shell_quote(value)));
        }
        for _ in 0..64 {
            let path = root.join(format!(
                "data-{}-{}.env",
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
                            "cannot write the private database credential file {}: {error}",
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
                        "cannot create the private database credential file {}: {error}",
                        path.display()
                    )))
                }
            }
        }
        Err(RunnerError::task_invocation(
            "cannot create a unique private database credential file",
        ))
    }

    pub(in crate::runner) fn path(&self) -> &Path {
        &self.path
    }

    /// The variable names the exec passes through (`-e NAME`) to the client.
    pub(in crate::runner) fn names(&self) -> &[String] {
        &self.names
    }

    pub(in crate::runner) fn remove(&self) -> std::io::Result<()> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }
}

/// Wrap a final, already-quoted client invocation so it runs with the exported
/// values of one private credential file. The bootstrap refuses when the file
/// cannot be sourced instead of running the client without a credential.
pub(in crate::runner) fn wrap_credential_bootstrap(command: &str) -> String {
    format!(
        ". \"$1\"; code=$?; [ \"$code\" = 0 ] || {{ echo 'effigy: cannot read the private database credential source' >&2; exit 125; }}; {command}"
    )
}

fn valid_env_name(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|first| first == '_' || first.is_ascii_alphabetic())
        && characters.all(|rest| rest == '_' || rest.is_ascii_alphanumeric())
}

/// The private directory this user's database credentials live in, created on
/// first use. `$HOME/.effigy` may be created by a concurrent command, so a lost
/// creation race is verified rather than reported as a failure.
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
                    "cannot inspect the private database credential directory {}: {error}",
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
                    "the private database credential directory {} is not a private directory this user owns; refusing to place a credential there",
                    path.display()
                )))
            }
        }
        Err(error) => Err(RunnerError::task_invocation(format!(
            "cannot create the private database credential directory {}: {error}",
            path.display()
        ))),
    }
}

#[cfg(test)]
mod tests;
