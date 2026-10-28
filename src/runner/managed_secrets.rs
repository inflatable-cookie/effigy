//! Private managed exec delivery. Values travel in the spawn environment and
//! stdin, or in an acknowledged, run-owned HOME file when stdin is occupied.
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs::{self, DirBuilder};
use std::io;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use effigy_core::shell::shell_quote;
use effigy_kernel::custody::EnvValue;
use effigy_kernel::ident::{self, ProcessIdentity};
use effigy_kernel::observe::Observed;
use effigy_kernel::run::{EffectSpec, Settlement};
use serde::{Deserialize, Serialize};

use super::task_engine::{EnvOverride, TaskRun};

pub(super) const NAMES: &str = "EFFIGY_INTERNAL_MANAGED_ENV_NAMES";
pub(super) const STDIN_FREE: &str = "EFFIGY_INTERNAL_MANAGED_STDIN_FREE";
const SCRIPT: &str = "EFFIGY_INTERNAL_MANAGED_ENV_SCRIPT";
const DIRECTORY: &str = "EFFIGY_INTERNAL_MANAGED_ENV_DIRECTORY";
pub(super) const EFFECT: &str = "managed:secret-delivery";
const ROOT: &str = ".effigy/managed-secrets";

/// The journal contains custody only, never values or script contents.
#[derive(Serialize, Deserialize)]
struct DeliveryDirectory {
    path: PathBuf,
    owner: ProcessIdentity,
}

#[derive(Serialize, Deserialize)]
struct DirectoryOwner {
    identity: ProcessIdentity,
    device: u64,
    inode: u64,
}

fn owner_uid() -> u32 {
    // SAFETY: geteuid has no preconditions and cannot fail.
    unsafe { libc::geteuid() }
}

fn private_dir(path: &Path) -> io::Result<()> {
    match DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            let meta = fs::symlink_metadata(path)?;
            if meta.is_dir()
                && meta.uid() == owner_uid()
                && meta.permissions().mode() & 0o777 == 0o700
            {
                Ok(())
            } else {
                Err(io::Error::other(
                    "managed secret directory is not private and owned",
                ))
            }
        }
        Err(error) => Err(error),
    }
}

fn root(home: &Path) -> io::Result<PathBuf> {
    // The HOME itself can be shared with the VM; only our private subtree is
    // created or inspected. Never change an existing directory's permissions.
    let parent = home.join(".effigy");
    if !parent.try_exists()? {
        private_dir(&parent)?;
    }
    let meta = fs::symlink_metadata(&parent)?;
    if !meta.is_dir() || meta.uid() != owner_uid() || meta.permissions().mode() & 0o022 != 0 {
        return Err(io::Error::other("managed secret parent is not owned"));
    }
    let root = home.join(ROOT);
    private_dir(&root)?;
    Ok(root)
}

fn valid_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// Attach a bounded bootstrap to the private spawn environment. Only a PTY or
/// inherited-input route creates a directory, journalled before the creation.
pub(super) fn prepare(
    run: &mut TaskRun,
    overrides: &[(String, EnvOverride)],
    env: &mut BTreeMap<OsString, EnvValue>,
    occupied: bool,
) -> Result<Option<PathBuf>, String> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    prepare_in(run, overrides, env, occupied, home.as_deref())
}

fn prepare_in(
    run: &mut TaskRun,
    overrides: &[(String, EnvOverride)],
    env: &mut BTreeMap<OsString, EnvValue>,
    occupied: bool,
    home: Option<&Path>,
) -> Result<Option<PathBuf>, String> {
    let Some(EnvValue::Plain(names)) = env.get(OsStr::new(NAMES)) else {
        return Ok(None);
    };
    let names = names.to_string_lossy().into_owned();
    let mut script = "set +xv\n".to_owned();
    for name in names.split_whitespace() {
        if !valid_name(name) {
            return Err("invalid managed environment name".into());
        }
        let value = match overrides
            .iter()
            .rev()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value)
        {
            Some(EnvOverride::Plain(value) | EnvOverride::Secret(value)) => value.clone(),
            None => return Err("managed environment value is missing".into()),
        };
        script.push_str(&format!("export {name}={}\n", shell_quote(&value)));
    }
    let bytes = script.into_bytes();
    let script =
        effigy_kernel::custody::SecretValue::new(bytes.clone()).map_err(|e| e.to_string())?;
    env.insert(SCRIPT.into(), EnvValue::Secret(script));
    // Do not inherit an outer run's delivery directory on a free-stdin route.
    env.remove(OsStr::new(DIRECTORY));
    if !occupied || env.contains_key(OsStr::new(STDIN_FREE)) {
        return Ok(None);
    }
    let owner = ident::process_identity(std::process::id())
        .verified()
        .cloned()
        .ok_or("cannot identify managed secret owner")?;
    let root =
        root(home.ok_or("managed secret delivery needs HOME")?).map_err(|e| e.to_string())?;
    let directory = DeliveryDirectory {
        path: root.join(format!("{}-{}", run.id(), unique())),
        owner,
    };
    let mut kernel = run.run_mut().map_err(|e| e.to_string())?;
    let target = kernel.fence().clone();
    let ticket = kernel
        .intend(EffectSpec {
            target,
            kind: EFFECT.into(),
            payload: serde_json::to_value(&directory).map_err(|e| e.to_string())?,
            settlement: Settlement::Release(serde_json::Value::Null),
            // Cleanup is mandatory for every release effect. This is not a
            // process-exit obligation (the workload owns that separately).
            required: false,
        })
        .map_err(|e| e.to_string())?;
    DirBuilder::new()
        .mode(0o700)
        .create(&directory.path)
        .map_err(|e| e.to_string())?;
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut owner_file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(directory.path.join("owner"))
        .map_err(|e| e.to_string())?;
    let meta = fs::symlink_metadata(&directory.path).map_err(|e| e.to_string())?;
    let owner = DirectoryOwner {
        identity: directory.owner.clone(),
        device: meta.dev(),
        inode: meta.ino(),
    };
    owner_file
        .write_all(&serde_json::to_vec(&owner).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(directory.path.join("env.delivery"))
        .map_err(|e| e.to_string())?;
    file.write_all(&bytes).map_err(|e| e.to_string())?;
    drop(file);
    let meta = fs::symlink_metadata(&directory.path).map_err(|e| e.to_string())?;
    kernel
        .applied(
            &ticket,
            serde_json::json!({"device": meta.dev(), "inode": meta.ino()}),
        )
        .map_err(|e| e.to_string())?;
    // Treat the private owner path as secret too, keeping it out of captures.
    env.insert(
        DIRECTORY.into(),
        EnvValue::Secret(
            effigy_kernel::custody::SecretValue::new(
                directory.path.as_os_str().as_encoded_bytes().to_vec(),
            )
            .map_err(|e| e.to_string())?,
        ),
    );
    Ok(Some(directory.path.join("env.delivery")))
}

fn unique() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

pub(super) fn release(effect: &effigy_kernel::run::Effect) -> io::Result<()> {
    let directory: DeliveryDirectory =
        serde_json::from_value(effect.spec.payload.clone()).map_err(io::Error::other)?;
    if let (Some(receipt), Ok(meta)) = (&effect.receipt, fs::symlink_metadata(&directory.path)) {
        if receipt.get("device").and_then(serde_json::Value::as_u64) != Some(meta.dev())
            || receipt.get("inode").and_then(serde_json::Value::as_u64) != Some(meta.ino())
        {
            return Err(io::Error::other("managed secret directory was replaced"));
        }
    }
    remove(&directory)
}

/// Never recursively remove a tree or follow a symlink. A foreign entry or
/// changed owner refuses cleanup; absence already fulfils the obligation.
fn remove(directory: &DeliveryDirectory) -> io::Result<()> {
    if !directory.path.parent().is_some_and(|p| p.ends_with(ROOT)) {
        return Err(io::Error::other("invalid managed secret namespace"));
    }
    let meta = match fs::symlink_metadata(&directory.path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    if !meta.is_dir() || meta.uid() != owner_uid() || meta.permissions().mode() & 0o777 != 0o700 {
        return Err(io::Error::other("managed secret ownership changed"));
    }
    let owner_path = directory.path.join("owner");
    let bytes = match fs::read(&owner_path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return fs::remove_dir(&directory.path),
        Err(e) => return Err(e),
    };
    let owner: DirectoryOwner = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
    if owner.identity != directory.owner || owner.device != meta.dev() || owner.inode != meta.ino()
    {
        return Err(io::Error::other("managed secret owner changed"));
    }
    let mut files = Vec::new();
    for entry in fs::read_dir(&directory.path)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let meta = fs::symlink_metadata(entry.path())?;
        if (name != "owner" && !name.starts_with("env."))
            || !meta.is_file()
            || meta.uid() != owner_uid()
            || meta.permissions().mode() & 0o777 != 0o600
        {
            return Err(io::Error::other("foreign managed secret entry"));
        }
        files.push(entry.path());
    }
    // Remove secret material before its ownership record. If cleanup itself
    // is interrupted, the next sweep still has custody for every unread file.
    files.sort_by_key(|file| file.file_name().is_some_and(|name| name == "owner"));
    for file in files {
        fs::remove_file(file)?;
    }
    fs::remove_dir(&directory.path)
}

/// Unknown liveness retains the file. A reused PID is distinguished by boot
/// and process start identity. This is disposal only, never runtime repair.
pub(super) fn sweep() -> io::Result<()> {
    let Some(home) = std::env::var_os("HOME") else {
        return Ok(());
    };
    sweep_in(Path::new(&home))
}

fn sweep_in(home: &Path) -> io::Result<()> {
    let directory = home.join(ROOT);
    if !directory.try_exists()? {
        return Ok(());
    }
    let root = root(home)?;
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let meta = fs::symlink_metadata(entry.path())?;
        if !meta.is_dir() || meta.uid() != owner_uid() || meta.permissions().mode() & 0o777 != 0o700
        {
            continue;
        }
        let Ok(bytes) = fs::read(entry.path().join("owner")) else {
            continue;
        };
        let Ok(owner) = serde_json::from_slice::<DirectoryOwner>(&bytes) else {
            continue;
        };
        if owner.device != meta.dev() || owner.inode != meta.ino() {
            continue;
        }
        let dead = match ident::process_identity(owner.identity.pid) {
            Observed::Absent(_) => true,
            Observed::Verified(current, _) => current != owner.identity,
            Observed::Unknown(_) => false,
        };
        if dead {
            remove(&DeliveryDirectory {
                path: entry.path(),
                owner: owner.identity,
            })?;
        }
    }
    Ok(())
}

/// Render one final CLI invocation. All interpolated arguments are public;
/// the bootstrap reaches stdin through shell builtins, or the prepared 0600 file.
/// Colima reads HOME on the host mount before sudo/nerdctl, never in the guest
/// container. Name-only `-e K` inherits the resulting invoking environment.
pub(super) fn relay(program: &OsStr, args: &[OsString], names: &[String]) -> String {
    let quote_args = |args: &[OsString]| {
        args.iter()
            .map(|a| shell_quote(&a.to_string_lossy()))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let colima = args.first().is_some_and(|a| a == "nerdctl")
        && program.to_string_lossy().ends_with("colima");
    let (launcher, command) = if colima {
        // compose_invocation and native exec both use this exact prefix.
        let profile = args.get(2).cloned().unwrap_or_default();
        (
            format!(
                "{} ssh --profile {} --",
                shell_quote(&program.to_string_lossy()),
                shell_quote(&profile.to_string_lossy())
            ),
            format!(
                "exec sudo --preserve-env={} nerdctl {}",
                shell_quote(&names.join(",")),
                quote_args(&args[4..])
            ),
        )
    } else {
        (
            String::new(),
            format!(
                "exec {} {}",
                shell_quote(&program.to_string_lossy()),
                quote_args(args)
            ),
        )
    };
    let file_bootstrap = format!("set +xv; . \"$1\"; code=$?; rm -f -- \"$1\" || exit 125; [ \"$code\" = 0 ] || exit 125; {command}");
    format!(
        r#"set +xv
if [ -n "${{{DIRECTORY}:-}}" ]; then
    file="${DIRECTORY}/env.delivery"
    {launcher} sh -c {file_bootstrap} sh "$file"
    exit "$?"
else
    {{ printf '%s' "${SCRIPT}"; printf '%s\n' {command}; }} | {launcher} sh -s
fi"#,
        file_bootstrap = shell_quote(&file_bootstrap),
        command = shell_quote(&command)
    )
}

#[cfg(test)]
mod tests;
