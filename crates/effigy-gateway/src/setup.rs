//! The sole standard gateway administrator action. Native platform jobs never
//! execute Effigy as root. JSON routes remain the operator's source of truth.
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
const MARKER: &str = "Managed by Effigy gateway setup v1";
const MAX_FILE: u64 = 8 << 20;

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
            "gateway setup is unavailable ({other:?}); run `effigy gateway setup`"
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
fn setup_owner() -> ResourceId {
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
    let root = StateRoot::effects(&cx, home).map_err(error)?;
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
        if !file.metadata()?.is_file() || file.metadata()?.len() > 128 << 20 {
            return Err(error("setup executable is not a bounded regular file"));
        }
        let mut bytes = Vec::new();
        file.take((128 << 20) + 1).read_to_end(&mut bytes)?;
        if bytes.len() > 128 << 20 {
            return Err(error("setup executable changed while staging"));
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
        return Err(error("gateway setup has unresolved cleanup"));
    }
    drop(staged);
    result
}
pub fn command(
    run: &mut Run<'_>,
    cx: &EffectContext,
    argv: Vec<OsString>,
    terminal: bool,
) -> io::Result<Vec<u8>> {
    let env: BTreeMap<_, _> = std::env::vars_os()
        .map(|(key, value)| (key, EnvValue::Plain(value)))
        .collect();
    let mut child = Custody::spawn(
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
            label: "gateway setup adapter".into(),
        },
        cx,
    )
    .map_err(|e| error(format!("setup spawn: {e:?}")))?;
    let exit = match child.wait(cx.operation()) {
        Observed::Verified(exit, _) => exit,
        other => return Err(error(format!("setup command outcome unknown: {other:?}"))),
    };
    child.complete(run, exit).map_err(error)?;
    let output = child.tail();
    if exit.code != 0 {
        return Err(error(format!(
            "gateway setup adapter failed (exit {}): {}",
            exit.code,
            String::from_utf8_lossy(&output)
        )));
    }
    Ok(output)
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
pub fn apply_platform(
    run: &mut Run<'_>,
    cx: &EffectContext,
    config: &GatewayConfig,
    suffixes: &[String],
) -> io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        let etc = DirFd::open(Path::new("/private/etc")).map_err(error)?;
        let resolvers = etc
            .prepare_child(cx, &FileName::new("resolver").map_err(error)?, 0o755)
            .map_err(error)?;
        for suffix in suffixes {
            let contents = format!("# {MARKER}\nnameserver 127.0.0.1\nport 15353\n");
            managed_file(run, cx, &resolvers, suffix, contents.as_bytes(), true)?;
        }
        let jobs = DirFd::open(Path::new("/Library/LaunchDaemons")).map_err(error)?;
        managed_file(
            run,
            cx,
            &jobs,
            "dev.effigy.gateway.loopback.plist",
            &macos_boot_job(),
            false,
        )?;
        // Apply aliases now; launchd runs the same fixed commands after reboot.
        for n in 1..=50 {
            command(
                run,
                cx,
                vec![
                    "/sbin/ifconfig".into(),
                    "lo0".into(),
                    "alias".into(),
                    format!("127.1.0.{n}").into(),
                    "up".into(),
                ],
                false,
            )?;
        }
    }
    #[cfg(target_os = "linux")]
    {
        let systemd = DirFd::open(Path::new("/etc/systemd/system")).map_err(error)?;
        let name = FileName::new("effigy-gateway-dns.service").map_err(error)?;
        match read_record::<Bytes>(&cx.operation().observe().map_err(error)?, &systemd, &name) {
            Observed::Verified(bytes, _) if String::from_utf8_lossy(&bytes.0).contains(MARKER) => {
                command(
                    run,
                    cx,
                    vec![
                        "/usr/bin/systemctl".into(),
                        "stop".into(),
                        name.as_str().into(),
                    ],
                    false,
                )?;
            }
            Observed::Absent(_) => {}
            other => return Err(error(format!("preserving unknown DNS unit: {other:?}"))),
        }
        managed_file(
            run,
            cx,
            &systemd,
            name.as_str(),
            &linux_boot_job(suffixes),
            false,
        )?;
        command(
            run,
            cx,
            vec!["/usr/bin/systemctl".into(), "daemon-reload".into()],
            false,
        )?;
        command(
            run,
            cx,
            vec![
                "/usr/bin/systemctl".into(),
                "enable".into(),
                "--now".into(),
                name.as_str().into(),
            ],
            false,
        )?;
        // A binary-local capability avoids lowering a host-wide port threshold.
        install_bind_capability()?;
    }
    let _ = config;
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
        .prepare_child(cx, &FileName::new("gateway").map_err(error)?, 0o700)
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
    for name in ["routes.json", "loopback-ips.json"] {
        gateway
            .adopt_setup_entry(&cx, &FileName::new(name).map_err(error)?, false, MAX_FILE)
            .map_err(error)?;
    }
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
            let record = snapshot
                .record()
                .ok_or_else(|| error("setup requires a v0.14.1 identity record"))?;
            match crate::identity::probe_live_identity(record) {
                crate::identity::GatewayIdentityProbe::Matched => Ok(Some(
                    crate::identity::read_live_process_identity(snapshot.pid())?.uid,
                )),
                crate::identity::GatewayIdentityProbe::Mismatch
                    if crate::server::probe_gateway_process(snapshot.pid())
                        == crate::server::GatewayProcessProbe::ConfirmedAbsent =>
                {
                    Ok(None)
                }
                _ => Err(error(
                    "gateway identity is unknown; setup made no process change",
                )),
            }
        },
        stop,
    )
}
fn migrate_root_with(
    config: &GatewayConfig,
    uid: u32,
    inspect: impl FnOnce(&crate::identity::GatewayRecordSnapshot) -> io::Result<Option<u32>>,
    stop: impl FnOnce(&crate::identity::GatewayRecordSnapshot) -> io::Result<()>,
) -> io::Result<()> {
    if let Some(snapshot) = crate::identity::read_snapshot(&config.pid_file_path).map_err(error)? {
        match inspect(&snapshot)? {
            Some(0) => {
                let version =
                    crate::identity::read_gateway_version(&config.pid_file_path).map_err(error)?;
                if !version
                    .as_deref()
                    .is_some_and(|v| v.starts_with("0.14.1") || v.starts_with("v0.14.1"))
                {
                    return Err(error(
                        "setup only migrates an authenticated v0.14.1 root gateway",
                    ));
                }
                stop(&snapshot)?;
            }
            Some(owner) if owner == uid => {}
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

#[cfg(target_os = "linux")]
fn install_bind_capability() -> io::Result<()> {
    use std::os::fd::AsRawFd;
    let path = std::env::current_exe()?;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let actual = std::fs::metadata("/proc/self/exe")?;
    let opened = file.metadata()?;
    if !opened.is_file()
        || opened.dev() != actual.dev()
        || opened.ino() != actual.ino()
        || opened.nlink() != 1
        || opened.mode() & 0o022 != 0
    {
        return Err(error("installed gateway binary changed during setup"));
    }
    // Linux vfs_cap_data revision 2, effective flag, permitted bit 10 only.
    // File capabilities do not grant privilege to arbitrary child programs.
    let capability: [u8; 20] = [1, 0, 0, 2, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    // SAFETY: verified live descriptor, fixed NUL-terminated xattr name and
    // exactly initialized Linux capability bytes. No path is reopened.
    if unsafe {
        libc::fsetxattr(
            file.as_raw_fd(),
            c"security.capability".as_ptr(),
            capability.as_ptr().cast(),
            capability.len(),
            0,
        )
    } != 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    fn with_run(work: impl FnOnce(&mut Run<'_>, &EffectContext, &Path)) {
        let home = tempfile::tempdir().unwrap();
        let op = operator::context(Duration::from_secs(10), EffectClass::Mutate);
        let cx = op.effects().unwrap();
        let root = StateRoot::effects(&cx, home.path()).unwrap();
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
    fn root_gateway_migration_preserves_every_legacy_mapping_and_unknown_stops_nothing() {
        let home = tempfile::tempdir().unwrap();
        let gateway = home.path().join(".effigy/gateway");
        std::fs::create_dir_all(&gateway).unwrap();
        let config = GatewayConfig::standard(gateway.clone());
        let fixtures=[(gateway.join("routes.json"),br#"{"_managed_by":"effigy-gateway-route-table-v1","routes":{"cbs.legacy.test":{"domain":"cbs.legacy.test","target":"127.0.0.1:8100","source":"manual","project":"/redacted/site","tls":true,"registered":"2026-01-01T00:00:00Z"},"ui.acowtancy.test":{"domain":"ui.acowtancy.test","target":"127.0.0.1:8300","source":"manual","project":"/redacted/ui","registered":"2026-01-01T00:00:00Z"}}}"#.as_slice()),(gateway.join("loopback-ips.json"),br#"{"assignments":{"site":{"ip":"127.1.0.7","scope":"/redacted/site"}}}"#.as_slice()),(home.path().join(".effigy/ports.json"),br#"{"allocations":{"site":{"base":8100,"range":100,"project":"/redacted/site","assigned_ports":{"mysql:3306":8106}}}}"#.as_slice())];
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
        assert_eq!(
            crate::routes::RouteTable::load(&config.route_table_path)
                .unwrap()
                .len(),
            2
        );
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
}
