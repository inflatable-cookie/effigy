//! Generic per-program fallback: `[programs.<name>]` in the Effigy user config.
//!
//! When a named program is not an executable on the inherited `PATH`, or its
//! block sets `force = true`, the run gets a shim named after the program, first
//! on its `PATH`. The shim `exec`s the fallback with the exact argv: Effigy
//! translates no flags or arguments and never retries a failure. An omitted
//! `allow` list means unrestricted exact argv; a present one matches the first
//! argument exactly and refuses anything else before `exec`.
//!
//! The fallback is resolved to an absolute path on the inherited `PATH` when
//! the shim is written, so a shim can never re-enter itself or another shim:
//! fallbacks do not chain, and a nested real `$CARGO` cannot recurse into one.
//!
//! Only the user-global file may declare programs. A project `effigy.toml`
//! refuses `[programs]` as an unknown field, because one committed file must
//! not silently replace a program for every contributor.
//!
//! The shim directory is created for the run that activates it and removed when
//! that run settles. A nested or step run inherits its parent's directory
//! instead of starting another, and only the run that opened it removes it.

use std::cell::RefCell;
use std::ffi::OsStr;
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// One validated `[programs.<name>]` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct Program {
    /// The program name the shim shadows, resolved through `PATH`.
    pub(in crate::runner) name: String,
    /// The program to run instead. Never the same name.
    pub(in crate::runner) fallback: String,
    /// Optional exact first-argument allow list. `None` is unrestricted.
    pub(in crate::runner) allow: Option<Vec<String>>,
    /// Activate the fallback even when the program is available.
    pub(in crate::runner) force: bool,
}

/// Why a shim is on this run's `PATH`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Trigger {
    /// The program is not an executable on the inherited `PATH`.
    Unavailable,
    /// `force = true` in the block.
    Forced,
}

/// One shim this run carries.
#[derive(Debug, Clone)]
struct Active {
    program: Program,
    /// The fallback as an absolute path on the inherited `PATH`.
    fallback: PathBuf,
    trigger: Trigger,
}

/// A run's prepared shim directory.
#[derive(Debug)]
struct Prepared {
    dir: PathBuf,
    active: Vec<Active>,
    /// Device and inode of the directory this run created, checked before
    /// removal so Effigy never deletes a path it did not name.
    identity: (u64, u64),
}

/// Why Effigy refused the configuration. The text is operator-facing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct Refusal(String);

impl Refusal {
    fn new(reason: impl Into<String>) -> Self {
        Self(reason.into())
    }

    #[cfg(test)]
    pub(in crate::runner) fn reason(&self) -> &str {
        &self.0
    }
}
impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

type Scope = Option<Arc<Prepared>>;

thread_local! {
    /// The shim directory in force on this thread, innermost last. A nested
    /// run or step reuses the entry it finds instead of creating another.
    static CURRENT: RefCell<Vec<Scope>> = const { RefCell::new(Vec::new()) };
}

static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

/// A run's hold on the shim directory: its own, or the one it inherited.
#[derive(Debug)]
pub(in crate::runner) struct Handle {
    scope: Scope,
}

/// The shim directory first on this run's `PATH`, when a fallback is active.
pub(in crate::runner) fn path_dir() -> Option<PathBuf> {
    CURRENT.with(|current| {
        current
            .borrow()
            .last()
            .cloned()
            .flatten()
            .map(|prepared| prepared.dir.clone())
    })
}

/// Activates the configured fallbacks for a run, or inherits its parent's.
///
/// A run that activates one prints an honest notice. A refusal is a run
/// refusal, never a silently ignored setting.
pub(in crate::runner) fn for_run() -> Result<Option<Handle>, Refusal> {
    let inherited = CURRENT.with(|current| current.borrow().last().cloned());
    if let Some(scope) = inherited {
        push(scope.clone());
        return Ok(Some(Handle { scope }));
    }
    let programs = configured()?;
    let prepared = if programs.is_empty() {
        None
    } else {
        let base = crate::runner::scratch::current_dir().unwrap_or_else(std::env::temp_dir);
        let path = std::env::var_os("PATH");
        prepare(&base, path.as_deref(), programs)?
    };
    let Some(prepared) = prepared else {
        push(None);
        return Ok(Some(Handle { scope: None }));
    };
    for notice in prepared.notices() {
        eprintln!("effigy: {notice}");
    }
    let scope = Some(Arc::new(prepared));
    push(scope.clone());
    Ok(Some(Handle { scope }))
}

/// Leaves this thread's shim scope. The run that created the directory removes
/// it; a nested run only drops its reference.
pub(in crate::runner) fn leave(handle: Handle) {
    let held = handle.scope;
    let popped = pop();
    if !CURRENT.with(|current| current.borrow().is_empty()) {
        return;
    }
    let same = match (&held, &popped) {
        (Some(held), Some(popped)) => Arc::ptr_eq(held, popped),
        (None, None) => true,
        _ => false,
    };
    drop(held);
    if !same {
        return;
    }
    let Some(prepared) = popped else {
        return;
    };
    // Another scope still holds the directory: it is not ours to remove now.
    let Ok(prepared) = Arc::try_unwrap(prepared) else {
        return;
    };
    prepared.remove();
}

fn push(scope: Scope) {
    CURRENT.with(|current| current.borrow_mut().push(scope));
}

fn pop() -> Scope {
    CURRENT.with(|current| current.borrow_mut().pop().flatten())
}

/// Every declared program, validated. A malformed block or an unreadable user
/// config is a refusal.
pub(in crate::runner) fn configured() -> Result<Vec<Program>, Refusal> {
    let user = effigy_manifest::load_user_config()
        .map_err(|error| Refusal::new(format!("the Effigy user config is unreadable: {error}")))?;
    user.programs
        .into_iter()
        .map(|(name, block)| parse_program(name, block))
        .collect()
}

/// Whether a configured block would put a shim on this run's `PATH`. Only
/// such a run needs durable custody: foreground memory runs carry no shims,
/// and an inactive block changes nothing (see `prepare`).
pub(in crate::runner) fn activates_here() -> Result<bool, Refusal> {
    let path = std::env::var_os("PATH");
    Ok(configured()?
        .iter()
        .any(|program| program.force || resolve(&program.name, path.as_deref()).is_none()))
}

fn parse_program(
    name: String,
    block: effigy_manifest::UserProgramConfig,
) -> Result<Program, Refusal> {
    if !valid_program_name(&name) {
        return Err(Refusal::new(format!(
            "`[programs.{name}]` is not a usable program name: use letters, digits, `.`, `_`, `+` or `-`, without `/` or whitespace"
        )));
    }
    let fallback = block
        .fallback
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            Refusal::new(format!(
                "`[programs.{name}]` needs `fallback = \"<program>\"`; remove the block or name the program to run instead"
            ))
        })?
        .to_owned();
    if !valid_program_name(&fallback) {
        return Err(Refusal::new(format!(
            "`[programs.{name}] fallback = \"{fallback}\"` is not a usable program name: use letters, digits, `.`, `_`, `+` or `-`, without `/` or whitespace"
        )));
    }
    if fallback == name {
        return Err(Refusal::new(format!(
            "`[programs.{name}] fallback = \"{name}\"` would recurse; a fallback names a different program"
        )));
    }
    let allow = match block.allow {
        None => None,
        Some(entries) => {
            let mut tokens = Vec::with_capacity(entries.len());
            for entry in entries {
                let token = entry.trim().to_owned();
                if token.is_empty() {
                    return Err(Refusal::new(format!(
                        "`[programs.{name}] allow` has an empty entry; remove it"
                    )));
                }
                if !valid_subcommand(&token) {
                    return Err(Refusal::new(format!(
                        "`[programs.{name}] allow = [\"{token}\"]` is not a literal first argument: use letters, digits, `.`, `_`, `:`, `/`, `=`, `@`, `,`, `+` or `-`"
                    )));
                }
                if !tokens.contains(&token) {
                    tokens.push(token);
                }
            }
            if tokens.is_empty() {
                return Err(Refusal::new(format!(
                    "`[programs.{name}] allow = []` would refuse every call; omit `allow` for unrestricted exact argv"
                )));
            }
            Some(tokens)
        }
    };
    Ok(Program {
        name,
        fallback,
        allow,
        force: block.force.unwrap_or(false),
    })
}

fn valid_program_name(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('-')
        && !value.contains('/')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._+-".contains(&byte))
}

fn valid_subcommand(value: &str) -> bool {
    !value.is_empty()
        && !value.chars().any(char::is_whitespace)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:@,+/=-".contains(&byte))
}

/// The first executable file named `program` on `path`, in `PATH` order. Any
/// execute bit counts, matching the documented "an executable on the inherited
/// PATH" trigger rather than this process's own permission check.
fn resolve(program: &str, path: Option<&OsStr>) -> Option<PathBuf> {
    let path = path?;
    for dir in std::env::split_paths(path) {
        if dir.as_os_str().is_empty() {
            continue;
        }
        let candidate = dir.join(program);
        let Ok(metadata) = std::fs::metadata(&candidate) else {
            continue;
        };
        if metadata.is_file() && metadata.permissions().mode() & 0o111 != 0 {
            return Some(candidate);
        }
    }
    None
}

/// Writes the run's shims. `None` when no declared program is active, so a
/// configured-but-inactive block changes nothing.
fn prepare(
    base: &Path,
    inherited: Option<&OsStr>,
    programs: Vec<Program>,
) -> Result<Option<Prepared>, Refusal> {
    let mut active = Vec::new();
    for program in programs {
        let trigger = match resolve(&program.name, inherited) {
            Some(_) if !program.force => continue,
            Some(_) => Trigger::Forced,
            None => Trigger::Unavailable,
        };
        let fallback = resolve(&program.fallback, inherited).ok_or_else(|| {
            Refusal::new(format!(
                "`[programs.{}] fallback = \"{}\"` is not an executable on the inherited PATH; a fallback names a real program, never another fallback",
                program.name, program.fallback
            ))
        })?;
        active.push(Active {
            program,
            fallback,
            trigger,
        });
    }
    if active.is_empty() {
        return Ok(None);
    }
    let dir = create_dir(base)?;
    let identity = match std::fs::metadata(&dir) {
        Ok(metadata) => (metadata.dev(), metadata.ino()),
        Err(error) => {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(Refusal::new(format!(
                "the program shim directory could not be observed: {error}"
            )));
        }
    };
    for active in &active {
        let script = shim_script(active);
        let path = dir.join(&active.program.name);
        if let Err(error) = write_executable(&path, &script) {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(Refusal::new(format!(
                "the `{}` shim could not be written: {error}",
                active.program.name
            )));
        }
    }
    Ok(Some(Prepared {
        dir,
        active,
        identity,
    }))
}

fn create_dir(base: &Path) -> Result<PathBuf, Refusal> {
    let mut last = None;
    for _ in 0..8 {
        let sequence = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|since| since.as_nanos())
            .unwrap_or_default();
        let dir = base.join(format!(
            "effigy-programs-{}-{stamp}-{sequence}",
            std::process::id()
        ));
        match std::fs::DirBuilder::new().mode(0o700).create(&dir) {
            Ok(()) => {
                // The umask may clear bits a shim directory must keep.
                let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
                return Ok(dir);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                last = Some(error);
                continue;
            }
            Err(error) => {
                return Err(Refusal::new(format!(
                    "the program shim directory could not be created under {}: {error}",
                    base.display()
                )));
            }
        }
    }
    Err(Refusal::new(format!(
        "the program shim directory could not be named under {}: {}",
        base.display(),
        last.map(|error| error.to_string()).unwrap_or_default()
    )))
}

fn write_executable(path: &Path, script: &str) -> std::io::Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o755)
        .open(path)?;
    file.write_all(script.as_bytes())?;
    file.sync_all()?;
    drop(file);
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
}

impl Prepared {
    /// The honest notice a run prints once per active fallback.
    fn notices(&self) -> Vec<String> {
        self.active.iter().map(notice).collect()
    }

    fn remove(self) {
        let Ok(metadata) = std::fs::metadata(&self.dir) else {
            return;
        };
        if (metadata.dev(), metadata.ino()) != self.identity {
            // The path is no longer the directory this run created.
            return;
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn notice(active: &Active) -> String {
    let program = &active.program;
    let trigger = match active.trigger {
        Trigger::Unavailable => format!(
            "`{}` was not found as an executable on the inherited PATH",
            program.name
        ),
        Trigger::Forced => format!("`force = true` in `[programs.{}]`", program.name),
    };
    let mut text = format!(
        "[programs.{}] fallback active ({trigger}), so every `{}` resolved through PATH in this run runs `{}` with the exact argv. Effigy translates no flags or arguments, adds nothing, and never retries a failure; the original program's behaviour and safeguards are lost.",
        program.name, program.name, program.fallback
    );
    if program.fallback == "cargo" {
        text.push_str(
            " For `oxide`, `oxide clean` becomes `cargo clean` without Oxide's ownership checks or managed storage, and cargo plugins and cargo-subcommand exec run as plain cargo would.",
        );
    }
    match &program.allow {
        Some(allow) => text.push_str(&format!(
            " `allow = [{}]`: only those exact first arguments run; anything else is refused before exec.",
            allow.join(", ")
        )),
        None => text.push_str(
            " No `allow` list is set, so the fallback is unrestricted exact argv.",
        ),
    }
    text.push_str(
        " Only inherited-PATH name resolution is affected: absolute paths, a replaced PATH and shell aliases or functions are not.",
    );
    text
}

/// The shim itself: `/bin/sh`, exact argv, no translation, no retry.
fn shim_script(active: &Active) -> String {
    let program = &active.program;
    let fallback = shell_quote(&active.fallback.to_string_lossy());
    let mut script = String::from("#!/bin/sh\n");
    script.push_str(&format!(
        "# Effigy program fallback for `{}` -> `{}`. Exact argv, no translation, no retry.\n# Effigy wrote this shim for one run and removes it at settle; do not edit.\n",
        program.name, program.fallback
    ));
    let allow = program.allow.as_ref();
    script.push_str("if [ \"$#\" -eq 0 ]; then\n");
    match allow {
        Some(allow) => {
            script.push_str(&format!(
                "  printf 'effigy: %s refused: no first argument, and [programs.%s] allow = [%s] names only these; nothing ran.\\n' {} {} {} >&2\n  exit 127\n",
                shell_quote(&program.name),
                shell_quote(&program.name),
                shell_quote(&allow.join(", "))
            ));
        }
        None => {
            script.push_str(&format!("  exec {fallback}\n"));
        }
    }
    script.push_str("fi\n");
    if let Some(allow) = allow {
        let patterns = allow
            .iter()
            .map(|token| shell_quote(token))
            .collect::<Vec<_>>()
            .join("|");
        script.push_str(&format!("case \"$1\" in\n  {patterns}) ;;\n  *)\n"));
        script.push_str(&format!(
            "    printf 'effigy: %s refused: %s is not in [programs.%s] allow = [%s]; nothing ran. Omit allow for unrestricted exact argv.\\n' {} \"$1\" {} {} >&2\n",
            shell_quote(&program.name),
            shell_quote(&program.name),
            shell_quote(&allow.join(", "))
        ));
        script.push_str("    exit 127\n    ;;\nesac\n");
    }
    script.push_str(&format!("exec {fallback} \"$@\"\n"));
    script
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use effigy_manifest::UserProgramConfig;

    fn program(name: &str, fallback: &str, allow: Option<&[&str]>, force: bool) -> Program {
        Program {
            name: name.to_owned(),
            fallback: fallback.to_owned(),
            allow: allow.map(|tokens| tokens.iter().map(|token| (*token).to_owned()).collect()),
            force,
        }
    }

    fn temp_root() -> effigy_test_support::Fixture {
        effigy_test_support::Fixture::new("effigy-programs-").expect("fixture root")
    }

    fn write_bin(dir: &Path, name: &str, body: &str) {
        std::fs::create_dir_all(dir).expect("bin dir");
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write bin");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod bin");
    }

    fn path_with(dirs: &[&Path]) -> std::ffi::OsString {
        let joined = dirs
            .iter()
            .map(|dir| dir.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(":");
        std::ffi::OsString::from(joined)
    }

    #[test]
    fn config_refuses_malformed_program_blocks() {
        let cases = [
            (
                "oxide",
                UserProgramConfig {
                    fallback: None,
                    allow: None,
                    force: None,
                },
            ),
            (
                "ox/ide",
                UserProgramConfig {
                    fallback: Some("cargo".into()),
                    allow: None,
                    force: None,
                },
            ),
            (
                "oxide",
                UserProgramConfig {
                    fallback: Some("oxide".into()),
                    allow: None,
                    force: None,
                },
            ),
            (
                "oxide",
                UserProgramConfig {
                    fallback: Some("cargo".into()),
                    allow: Some(vec!["  ".into()]),
                    force: None,
                },
            ),
            (
                "oxide",
                UserProgramConfig {
                    fallback: Some("cargo".into()),
                    allow: Some(Vec::new()),
                    force: None,
                },
            ),
            (
                "oxide",
                UserProgramConfig {
                    fallback: Some("cargo".into()),
                    allow: Some(vec!["a b".into()]),
                    force: None,
                },
            ),
        ];
        for (name, block) in cases {
            assert!(
                parse_program(name.to_owned(), block.clone()).is_err(),
                "{name} / {block:?} should be refused"
            );
        }
    }

    #[test]
    fn config_accepts_an_omitted_allow_list_and_deduplicates_entries() {
        let parsed = parse_program(
            "oxide".into(),
            UserProgramConfig {
                fallback: Some(" cargo ".into()),
                allow: Some(vec!["check".into(), "check".into(), "--version".into()]),
                force: Some(true),
            },
        )
        .expect("valid block");
        assert_eq!(parsed.fallback, "cargo");
        assert_eq!(
            parsed.allow,
            Some(vec!["check".to_owned(), "--version".to_owned()])
        );
        assert!(parsed.force);
        let unrestricted = parse_program(
            "oxide".into(),
            UserProgramConfig {
                fallback: Some("cargo".into()),
                allow: None,
                force: None,
            },
        )
        .expect("valid block");
        assert!(unrestricted.allow.is_none());
        assert!(!unrestricted.force);
    }

    #[test]
    fn unavailable_program_gets_a_shim_that_passes_the_exact_argv() {
        let root = temp_root();
        let bin = root.path().join("bin");
        let record = root.path().join("argv");
        write_bin(
            &bin,
            "cargo",
            &format!(
                "printf '%s\\n' \"$#\" > '{}';\nfor arg in \"$@\"; do printf '[%s]\\n' \"$arg\" >> '{}'; done",
                record.display(),
                record.display()
            ),
        );
        let inherited = path_with(&[&bin]);
        let prepared = prepare(
            root.path(),
            Some(&inherited),
            vec![program("oxide", "cargo", None, false)],
        )
        .expect("prepare")
        .expect("active");

        let shim = prepared.dir.join("oxide");
        let output = std::process::Command::new(&shim)
            .args(["check", "a b", "", "c\"d", "$HOME", "*"])
            .output()
            .expect("run shim");
        assert!(
            output.status.success(),
            "shim failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let recorded = std::fs::read_to_string(&record).expect("record");
        assert_eq!(recorded, "6\n[check]\n[a b]\n[]\n[c\"d]\n[$HOME]\n[*]\n");
        prepared.remove();
        assert!(!shim.exists());
    }

    #[test]
    fn allow_list_refuses_unlisted_arguments_before_exec() {
        let root = temp_root();
        let bin = root.path().join("bin");
        let record = root.path().join("ran");
        write_bin(
            &bin,
            "cargo",
            &format!("printf 'ran %s\\n' \"$1\" >> '{}'", record.display()),
        );
        let inherited = path_with(&[&bin]);
        let prepared = prepare(
            root.path(),
            Some(&inherited),
            vec![program("oxide", "cargo", Some(&["check", "build"]), false)],
        )
        .expect("prepare")
        .expect("active");
        let shim = prepared.dir.join("oxide");

        let allowed = std::process::Command::new(&shim)
            .args(["check", "--all"])
            .output()
            .expect("run shim");
        assert!(allowed.status.success());
        assert_eq!(
            std::fs::read_to_string(&record).expect("record"),
            "ran check\n"
        );

        let refused = std::process::Command::new(&shim)
            .arg("clean")
            .output()
            .expect("run shim");
        assert_eq!(refused.status.code(), Some(127));
        let stderr = String::from_utf8_lossy(&refused.stderr);
        assert!(stderr.contains("refused"), "{stderr}");
        assert!(stderr.contains("clean"), "{stderr}");
        assert!(stderr.contains("[programs.oxide]"), "{stderr}");
        assert_eq!(
            std::fs::read_to_string(&record).expect("record"),
            "ran check\n",
            "a refused argument must never reach the fallback"
        );

        let no_argument = std::process::Command::new(&shim)
            .output()
            .expect("run shim");
        assert_eq!(no_argument.status.code(), Some(127));
        prepared.remove();
    }

    #[test]
    fn force_shadows_an_available_program_and_omitted_allow_is_unrestricted() {
        let root = temp_root();
        let bin = root.path().join("bin");
        let record = root.path().join("ran");
        write_bin(
            &bin,
            "oxide",
            &format!("printf 'real oxide\\n' >> '{}'", record.display()),
        );
        write_bin(
            &bin,
            "cargo",
            &format!("printf 'cargo %s\\n' \"$1\" >> '{}'", record.display()),
        );
        let inherited = path_with(&[&bin]);

        // Not forced and available: nothing changes.
        assert!(prepare(
            root.path(),
            Some(&inherited),
            vec![program("oxide", "cargo", None, false)]
        )
        .expect("prepare")
        .is_none());

        let prepared = prepare(
            root.path(),
            Some(&inherited),
            vec![program("oxide", "cargo", None, true)],
        )
        .expect("prepare")
        .expect("active");
        let shim = prepared.dir.join("oxide");
        let output = std::process::Command::new(&shim)
            .args(["clean", "--all"])
            .output()
            .expect("run shim");
        assert!(output.status.success());
        assert_eq!(
            std::fs::read_to_string(&record).expect("record"),
            "cargo clean\n"
        );
        prepared.remove();
    }

    #[test]
    fn fallbacks_resolve_against_the_inherited_path_so_they_never_chain() {
        let root = temp_root();
        let bin = root.path().join("bin");
        let record = root.path().join("ran");
        write_bin(
            &bin,
            "cargo",
            &format!("printf 'real %s\\n' \"$1\" >> '{}'", record.display()),
        );
        write_bin(
            &bin,
            "other",
            &format!("printf 'other %s\\n' \"$1\" >> '{}'", record.display()),
        );
        let inherited = path_with(&[&bin]);
        // `cargo` has an active forced fallback, and `oxide` names `cargo`.
        // `oxide` must resolve the real `cargo`, never cargo's shim.
        let prepared = prepare(
            root.path(),
            Some(&inherited),
            vec![
                program("cargo", "other", None, true),
                program("oxide", "cargo", None, false),
            ],
        )
        .expect("prepare")
        .expect("active");
        let output = std::process::Command::new(prepared.dir.join("oxide"))
            .arg("run")
            .output()
            .expect("run shim");
        assert!(output.status.success());
        assert_eq!(
            std::fs::read_to_string(&record).expect("record"),
            "real run\n"
        );
        prepared.remove();
    }

    #[test]
    fn a_nested_real_cargo_never_reaches_the_shim() {
        let root = temp_root();
        let bin = root.path().join("bin");
        let record = root.path().join("ran");
        write_bin(
            &bin,
            "cargo",
            &format!(
                "printf 'real %s\\n' \"$1\" >> '{}';\n\"$CARGO\" x",
                record.display()
            ),
        );
        write_bin(
            &bin,
            "cargo-shim-target",
            &format!("printf 'shimmed %s\\n' \"$1\" >> '{}'", record.display()),
        );
        write_bin(
            &bin,
            "nested-cargo",
            &format!("printf 'nested %s\\n' \"$1\" >> '{}'", record.display()),
        );
        let inherited = path_with(&[&bin]);
        // Both `cargo` (forced) and `oxide` carry shims. Oxide's fallback and a
        // nested real `$CARGO` are absolute paths, so neither can re-enter a
        // shim even though the shim directory is first on `PATH`.
        let prepared = prepare(
            root.path(),
            Some(&inherited),
            vec![
                program("cargo", "cargo-shim-target", None, true),
                program("oxide", "cargo", None, false),
            ],
        )
        .expect("prepare")
        .expect("active");
        let output = std::process::Command::new(prepared.dir.join("oxide"))
            .arg("build")
            .env("CARGO", bin.join("nested-cargo"))
            .env(
                "PATH",
                format!("{}:{}", prepared.dir.display(), inherited.to_string_lossy()),
            )
            .output()
            .expect("run shim");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        // The real cargo ran (not cargo's own shim), and its nested `$CARGO`
        // was the binary it named, not the shim.
        assert_eq!(
            std::fs::read_to_string(&record).expect("record"),
            "real build\nnested x\n"
        );
        prepared.remove();
    }

    #[test]
    fn a_missing_fallback_refuses_instead_of_guessing() {
        let root = temp_root();
        let bin = root.path().join("bin");
        write_bin(&bin, "elsewhere", "exit 0");
        let inherited = path_with(&[&bin]);
        let error = prepare(
            root.path(),
            Some(&inherited),
            vec![program("oxide", "cargo", None, false)],
        )
        .expect_err("unresolvable fallback");
        assert!(error.reason().contains("not an executable"), "{error}");
    }

    #[test]
    fn shim_directory_is_private_and_removed_by_the_run_that_created_it() {
        let root = temp_root();
        let bin = root.path().join("bin");
        write_bin(&bin, "cargo", "exit 0");
        let inherited = path_with(&[&bin]);
        let prepared = prepare(
            root.path(),
            Some(&inherited),
            vec![program("oxide", "cargo", None, false)],
        )
        .expect("prepare")
        .expect("active");
        let dir_mode = std::fs::metadata(&prepared.dir)
            .expect("dir")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(dir_mode, 0o700);
        let shim = prepared.dir.join("oxide");
        let shim_mode = std::fs::metadata(&shim).expect("shim").permissions().mode() & 0o777;
        assert_eq!(shim_mode, 0o755);

        let dir = prepared.dir.clone();
        let handle = Handle {
            scope: Some(Arc::new(prepared)),
        };
        push(handle.scope.clone());
        assert_eq!(path_dir().as_deref(), Some(dir.as_path()));
        leave(handle);
        assert!(path_dir().is_none());
        assert!(!shim.exists());
    }

    #[test]
    fn notices_document_the_trigger_scope_allow_list_and_lost_safeguards() {
        let active = Active {
            program: program("oxide", "cargo", Some(&["check"]), false),
            fallback: PathBuf::from("/usr/bin/cargo"),
            trigger: Trigger::Unavailable,
        };
        let text = notice(&active);
        for token in [
            "was not found as an executable on the inherited PATH",
            "exact argv",
            "never retries a failure",
            "`oxide clean` becomes `cargo clean`",
            "cargo plugins and cargo-subcommand exec",
            "allow = [check]",
            "absolute paths, a replaced PATH and shell aliases or functions",
        ] {
            assert!(text.contains(token), "{token} missing from {text}");
        }
        let forced = Active {
            program: program("oxide", "cargo", None, true),
            fallback: PathBuf::from("/usr/bin/cargo"),
            trigger: Trigger::Forced,
        };
        let forced_text = notice(&forced);
        assert!(forced_text.contains("force = true"), "{forced_text}");
        assert!(
            forced_text
                .contains("No `allow` list is set, so the fallback is unrestricted exact argv"),
            "{forced_text}"
        );
    }

    #[test]
    fn a_nested_run_inherits_the_directory_without_printing_again() {
        let root = temp_root();
        let bin = root.path().join("bin");
        write_bin(&bin, "cargo", "exit 0");
        let inherited = path_with(&[&bin]);
        let prepared = prepare(
            root.path(),
            Some(&inherited),
            vec![program("oxide", "cargo", None, false)],
        )
        .expect("prepare")
        .expect("active");
        let dir = prepared.dir.clone();
        let outer = Handle {
            scope: Some(Arc::new(prepared)),
        };
        push(outer.scope.clone());
        // The nested run inherits: same directory, no new shims.
        let inner = Handle {
            scope: outer.scope.clone(),
        };
        push(inner.scope.clone());
        assert_eq!(path_dir().as_deref(), Some(dir.as_path()));
        leave(inner);
        assert_eq!(path_dir().as_deref(), Some(dir.as_path()));
        leave(outer);
        assert!(path_dir().is_none());
        assert!(!dir.exists());
    }
}
