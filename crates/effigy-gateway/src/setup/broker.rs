//! Setup-only macOS broker installation. All paths are root-owned; fixtures
//! substitute directories and the command adapter without touching launchd.
use super::*;
use effigy_kernel::id::Digest;

#[derive(Debug)]
struct Image(Vec<u8>);
impl Record for Image {
    const MAX_BYTES: u64 = 512 << 20;
    fn decode(bytes: &[u8]) -> Result<Self, String> {
        Ok(Self(bytes.to_vec()))
    }
}
fn image(source: &Path) -> io::Result<Vec<u8>> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(source)?;
    if !file.metadata()?.is_file() || file.metadata()?.len() > Image::MAX_BYTES {
        return Err(error("broker helper must be a bounded regular executable"));
    }
    let mut bytes = Vec::new();
    file.take(Image::MAX_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > Image::MAX_BYTES {
        return Err(error("broker executable changed while copying"));
    }
    Ok(bytes)
}
fn owned_image(cx: &EffectContext, dir: &DirFd) -> io::Result<Option<Vec<u8>>> {
    let observe = cx.operation().observe().map_err(error)?;
    let name = FileName::new("effigy").map_err(error)?;
    match read_record::<Image>(&observe, dir, &name) {
        Observed::Absent(_) => Ok(None),
        Observed::Verified(bytes, _) => {
            let expected = format!("{MARKER}\n{}\n", hex::encode(Digest::of(&bytes.0).0));
            match read_record::<Bytes>(&observe, dir, &FileName::new("identity").map_err(error)?) {
                Observed::Verified(record, _) if record.0 == expected.as_bytes() => {
                    Ok(Some(bytes.0))
                }
                _ => Err(error("preserving unrecognized privileged helper")),
            }
        }
        other => Err(error(format!(
            "preserving unknown privileged helper: {other:?}"
        ))),
    }
}
pub(super) fn definition(uid: u32, gid: u32, program: &Path) -> Vec<u8> {
    // Production paths contain only fixed components and a decimal UID.
    let program = program
        .to_string_lossy()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let label = crate::bind_broker::label(uid);
    let socket = crate::bind_broker::socket_path(uid);
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!-- {MARKER} -->\n<plist version=\"1.0\"><dict><key>Label</key><string>{label}</string><key>UserName</key><string>root</string><key>ProgramArguments</key><array><string>{program}</string><string>__gateway-bind-broker</string><string>{uid}</string></array><key>Sockets</key><dict><key>broker</key><dict><key>SockPathName</key><string>{}</string><key>SockPathOwner</key><integer>0</integer><key>SockPathGroup</key><integer>{gid}</integer><key>SockPathMode</key><integer>432</integer><key>SockType</key><string>stream</string></dict></dict></dict></plist>\n", socket.display()).into_bytes()
}
pub(super) fn owned_job(cx: &EffectContext, jobs: &DirFd, name: &str) -> io::Result<bool> {
    match read_record::<Bytes>(
        &cx.operation().observe().map_err(error)?,
        jobs,
        &FileName::new(name).map_err(error)?,
    ) {
        Observed::Absent(_) => Ok(false),
        Observed::Verified(bytes, _) if String::from_utf8_lossy(&bytes.0).contains(MARKER) => {
            Ok(true)
        }
        other => Err(error(format!(
            "preserving foreign broker definition: {other:?}"
        ))),
    }
}
// launchctl documents ESRCH (3) for a missing service. Its CLI maps that
// observation to 113 on current macOS. Only that exact outcome is absence;
// every other failure remains unknown and prevents mutation.
pub(super) fn stop_job(
    run: &mut Run<'_>,
    cx: &EffectContext,
    label: &str,
    paths: (&Path, &Path),
    command: &mut impl FnMut(&mut Run<'_>, &EffectContext, Vec<OsString>) -> io::Result<(i32, Vec<u8>)>,
) -> io::Result<()> {
    let target = format!("system/{label}");
    let (code, observation) = command(
        run,
        cx,
        vec![
            "/bin/launchctl".into(),
            "print".into(),
            target.clone().into(),
        ],
    )?;
    match code {
        113 => Ok(()),
        0 => {
            let observation = String::from_utf8(observation).map_err(error)?;
            for expected in [
                format!("path = {}", paths.0.display()),
                format!("program = {}", paths.1.display()),
            ] {
                if !observation.lines().any(|line| line.trim() == expected) {
                    return Err(error("preserving foreign or unknown launchd job"));
                }
            }
            let (code, output) = command(
                run,
                cx,
                vec!["/bin/launchctl".into(), "bootout".into(), target.into()],
            )?;
            if code != 0 {
                return Err(error(format!(
                    "broker bootout failed: {}",
                    String::from_utf8_lossy(&output)
                )));
            }
            Ok(())
        }
        _ => Err(error(format!(
            "broker launchd observation unknown (exit {code})"
        ))),
    }
}
pub(super) fn install(
    run: &mut Run<'_>,
    cx: &EffectContext,
    jobs: &DirFd,
    helpers: &DirFd,
    identity: (u32, u32),
    source: &Path,
    mut command: impl FnMut(&mut Run<'_>, &EffectContext, Vec<OsString>) -> io::Result<(i32, Vec<u8>)>,
) -> io::Result<()> {
    let (uid, gid) = identity;
    let name = format!("{}.plist", crate::bind_broker::label(uid));
    let existing = owned_job(cx, jobs, &name)?;
    let dir = helpers
        .prepare_setup_child(
            cx,
            &FileName::new(&crate::bind_broker::label(uid)).map_err(error)?,
            0o700,
        )
        .map_err(error)?;
    let old = owned_image(cx, &dir)?;
    let bytes = image(source)?;
    // Observe all existing ownership before stopping or replacing anything.
    if existing {
        stop_job(
            run,
            cx,
            &crate::bind_broker::label(uid),
            (&jobs.path().join(&name), &dir.path().join("effigy")),
            &mut command,
        )?;
    }
    if old.as_deref() != Some(bytes.as_slice()) {
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
        let identity = format!("{MARKER}\n{}\n", hex::encode(Digest::of(&bytes).0));
        managed_file(run, cx, &dir, "identity", identity.as_bytes(), false)?;
    }
    managed_file(
        run,
        cx,
        jobs,
        &name,
        &definition(uid, gid, &dir.path().join("effigy")),
        false,
    )?;
    let (code, output) = command(
        run,
        cx,
        vec![
            "/bin/launchctl".into(),
            "bootstrap".into(),
            "system".into(),
            jobs.path().join(&name).into(),
        ],
    )?;
    if code != 0 {
        return Err(error(format!(
            "broker bootstrap failed: {}",
            String::from_utf8_lossy(&output)
        )));
    }
    Ok(())
}
pub(super) fn remove(
    run: &mut Run<'_>,
    cx: &EffectContext,
    jobs: &DirFd,
    helpers: &DirFd,
    uid: u32,
    mut command: impl FnMut(&mut Run<'_>, &EffectContext, Vec<OsString>) -> io::Result<(i32, Vec<u8>)>,
) -> io::Result<()> {
    let name = format!("{}.plist", crate::bind_broker::label(uid));
    if !owned_job(cx, jobs, &name)? {
        let (code, _) = command(
            run,
            cx,
            vec![
                "/bin/launchctl".into(),
                "print".into(),
                format!("system/{}", crate::bind_broker::label(uid)).into(),
            ],
        )?;
        return if code == 113 {
            Ok(())
        } else {
            Err(error(
                "broker definition missing with an unknown or loaded job; preserving it",
            ))
        };
    }
    let dir = DirFd::open(&helpers.path().join(crate::bind_broker::label(uid))).map_err(error)?;
    if owned_image(cx, &dir)?.is_none() {
        return Err(error("broker helper missing; preserving definition"));
    }
    stop_job(
        run,
        cx,
        &crate::bind_broker::label(uid),
        (&jobs.path().join(&name), &dir.path().join("effigy")),
        &mut command,
    )?;
    // These exact paths are under verified root-owned directories. The kernel
    // setup gate excludes other setup writers; no recursive deletion is used.
    let (code, _) = command(
        run,
        cx,
        vec![
            "/bin/rm".into(),
            jobs.path().join(name).into(),
            dir.path().join("effigy").into(),
            dir.path().join("identity").into(),
        ],
    )?;
    if code != 0 {
        return Err(error("broker removal failed"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn loaded_foreign_job_and_unknown_observation_never_bootout() {
        crate::setup::tests::with_run(|run, cx, home| {
            for code in [0, 1] {
                let mut calls = 0;
                assert!(stop_job(
                    run,
                    cx,
                    "dev.effigy.gateway.bind-broker.501",
                    (&home.join("ours.plist"), &home.join("effigy")),
                    &mut |_, _, argv| {
                        calls += 1;
                        assert_eq!(argv[1], "print");
                        Ok((
                            code,
                            b"path = /foreign/job.plist\nprogram = /foreign/effigy\n".to_vec(),
                        ))
                    }
                )
                .is_err());
                assert_eq!(calls, 1);
            }
        });
    }
    #[test]
    fn setup_and_teardown_broker_verify_ownership_before_launchd_or_removal() {
        crate::setup::tests::with_run(|run, cx, home| {
            let jobs_path = home.join("jobs");
            let helpers_path = home.join("helpers");
            std::fs::create_dir(&jobs_path).unwrap();
            std::fs::create_dir(&helpers_path).unwrap();
            let jobs = DirFd::open(&jobs_path).unwrap();
            let helpers = DirFd::open(&helpers_path).unwrap();
            let source = home.join("source");
            std::fs::write(&source, b"authorized helper fixture").unwrap();
            let calls = std::cell::RefCell::new(Vec::new());
            let adapter = |_: &mut Run<'_>, _: &EffectContext, argv: Vec<OsString>| {
                calls.borrow_mut().push(argv.clone());
                if argv[0] == "/bin/rm" {
                    for path in &argv[1..] {
                        std::fs::remove_file(path)?;
                    }
                }
                Ok((if argv[1] == "print" { 113 } else { 0 }, Vec::new()))
            };
            install(run, cx, &jobs, &helpers, (501, 20), &source, adapter).unwrap();
            let plist = jobs_path.join("dev.effigy.gateway.bind-broker.501.plist");
            let xml = std::fs::read_to_string(&plist).unwrap();
            assert!(xml.contains("<integer>432</integer>"));
            assert!(xml.contains("<key>SockPathOwner</key><integer>0</integer>"));
            assert!(!xml.contains("__gateway-user-run"));
            remove(run, cx, &jobs, &helpers, 501, adapter).unwrap();
            assert!(!plist.exists());
            std::fs::write(&plist, b"foreign definition").unwrap();
            let before = calls.borrow().len();
            assert!(install(run, cx, &jobs, &helpers, (501, 20), &source, adapter).is_err());
            assert!(remove(run, cx, &jobs, &helpers, 501, adapter).is_err());
            assert_eq!(calls.borrow().len(), before);
            assert_eq!(std::fs::read(&plist).unwrap(), b"foreign definition");
        });
    }
}
