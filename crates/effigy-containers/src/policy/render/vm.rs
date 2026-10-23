//! Compose interpolation for the VM-hosted Colima backend.
//!
//! `colima nerdctl` runs Compose inside the VM, so the host process
//! environment never reaches Compose interpolation there. `EFFIGY_RENDER_ROOT`
//! and the values a Compose plan forwards (container secrets delivered through
//! Compose's own environment) therefore travel in an interpolation file the VM
//! can read, named with `--env-file`. Compose's default `.env` lookup is the
//! first Compose file's directory, which inside a rendered generation is the
//! sealed object directory: it never holds an `.env`, so naming this file drops
//! no interpolation the invocation had before.
//!
//! The file lives under the Effigy home, never inside an object closure, so a
//! generation's content digest stays independent of the state-home path. It is
//! private to this process (0600 inside a 0700 directory) and reclaimed by the
//! next invocation once its writer is gone.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use nix::errno::Errno;
use nix::sys::signal::kill;
use nix::unistd::Pid;

use super::{render_root, ROOT_VARIABLE};

/// Compose's interpolation file flag. `nerdctl compose` accepts exactly one.
const ENV_FILE_FLAG: &str = "--env-file";
/// Effigy-home directory that holds each interpolation set.
const DIRECTORY: &str = "compose-env";
/// A retained file this old is reclaimed even while its writer lives.
const MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);
/// Upper bound on retained interpolation files.
const MAX_FILES: usize = 64;
/// The Compose subcommand token every Compose invocation carries.
const COMPOSE: &str = "compose";
/// The host adapter whose Compose process runs inside the Colima VM.
const COLIMA: &str = "colima";

/// One Compose invocation's arguments, with the interpolation values a
/// VM-hosted backend cannot inherit. A host backend already owns the caller's
/// environment and is returned unchanged; an invocation that carries no value
/// to deliver is unchanged too.
pub fn compose_invocation_args(
    program: &OsStr,
    args: &[OsString],
    env: &[(String, OsString)],
) -> io::Result<Vec<OsString>> {
    if !runs_in_vm(program) {
        return Ok(args.to_vec());
    }
    let Some(index) = args.iter().position(|argument| argument == COMPOSE) else {
        return Ok(args.to_vec());
    };
    let mut entries = BTreeMap::new();
    for (name, value) in env {
        validate_name(name)?;
        entries.insert(name.clone(), value.clone());
    }
    // The generation root is Effigy's own value: it outranks a caller pair of
    // the same name and is the last value written.
    if let Some(root) = render_root(args) {
        entries.insert(ROOT_VARIABLE.to_owned(), root.into_os_string());
    }
    if entries.is_empty() {
        return Ok(args.to_vec());
    }
    let path = write_interpolation_file(&entries)?;
    let mut resolved = args.to_vec();
    resolved.insert(index + 1, OsString::from(ENV_FILE_FLAG));
    resolved.insert(index + 2, path.into_os_string());
    Ok(resolved)
}

/// Only the Colima adapter runs Compose on the other side of a VM boundary.
fn runs_in_vm(program: &OsStr) -> bool {
    Path::new(program).file_name() == Some(OsStr::new(COLIMA))
}

/// A Compose variable name. The file is a shell-free format, but a name that is
/// not a portable identifier can never be forwarded faithfully.
fn validate_name(name: &str) -> io::Result<()> {
    let mut characters = name.chars();
    let started = characters
        .next()
        .is_some_and(|first| first == '_' || first.is_ascii_alphabetic());
    if !started || !characters.all(|rest| rest == '_' || rest.is_ascii_alphanumeric()) {
        return Err(invalid(format!(
            "Compose interpolation variable `{name}` is not a portable identifier"
        )));
    }
    Ok(())
}

/// One `NAME=value` line per variable. Values are double quoted: `\\`, `\"`
/// and newlines are escaped and `$` is doubled, so Compose reads the value
/// literally instead of interpolating it a second time.
fn render(entries: &BTreeMap<String, OsString>) -> io::Result<String> {
    let mut body = String::new();
    for (name, value) in entries {
        let value = value.to_str().ok_or_else(|| {
            invalid(format!(
                "the Compose interpolation value for `{name}` is not UTF-8 and cannot travel through an env file"
            ))
        })?;
        body.push_str(name);
        body.push('=');
        body.push('"');
        for character in value.chars() {
            match character {
                '\\' => body.push_str("\\\\"),
                '"' => body.push_str("\\\""),
                '\n' => body.push_str("\\n"),
                '$' => body.push_str("$$"),
                '\r' => {
                    return Err(invalid(format!(
                        "the Compose interpolation value for `{name}` carries a carriage return, which an env file cannot represent"
                    )))
                }
                other => body.push(other),
            }
        }
        body.push_str("\"\n");
    }
    Ok(body)
}

fn invalid(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

/// Write one interpolation set, reusing an identical file this process already
/// published. The name carries the writer's pid so a later invocation can
/// reclaim it once its writer is gone.
fn write_interpolation_file(entries: &BTreeMap<String, OsString>) -> io::Result<PathBuf> {
    let body = render(entries)?;
    let directory = directory()?;
    prune(&directory);
    let digest = effigy_gateway::ports::sha256_hex(body.as_bytes());
    let path = directory.join(format!("{}-{}.env", std::process::id(), &digest[..16]));
    if path.is_file() {
        return Ok(path);
    }
    let temporary = directory.join(format!(".{}-{}.tmp", std::process::id(), &digest[..16]));
    {
        let mut file = private_file(&temporary)?;
        file.write_all(body.as_bytes())?;
        file.sync_all()?;
    }
    std::fs::rename(&temporary, &path)?;
    Ok(path)
}

fn directory() -> io::Result<PathBuf> {
    let home = crate::effigy_home().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "the Effigy home is unknown (HOME is required to deliver Compose interpolation to the Colima VM)",
        )
    })?;
    let directory = home.join(DIRECTORY);
    private_dir(&directory)?;
    Ok(directory)
}

fn private_dir(directory: &Path) -> io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(directory)
}

fn private_file(path: &Path) -> io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

/// Reclaim interpolation files whose writer is gone, whose age makes them
/// stale, or that exceed the retained-file bound. A file that cannot be
/// removed is never fatal: only Compose's own read of the current file matters.
fn prune(directory: &Path) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let mut retained = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(OsStr::to_str).map(str::to_owned) else {
            continue;
        };
        let written = entry.metadata().and_then(|meta| meta.modified()).ok();
        let abandoned = name
            .split('-')
            .next()
            .and_then(|pid| pid.parse::<i32>().ok())
            .is_some_and(|pid| !alive(pid));
        let stale = written
            .and_then(|written| written.elapsed().ok())
            .is_some_and(|age| age > MAX_AGE);
        if abandoned || stale {
            let _ = std::fs::remove_file(&path);
        } else {
            retained.push((written, path));
        }
    }
    if retained.len() <= MAX_FILES {
        return;
    }
    retained.sort();
    let excess = retained.len() - MAX_FILES;
    for (_, path) in retained.into_iter().take(excess) {
        let _ = std::fs::remove_file(path);
    }
}

fn alive(pid: i32) -> bool {
    pid > 0 && matches!(kill(Pid::from_raw(pid), None), Ok(()) | Err(Errno::EPERM))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn args(paths: &[&str]) -> Vec<OsString> {
        ["nerdctl", "--profile", "effigy", "--", "compose"]
            .into_iter()
            .chain(paths.iter().copied())
            .map(OsString::from)
            .collect()
    }

    fn object_root(home: &Path) -> PathBuf {
        let root = home.join(".effigy/state/objects").join("a".repeat(64));
        std::fs::create_dir_all(root.join("checkout")).expect("mkdir object root");
        root
    }

    #[test]
    fn a_vm_hosted_invocation_names_a_file_the_vm_can_read() {
        let home = tempfile::tempdir().expect("home");
        let root = object_root(home.path());
        let invocation = args(&[
            "-f",
            root.join("checkout/compose.yml").to_str().expect("path"),
            "-p",
            "probe",
            "up",
            "-d",
        ]);
        let resolved = crate::with_test_effigy_home(home.path(), || {
            compose_invocation_args(
                OsStr::new("colima"),
                &invocation,
                &[(
                    "DATABASE_URL".to_owned(),
                    OsString::from("postgres://user:pa$$ word@db/app"),
                )],
            )
        })
        .expect("interpolation file");
        let index = resolved
            .iter()
            .position(|argument| argument == ENV_FILE_FLAG)
            .expect("--env-file");
        assert_eq!(
            resolved[index - 1],
            OsString::from(COMPOSE),
            "--env-file is a Compose flag, ahead of the subcommand"
        );
        let path = PathBuf::from(&resolved[index + 1]);
        assert!(path.starts_with(home.path()), "{path:?}");
        assert!(
            path.is_file(),
            "the invocation names an interpolation file the VM can read"
        );
        let body = std::fs::read_to_string(&path).expect("interpolation file");
        assert_eq!(
            body,
            format!(
                "DATABASE_URL=\"postgres://user:pa$$$$ word@db/app\"\n{ROOT_VARIABLE}=\"{}\"\n",
                root.display()
            ),
            "values are literal and the generation root is explicit"
        );
        assert_eq!(
            std::fs::metadata(&path)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    #[test]
    fn a_host_backend_keeps_the_process_environment() {
        let home = tempfile::tempdir().expect("home");
        let root = object_root(home.path());
        let invocation = [
            OsString::from("compose"),
            OsString::from("-f"),
            root.join("checkout/compose.yml").into_os_string(),
            OsString::from("up"),
        ];
        let resolved = crate::with_test_effigy_home(home.path(), || {
            compose_invocation_args(
                OsStr::new("/usr/local/bin/docker"),
                &invocation,
                &[("TOKEN".to_owned(), OsString::from("secret"))],
            )
        })
        .expect("unchanged invocation");
        assert_eq!(resolved, invocation);
        assert!(
            !home.path().join(DIRECTORY).exists(),
            "a host backend writes no interpolation file"
        );
    }

    #[test]
    fn an_invocation_with_nothing_to_deliver_is_unchanged() {
        let home = tempfile::tempdir().expect("home");
        let invocation = args(&["ps"]);
        let resolved = crate::with_test_effigy_home(home.path(), || {
            compose_invocation_args(OsStr::new("colima"), &invocation, &[])
        })
        .expect("unchanged invocation");
        assert_eq!(resolved, invocation);
    }

    #[test]
    fn a_runtime_command_is_never_rewritten_as_compose() {
        let home = tempfile::tempdir().expect("home");
        let invocation = [
            OsString::from("nerdctl"),
            OsString::from("--profile"),
            OsString::from("effigy"),
            OsString::from("--"),
            OsString::from("ps"),
        ];
        let resolved = crate::with_test_effigy_home(home.path(), || {
            compose_invocation_args(
                OsStr::new("colima"),
                &invocation,
                &[("TOKEN".to_owned(), OsString::from("secret"))],
            )
        })
        .expect("unchanged runtime invocation");
        assert_eq!(resolved, invocation);
    }

    #[test]
    fn values_round_trip_through_the_interpolation_file_encoding() {
        for value in [
            "plain",
            "a\\b",
            "a\"b",
            "lit\\n not newline",
            "real\nnewline",
            "cost $5",
            "${HOME}",
            "end\\",
            "quote'and\"both",
            "",
        ] {
            let entries = BTreeMap::from([("VALUE".to_owned(), OsString::from(value))]);
            let body = render(&entries).expect("encoded value");
            assert_eq!(body, format!("VALUE=\"{}\"\n", encode(value)));
            assert_eq!(decode(&body), value, "round trip for {value:?}");
        }
        let carriage_return = BTreeMap::from([("VALUE".to_owned(), OsString::from("a\rb"))]);
        assert!(render(&carriage_return).is_err());
    }

    /// The inverse of [`render`] for one double-quoted value, mirroring the
    /// env-file grammar Compose applies.
    fn decode(body: &str) -> String {
        let value = body
            .strip_prefix("VALUE=\"")
            .and_then(|value| value.strip_suffix("\"\n"))
            .expect("encoded shape");
        let mut decoded = String::new();
        let mut characters = value.chars();
        while let Some(character) = characters.next() {
            match character {
                '\\' => match characters.next().expect("escape") {
                    'n' => decoded.push('\n'),
                    '\\' => decoded.push('\\'),
                    '"' => decoded.push('"'),
                    other => panic!("unexpected escape {other:?}"),
                },
                '$' => {
                    assert_eq!(characters.next(), Some('$'), "literal dollars double");
                    decoded.push('$');
                }
                other => decoded.push(other),
            }
        }
        decoded
    }

    fn encode(value: &str) -> String {
        let mut encoded = String::new();
        for character in value.chars() {
            match character {
                '\\' => encoded.push_str("\\\\"),
                '"' => encoded.push_str("\\\""),
                '\n' => encoded.push_str("\\n"),
                '$' => encoded.push_str("$$"),
                other => encoded.push(other),
            }
        }
        encoded
    }

    #[test]
    fn an_abandoned_interpolation_file_is_reclaimed_by_the_next_invocation() {
        let home = tempfile::tempdir().expect("home");
        let directory = home.path().join(DIRECTORY);
        private_dir(&directory).expect("private directory");
        // A pid that cannot be running: above the kernel's pid ceiling.
        let abandoned = directory.join("2147483646-0123456789abcdef.env");
        std::fs::write(&abandoned, "STALE=\"1\"\n").expect("abandoned file");
        let live = directory.join(format!("{}-0123456789abcdef.env", std::process::id()));
        std::fs::write(&live, "LIVE=\"1\"\n").expect("live file");

        crate::with_test_effigy_home(home.path(), || {
            compose_invocation_args(
                OsStr::new("colima"),
                &args(&["up"]),
                &[("TOKEN".to_owned(), OsString::from("secret"))],
            )
        })
        .expect("interpolation file");

        assert!(!abandoned.exists(), "a dead writer's file is reclaimed");
        assert!(live.exists(), "a live writer's file is retained");
    }
}
