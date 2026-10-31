//! The sole standard gateway administrator action. Native platform jobs never
//! serve application traffic as root. Explicit removal exports the endpoint Store.
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::{self, Read};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

use effigy_kernel::ctx::{EffectClass, EffectContext};
use effigy_kernel::custody::{Custody, EnvValue, Lifetime, ProcessSpec, Terminal};
use effigy_kernel::fs::{DirFd, FileName};
use effigy_kernel::gate::Wait;
use effigy_kernel::id::{ResourceId, Scope};
use effigy_kernel::observe::Observed;
use effigy_kernel::outcome::WorkloadOutcome;
use effigy_kernel::record::{read_record, Record, ReplaceOpts};
use effigy_kernel::root::StateRoot;
use effigy_kernel::run::{BeginOpts, Run};
use effigy_kernel::supervise::{Executable, LifecyclePlan, Placement};
use serde::{Deserialize, Serialize};

use crate::operator::{self, error};
use crate::server::GatewayConfig;
#[cfg(any(target_os = "macos", test))]
mod broker;
pub(crate) const MARKER: &str = "Managed by Effigy gateway setup v1";
const MAX_FILE: u64 = 8 << 20;
/// Bound on one executable image staged for privileged setup. A developer
/// build of `effigy` with debug info is well past 128 MiB, so the bound leaves
/// room for one (#392); a larger file is not the binary setup just ran.
const MAX_STAGED_EXECUTABLE: u64 = 512 << 20;

/// True only when a staged image of this length may be read for setup.
fn staged_executable_fits(len: u64) -> bool {
    len <= MAX_STAGED_EXECUTABLE
}

/// The refusal for an image past the staging bound. It names the bound and the
/// remedy: a debug build is past it, and nothing about that is the operator's
/// fault or a reason to loosen the bound (#473).
fn staged_executable_refusal(len: u64) -> String {
    format!(
        "setup stages at most {} MiB of the running executable, and this one is {} MiB; build and install a release binary with `effigy install:local`, then run `effigy gateway setup` again",
        MAX_STAGED_EXECUTABLE >> 20,
        len.div_ceil(1 << 20)
    )
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub version: u8,
    pub uid: u32,
    pub suffixes: Vec<String>,
}
impl Record for Receipt {
    const MAX_BYTES: u64 = 65536;
    fn decode(bytes: &[u8]) -> Result<Self, String> {
        serde_json::from_slice(bytes).map_err(|e| e.to_string())
    }
}
pub fn read_receipt(home: &Path) -> io::Result<Receipt> {
    let dir = DirFd::open(&home.join(".effigy/gateway")).map_err(error)?;
    let op = operator::context(Duration::from_secs(1), EffectClass::Observe);
    match read_record::<Receipt>(
        &op.observe().map_err(error)?,
        &dir,
        &FileName::new("setup.json").map_err(error)?,
    ) {
        Observed::Verified(receipt, _)
            if receipt.version == 1 && receipt.uid == nix::unistd::Uid::effective().as_raw() =>
        {
            Ok(receipt)
        }
        other => Err(error(format!(
            "gateway setup is unavailable ({}); run `effigy gateway setup`",
            other.unproven()
        ))),
    }
}
pub fn require_domain(home: &Path, domain: &str) -> io::Result<()> {
    let receipt = read_receipt(home)?;
    let domain = domain.trim_end_matches('.').to_ascii_lowercase();
    if receipt
        .suffixes
        .iter()
        .any(|suffix| domain == *suffix || domain.ends_with(&format!(".{suffix}")))
    {
        Ok(())
    } else {
        Err(error(format!("gateway domain `{domain}` needs resolver approval; run `effigy gateway setup --suffix {domain}`")))
    }
}
pub fn approved_suffixes(config: &GatewayConfig, extra: &[String]) -> io::Result<Vec<String>> {
    let table = crate::routes::RouteTable::load(&config.route_table_path).map_err(error)?;
    let mut suffixes =
        crate::resolver_setup::route_driven_resolver_suffixes(table.routes.keys(), &config.dns.tld);
    suffixes.push(config.dns.tld.clone());
    suffixes.extend(extra.iter().cloned());
    validate_suffixes(suffixes)
}
fn validate_suffixes(mut suffixes: Vec<String>) -> io::Result<Vec<String>> {
    for suffix in &mut suffixes {
        *suffix = suffix.trim_end_matches('.').to_ascii_lowercase();
        if suffix.is_empty()
            || suffix.len() > 253
            || suffix.split('.').any(|label| {
                label.is_empty()
                    || label.len() > 63
                    || label.starts_with('-')
                    || label.ends_with('-')
                    || !label
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'-')
            })
        {
            return Err(error("invalid gateway setup suffix"));
        }
    }
    suffixes.sort();
    suffixes.dedup();
    Ok(suffixes)
}
/// Privileged setup reads only explicit legacy files through its authenticated
/// operator descriptor. It never opens the user's kernel Store as root.
pub fn platform_suffixes(
    cx: &EffectContext,
    uid: u32,
    home: &Path,
    extra: &[String],
) -> io::Result<Vec<String>> {
    let root = DirFd::open_setup(cx, home, uid).map_err(error)?;
    let mut suffixes = vec!["test".to_owned()];
    suffixes.extend(extra.iter().cloned());
    if let Some(effigy) = root
        .setup_child(&FileName::new(".effigy").map_err(error)?)
        .map_err(error)?
    {
        if let Some(gateway) = effigy
            .setup_child(&FileName::new("gateway").map_err(error)?)
            .map_err(error)?
        {
            let observe = cx.operation().observe().map_err(error)?;
            match read_record::<Bytes>(
                &observe,
                &gateway,
                &FileName::new("routes.json").map_err(error)?,
            ) {
                Observed::Verified(bytes, _) => {
                    let table: crate::routes::RouteTable =
                        serde_json::from_slice(&bytes.0).map_err(error)?;
                    suffixes.extend(crate::resolver_setup::route_driven_resolver_suffixes(
                        table.routes.keys(),
                        "test",
                    ));
                }
                Observed::Absent(_) => {}
                other => {
                    return Err(error(format!(
                        "legacy resolver inputs are unknown: {}",
                        other.unproven()
                    )))
                }
            }
            match read_record::<Receipt>(
                &observe,
                &gateway,
                &FileName::new("setup.json").map_err(error)?,
            ) {
                Observed::Verified(receipt, _) if receipt.version == 1 && receipt.uid == uid => {
                    suffixes.extend(receipt.suffixes)
                }
                Observed::Absent(_) => {}
                other => {
                    return Err(error(format!(
                        "setup resolver receipt is unknown: {}",
                        other.unproven()
                    )))
                }
            }
        }
    }
    validate_suffixes(suffixes)
}
pub fn authenticated_operator() -> io::Result<(u32, PathBuf)> {
    if !nix::unistd::Uid::effective().is_root() {
        return Err(error(
            "platform setup requires the setup administrator child",
        ));
    }
    let uid: u32 = std::env::var("SUDO_UID")
        .map_err(error)?
        .parse()
        .map_err(error)?;
    if uid == 0 {
        return Err(error("run gateway setup from a non-root operator account"));
    }
    let user = nix::unistd::User::from_uid(nix::unistd::Uid::from_raw(uid))
        .map_err(error)?
        .ok_or_else(|| error("operator account is missing"))?;
    let metadata = std::fs::symlink_metadata(&user.dir)?;
    if !metadata.is_dir() || metadata.uid() != uid || metadata.mode() & 0o022 != 0 {
        return Err(error("operator home is untrusted"));
    }
    Ok((uid, user.dir))
}
pub fn setup_owner() -> ResourceId {
    ResourceId::store(Scope::Machine, "gateway-setup").expect("fixed setup identity")
}
fn root_home() -> io::Result<PathBuf> {
    Ok(nix::unistd::User::from_uid(nix::unistd::Uid::effective())
        .map_err(error)?
        .ok_or_else(|| error("setup account missing"))?
        .dir)
}
/// Run one bounded setup session, with independent cleanup. The privileged
/// child stages the already-authorized executable under its own UID before
/// registering its kernel sentinel; it never opens the user's kernel store.
pub fn session<T>(
    home: &Path,
    privileged: bool,
    work: impl FnOnce(&mut Run<'_>, &EffectContext) -> io::Result<T>,
) -> io::Result<T> {
    let op = operator::context(Duration::from_secs(120), EffectClass::Mutate);
    let cx = op.effects().map_err(error)?;
    let cleanup = op.cleanup(Duration::from_secs(15)).map_err(error)?;
    let root = StateRoot::runtime_machine_effects(&cx, home).map_err(error)?;
    let mut adapter = operator::Adapter;
    let mut run = Run::begin(
        &cx,
        &root,
        &operator::invocation(setup_owner()),
        BeginOpts {
            id: operator::run_id()?,
            wait: Wait::FailFast,
            acknowledge: vec![],
            next: operator::next(),
            custody: None,
        },
        &cleanup,
        &mut adapter,
    )
    .map_err(error)?;
    let mut staged = None;
    let executable = if privileged {
        let temp = tempfile::Builder::new()
            .prefix("gateway-setup-")
            .tempdir_in(root.path())?;
        let source = std::env::current_exe()?;
        let file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(source)?;
        if !file.metadata()?.is_file() {
            return Err(error("setup executable is not a regular file"));
        }
        if !staged_executable_fits(file.metadata()?.len()) {
            return Err(error(staged_executable_refusal(file.metadata()?.len())));
        }
        let mut bytes = Vec::new();
        file.take(MAX_STAGED_EXECUTABLE + 1)
            .read_to_end(&mut bytes)?;
        if !staged_executable_fits(bytes.len() as u64) {
            return Err(error(format!(
                "setup executable changed while staging: {}",
                staged_executable_refusal(bytes.len() as u64)
            )));
        }
        let dir = DirFd::open(temp.path()).map_err(error)?;
        run.replace_file(
            &dir,
            &FileName::new("effigy").map_err(error)?,
            &bytes,
            ReplaceOpts {
                mode: 0o700,
                keep_previous: false,
            },
        )
        .map_err(error)?;
        let executable = Executable::capture(&temp.path().join("effigy"))?;
        staged = Some(temp);
        executable
    } else {
        Executable::current()?
    };
    run.enable_custody(&LifecyclePlan {
        executable,
        placement: Placement::Inline,
    })
    .map_err(error)?;
    let result = work(&mut run, &cx);
    run.finish(if result.is_ok() {
        WorkloadOutcome::Succeeded
    } else {
        WorkloadOutcome::Failed { code: 1 }
    })
    .map_err(error)?;
    let settled = run.settle(&cleanup, &mut adapter).map_err(error)?;
    if settled.cleanup() != effigy_kernel::outcome::CleanupOutcome::Complete {
        return Err(unresolved_cleanup(&settled));
    }
    drop(staged);
    result
}

/// What setup could not release, by effect and custody reason. The settle can
/// stop on its last open effect, where no later effect carries the reason, so
/// this reads the settled outcome instead of reporting a bare "unresolved
/// cleanup" the operator cannot act on (effigy#476).
fn unresolved_cleanup(
    settled: &effigy_kernel::outcome::Outcome<effigy_kernel::run::Settled>,
) -> io::Error {
    let unresolved = settled.unresolved();
    if unresolved.is_empty() {
        return error("gateway setup has unresolved cleanup");
    }
    error(format!(
        "gateway setup left cleanup unresolved: {}",
        unresolved.join("; ")
    ))
}
pub fn command(
    run: &mut Run<'_>,
    cx: &EffectContext,
    argv: Vec<OsString>,
    terminal: bool,
) -> io::Result<Vec<u8>> {
    let (code, output) = command_outcome(run, cx, argv, terminal)?;
    if code != 0 {
        return Err(error(format!(
            "gateway setup adapter failed (exit {code}): {}",
            String::from_utf8_lossy(&output)
        )));
    }
    Ok(output)
}

/// The command an adapter child runs, as a safe attributable identity: the
/// program's own file name. Every setup command shares the same generic label,
/// so a residue that named only the label could not tell two setup process
/// effects apart; the journal already redacts this label (effigy#476).
fn adapter_label(argv: &[OsString]) -> String {
    let program = argv
        .first()
        .and_then(|program| Path::new(program).file_name())
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty());
    match program {
        Some(program) => format!("gateway setup adapter: {program}"),
        None => "gateway setup adapter".to_owned(),
    }
}

/// Run one setup child and keep its exit code with its output. A caller that
/// reads a structured report from a child which exits non-zero on refusal (the
/// low-port bind probe) needs both; [`command`] keeps only the success bytes.
pub fn command_outcome(
    run: &mut Run<'_>,
    cx: &EffectContext,
    argv: Vec<OsString>,
    terminal: bool,
) -> io::Result<(i32, Vec<u8>)> {
    let mut child = spawn_setup_child(run, cx, argv, terminal)?;
    let exit = match child.wait(cx.operation()) {
        Observed::Verified(exit, _) => exit,
        other => return Err(error(format!("setup command outcome unknown: {other:?}"))),
    };
    child.complete(run, exit).map_err(error)?;
    Ok((exit.code, child.tail()))
}

/// Run the elevated setup child and report each platform phase it prints while
/// it runs. The spawn, the outcome and the refusal are [`command`]'s; the
/// phases are the child's own progress, never its result.
pub fn command_reporting_platform_phases(
    run: &mut Run<'_>,
    cx: &EffectContext,
    argv: Vec<OsString>,
    terminal: bool,
    report: &mut dyn FnMut(&str),
) -> io::Result<Vec<u8>> {
    let mut child = spawn_setup_child(run, cx, argv, terminal)?;
    // `wait` would only show this child's output after it exited, too late to
    // name the phase the operator is waiting in. This loop reads the captured
    // stdout between polls; the operation check, the child's deadline and the
    // verdict it reports stay the kernel's own, so this path cannot drift from
    // [`command_outcome`].
    let mut stdout = Vec::new();
    let observed = loop {
        let polled = child.poll();
        if matches!(polled, Observed::Verified(..)) {
            break polled;
        }
        let (out, _err) = child.take_stream_output();
        stdout.extend_from_slice(&out);
        report_phase_lines(&mut stdout, report);
        if child.expired() || cx.operation().check("process wait").is_err() {
            break child.wait(cx.operation());
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let exit = match observed {
        Observed::Verified(exit, _) => exit,
        other => return Err(error(format!("setup command outcome unknown: {other:?}"))),
    };
    child.complete(run, exit).map_err(error)?;
    // Our own progress lines are not the child's diagnostic: a refusal still
    // reads as what the child said.
    let output = without_phase_lines(&child.tail());
    if exit.code != 0 {
        return Err(error(format!(
            "gateway setup adapter failed (exit {}): {}",
            exit.code,
            String::from_utf8_lossy(&output)
        )));
    }
    Ok(output)
}

/// One setup child with the adapter's own working directory, environment,
/// grace and lifetime bounds.
fn spawn_setup_child(
    run: &mut Run<'_>,
    cx: &EffectContext,
    argv: Vec<OsString>,
    terminal: bool,
) -> io::Result<effigy_kernel::custody::OwnedProcess> {
    let env: BTreeMap<_, _> = std::env::vars_os()
        .map(|(key, value)| (key, EnvValue::Plain(value)))
        .collect();
    let label = adapter_label(&argv);
    Custody::spawn(
        run,
        ProcessSpec {
            argv,
            cwd: root_home()?,
            env,
            grace: Duration::from_secs(5),
            lifetime: Lifetime::Finite(Duration::from_secs(90)),
            terminal: if terminal {
                Terminal::Foreground
            } else {
                Terminal::None
            },
            label,
        },
        cx,
    )
    .map_err(|e| error(format!("setup spawn: {e:?}")))
}

#[derive(Debug)]
struct Bytes(Vec<u8>);
impl Record for Bytes {
    const MAX_BYTES: u64 = MAX_FILE;
    fn decode(bytes: &[u8]) -> Result<Self, String> {
        Ok(Self(bytes.to_vec()))
    }
}
fn managed_file(
    run: &mut Run<'_>,
    cx: &EffectContext,
    dir: &DirFd,
    name: &str,
    bytes: &[u8],
    resolver: bool,
) -> io::Result<()> {
    let name = FileName::new(name).map_err(error)?;
    let observed = read_record::<Bytes>(&cx.operation().observe().map_err(error)?, dir, &name);
    match observed {
        Observed::Verified(old, _) if old.0 == bytes => return Ok(()),
        Observed::Verified(old, _)
            if String::from_utf8_lossy(&old.0).contains(MARKER)
                || (resolver
                    && String::from_utf8_lossy(&old.0).contains("Managed by Effigy gateway")) => {}
        Observed::Absent(_) => {}
        other => {
            return Err(error(format!(
                "preserving foreign or unreadable setup output {}: {other:?}",
                dir.path().join(name.as_str()).display()
            )))
        }
    }
    run.replace_file(
        dir,
        &name,
        bytes,
        ReplaceOpts {
            mode: 0o644,
            keep_previous: true,
        },
    )
    .map_err(error)
}
/// Fixed native boot job: no mutable operator script or Effigy executable runs
/// with privilege. Every alias is explicit, and the job repeats after reboot.
pub fn macos_boot_job() -> Vec<u8> {
    let script = (1..=50)
        .map(|n| format!("/sbin/ifconfig lo0 alias 127.1.0.{n} up || exit $?"))
        .collect::<Vec<_>>()
        .join("; ");
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!-- {MARKER} -->\n<plist version=\"1.0\"><dict><key>Label</key><string>dev.effigy.gateway.loopback</string><key>RunAtLoad</key><true/><key>ProgramArguments</key><array><string>/bin/sh</string><string>-c</string><string>{script}</string></array></dict></plist>\n").into_bytes()
}
/// A dedicated dummy link scopes split DNS to approved suffixes and has no
/// default-route role. It does not replace the host's global DNS configuration.
pub fn linux_boot_job(suffixes: &[String]) -> Vec<u8> {
    let domains = suffixes
        .iter()
        .map(|suffix| format!("~{suffix}"))
        .collect::<Vec<_>>()
        .join(" ");
    format!("# {MARKER}\n[Unit]\nDescription=Effigy gateway split DNS\nAfter=systemd-resolved.service\nRequires=systemd-resolved.service\n[Service]\nType=oneshot\nRemainAfterExit=yes\nExecStart=/usr/sbin/ip link add effigy-gw type dummy\nExecStart=/usr/sbin/ip link set effigy-gw up\nExecStart=/usr/bin/resolvectl dns effigy-gw 127.0.0.1:15353\nExecStart=/usr/bin/resolvectl domain effigy-gw {domains}\nExecStart=/usr/bin/resolvectl default-route effigy-gw no\nExecStop=/usr/bin/resolvectl revert effigy-gw\nExecStop=/usr/sbin/ip link delete effigy-gw\n[Install]\nWantedBy=multi-user.target\n").into_bytes()
}
/// Run one platform command through the injected adapter, refusing a non-zero
/// exit with the adapter's own output, exactly as [`command`] does.
fn platform_command(
    run: &mut Run<'_>,
    cx: &EffectContext,
    command: &mut impl FnMut(&mut Run<'_>, &EffectContext, Vec<OsString>) -> io::Result<(i32, Vec<u8>)>,
    argv: Vec<OsString>,
) -> io::Result<Vec<u8>> {
    let (code, output) = command(run, cx, argv)?;
    if code != 0 {
        return Err(error(format!(
            "gateway setup adapter failed (exit {code}): {}",
            String::from_utf8_lossy(&output)
        )));
    }
    Ok(output)
}

/// The platform actions' root-owned directories, operator identity and helper
/// image. Production names `/private/etc`, `/Library/LaunchDaemons`,
/// `/Library` and the running executable; a fixture substitutes its own, so
/// nothing here can reach the machine's resolver, launchd or interfaces.
#[cfg(any(target_os = "macos", test))]
pub(crate) struct MacosPlatform<'a> {
    pub identity: (u32, u32),
    pub helper: &'a Path,
    pub etc: &'a DirFd,
    pub jobs: &'a DirFd,
    pub library: &'a DirFd,
}

/// The schema of one platform setup phase line. The elevated setup child writes
/// these to its captured stdout while the platform adapter runs, so the
/// operator command waiting on it can name the object being changed. It is an
/// internal process channel, not an operator-facing artifact: the child's
/// stdout is a pipe the parent owns, and a direct run prints none of these.
pub const PLATFORM_PHASE_SCHEMA: &str = "effigy.gateway.setup_phase.v1";

/// One phase line: the schema marker and the phase itself.
pub fn platform_phase_line(phase: &str) -> String {
    serde_json::json!({
        "schema": PLATFORM_PHASE_SCHEMA,
        "phase": phase,
    })
    .to_string()
}

/// The phase a line reports, when it is one of ours. Any other line the child
/// printed for its own reasons is not a phase and never reaches the operator's
/// step.
pub fn platform_phase_from_line(line: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let object = value.as_object()?;
    if object.len() != 2 || object.get("schema")?.as_str()? != PLATFORM_PHASE_SCHEMA {
        return None;
    }
    object.get("phase")?.as_str().map(str::to_owned)
}

/// Write one phase line for the command waiting on this child. Only the
/// elevated child does this: its stdout is the captured pipe the operator
/// command reads, so the line never reaches a terminal, a pipe or a JSON
/// envelope.
pub fn print_platform_phase(phase: &str) {
    println!("{}", platform_phase_line(phase));
}

/// Longest phase line the parent keeps while waiting for its newline.
const MAX_PHASE_LINE: usize = 8 << 10;

/// Report every complete phase line in `buffer`, keeping any partial last line
/// for the next read: the child's stdout can be split anywhere.
fn report_phase_lines(buffer: &mut Vec<u8>, report: &mut dyn FnMut(&str)) {
    while let Some(newline) = buffer.iter().position(|byte| *byte == b'\n') {
        let line: Vec<u8> = buffer.drain(..=newline).collect();
        let text = String::from_utf8_lossy(&line);
        if let Some(phase) = platform_phase_from_line(text.trim_end()) {
            report(&phase);
        }
    }
    // A child that wrote no newline in a long run is not reporting phases;
    // keep only enough to finish one line.
    if buffer.len() > MAX_PHASE_LINE {
        let stale = buffer.len() - MAX_PHASE_LINE;
        buffer.drain(..stale);
    }
}

/// The child's output with this channel's own phase lines removed, so a refusal
/// still reads as the child's diagnostic and not as our progress bookkeeping.
fn without_phase_lines(output: &[u8]) -> Vec<u8> {
    let mut kept = Vec::new();
    for line in output.split_inclusive(|byte| *byte == b'\n') {
        let text = String::from_utf8_lossy(line);
        if platform_phase_from_line(text.trim_end()).is_none() {
            kept.extend_from_slice(line);
        }
    }
    kept
}

/// The macOS platform action: local resolver files, the loopback boot job and
/// the privileged bind broker. `report` names each operation before it starts,
/// so the operator's step shows the object being changed rather than one
/// combined label for the whole action.
#[cfg(any(target_os = "macos", test))]
pub(crate) fn apply_macos(
    run: &mut Run<'_>,
    cx: &EffectContext,
    suffixes: &[String],
    platform: &MacosPlatform<'_>,
    command: &mut impl FnMut(&mut Run<'_>, &EffectContext, Vec<OsString>) -> io::Result<(i32, Vec<u8>)>,
    report: &mut dyn FnMut(&str),
) -> io::Result<()> {
    let (uid, gid) = platform.identity;
    let (helper, etc, jobs, library) = (
        platform.helper,
        platform.etc,
        platform.jobs,
        platform.library,
    );
    report(&format!(
        "writing the resolver files for {}",
        suffixes.join(", ")
    ));
    let resolvers = etc
        .prepare_setup_child(cx, &FileName::new("resolver").map_err(error)?, 0o755)
        .map_err(error)?;
    for suffix in suffixes {
        let contents = format!("# {MARKER}\nnameserver 127.0.0.1\nport 15353\n");
        managed_file(run, cx, &resolvers, suffix, contents.as_bytes(), true)?;
    }
    report("installing the gateway loopback launch job");
    managed_file(
        run,
        cx,
        jobs,
        "dev.effigy.gateway.loopback.plist",
        &macos_boot_job(),
        false,
    )?;
    let helpers = library
        .prepare_setup_child(
            cx,
            &FileName::new("PrivilegedHelperTools").map_err(error)?,
            0o755,
        )
        .map_err(error)?;
    report("installing the privileged bind broker");
    broker::install(run, cx, jobs, &helpers, (uid, gid), helper, &mut *command)?;
    // Observe the real interface before adding missing aliases. Repeated setup
    // never treats a failed or unavailable observation as absence.
    report("observing the loopback interface");
    let interfaces = platform_command(
        run,
        cx,
        &mut *command,
        vec!["/sbin/ifconfig".into(), "lo0".into()],
    )?;
    let interfaces = String::from_utf8(interfaces).map_err(error)?;
    for n in 1..=50 {
        let address = format!("127.1.0.{n}");
        if interfaces.lines().any(|line| {
            line.split_whitespace().collect::<Vec<_>>().get(..2)
                == Some(&["inet", address.as_str()])
        }) {
            continue;
        }
        report(&format!("adding loopback address {address}"));
        platform_command(
            run,
            cx,
            &mut *command,
            vec![
                "/sbin/ifconfig".into(),
                "lo0".into(),
                "alias".into(),
                address.into(),
                "up".into(),
            ],
        )?;
    }
    Ok(())
}

/// The Linux platform action's targets: the root-owned unit directory and the
/// operator uid the low-port capability is installed for.
#[cfg(any(target_os = "linux", test))]
pub(crate) struct LinuxUnit<'a> {
    pub uid: u32,
    pub systemd: &'a DirFd,
}

/// The Linux platform action: the split-DNS unit and the binary-local low-port
/// capability. The unit directory, the command adapter and the capability
/// install are parameters, so a fixture substitutes its own root and a fake
/// capability and touches no unit, interface or binary attribute. `report`
/// names each operation before it starts.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn apply_linux(
    run: &mut Run<'_>,
    cx: &EffectContext,
    suffixes: &[String],
    unit: &LinuxUnit<'_>,
    command: &mut impl FnMut(&mut Run<'_>, &EffectContext, Vec<OsString>) -> io::Result<(i32, Vec<u8>)>,
    install_capability: &mut impl FnMut(u32) -> io::Result<()>,
    report: &mut dyn FnMut(&str),
) -> io::Result<()> {
    let (uid, systemd) = (unit.uid, unit.systemd);
    let name = FileName::new("effigy-gateway-dns.service").map_err(error)?;
    match read_record::<Bytes>(&cx.operation().observe().map_err(error)?, systemd, &name) {
        Observed::Verified(bytes, _) if String::from_utf8_lossy(&bytes.0).contains(MARKER) => {
            report("stopping the previous gateway DNS unit");
            platform_command(
                run,
                cx,
                &mut *command,
                vec![
                    "/usr/bin/systemctl".into(),
                    "stop".into(),
                    name.as_str().into(),
                ],
            )?;
        }
        Observed::Absent(_) => {}
        other => return Err(error(format!("preserving unknown DNS unit: {other:?}"))),
    }
    report("writing the gateway DNS unit");
    managed_file(
        run,
        cx,
        systemd,
        name.as_str(),
        &linux_boot_job(suffixes),
        false,
    )?;
    report("reloading the systemd manager");
    platform_command(
        run,
        cx,
        &mut *command,
        vec!["/usr/bin/systemctl".into(), "daemon-reload".into()],
    )?;
    report("enabling the gateway DNS unit");
    platform_command(
        run,
        cx,
        &mut *command,
        vec![
            "/usr/bin/systemctl".into(),
            "enable".into(),
            "--now".into(),
            name.as_str().into(),
        ],
    )?;
    // A binary-local capability avoids lowering a host-wide port threshold.
    // It is bound to exactly the binary setup ran, and a foreign record is
    // preserved rather than replaced.
    report(&format!("installing the low-port capability for uid {uid}"));
    install_capability(uid)
}

pub fn apply_platform(
    run: &mut Run<'_>,
    cx: &EffectContext,
    config: &GatewayConfig,
    suffixes: &[String],
    uid: u32,
    report: &mut dyn FnMut(&str),
) -> io::Result<()> {
    let mut command = |run: &mut Run<'_>, cx: &EffectContext, argv: Vec<OsString>| {
        command_outcome(run, cx, argv, false)
    };
    #[cfg(target_os = "macos")]
    {
        // The operator identity is read before anything is written, so an
        // untrusted or unauthenticated operator refusal changes nothing.
        let (operator, _) = authenticated_operator()?;
        let user = nix::unistd::User::from_uid(nix::unistd::Uid::from_raw(operator))
            .map_err(error)?
            .ok_or_else(|| error("operator account missing"))?;
        let etc = DirFd::open(Path::new("/private/etc")).map_err(error)?;
        let jobs = DirFd::open(Path::new("/Library/LaunchDaemons")).map_err(error)?;
        let library = DirFd::open(Path::new("/Library")).map_err(error)?;
        apply_macos(
            run,
            cx,
            suffixes,
            &MacosPlatform {
                identity: (operator, user.gid.as_raw()),
                helper: &std::env::current_exe()?,
                etc: &etc,
                jobs: &jobs,
                library: &library,
            },
            &mut command,
            report,
        )?;
    }
    #[cfg(target_os = "linux")]
    {
        let systemd = DirFd::open(Path::new("/etc/systemd/system")).map_err(error)?;
        let unit = LinuxUnit {
            uid,
            systemd: &systemd,
        };
        apply_linux(
            run,
            cx,
            suffixes,
            &unit,
            &mut command,
            &mut |uid| crate::linux_operator::install_current_capability(uid).map(|_| ()),
            report,
        )?;
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        return Err(error("gateway setup has no native platform adapter"));
    }
    let _ = (config, uid, command, report);
    Ok(())
}
pub fn save_receipt(
    run: &mut Run<'_>,
    cx: &EffectContext,
    home: &Path,
    suffixes: Vec<String>,
) -> io::Result<()> {
    let effigy = DirFd::open(&home.join(".effigy")).map_err(error)?;
    let dir = effigy
        .prepare_setup_child(cx, &FileName::new("gateway").map_err(error)?, 0o700)
        .map_err(error)?;
    let receipt = Receipt {
        version: 1,
        uid: nix::unistd::Uid::effective().as_raw(),
        suffixes,
    };
    run.replace_file(
        &dir,
        &FileName::new("setup.json").map_err(error)?,
        &serde_json::to_vec(&receipt).map_err(error)?,
        ReplaceOpts::default(),
    )
    .map_err(error)
}
fn remove_managed(
    run: &mut Run<'_>,
    cx: &EffectContext,
    dir: &DirFd,
    name: &str,
) -> io::Result<()> {
    remove_managed_with(cx, dir, name, |path| {
        command(run, cx, vec!["/bin/rm".into(), path.into()], false)?;
        Ok(())
    })
}
pub(crate) fn remove_managed_with(
    cx: &EffectContext,
    dir: &DirFd,
    name: &str,
    remove: impl FnOnce(&Path) -> io::Result<()>,
) -> io::Result<()> {
    match read_record::<Bytes>(
        &cx.operation().observe().map_err(error)?,
        dir,
        &FileName::new(name).map_err(error)?,
    ) {
        Observed::Absent(_) => return Ok(()),
        Observed::Verified(bytes, _) if String::from_utf8_lossy(&bytes.0).contains(MARKER) => {}
        other => return Err(error(format!("preserving foreign setup output: {other:?}"))),
    }
    // Verified directory ownership and the setup gate exclude other setup writers.
    remove(&dir.path().join(name))
}
pub fn remove_receipt(run: &mut Run<'_>, cx: &EffectContext, home: &Path) -> io::Result<()> {
    let dir = DirFd::open(&home.join(".effigy/gateway")).map_err(error)?;
    match read_record::<Receipt>(
        &cx.operation().observe().map_err(error)?,
        &dir,
        &FileName::new("setup.json").map_err(error)?,
    ) {
        Observed::Absent(_) => return Ok(()),
        Observed::Verified(receipt, _)
            if receipt.version == 1 && receipt.uid == nix::unistd::Uid::effective().as_raw() => {}
        other => {
            return Err(error(format!(
                "preserving unknown setup receipt: {other:?}"
            )))
        }
    }
    command(
        run,
        cx,
        vec![
            "/bin/rm".into(),
            home.join(".effigy/gateway/setup.json").into(),
        ],
        false,
    )?;
    Ok(())
}
/// Explicit setup teardown preserves route registries, certificates and CA
/// trust. It changes only positively identified platform output.
pub fn remove_platform(
    run: &mut Run<'_>,
    cx: &EffectContext,
    uid: u32,
    suffixes: &[String],
) -> io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        let jobs = DirFd::open(Path::new("/Library/LaunchDaemons")).map_err(error)?;
        let helpers = DirFd::open(Path::new("/Library/PrivilegedHelperTools")).map_err(error)?;
        broker::remove(run, cx, &jobs, &helpers, uid, |run, cx, argv| {
            command_outcome(run, cx, argv, false)
        })?;
        let resolvers = DirFd::open(Path::new("/private/etc/resolver")).map_err(error)?;
        for suffix in suffixes {
            remove_managed(run, cx, &resolvers, suffix)?;
        }
        let alias_job = "dev.effigy.gateway.loopback.plist";
        if broker::owned_job(cx, &jobs, alias_job)? {
            broker::stop_job(
                run,
                cx,
                "dev.effigy.gateway.loopback",
                (&jobs.path().join(alias_job), Path::new("/bin/sh")),
                &mut |run, cx, argv| command_outcome(run, cx, argv, false),
            )?;
            remove_managed(run, cx, &jobs, alias_job)?;
            // Existing aliases may predate setup. Removing the owned boot job
            // does not prove ownership of live interface addresses. Preserve
            // them for other consumers and for the restored v0.14.1 gateway.
        }
    }
    #[cfg(target_os = "linux")]
    {
        let systemd = DirFd::open(Path::new("/etc/systemd/system")).map_err(error)?;
        let name = "effigy-gateway-dns.service";
        match read_record::<Bytes>(
            &cx.operation().observe().map_err(error)?,
            &systemd,
            &FileName::new(name).map_err(error)?,
        ) {
            Observed::Verified(bytes, _) if String::from_utf8_lossy(&bytes.0).contains(MARKER) => {
                command(
                    run,
                    cx,
                    vec![
                        "/usr/bin/systemctl".into(),
                        "disable".into(),
                        "--now".into(),
                        name.into(),
                    ],
                    false,
                )?;
                remove_managed(run, cx, &systemd, name)?;
                command(
                    run,
                    cx,
                    vec!["/usr/bin/systemctl".into(), "daemon-reload".into()],
                    false,
                )?;
            }
            Observed::Absent(_) => {}
            other => return Err(error(format!("preserving foreign DNS unit: {other:?}"))),
        }
        crate::linux_operator::remove_current_capability(uid)?;
    }
    let _ = (uid, suffixes);
    Ok(())
}
/// Repair only the three explicit legacy registries and leaf-certificate
/// files after the authenticated root daemon has stopped. Never recurse or
/// change CA material. Descriptor operations preserve every input byte.
pub fn adopt_legacy_files(home: &Path, uid: u32) -> io::Result<()> {
    let op = operator::context(Duration::from_secs(5), EffectClass::Mutate);
    let cx = op.effects().map_err(error)?;
    let dir = DirFd::open_setup(&cx, home, uid).map_err(error)?;
    let Some(effigy) = dir
        .setup_child(&FileName::new(".effigy").map_err(error)?)
        .map_err(error)?
    else {
        return Ok(());
    };
    effigy
        .adopt_setup_entry(
            &cx,
            &FileName::new("ports.json").map_err(error)?,
            false,
            MAX_FILE,
        )
        .map_err(error)?;
    let Some(gateway) = effigy
        .setup_child(&FileName::new("gateway").map_err(error)?)
        .map_err(error)?
    else {
        return Ok(());
    };
    gateway
        .adopt_setup_entry(
            &cx,
            &FileName::new("routes.json").map_err(error)?,
            false,
            MAX_FILE,
        )
        .map_err(error)?;
    let certs_name = FileName::new("certs").map_err(error)?;
    if let Some(certs) = gateway.setup_child(&certs_name).map_err(error)? {
        for name in certs.setup_entries(4096).map_err(error)? {
            if Path::new(&name).extension().is_some_and(|ext| ext == "pem") {
                let name = FileName::new(
                    name.to_str()
                        .ok_or_else(|| error("invalid certificate file name"))?,
                )
                .map_err(error)?;
                certs
                    .adopt_setup_entry(&cx, &name, false, MAX_FILE)
                    .map_err(error)?;
            }
        }
        gateway
            .adopt_setup_entry(&cx, &certs_name, true, MAX_FILE)
            .map_err(error)?;
    }
    effigy
        .adopt_setup_entry(
            &cx,
            &FileName::new("gateway").map_err(error)?,
            true,
            MAX_FILE,
        )
        .map_err(error)?;
    dir.adopt_setup_entry(
        &cx,
        &FileName::new(".effigy").map_err(error)?,
        true,
        MAX_FILE,
    )
    .map_err(error)?;
    Ok(())
}

/// v0.14.1 root migration only. The caller is the authenticated setup child;
/// the injected stop uses the existing exact-generation privileged stopper.
pub fn migrate_root(
    config: &GatewayConfig,
    uid: u32,
    stop: impl FnOnce(&crate::identity::GatewayRecordSnapshot) -> io::Result<()>,
) -> io::Result<()> {
    migrate_root_with(
        config,
        uid,
        |snapshot| {
            inspect_migration_record(
                snapshot,
                crate::identity::probe_live_identity,
                crate::server::probe_gateway_process,
                |pid| crate::identity::read_live_process_identity(pid).map(|live| live.uid),
            )
        },
        stop,
    )
}
/// Classify one persisted record for migration.
///
/// The kernel's word on the PID comes first: a process confirmed absent is an
/// absent record whatever its identity sidecar says, because a stopped daemon
/// leaves both files behind and a dead PID never matches its recorded
/// generation. Everything else that cannot be proven matched stays unknown, so
/// setup signals no process it cannot authenticate.
fn inspect_migration_record(
    snapshot: &crate::identity::GatewayRecordSnapshot,
    identity: impl FnOnce(
        &crate::identity::GatewayIdentityRecord,
    ) -> crate::identity::GatewayIdentityProbe,
    process: impl FnOnce(u32) -> crate::server::GatewayProcessProbe,
    live_uid: impl FnOnce(u32) -> io::Result<u32>,
) -> io::Result<Option<u32>> {
    if process(snapshot.pid()).is_confirmed_absent() {
        return Ok(None);
    }
    let record = snapshot
        .record()
        .ok_or_else(|| error("setup requires a v0.14.1 identity record"))?;
    match identity(record) {
        crate::identity::GatewayIdentityProbe::Matched => Ok(Some(live_uid(snapshot.pid())?)),
        _ => Err(error(
            "gateway identity is unknown; setup made no process change",
        )),
    }
}
/// The recorded version proves a v0.14.1 gateway left this record behind.
fn recorded_v0141(config: &GatewayConfig) -> io::Result<bool> {
    let version = crate::identity::read_gateway_version(&config.pid_file_path).map_err(error)?;
    Ok(version
        .as_deref()
        .is_some_and(|v| v.starts_with("0.14.1") || v.starts_with("v0.14.1")))
}
fn migrate_root_with(
    config: &GatewayConfig,
    uid: u32,
    inspect: impl FnOnce(&crate::identity::GatewayRecordSnapshot) -> io::Result<Option<u32>>,
    stop: impl FnOnce(&crate::identity::GatewayRecordSnapshot) -> io::Result<()>,
) -> io::Result<()> {
    // A gateway directory absent for *the operator*, not merely for this
    // caller, holds no legacy state to classify.
    let snapshot = if gateway_directory_verified_absent(config, uid)? {
        None
    } else {
        read_migration_record(config)?
    };
    if let Some(snapshot) = snapshot {
        match inspect(&snapshot)? {
            Some(0) => {
                if !recorded_v0141(config)? {
                    return Err(error(
                        "setup only migrates an authenticated v0.14.1 root gateway",
                    ));
                }
                stop(&snapshot)?;
            }
            // A v0.14.1 gateway the operator started holds the same legacy
            // files the root one does, so it is stopped the same way. A
            // current-generation operator gateway stays untouched.
            Some(owner) if owner == uid => {
                if recorded_v0141(config)? {
                    stop(&snapshot)?;
                }
            }
            Some(_) => return Err(error("preserving a foreign gateway process")),
            None => {
                crate::identity::remove_if_unchanged(&snapshot).map_err(error)?;
            }
        }
    }
    adopt_legacy_files(
        config
            .route_table_path
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
            .ok_or_else(|| error("invalid gateway home"))?,
        uid,
    )
}

/// Whether the gateway directory is verified absent *for the operator*.
///
/// Only a `NotFound` stat of that directory is absence; an unreadable path and
/// the existing record rules still refuse. The stat alone is not the whole
/// answer: the elevated setup child runs as root, and root can stat a path its
/// owner cannot read, so setup also proves the operator's own ownership and
/// read/search access on the path down to the missing directory. Setup must not
/// change a machine's platform while the state it skipped is merely unreadable
/// by the account that will use it.
fn gateway_directory_verified_absent(config: &GatewayConfig, uid: u32) -> io::Result<bool> {
    let gateway = config
        .pid_file_path
        .parent()
        .ok_or_else(|| error("invalid gateway directory"))?;
    match std::fs::symlink_metadata(gateway) {
        Ok(_) => return Ok(false),
        Err(cause) if cause.kind() == io::ErrorKind::NotFound => {}
        Err(cause) => return Err(unreadable_state(gateway, &cause)),
    }
    operator_owns_state_path(config, uid)?;
    Ok(true)
}

/// Prove the operator can itself read and search the state path below its own
/// home: the operator home and, when it exists, the `.effigy` directory.
///
/// This reads the recorded metadata instead of this caller's own access, so the
/// same answer holds inside the elevated root child. A `.effigy` owned by root
/// or another account is a refusal here even though root can enter it and the
/// operator cannot: that is unclassifiable state, not an empty home. The
/// v0.14.1 legacy path never reaches this proof, because it has a gateway
/// directory and migration reads that state instead.
fn operator_owns_state_path(config: &GatewayConfig, uid: u32) -> io::Result<()> {
    use std::os::unix::fs::MetadataExt;
    let gateway = config
        .pid_file_path
        .parent()
        .ok_or_else(|| error("invalid gateway directory"))?;
    let state = gateway
        .parent()
        .ok_or_else(|| error("invalid gateway directory"))?;
    let home = state
        .parent()
        .ok_or_else(|| error("invalid gateway directory"))?;
    for path in [home, state] {
        match std::fs::symlink_metadata(path) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink()
                    || !metadata.is_dir()
                    || metadata.uid() != uid
                    || metadata.mode() & 0o500 != 0o500
                {
                    return Err(error(format!(
                        "gateway setup will not treat {} as absent gateway state: the operator cannot read and search {}. Make it the operator's own directory with owner read and search, then run `effigy gateway setup` again",
                        gateway.display(),
                        path.display()
                    )));
                }
            }
            Err(cause) if cause.kind() == io::ErrorKind::NotFound => continue,
            Err(cause) => return Err(unreadable_state(path, &cause)),
        }
    }
    Ok(())
}

/// The refusal for a state path this caller cannot read: it names the path and
/// the repair, because the operator is the one who can act on it.
fn unreadable_state(path: &Path, cause: impl std::fmt::Display) -> io::Error {
    error(format!(
        "gateway setup cannot read the gateway state at {}: {cause}; make the operator the owner of that directory and grant it owner read, write and search access, then run `effigy gateway setup` again",
        path.display()
    ))
}

/// The operator's persisted record, or none.
///
/// A state directory this caller cannot read is not absence. It refuses with
/// the path and the repair, because the operator is the one who can act on it.
fn read_migration_record(
    config: &GatewayConfig,
) -> io::Result<Option<crate::identity::GatewayRecordSnapshot>> {
    match crate::identity::read_snapshot(&config.pid_file_path) {
        Ok(record) => Ok(record),
        Err(cause) => {
            let unreadable = matches!(
                &cause,
                crate::GatewayError::Io(io_error) if io_error.kind() == io::ErrorKind::PermissionDenied
            );
            if !unreadable {
                return Err(error(cause.to_string()));
            }
            let gateway = config
                .pid_file_path
                .parent()
                .unwrap_or(&config.pid_file_path);
            Err(unreadable_state(gateway, &cause))
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// One phase line is one internal line: the schema marks it as ours, and a
    /// line for anything else is never read as a phase.
    #[test]
    fn a_platform_phase_line_round_trips_and_nothing_else_does() {
        let line = platform_phase_line("installing the privileged bind broker");
        assert_eq!(
            platform_phase_from_line(&line).as_deref(),
            Some("installing the privileged bind broker")
        );
        // A child's own diagnostic, a foreign schema, a missing phase and a
        // line carrying more than this channel's two fields are all not ours.
        for other in [
            "the resolver directory is not writable",
            "{\"schema\":\"something.else.v1\",\"phase\":\"writing\"}",
            "{\"schema\":\"effigy.gateway.setup_phase.v1\"}",
            "{\"schema\":\"effigy.gateway.setup_phase.v1\",\"phase\":\"writing\",\"uid\":0}",
            "not json at all",
            "",
        ] {
            assert_eq!(platform_phase_from_line(other), None, "{other}");
        }
    }

    /// The child's stdout can split a line anywhere: complete lines are
    /// reported as they arrive and the partial tail waits for its newline.
    #[test]
    fn reported_phase_lines_survive_a_split_stream() {
        let mut buffer = Vec::new();
        let mut phases = Vec::new();
        let first = platform_phase_line("writing the gateway DNS unit");
        let second = platform_phase_line("reloading the systemd manager");
        let (head, tail) = first.split_at(first.len() / 2);
        buffer.extend_from_slice(head.as_bytes());
        report_phase_lines(&mut buffer, &mut |phase: &str| {
            phases.push(phase.to_owned())
        });
        assert!(phases.is_empty(), "a partial line is not reported");
        buffer.extend_from_slice(tail.as_bytes());
        buffer.push(b'\n');
        buffer.extend_from_slice(second.as_bytes());
        buffer.push(b'\n');
        buffer.extend_from_slice(b"the unit was refused\n");
        buffer.extend_from_slice(second.as_bytes());
        report_phase_lines(&mut buffer, &mut |phase: &str| {
            phases.push(phase.to_owned())
        });
        assert_eq!(
            phases,
            vec![
                "writing the gateway DNS unit".to_owned(),
                "reloading the systemd manager".to_owned(),
            ]
        );
        // The trailing partial line is kept for the next read.
        assert_eq!(buffer, second.as_bytes());
    }

    /// The channel's own lines are removed from a refusal, so the operator
    /// still reads the child's diagnostic.
    #[test]
    fn a_refusal_loses_the_phase_lines_and_keeps_the_child_diagnostic() {
        let mut output = platform_phase_line("observing the loopback interface").into_bytes();
        output.push(b'\n');
        output.extend_from_slice(b"ifconfig failed: operation not permitted\n");
        let kept = without_phase_lines(&output);
        assert_eq!(
            String::from_utf8_lossy(&kept),
            "ifconfig failed: operation not permitted\n"
        );
    }

    pub(crate) fn with_run(work: impl FnOnce(&mut Run<'_>, &EffectContext, &Path)) {
        let home = tempfile::tempdir().unwrap();
        let op = operator::context(Duration::from_secs(10), EffectClass::Mutate);
        let cx = op.effects().unwrap();
        let root = StateRoot::runtime_machine_effects(&cx, home.path()).unwrap();
        let mut adapter = operator::Adapter;
        let mut run = Run::begin(
            &cx,
            &root,
            &operator::invocation(setup_owner()),
            BeginOpts {
                id: operator::run_id().unwrap(),
                wait: Wait::FailFast,
                acknowledge: vec![],
                next: operator::next(),
                custody: None,
            },
            &op.cleanup(Duration::from_secs(5)).unwrap(),
            &mut adapter,
        )
        .unwrap();
        work(&mut run, &cx, home.path());
        run.settle(&op.cleanup(Duration::from_secs(5)).unwrap(), &mut adapter)
            .unwrap();
    }
    #[test]
    fn unresolved_cleanup_names_the_effect_and_the_custody_reason() {
        // The settle can stop on its last open effect (effigy#476), so setup
        // reports the settled outcome itself instead of a bare unresolved
        // cleanup: the effect kind, the label its intent recorded and the
        // kernel's own custody reason. The recorded reason is already a
        // rendered observation, so it must not be wrapped a second time.
        let recorded = "could not tell: the evidence contradicts itself (group anchor unavailable); retry, and if it persists run `effigy doctor`";
        let mut settled = effigy_kernel::outcome::Outcome::new(
            Err(effigy_kernel::outcome::ErrorInfo {
                code: effigy_kernel::outcome::ErrorCode::Incomplete,
                message: "cleanup has unresolved effects".into(),
                details: serde_json::Value::Null,
            }),
            WorkloadOutcome::Succeeded,
            effigy_kernel::outcome::CleanupOutcome::Incomplete,
            &[],
        )
        .unwrap();
        let id = || ResourceId::store(Scope::Machine, "gateway-setup").unwrap();
        for sequence in [5, 7] {
            settled.add_residue(effigy_kernel::outcome::Residue::unresolved(
                sequence,
                "kernel:process",
                id(),
                Some("gateway setup adapter: sudo"),
                recorded,
                operator::next(),
            ));
        }
        let message = unresolved_cleanup(&settled).to_string();
        assert!(message.contains("`kernel:process`"), "{message}");
        assert!(message.contains("gateway setup adapter: sudo"), "{message}");
        assert!(message.contains("group anchor unavailable"), "{message}");
        // Otherwise identical effects are told apart by their own sequence.
        assert!(message.contains("effect 5"), "{message}");
        assert!(message.contains("effect 7"), "{message}");
        assert_eq!(
            message.matches("the evidence contradicts itself").count(),
            2,
            "a recorded reason is not wrapped twice: {message}"
        );
    }

    /// Every gateway setup command shares the adapter label, so the label names
    /// the program it runs: an unresolved effect is then attributable without
    /// journaling argv (effigy#476).
    #[test]
    fn adapter_labels_name_the_command_and_stay_distinct() {
        let labels: Vec<String> = [
            "/opt/homebrew/bin/mkcert",
            "/usr/bin/sudo",
            "/home/operator/.local/bin/effigy",
        ]
        .into_iter()
        .map(|program| {
            adapter_label(&[
                program.into(),
                "--".into(),
                "gateway".into(),
                "setup".into(),
            ])
        })
        .collect();
        assert_eq!(
            labels,
            [
                "gateway setup adapter: mkcert",
                "gateway setup adapter: sudo",
                "gateway setup adapter: effigy"
            ]
        );
        // No path, no argv and no environment value reaches the label.
        assert!(labels.iter().all(|label| !label.contains('/')));
        assert_eq!(
            adapter_label(&[]),
            "gateway setup adapter",
            "an argv-less spec keeps the generic label instead of inventing one"
        );
    }

    #[test]
    #[ignore = "proof: needs the kernel custody fixture (EFFIGY_KERNEL_CUSTODY_FIXTURE)"]
    fn setup_sequence_runs_two_foreground_commands_in_one_run() {
        // `effigy gateway setup` runs `mkcert -install` and then
        // `sudo -H -- effigy gateway setup`, both as `Terminal::Foreground`
        // children of one run. A first lease that outlives its child leaves the
        // owner in the background, and the second `tcsetpgrp` then stops the
        // whole setup with SIGTTOU.
        //
        // `command` hands the operator environment to its children, so the
        // nonce of the custody process running this proof (a selector runs it
        // inside Effigy) must not leak into the fakes.
        let _env = crate::env_test_support::EnvGuard::set_many(&[
            ("EFFIGY_CUSTODY_NONCE", None),
            ("EFFIGY_PROBE_NONCE", None),
        ]);
        let home = tempfile::tempdir().unwrap();
        let calls = home.path().join("calls");
        let fake = |name: &str| {
            let path = home.path().join(name);
            std::fs::write(
                &path,
                format!(
                    "#!/bin/sh\nprintf '%s\\n' {name}\nprintf '{name} %s\\n' \"$*\" >> {}\n",
                    calls.display()
                ),
            )
            .unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path
        };
        let mkcert = fake("mkcert");
        let sudo = fake("sudo");
        let executable = std::env::var_os("EFFIGY_KERNEL_CUSTODY_FIXTURE")
            .map(PathBuf::from)
            .expect("EFFIGY_KERNEL_CUSTODY_FIXTURE: the kernel custody fixture");
        let op = operator::context(Duration::from_secs(30), EffectClass::Mutate);
        let cx = op.effects().unwrap();
        let cleanup = op.cleanup(Duration::from_secs(10)).unwrap();
        let root = StateRoot::runtime_machine_effects(&cx, home.path()).unwrap();
        let mut adapter = operator::Adapter;
        let mut run = Run::begin(
            &cx,
            &root,
            &operator::invocation(setup_owner()),
            BeginOpts {
                id: operator::run_id().unwrap(),
                wait: Wait::FailFast,
                acknowledge: vec![],
                // The qualification sentinel is the kernel custody fixture,
                // whose recovery entry takes this one fixed invocation.
                next: effigy_kernel::outcome::Invocation::validated(
                    vec!["recover".into()],
                    |args| args == ["recover"],
                )
                .unwrap(),
                custody: Some(LifecyclePlan {
                    executable: Executable::capture(&executable).unwrap(),
                    placement: Placement::Inline,
                }),
            },
            &cleanup,
            &mut adapter,
        )
        .unwrap();
        let installed = command(
            &mut run,
            &cx,
            vec![mkcert.clone().into_os_string(), "-install".into()],
            true,
        )
        .unwrap();
        let elevated = command(
            &mut run,
            &cx,
            vec![
                sudo.into_os_string(),
                "-H".into(),
                "--".into(),
                mkcert.into_os_string(),
                "gateway".into(),
                "setup".into(),
            ],
            true,
        )
        .unwrap();
        run.finish(WorkloadOutcome::Succeeded).unwrap();
        let settled = run.settle(&cleanup, &mut adapter).unwrap();
        assert_eq!(
            settled.cleanup(),
            effigy_kernel::outcome::CleanupOutcome::Complete
        );
        assert_eq!(String::from_utf8_lossy(&installed), "mkcert\n");
        assert_eq!(String::from_utf8_lossy(&elevated), "sudo\n");
        let calls = std::fs::read_to_string(&calls).unwrap();
        let lines: Vec<&str> = calls.lines().collect();
        assert_eq!(lines.len(), 2, "{calls:?}");
        assert_eq!(lines.first(), Some(&"mkcert -install"));
        assert!(
            lines
                .last()
                .is_some_and(|line| line.starts_with("sudo -H -- ")
                    && line.ends_with("/mkcert gateway setup")),
            "{calls:?}"
        );
    }

    #[test]
    fn managed_setup_output_preserves_foreign_files_and_is_idempotent() {
        with_run(|run, cx, home| {
            let dir = DirFd::open(home).unwrap();
            let path = home.join("resolver");
            std::fs::write(&path, b"foreign resolver").unwrap();
            let owned = format!("# {MARKER}\nnameserver 127.0.0.1\nport 15353\n");
            assert!(managed_file(run, cx, &dir, "resolver", owned.as_bytes(), true).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), b"foreign resolver");
            std::fs::remove_file(&path).unwrap();
            managed_file(run, cx, &dir, "resolver", owned.as_bytes(), true).unwrap();
            let count = run.effects().unwrap().len();
            managed_file(run, cx, &dir, "resolver", owned.as_bytes(), true).unwrap();
            assert_eq!(run.effects().unwrap().len(), count);
        });
    }
    #[test]
    fn teardown_removes_only_verified_outputs_and_never_follows_a_link() {
        with_run(|_run, cx, home| {
            let dir = DirFd::open(home).unwrap();
            let owned = home.join("test");
            let foreign = home.join("foreign");
            std::fs::write(&owned, format!("# {MARKER}\nnameserver 127.0.0.1\n")).unwrap();
            std::fs::write(&foreign, b"foreign resolver").unwrap();
            std::os::unix::fs::symlink(&foreign, home.join("linked")).unwrap();
            let removals = std::cell::RefCell::new(Vec::new());
            let remove = |path: &Path| {
                removals.borrow_mut().push(path.to_owned());
                std::fs::remove_file(path)
            };
            remove_managed_with(cx, &dir, "test", remove).unwrap();
            remove_managed_with(cx, &dir, "test", remove).unwrap();
            assert!(remove_managed_with(cx, &dir, "foreign", remove).is_err());
            assert!(remove_managed_with(cx, &dir, "linked", remove).is_err());
            assert_eq!(*removals.borrow(), vec![dir.path().join("test")]);
            assert_eq!(std::fs::read(foreign).unwrap(), b"foreign resolver");
        });
    }
    #[test]
    fn privileged_suffix_inventory_uses_legacy_files_and_receipt_not_kernel_store() {
        with_run(|_run, cx, home| {
            let operator_home = home.join("operator");
            let gateway = operator_home.join(".effigy/gateway");
            std::fs::create_dir_all(&gateway).unwrap();
            let state = operator_home.join(".effigy/state");
            std::fs::create_dir_all(&state).unwrap();
            for name in ["kernel.db", "machine.db"] {
                std::fs::write(
                    state.join(name),
                    b"must never be opened by privileged setup",
                )
                .unwrap();
            }
            std::fs::write(gateway.join("routes.json"), br#"{"routes":{"dev.cumberland.co.uk":{"domain":"dev.cumberland.co.uk","target":"127.0.0.1:8100","project":"/fixture/site","source":"manual","registered":"2026-01-01T00:00:00Z"}}}"#).unwrap();
            let uid = nix::unistd::Uid::effective().as_raw();
            std::fs::write(
                gateway.join("setup.json"),
                serde_json::to_vec(&Receipt {
                    version: 1,
                    uid,
                    suffixes: vec!["previous.example".into()],
                })
                .unwrap(),
            )
            .unwrap();
            let suffixes =
                platform_suffixes(cx, uid, &operator_home, &["added.example".into()]).unwrap();
            for expected in [
                "test",
                "dev.cumberland.co.uk",
                "previous.example",
                "added.example",
            ] {
                assert!(suffixes.contains(&expected.to_owned()));
            }
            assert!(platform_suffixes(cx, uid, &operator_home, &["../invalid".into()]).is_err());
        });
    }
    #[test]
    fn setup_symlink_is_refused_without_changing_its_target() {
        with_run(|run, cx, home| {
            let target = home.join("foreign");
            std::fs::write(&target, b"unchanged").unwrap();
            std::os::unix::fs::symlink(&target, home.join("resolver")).unwrap();
            assert!(managed_file(
                run,
                cx,
                &DirFd::open(home).unwrap(),
                "resolver",
                b"replacement",
                true
            )
            .is_err());
            assert_eq!(std::fs::read(target).unwrap(), b"unchanged");
        });
    }
    #[test]
    fn setup_receipt_approves_only_label_aligned_suffixes() {
        with_run(|run, cx, home| {
            save_receipt(
                run,
                cx,
                home,
                vec!["test".into(), "dev.cumberland.co.uk".into()],
            )
            .unwrap();
            assert!(require_domain(home, "mysql.cbs.legacy.test").is_ok());
            assert!(require_domain(home, "postgres.acowtancy.test").is_ok());
            assert!(require_domain(home, "dev.cumberland.co.uk").is_ok());
            assert!(require_domain(home, "foreigncumberland.co.uk").is_err());
            assert!(require_domain(home, "eviltest").is_err());
        });
    }
    #[test]
    fn root_gateway_migration_stops_the_legacy_daemon_and_leaves_its_files_alone() {
        let home = tempfile::tempdir().unwrap();
        let gateway = home.path().join(".effigy/gateway");
        std::fs::create_dir_all(&gateway).unwrap();
        let config = GatewayConfig::standard(gateway.clone());
        let fixtures=[(gateway.join("routes.json"),br#"{"_managed_by":"effigy-gateway-route-table-v1","routes":{"cbs.legacy.test":{"domain":"cbs.legacy.test","target":"127.0.0.1:8100","source":"manual","project":"/redacted/site","tls":true,"registered":"2026-01-01T00:00:00Z"},"ui.acowtancy.test":{"domain":"ui.acowtancy.test","target":"127.0.0.1:8300","source":"manual","project":"/redacted/ui","registered":"2026-01-01T00:00:00Z"}}}"#.as_slice()),(home.path().join(".effigy/ports.json"),br#"{"allocations":{"site":{"base":8100,"range":100,"project":"/redacted/site","assigned_ports":{"mysql:3306":8106}}}}"#.as_slice())];
        for (path, bytes) in &fixtures {
            std::fs::write(path, bytes).unwrap();
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        let uid = nix::unistd::Uid::effective().as_raw();
        crate::identity::publish_current_gateway(&config.pid_file_path, uid).unwrap();
        crate::identity::publish_gateway_version(&config.pid_file_path, "0.14.1").unwrap();
        let stops = std::cell::Cell::new(0);
        assert!(migrate_root_with(
            &config,
            uid,
            |_| Err(error("unknown identity")),
            |_| {
                stops.set(stops.get() + 1);
                Ok(())
            }
        )
        .is_err());
        assert_eq!(stops.get(), 0);
        migrate_root_with(
            &config,
            uid,
            |_| Ok(Some(0)),
            |snapshot| {
                stops.set(stops.get() + 1);
                crate::identity::remove_if_unchanged(snapshot).map_err(error)?;
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(stops.get(), 1);
        for (path, bytes) in &fixtures {
            assert_eq!(std::fs::read(path).unwrap(), *bytes);
        }
        // The endpoint Store is the only route authority; the legacy files are
        // left exactly as the old daemon wrote them.
        assert!(crate::routes::RouteTable::load(&config.route_table_path)
            .unwrap()
            .is_empty());
        assert!(!config.pid_file_path.exists());
    }
    #[test]
    fn native_boot_jobs_limit_privilege_and_preserve_global_dns() {
        let mac = String::from_utf8(macos_boot_job()).unwrap();
        assert_eq!(mac.matches("/sbin/ifconfig lo0 alias").count(), 50);
        assert!(!mac.contains("__gateway-run"));
        assert!(!mac.contains("KeepAlive"));
        let linux = String::from_utf8(linux_boot_job(&[
            "test".into(),
            "dev.cumberland.co.uk".into(),
        ]))
        .unwrap();
        assert!(linux.contains("domain effigy-gw ~test ~dev.cumberland.co.uk"));
        assert!(linux.contains("default-route effigy-gw no"));
        assert!(linux.contains("ExecStop=/usr/sbin/ip link delete effigy-gw"));
        assert!(!linux.contains("resolved.conf"));
        assert!(!linux.contains("resolv.conf"));
    }
    #[test]
    #[ignore = "requires explicit redacted operator snapshot; run proof:gateway:operator:migration"]
    fn root_gateway_migration_preserves_redacted_operator_snapshot() {
        let source = PathBuf::from(
            std::env::var_os("EFFIGY_GATEWAY_MIGRATION_SNAPSHOT").expect("explicit snapshot path"),
        );
        let home = tempfile::tempdir().unwrap();
        let gateway = home.path().join(".effigy/gateway");
        std::fs::create_dir_all(&gateway).unwrap();
        let config = GatewayConfig::standard(gateway.clone());
        let mut originals = Vec::new();
        for (name, target) in [
            ("routes.json", gateway.join("routes.json")),
            ("ports.json", home.path().join(".effigy/ports.json")),
        ] {
            let bytes = std::fs::read(source.join(name)).unwrap();
            std::fs::write(&target, &bytes).unwrap();
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
            originals.push((target, bytes));
        }
        let before = crate::routes::RouteTable::load(&config.route_table_path)
            .unwrap()
            .len();
        assert!(before > 0, "snapshot must contain real route mappings");
        let uid = nix::unistd::Uid::effective().as_raw();
        crate::identity::publish_current_gateway(&config.pid_file_path, uid).unwrap();
        crate::identity::publish_gateway_version(&config.pid_file_path, "0.14.1").unwrap();
        migrate_root_with(
            &config,
            uid,
            |_| Ok(Some(0)),
            |snapshot| {
                crate::identity::remove_if_unchanged(snapshot).map_err(error)?;
                Ok(())
            },
        )
        .unwrap();
        for (path, bytes) in originals {
            assert_eq!(std::fs::read(path).unwrap(), bytes);
        }
        assert_eq!(
            crate::routes::RouteTable::load(&config.route_table_path)
                .unwrap()
                .len(),
            before
        );
        println!("preserved {before} redacted operator routes and every allocation byte");
    }

    /// An empty operator gateway home, as the migration finds it.
    fn gateway_fixture() -> (tempfile::TempDir, GatewayConfig) {
        let home = tempfile::tempdir().unwrap();
        let gateway = home.path().join(".effigy/gateway");
        std::fs::create_dir_all(&gateway).unwrap();
        let config = GatewayConfig::standard(gateway);
        (home, config)
    }

    /// A PID the kernel agreed is gone: a reaped child, never this process.
    fn confirmed_absent_pid() -> u32 {
        let mut child = std::process::Command::new("/bin/sh")
            .args(["-c", "exit 0"])
            .spawn()
            .unwrap();
        let pid = child.id();
        assert!(child.wait().unwrap().success());
        let deadline = std::time::Instant::now() + Duration::from_secs(60);
        while !crate::server::probe_gateway_process(pid).is_confirmed_absent() {
            assert!(
                std::time::Instant::now() < deadline,
                "reaped child {pid} is still reported live"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        pid
    }

    #[test]
    fn migration_stops_an_operator_owned_v0141_gateway_and_leaves_a_current_one_alone() {
        // A v0.14.1 gateway the operator started holds the same legacy files
        // the root one does. Migration used to treat a matching operator owner
        // as nothing to do, so setup needed those files moved aside by hand
        // (Tom, 2026-10-22). A current-generation operator gateway stays a
        // no-op.
        let (_home, config) = gateway_fixture();
        let uid = nix::unistd::Uid::effective().as_raw();
        crate::identity::write_test_record(&config.pid_file_path, std::process::id());
        crate::identity::publish_gateway_version(&config.pid_file_path, "0.15.0").unwrap();
        let stops = std::cell::Cell::new(0);
        migrate_root_with(
            &config,
            uid,
            |_| Ok(Some(uid)),
            |_| {
                stops.set(stops.get() + 1);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(stops.get(), 0, "a current operator gateway stays a no-op");
        assert!(config.pid_file_path.exists());

        crate::identity::publish_gateway_version(&config.pid_file_path, "0.14.1").unwrap();
        migrate_root_with(
            &config,
            uid,
            |_| Ok(Some(uid)),
            |snapshot| {
                stops.set(stops.get() + 1);
                crate::identity::remove_if_unchanged(snapshot).map_err(error)?;
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(stops.get(), 1);
        assert!(!config.pid_file_path.exists());
    }

    #[test]
    fn migration_removes_a_record_whose_pid_is_confirmed_absent() {
        // A stopped gateway leaves its record pair behind. The dead PID's
        // identity can never match, so the record used to read as unknown and
        // setup made no process change (Tom, 2026-10-22).
        let (_home, config) = gateway_fixture();
        let uid = nix::unistd::Uid::effective().as_raw();
        crate::identity::write_test_record(&config.pid_file_path, confirmed_absent_pid());
        crate::identity::publish_gateway_version(&config.pid_file_path, "0.14.1").unwrap();
        let routes = br#"{"_managed_by":"effigy-gateway-route-table-v1","routes":{"cbs.legacy.test":{"domain":"cbs.legacy.test","target":"127.0.0.1:8100","source":"manual","project":"/redacted/site","registered":"2026-01-01T00:00:00Z"}}}"#;
        std::fs::write(config.route_table_path.clone(), routes.as_slice()).unwrap();
        std::fs::set_permissions(
            &config.route_table_path,
            std::fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        migrate_root(&config, uid, |_| {
            panic!("migration must never signal a confirmed-absent PID")
        })
        .unwrap();
        assert!(!config.pid_file_path.exists());
        assert!(!config.pid_file_path.with_extension("identity").exists());
        assert!(!config.pid_file_path.with_extension("version").exists());
        assert_eq!(std::fs::read(&config.route_table_path).unwrap(), routes);
    }

    #[test]
    fn migration_never_signals_a_pid_whose_identity_is_unmatched() {
        let (_home, config) = gateway_fixture();
        crate::identity::write_test_record(&config.pid_file_path, std::process::id());
        let snapshot = crate::identity::read_snapshot(&config.pid_file_path)
            .unwrap()
            .unwrap();
        let unmatched = [
            crate::identity::GatewayIdentityProbe::Mismatch,
            crate::identity::GatewayIdentityProbe::PermissionDenied,
            crate::identity::GatewayIdentityProbe::Unknown,
        ];
        for identity in unmatched {
            // A live PID whose recorded generation does not match is neither
            // absent nor owned: setup refuses before any process change.
            assert!(inspect_migration_record(
                &snapshot,
                |_| identity,
                |_| crate::server::GatewayProcessProbe::Running,
                |_| panic!("an unmatched PID must not be resolved"),
            )
            .is_err());
            // The kernel's confirmed absence wins whatever the identity probe
            // says; only then is the stale record removed.
            assert_eq!(
                inspect_migration_record(
                    &snapshot,
                    |_| identity,
                    |_| crate::server::GatewayProcessProbe::ConfirmedAbsent,
                    |_| panic!("an absent PID must not be resolved"),
                )
                .unwrap(),
                None
            );
        }
        // A matched generation is the live process whatever the `ps` probe
        // says, and a matched live generation reports its kernel owner.
        assert_eq!(
            inspect_migration_record(
                &snapshot,
                |_| crate::identity::GatewayIdentityProbe::Matched,
                |_| crate::server::GatewayProcessProbe::Unknown,
                |_| Ok(0),
            )
            .unwrap(),
            Some(0)
        );
        assert_eq!(
            inspect_migration_record(
                &snapshot,
                |_| crate::identity::GatewayIdentityProbe::Matched,
                |_| crate::server::GatewayProcessProbe::Running,
                |_| Ok(501),
            )
            .unwrap(),
            Some(501)
        );
    }

    #[test]
    fn staged_executable_bound_accepts_a_dev_build() {
        // Before privilege escalation setup reads the running binary. A debug
        // build of `effigy` is far past the old 128 MiB bound, and refusing it
        // made `effigy gateway setup` fail on a developer machine (#392).
        assert!(staged_executable_fits(300 << 20));
        assert!(staged_executable_fits(MAX_STAGED_EXECUTABLE));
        assert!(!staged_executable_fits(MAX_STAGED_EXECUTABLE + 1));
        // A refusal past the bound names the bound and the remedy, because the
        // operator can only act on a release build (#473).
        let refusal = staged_executable_refusal(MAX_STAGED_EXECUTABLE + 1);
        assert!(refusal.contains("512 MiB"), "{refusal}");
        assert!(refusal.contains("release"), "{refusal}");
        assert!(refusal.contains("effigy install:local"), "{refusal}");
        assert!(
            refusal.contains("`effigy gateway setup` again"),
            "{refusal}"
        );
    }

    /// A completely empty operator account home: the account exists and Effigy
    /// has never run there. Nothing below it exists, not even `.effigy`.
    fn empty_operator_home(parent: &Path) -> (PathBuf, GatewayConfig) {
        let home = parent.join("operator-home");
        std::fs::create_dir(&home).unwrap();
        let config = GatewayConfig::standard(home.join(".effigy/gateway"));
        (home, config)
    }

    /// The platform roots and the closed command adapter a fixture substitutes
    /// for the machine's own. Every file the platform action writes lands in
    /// this temporary tree and every command it would run is recorded.
    struct FakePlatform {
        root: tempfile::TempDir,
        calls: std::cell::RefCell<Vec<Vec<String>>>,
    }

    impl FakePlatform {
        fn new(parent: &Path) -> Self {
            Self {
                root: tempfile::Builder::new()
                    .prefix("platform-")
                    .tempdir_in(parent)
                    .unwrap(),
                calls: std::cell::RefCell::new(Vec::new()),
            }
        }
        fn dir(&self, name: &str) -> DirFd {
            let path = self.root.path().join(name);
            std::fs::create_dir_all(&path).unwrap();
            DirFd::open(&path).unwrap()
        }
        fn path(&self, name: &str) -> PathBuf {
            self.root.path().join(name)
        }
        fn run(
            &self,
            _run: &mut Run<'_>,
            _cx: &EffectContext,
            argv: Vec<OsString>,
        ) -> io::Result<(i32, Vec<u8>)> {
            self.calls.borrow_mut().push(
                argv.iter()
                    .map(|arg| arg.to_string_lossy().into_owned())
                    .collect(),
            );
            // `ifconfig lo0` observes an interface with no aliases yet.
            Ok((
                0,
                b"lo0: flags=8049<UP,LOOPBACK,RUNNING>\n\tinet 127.0.0.1 netmask 0xff000000\n"
                    .to_vec(),
            ))
        }
        fn requested(&self, program: &str) -> Vec<Vec<String>> {
            self.calls
                .borrow()
                .iter()
                .filter(|argv| argv.first().map(String::as_str) == Some(program))
                .cloned()
                .collect()
        }
    }

    /// A first-ever setup on macOS: the operator home has never held Effigy
    /// state, so migration classifies no legacy state and the platform action
    /// runs on the substituted roots (#473).
    #[test]
    fn an_empty_operator_home_reaches_the_macos_platform_action() {
        let root = tempfile::tempdir().unwrap();
        let (operator_home, config) = empty_operator_home(root.path());
        let uid = nix::unistd::Uid::effective().as_raw();
        migrate_root(&config, uid, |_| {
            panic!("a first-ever home records no gateway process")
        })
        .expect("an empty operator home is no legacy state to migrate");
        assert!(
            !operator_home.join(".effigy").exists(),
            "classifying no legacy state creates no gateway state"
        );

        let platform = FakePlatform::new(root.path());
        let etc = platform.dir("private/etc");
        let jobs = platform.dir("LaunchDaemons");
        let library = platform.dir("Library");
        let helper = root.path().join("helper");
        std::fs::write(&helper, b"fixture helper image").unwrap();
        with_run(|run, cx, _| {
            let mut command = |run: &mut Run<'_>, cx: &EffectContext, argv: Vec<OsString>| {
                platform.run(run, cx, argv)
            };
            let mut phases = Vec::new();
            apply_macos(
                run,
                cx,
                &["test".to_owned()],
                &MacosPlatform {
                    identity: (uid, 20),
                    helper: &helper,
                    etc: &etc,
                    jobs: &jobs,
                    library: &library,
                },
                &mut command,
                &mut |phase| phases.push(phase.to_owned()),
            )
            .expect("the macOS platform action runs after an empty-home migration");
            // Each operation is its own phase, named before it starts, and an
            // alias already on the interface is not reported as added.
            assert_eq!(
                phases.first().map(String::as_str),
                Some("writing the resolver files for test")
            );
            assert!(phases.contains(&"installing the gateway loopback launch job".to_owned()));
            assert!(phases.contains(&"installing the privileged bind broker".to_owned()));
            assert!(phases.contains(&"observing the loopback interface".to_owned()));
            assert!(phases.contains(&"adding loopback address 127.1.0.1".to_owned()));
            assert_eq!(
                phases.last().map(String::as_str),
                Some("adding loopback address 127.1.0.50")
            );
            assert_eq!(
                phases
                    .iter()
                    .filter(|phase| phase.starts_with("adding loopback address "))
                    .count(),
                50,
                "{phases:?}"
            );
        });

        let resolver = std::fs::read_to_string(platform.path("private/etc/resolver/test")).unwrap();
        assert!(resolver.contains("nameserver 127.0.0.1"), "{resolver}");
        assert!(platform
            .path("LaunchDaemons/dev.effigy.gateway.loopback.plist")
            .exists());
        assert!(platform
            .path(&format!(
                "LaunchDaemons/{}.plist",
                crate::bind_broker::label(uid)
            ))
            .exists());
        // One observation of lo0 and one alias per address in the pool.
        assert_eq!(platform.requested("/sbin/ifconfig").len(), 51);
        assert_eq!(platform.requested("/bin/launchctl").len(), 1);
    }

    /// The same first-ever setup on Linux: no unit, no capability and no
    /// gateway state beforehand, and the platform action still runs (#473).
    #[test]
    fn an_empty_operator_home_reaches_the_linux_platform_action() {
        let root = tempfile::tempdir().unwrap();
        let (operator_home, config) = empty_operator_home(root.path());
        let uid = nix::unistd::Uid::effective().as_raw();
        migrate_root(&config, uid, |_| {
            panic!("a first-ever home records no gateway process")
        })
        .expect("an empty operator home is no legacy state to migrate");
        assert!(!operator_home.join(".effigy").exists());

        let platform = FakePlatform::new(root.path());
        let systemd = platform.dir("etc/systemd/system");
        let capabilities = std::cell::Cell::new(0);
        with_run(|run, cx, _| {
            let mut command = |run: &mut Run<'_>, cx: &EffectContext, argv: Vec<OsString>| {
                platform.run(run, cx, argv)
            };
            let mut phases = Vec::new();
            apply_linux(
                run,
                cx,
                &["test".to_owned()],
                &LinuxUnit {
                    uid,
                    systemd: &systemd,
                },
                &mut command,
                &mut |_: u32| -> io::Result<()> {
                    capabilities.set(capabilities.get() + 1);
                    Ok(())
                },
                &mut |phase| phases.push(phase.to_owned()),
            )
            .expect("the Linux platform action runs after an empty-home migration");
            // No unit existed, so there is no stop phase; every operation that
            // does run is named in order, ending with the capability install.
            assert_eq!(
                phases,
                vec![
                    "writing the gateway DNS unit".to_owned(),
                    "reloading the systemd manager".to_owned(),
                    "enabling the gateway DNS unit".to_owned(),
                    format!("installing the low-port capability for uid {uid}"),
                ]
            );
        });

        let unit =
            std::fs::read_to_string(platform.path("etc/systemd/system/effigy-gateway-dns.service"))
                .unwrap();
        assert!(unit.contains("domain effigy-gw ~test"), "{unit}");
        let requested: Vec<String> = platform
            .requested("/usr/bin/systemctl")
            .into_iter()
            .map(|argv| argv[1].clone())
            .collect();
        assert_eq!(requested, vec!["daemon-reload", "enable"]);
        assert_eq!(capabilities.get(), 1);
    }

    #[test]
    fn a_symlinked_gateway_directory_is_refused_from_an_empty_home() {
        // Absence is proven by a no-follow stat of the gateway directory. A
        // symlink there is not absence: it is refused and its target is left
        // exactly as it was.
        let root = tempfile::tempdir().unwrap();
        let (home, config) = empty_operator_home(root.path());
        let foreign = root.path().join("foreign");
        std::fs::create_dir(&foreign).unwrap();
        let foreign_pid = foreign.join("gateway.pid");
        std::fs::write(&foreign_pid, b"4242\n").unwrap();
        std::fs::create_dir(home.join(".effigy")).unwrap();
        std::os::unix::fs::symlink(&foreign, home.join(".effigy/gateway")).unwrap();
        let uid = nix::unistd::Uid::effective().as_raw();
        let error = migrate_root(&config, uid, |_| {
            panic!("a symlinked gateway directory is never classified")
        })
        .expect_err("a symlinked gateway directory fails closed");
        assert!(error.to_string().contains("unsafe"), "{error}");
        assert_eq!(std::fs::read(&foreign_pid).unwrap(), b"4242\n");
        assert!(std::fs::symlink_metadata(home.join(".effigy/gateway"))
            .unwrap()
            .file_type()
            .is_symlink());
    }

    #[test]
    fn an_unsafe_gateway_directory_is_refused_before_the_platform_action() {
        let root = tempfile::tempdir().unwrap();
        let (_home, config) = empty_operator_home(root.path());
        std::fs::create_dir_all(config.pid_file_path.parent().unwrap()).unwrap();
        std::fs::set_permissions(
            config.pid_file_path.parent().unwrap(),
            std::fs::Permissions::from_mode(0o777),
        )
        .unwrap();
        let uid = nix::unistd::Uid::effective().as_raw();
        let error = migrate_root(&config, uid, |_| {
            panic!("an unsafe gateway directory is never classified")
        })
        .expect_err("a group- and other-writable gateway directory fails closed");
        assert!(error.to_string().contains("unsafe"), "{error}");
    }

    #[test]
    fn an_unreadable_gateway_home_refuses_with_the_path_and_the_repair() {
        let root = tempfile::tempdir().unwrap();
        let (home, config) = empty_operator_home(root.path());
        let state = home.join(".effigy");
        std::fs::create_dir(&state).unwrap();
        std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o000)).unwrap();
        let uid = nix::unistd::Uid::effective().as_raw();
        let error = migrate_root(&config, uid, |_| {
            panic!("an unreadable home has no verified process")
        })
        .expect_err("an unreadable home is never read as an empty one");
        std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o700)).unwrap();
        let message = error.to_string();
        // The refusal names the state path and the repair whether setup sees a
        // permission error itself or refuses the operator's own access.
        assert!(message.contains(&state.display().to_string()), "{message}");
        assert!(message.contains("the operator"), "{message}");
        assert!(
            message.contains("`effigy gateway setup` again"),
            "{message}"
        );
        assert!(!home.join(".effigy/gateway").exists());
    }

    /// The elevated child runs as root, and root can stat a path its owner
    /// cannot read. Absence therefore has to be the *operator's* answer, proven
    /// from the recorded metadata, or setup changes a machine's platform while
    /// skipping state the operator cannot even reach.
    #[test]
    fn the_operator_own_access_decides_absence_and_never_the_caller_privilege() {
        let root = tempfile::tempdir().unwrap();
        let (home, config) = empty_operator_home(root.path());
        let uid = nix::unistd::Uid::effective().as_raw();
        operator_owns_state_path(&config, uid).expect("a fresh home is absent for its operator");

        let state = home.join(".effigy");
        std::fs::create_dir(&state).unwrap();
        std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o700)).unwrap();
        operator_owns_state_path(&config, uid).expect("the operator's own .effigy is reachable");

        // Read without search, then search without read: neither reaches the
        // state its owner would have to classify.
        for (mode, why) in [
            (0o400, "read without search"),
            (0o100, "search without read"),
        ] {
            std::fs::set_permissions(&state, std::fs::Permissions::from_mode(mode)).unwrap();
            assert!(
                operator_owns_state_path(&config, uid).is_err(),
                "{why} is unreachable state"
            );
        }
        std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o700)).unwrap();

        // A home the operator cannot search is refused and named, and setup
        // creates nothing below it.
        std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o600)).unwrap();
        let error = operator_owns_state_path(&config, uid)
            .expect_err("a home the operator cannot search is not an empty home");
        assert!(
            error.to_string().contains(&home.display().to_string()),
            "{error}"
        );
        assert!(migrate_root(&config, uid, |_| panic!("no verified process")).is_err());
        std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(!home.join(".effigy/gateway").exists());
    }

    #[test]
    fn a_partial_identity_is_never_classified_as_absent_legacy_state() {
        // A version-only record is an interrupted or partial existing identity.
        // Setup preserves it and refuses instead of starting over as if the
        // home were new.
        let root = tempfile::tempdir().unwrap();
        let (_home, config) = empty_operator_home(root.path());
        std::fs::create_dir_all(config.pid_file_path.parent().unwrap()).unwrap();
        let version = config.pid_file_path.with_extension("version");
        std::fs::write(&version, b"v0.14.1\n").unwrap();
        std::fs::set_permissions(&version, std::fs::Permissions::from_mode(0o600)).unwrap();
        let uid = nix::unistd::Uid::effective().as_raw();
        let error = migrate_root(&config, uid, |_| {
            panic!("a version-only record authenticates no process")
        })
        .expect_err("a partial existing identity fails closed");
        assert!(
            error
                .to_string()
                .contains("gateway version exists without its PID"),
            "{error}"
        );
        assert_eq!(std::fs::read(&version).unwrap(), b"v0.14.1\n");
        assert!(!config.pid_file_path.exists());
    }
}
