//! Compose interpolation for the VM-hosted Colima backend.
//!
//! `colima nerdctl` runs Compose inside the VM, so the host process
//! environment never reaches Compose interpolation there. The values a Compose
//! plan forwards (`secrets.delivery = "compose-env"`) therefore travel in a
//! file the VM can read, named with `--env-file`. `nerdctl compose` also hands
//! that file to every service's `nerdctl run`, so a value is written in the one
//! form both readers agree on: a plain value stays unquoted, and only a value
//! Compose must read literally is double quoted and escaped.
//!
//! The file lives under the Effigy home, never inside an object closure, so a
//! generation's content digest stays independent of the state-home path. Its
//! name is the digest of its contents: the same plan names the same file, so a
//! re-up adds nothing. It is private to this user (0600 inside a 0700
//! directory) and reclaimed by age and by count.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use effigy_kernel::fs::{DirFd, FileName};
use effigy_kernel::gate::{Gate, Wait};

/// Compose's interpolation file flag. `nerdctl compose` accepts exactly one.
const ENV_FILE_FLAG: &str = "--env-file";
/// Effigy-home directory that holds each interpolation set.
const DIRECTORY: &str = "compose-env";
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
    if !runs_in_vm(program) || env.is_empty() {
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

/// One `NAME=value` line per variable.
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
        match literal(value) {
            Some(value) => body.push_str(value),
            None => quote(&mut body, name, value)?,
        }
        body.push('\n');
    }
    Ok(body)
}

/// The value itself, when both Compose and nerdctl read it unchanged: no quoting
/// and nothing either of them would escape, trim or expand.
fn literal(value: &str) -> Option<&str> {
    (!value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_graphic() && !"\"'$\\#".contains(character)))
    .then_some(value)
}

/// The double-quoted spelling of a value Compose must read literally: `\\`,
/// `\"` and newlines are escaped and `$` is doubled, so Compose substitutes the
/// value instead of interpolating it a second time.
fn quote(body: &mut String, name: &str, value: &str) -> io::Result<()> {
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
    body.push('"');
    Ok(())
}

fn invalid(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

/// Write one interpolation set, reusing an identical file this user already
/// published. The name is the digest of the contents, so concurrent
/// invocations of the same plan converge on one file.
fn write_interpolation_file(entries: &BTreeMap<String, OsString>) -> io::Result<PathBuf> {
    let body = render(entries)?;
    let op = crate::environment::operation(
        effigy_kernel::ctx::EffectClass::Mutate,
        effigy_kernel::ctx::CancelToken::default(),
        Duration::from_secs(10),
    );
    let cx = op.effects().map_err(|e| invalid(format!("{e:?}")))?;
    let home = crate::effigy_home().ok_or_else(|| invalid("the Effigy home is unknown (HOME is required to deliver Compose interpolation to the Colima VM)".into()))?;
    let home_dir = match DirFd::open(&home) {
        Ok(dir) => dir,
        Err(_) if !home.exists() => {
            let parent = home
                .parent()
                .ok_or_else(|| invalid("Effigy home has no parent".into()))?;
            let name = home
                .file_name()
                .and_then(OsStr::to_str)
                .ok_or_else(|| invalid("invalid Effigy home".into()))?;
            DirFd::open(parent)
                .and_then(|dir| dir.prepare_setup_child(&cx, &FileName::new(name)?, 0o700))
                .map_err(|e| invalid(e.to_string()))?
        }
        Err(error) => return Err(invalid(error.to_string())),
    };
    let directory = home_dir
        .prepare_setup_child(
            &cx,
            &FileName::new(DIRECTORY).map_err(|e| invalid(e.to_string()))?,
            0o700,
        )
        .map_err(|e| invalid(e.to_string()))?;
    let gate = Gate::files(
        &cx,
        &directory,
        "compose-interpolation",
        Wait::Until(std::time::Instant::now() + Duration::from_secs(10)),
    )
    .map_err(|e| invalid(e.to_string()))?;
    let digest = effigy_gateway::ports::sha256_hex(body.as_bytes());
    let name =
        FileName::new(&format!("{}.env", &digest[..16])).map_err(|e| invalid(e.to_string()))?;
    let path = home.join(DIRECTORY).join(name.as_str());
    if path
        .symlink_metadata()
        .is_ok_and(|metadata| metadata.file_type().is_file())
    {
        return Ok(path);
    }
    effigy_kernel::record::atomic_replace(
        &cx,
        &gate,
        &directory,
        &name,
        body.as_bytes(),
        Default::default(),
    )
    .map_err(|e| invalid(e.to_string()))?;
    Ok(path)
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

    fn secret(value: &str) -> Vec<(String, OsString)> {
        vec![("DATABASE_URL".to_owned(), OsString::from(value))]
    }

    #[test]
    fn a_vm_hosted_invocation_names_a_file_the_vm_can_read() {
        let home = tempfile::tempdir().expect("home");
        let invocation = args(&["-f", "checkout/compose.yml", "-p", "probe", "up", "-d"]);
        let resolved = crate::with_test_effigy_home(home.path(), || {
            compose_invocation_args(
                OsStr::new("colima"),
                &invocation,
                &secret("postgres://db/app"),
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
        assert_eq!(
            std::fs::read_to_string(&path).expect("interpolation file"),
            "DATABASE_URL=postgres://db/app\n",
            "a plain value is unquoted, so nerdctl's container copy stays exact"
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
    fn the_same_values_name_the_same_file_and_new_values_add_one() {
        let home = tempfile::tempdir().expect("home");
        let first = crate::with_test_effigy_home(home.path(), || {
            compose_invocation_args(OsStr::new("colima"), &args(&["up"]), &secret("one"))
        })
        .expect("interpolation file");
        let same = crate::with_test_effigy_home(home.path(), || {
            compose_invocation_args(OsStr::new("colima"), &args(&["up"]), &secret("one"))
        })
        .expect("interpolation file");
        let changed = crate::with_test_effigy_home(home.path(), || {
            compose_invocation_args(OsStr::new("colima"), &args(&["up"]), &secret("two"))
        })
        .expect("interpolation file");
        assert_eq!(first, same, "an unchanged plan names the same file");
        assert_ne!(first, changed, "a changed secret names its own file");
    }

    #[test]
    fn a_host_backend_keeps_the_process_environment() {
        let home = tempfile::tempdir().expect("home");
        let invocation = [
            OsString::from("compose"),
            OsString::from("-f"),
            OsString::from("checkout/compose.yml"),
            OsString::from("up"),
        ];
        let resolved = crate::with_test_effigy_home(home.path(), || {
            compose_invocation_args(
                OsStr::new("/usr/local/bin/docker"),
                &invocation,
                &secret("secret"),
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
            compose_invocation_args(OsStr::new("colima"), &invocation, &secret("secret"))
        })
        .expect("unchanged runtime invocation");
        assert_eq!(resolved, invocation);
    }

    #[test]
    fn values_round_trip_through_the_interpolation_file_encoding() {
        for value in [
            "plain",
            "postgres://user:pa$$word@db/app",
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
            assert_eq!(decode(&body), value, "round trip for {value:?}");
        }
        let carriage_return = BTreeMap::from([("VALUE".to_owned(), OsString::from("a\rb"))]);
        assert!(render(&carriage_return).is_err());
    }

    /// The value Compose reads back, from either spelling.
    fn decode(body: &str) -> String {
        let value = body
            .strip_prefix("VALUE=")
            .and_then(|value| value.strip_suffix('\n'))
            .expect("encoded shape");
        let Some(quoted) = value.strip_prefix('"') else {
            return value.to_owned();
        };
        let mut decoded = String::new();
        let mut characters = quoted.strip_suffix('"').expect("closing quote").chars();
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
}
