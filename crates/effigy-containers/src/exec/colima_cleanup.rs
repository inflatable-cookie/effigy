//! Exact PID-file authority for explicit profile recovery.
use effigy_kernel::ident::{self, ProcessIdentity};
use effigy_kernel::observe::Observed;
use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::Read;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

#[derive(Clone, Debug, PartialEq, Eq)]
struct PidFile {
    pid: u32,
    modified: SystemTime,
    device: u64,
    inode: u64,
}

fn read_pidfile(path: &Path) -> Result<Option<PidFile>, String> {
    let mut file = match OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW)
        .open(path)
    {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("cannot open {}: {e}", path.display())),
    };
    let before = file.metadata().map_err(|e| e.to_string())?;
    if !before.is_file() {
        return Err(format!("{} is not a regular pidfile", path.display()));
    }
    let mut text = String::new();
    Read::by_ref(&mut file)
        .take(65)
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    let after = file.metadata().map_err(|e| e.to_string())?;
    let named = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if before.dev() != after.dev()
        || before.ino() != after.ino()
        || before.mtime() != after.mtime()
        || before.mtime_nsec() != after.mtime_nsec()
        || after.dev() != named.dev()
        || after.ino() != named.ino()
        || !named.is_file()
    {
        return Err(format!("{} changed while reading", path.display()));
    }
    let pid = text
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|pid| *pid > 1 && *pid <= i32::MAX as u32)
        .ok_or_else(|| format!("{} has an invalid PID", path.display()))?;
    Ok(Some(PidFile {
        pid,
        modified: after.modified().map_err(|e| e.to_string())?,
        device: after.dev(),
        inode: after.ino(),
    }))
}

trait Processes {
    fn identity(&mut self, pid: u32) -> Result<Option<ProcessIdentity>, String>;
    fn arguments(&mut self, pid: u32) -> Result<Vec<OsString>, String>;
    fn signal(
        &mut self,
        identity: &ProcessIdentity,
        path: &Path,
        expected: &PidFile,
        instance: &Path,
    ) -> Result<bool, String>;
}

struct Host;
impl Processes for Host {
    fn identity(&mut self, pid: u32) -> Result<Option<ProcessIdentity>, String> {
        match ident::process_identity(pid) {
            Observed::Verified(identity, _) => Ok(Some(identity)),
            Observed::Absent(_) => Ok(None),
            other => Err(other.unproven()),
        }
    }
    fn arguments(&mut self, pid: u32) -> Result<Vec<OsString>, String> {
        ident::process_arguments(pid).map_err(|e| e.to_string())
    }
    fn signal(
        &mut self,
        identity: &ProcessIdentity,
        path: &Path,
        expected: &PidFile,
        instance: &Path,
    ) -> Result<bool, String> {
        let observed =
            effigy_kernel::custody::signal_verified_process(identity, nix::libc::SIGKILL, || {
                read_pidfile(path).ok().flatten().as_ref() == Some(expected)
                    && ident::process_arguments(identity.pid)
                        .is_ok_and(|args| owns_hostagent(&args, instance))
                    && ident::process_identity(identity.pid).verified() == Some(identity)
            });
        match observed {
            Observed::Verified(_, _) => Ok(true),
            Observed::Absent(_) => Ok(false),
            _ if matches!(ident::process_identity(identity.pid), Observed::Absent(_)) => Ok(false),
            other => Err(other.unproven()),
        }
    }
}

fn owns_hostagent(args: &[OsString], instance: &Path) -> bool {
    let executable = args.first().map(Path::new).and_then(Path::file_name);
    let expected = instance.join("ha.pid");
    executable == Some(std::ffi::OsStr::new("limactl"))
        && (args.get(1).is_some_and(|arg| arg == "hostagent")
            || (args.get(1).is_some_and(|arg| arg == "--debug")
                && args.get(2).is_some_and(|arg| arg == "hostagent")))
        && args
            .windows(2)
            .any(|pair| pair[0] == "--pidfile" && Path::new(&pair[1]) == expected)
        && args
            .windows(2)
            .any(|pair| pair[0] == "--socket" && Path::new(&pair[1]) == instance.join("ha.sock"))
}

fn cleanup<P: Processes>(
    processes: &mut P,
    instance: &Path,
    graceful_ok: bool,
) -> Result<Vec<String>, String> {
    let mut steps = Vec::new();
    let mut errors = Vec::new();
    let mut missing = Vec::new();
    let mut present = false;
    for name in ["ha.pid", "vz.pid"] {
        let path = instance.join(name);
        let file = match read_pidfile(&path) {
            Ok(Some(file)) => {
                present = true;
                file
            }
            Ok(None) => {
                missing.push(name);
                continue;
            }
            Err(error) => {
                errors.push(error);
                continue;
            }
        };
        let identity = match processes.identity(file.pid) {
            Ok(Some(identity)) => identity,
            Ok(None) => {
                steps.push(format!("{}: PID {} already exited", name, file.pid));
                continue;
            }
            Err(error) => {
                errors.push(format!(
                    "{name}: PID {} identity unavailable: {error}",
                    file.pid
                ));
                continue;
            }
        };
        if !ident::started_before(&identity, file.modified) {
            errors.push(format!(
                "{name}: PID {} started after the pidfile write or its start time is unprovable",
                file.pid
            ));
            continue;
        }
        match processes.arguments(file.pid) {
            Ok(args) if owns_hostagent(&args, instance) => {}
            Ok(_) => {
                errors.push(format!(
                    "{name}: PID {} is not the selected profile's host agent",
                    file.pid
                ));
                continue;
            }
            Err(error) => {
                if matches!(processes.identity(file.pid), Ok(None)) {
                    steps.push(format!(
                        "{name}: PID {} exited while reading arguments",
                        file.pid
                    ));
                } else {
                    errors.push(format!(
                        "{name}: PID {} arguments unavailable: {error}",
                        file.pid
                    ));
                }
                continue;
            }
        }
        match processes.signal(&identity, &path, &file, instance) {
            Ok(false) => steps.push(format!("{name}: PID {} exited before signalling", file.pid)),
            Err(error) => errors.push(format!("{name}: PID {} refused: {error}", file.pid)),
            Ok(true) => {
                let deadline = Instant::now() + Duration::from_secs(5);
                loop {
                    match processes.identity(file.pid) {
                        Ok(None) => {
                            steps.push(format!("{name}: stopped PID {}", file.pid));
                            break;
                        }
                        Ok(Some(current)) if current != identity => {
                            steps.push(format!("{name}: stopped PID {} (PID reused)", file.pid));
                            break;
                        }
                        Ok(Some(_)) if Instant::now() < deadline => {
                            std::thread::sleep(Duration::from_millis(100))
                        }
                        Ok(Some(_)) => {
                            errors.push(format!("{name}: PID {} survived SIGKILL", file.pid));
                            break;
                        }
                        Err(error) => {
                            errors
                                .push(format!("{name}: PID {} exit unverified: {error}", file.pid));
                            break;
                        }
                    }
                }
            }
        }
    }
    if (!graceful_ok || present) && !missing.is_empty() {
        errors.push(format!(
            "missing profile pidfile(s): {}",
            missing.join(", ")
        ));
    }
    if errors.is_empty() {
        Ok(steps)
    } else {
        Err(format!("{}\n{}", steps.join("\n"), errors.join("\n")))
    }
}

pub(super) fn terminate(profile: &str, graceful_ok: bool) -> Result<Vec<String>, String> {
    terminate_with_home(profile, graceful_ok, std::env::var_os("HOME"))
}

fn terminate_with_home(
    profile: &str,
    graceful_ok: bool,
    home: Option<OsString>,
) -> Result<Vec<String>, String> {
    let safe_profile = !profile.is_empty()
        && profile
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'));
    let result = (|| {
        if !safe_profile {
            return Err("profile name cannot identify a safe Lima instance directory".into());
        }
        let home = home.ok_or("HOME is unavailable")?;
        let instance: PathBuf = Path::new(&home)
            .join(".colima/_lima")
            .join(format!("colima-{profile}"));
        cleanup(&mut Host, &instance, graceful_ok)
    })();
    let profile_hint = if safe_profile { profile } else { "<profile>" };
    result.map_err(|error| format!("{error}\nVerify the profile pidfile ownership before manually running `colima stop -p {profile_hint} --force`; Effigy did not run it. External limactl shell and SSH processes are not signalled."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use effigy_kernel::ident::ProcessStartIdentity;
    use std::collections::BTreeMap;

    struct Fake {
        identities: BTreeMap<u32, ProcessIdentity>,
        arguments: BTreeMap<u32, Vec<OsString>>,
        changed: Vec<u32>,
        exited: Vec<u32>,
        failed: Vec<u32>,
        unknown: Vec<u32>,
        signalled: Vec<u32>,
    }
    impl Processes for Fake {
        fn identity(&mut self, pid: u32) -> Result<Option<ProcessIdentity>, String> {
            if self.unknown.contains(&pid) {
                return Err("unreadable process identity".into());
            }
            Ok(self.identities.get(&pid).cloned())
        }
        fn arguments(&mut self, pid: u32) -> Result<Vec<OsString>, String> {
            self.arguments
                .get(&pid)
                .cloned()
                .ok_or_else(|| "unreadable argv".into())
        }
        fn signal(
            &mut self,
            identity: &ProcessIdentity,
            path: &Path,
            file: &PidFile,
            instance: &Path,
        ) -> Result<bool, String> {
            if self.exited.contains(&identity.pid) {
                self.identities.remove(&identity.pid);
                return Ok(false);
            }
            if self.changed.contains(&identity.pid)
                || self.failed.contains(&identity.pid)
                || read_pidfile(path).ok().flatten().as_ref() != Some(file)
                || !self
                    .arguments
                    .get(&identity.pid)
                    .is_some_and(|args| owns_hostagent(args, instance))
            {
                return Err("changed or unprovable identity".into());
            }
            self.signalled.push(identity.pid);
            self.identities.remove(&identity.pid);
            Ok(true)
        }
    }
    fn fixture() -> (tempfile::TempDir, Fake) {
        let dir = tempfile::tempdir().unwrap();
        let fake = Fake {
            identities: BTreeMap::new(),
            arguments: BTreeMap::new(),
            changed: vec![],
            exited: vec![],
            failed: vec![],
            unknown: vec![],
            signalled: vec![],
        };
        (dir, fake)
    }
    fn add(fake: &mut Fake, dir: &Path, name: &str, pid: u32) {
        std::fs::write(dir.join(name), pid.to_string()).unwrap();
        let mut identity = ident::process_identity(std::process::id())
            .verified()
            .unwrap()
            .clone();
        identity.pid = pid;
        identity.start = ProcessStartIdentity::MacOs {
            seconds: 1,
            microseconds: 0,
        };
        fake.identities.insert(pid, identity);
        fake.arguments.insert(
            pid,
            vec![
                "/opt/bin/limactl".into(),
                "hostagent".into(),
                "--pidfile".into(),
                dir.join("ha.pid").into_os_string(),
                "--socket".into(),
                dir.join("ha.sock").into_os_string(),
            ],
        );
    }
    #[test]
    fn colima_runtime_owned_pidfiles_signal_only_verified_processes() {
        let (dir, mut fake) = fixture();
        add(&mut fake, dir.path(), "ha.pid", 40);
        add(&mut fake, dir.path(), "vz.pid", 41);
        let steps = cleanup(&mut fake, dir.path(), false).unwrap();
        assert_eq!(fake.signalled, [40, 41]);
        assert!(steps.join(" ").contains("stopped PID 40"));
    }
    #[test]
    fn colima_runtime_reused_or_changed_pid_is_never_signalled() {
        let (dir, mut fake) = fixture();
        add(&mut fake, dir.path(), "ha.pid", 40);
        add(&mut fake, dir.path(), "vz.pid", 41);
        fake.identities.get_mut(&40).unwrap().start = ProcessStartIdentity::MacOs {
            seconds: u64::MAX,
            microseconds: 0,
        };
        fake.changed.push(41);
        let error = cleanup(&mut fake, dir.path(), false).unwrap_err();
        assert!(fake.signalled.is_empty());
        assert!(error.contains("started after") && error.contains("changed"));
    }
    #[test]
    fn colima_runtime_foreign_shell_ssh_and_similar_profile_are_refused() {
        let (dir, mut fake) = fixture();
        add(&mut fake, dir.path(), "ha.pid", 40);
        let foreign_dir = std::path::PathBuf::from(format!("{}-other", dir.path().display()));
        for args in [
            vec![
                "limactl".into(),
                "shell".into(),
                "hostagent".into(),
                "--pidfile".into(),
                dir.path().join("ha.pid").into_os_string(),
                "--socket".into(),
                dir.path().join("ha.sock").into_os_string(),
            ],
            vec![
                "ssh".into(),
                "hostagent".into(),
                "--pidfile".into(),
                dir.path().join("ha.pid").into_os_string(),
                "--socket".into(),
                dir.path().join("ha.sock").into_os_string(),
            ],
            vec![
                "limactl".into(),
                "hostagent".into(),
                "--pidfile".into(),
                foreign_dir.join("ha.pid").into_os_string(),
                "--socket".into(),
                dir.path().join("ha.sock").into_os_string(),
            ],
        ] {
            fake.arguments.insert(40, args);
            assert!(cleanup(&mut fake, dir.path(), false).is_err());
            assert!(fake.signalled.is_empty());
        }
    }
    #[test]
    fn colima_runtime_partial_failure_and_exit_are_reported() {
        let (dir, mut fake) = fixture();
        add(&mut fake, dir.path(), "ha.pid", 40);
        add(&mut fake, dir.path(), "vz.pid", 41);
        fake.failed.push(41);
        let error = cleanup(&mut fake, dir.path(), false).unwrap_err();
        assert_eq!(fake.signalled, [40]);
        assert!(error.contains("stopped PID 40") && error.contains("PID 41 refused"));
        fake.failed.clear();
        fake.exited.push(41);
        assert!(cleanup(&mut fake, dir.path(), false)
            .unwrap()
            .join(" ")
            .contains("exited before"));
    }
    #[test]
    fn colima_runtime_missing_and_invalid_evidence_refuses() {
        let (dir, mut fake) = fixture();
        assert!(cleanup(&mut fake, dir.path(), false).is_err());
        std::fs::write(dir.path().join("ha.pid"), "invalid").unwrap();
        assert!(cleanup(&mut fake, dir.path(), false).is_err());
        add(&mut fake, dir.path(), "ha.pid", 40);
        fake.arguments.clear();
        assert!(cleanup(&mut fake, dir.path(), false).is_err());
        fake.unknown.push(40);
        assert!(cleanup(&mut fake, dir.path(), false)
            .unwrap_err()
            .contains("identity unavailable"));
        assert!(fake.signalled.is_empty());
    }

    #[test]
    fn colima_runtime_missing_companion_pidfile_is_reported() {
        let (dir, mut fake) = fixture();
        add(&mut fake, dir.path(), "ha.pid", 40);
        let error = cleanup(&mut fake, dir.path(), false).unwrap_err();
        assert_eq!(fake.signalled, [40]);
        assert!(error.contains("missing profile pidfile(s): vz.pid"));
        assert!(error.contains("stopped PID 40"));
    }

    #[test]
    fn colima_runtime_refusal_names_manual_force_stop_and_verification() {
        let dir = tempfile::tempdir().unwrap();
        let error = terminate_with_home("effigy", false, Some(dir.path().into())).unwrap_err();
        assert!(error.contains("missing profile pidfile(s): ha.pid, vz.pid"));
        assert!(error.contains("Verify the profile pidfile ownership"));
        assert!(error.contains("colima stop -p effigy --force"));
        assert!(error.contains("Effigy did not run it"));
    }
}
