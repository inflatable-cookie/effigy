use std::path::{Path, PathBuf};
use std::process::Command;

const ACTIVE_VERSION_ENV: &str = "EFFIGY_ACTIVE_VERSION";
const LOCAL_ACTIVE_VERSION_EXTENSION: &str = "active-version";
const LOCAL_INSTALL_DIR_NAME: &str = ".local-install";
const LOCAL_INSTALL_BIN_DIR_NAME: &str = "bin";
const LOCAL_INSTALL_EXECUTABLE_NAME: &str = "effigy";
const LOCAL_INSTALL_IDENTITY_MARKER: &str = "+local.";
const LOCAL_INSTALL_DIRTY_SUFFIX: &str = ".dirty";

/// Provenance facts proving a repository-local Effigy install is behind the
/// checkout whose manifest failed a strict parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaleLocalInstall {
    /// Installed executable under `<repo_root>/.local-install/bin/effigy`.
    pub executable: std::path::PathBuf,
    /// Checkout both the executable and the failing manifest belong to.
    pub repo_root: std::path::PathBuf,
    /// Identity recorded next to the installed binary at install time, such
    /// as `v0.12.1+local.abc1234`.
    pub installed_identity: String,
    /// Identity of the current checkout, such as
    /// `v0.12.1+local.def5678.dirty`.
    pub current_identity: String,
}

/// Reports a stale repository-local install for the running executable when
/// the manifest at `failing_manifest` could not be parsed: the executable is
/// the failing manifest's checkout `.local-install/bin/effigy`, its recorded
/// `+local.<sha>` identity resolves in that checkout, and the recorded commit
/// is a strict ancestor of the checkout `HEAD`. Returns `None` whenever any
/// of those facts cannot be proved, so release binaries, consumer
/// repositories, and current or divergent local builds keep the ordinary
/// error.
pub fn stale_repo_local_install(failing_manifest: &Path) -> Option<StaleLocalInstall> {
    let executable = std::env::current_exe().ok()?;
    stale_repo_local_install_for(&executable, failing_manifest)
}

/// Testable form of [`stale_repo_local_install`] that takes the executable
/// path explicitly instead of reading the running process.
pub fn stale_repo_local_install_for(
    executable: &Path,
    failing_manifest: &Path,
) -> Option<StaleLocalInstall> {
    if !is_repo_local_install_executable(executable) {
        return None;
    }
    let repo_root = discover_repo_root_from_path(executable)?;
    if discover_repo_root_from_path(failing_manifest)? != repo_root {
        return None;
    }
    let installed_identity = read_active_version_file_for(executable)?;
    let recorded_commit = local_commit_from_identity(&installed_identity)?;
    let recorded = git_stdout(
        &repo_root,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{recorded_commit}^{{commit}}"),
        ],
    )?;
    let head = git_stdout(&repo_root, &["rev-parse", "HEAD"])?;
    if recorded == head {
        return None;
    }
    if !git_command_succeeds(
        &repo_root,
        &["merge-base", "--is-ancestor", &recorded, &head],
    ) {
        return None;
    }
    Some(StaleLocalInstall {
        executable: executable.to_path_buf(),
        current_identity: infer_repo_local_version(executable)?,
        repo_root,
        installed_identity,
    })
}

pub fn package_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn active_version() -> String {
    if let Some(version) = read_active_version_env() {
        return version;
    }
    std::env::current_exe()
        .ok()
        .and_then(|path| {
            read_active_version_file_for(&path).or_else(|| infer_repo_local_version(&path))
        })
        .unwrap_or_else(|| package_version().to_owned())
}

pub fn display_version() -> String {
    let active = active_version();
    if active.starts_with('v') {
        active
    } else {
        format!("v{active}")
    }
}

fn active_version_file_for(executable: &Path) -> PathBuf {
    executable.with_extension(LOCAL_ACTIVE_VERSION_EXTENSION)
}

fn read_active_version_env() -> Option<String> {
    let raw = std::env::var(ACTIVE_VERSION_ENV).ok()?;
    let value = raw.trim();
    if value.is_empty() {
        return None;
    }
    Some(value.to_owned())
}

fn read_active_version_file_for(executable: &Path) -> Option<String> {
    let path = active_version_file_for(executable);
    let raw = std::fs::read_to_string(path).ok()?;
    let value = raw.trim();
    if value.is_empty() {
        return None;
    }
    Some(value.to_owned())
}

fn infer_repo_local_version(executable: &Path) -> Option<String> {
    let repo_root = discover_repo_root_from_executable(executable)?;
    let hash = git_stdout(&repo_root, &["rev-parse", "--short=7", "HEAD"])?;
    let dirty_suffix = if git_repo_is_dirty(&repo_root)? {
        ".dirty"
    } else {
        ""
    };
    Some(format!(
        "{}+local.{hash}{dirty_suffix}",
        display_version_prefix(package_version()),
    ))
}

fn discover_repo_root_from_executable(executable: &Path) -> Option<PathBuf> {
    discover_repo_root_from_path(executable)
}

fn discover_repo_root_from_path(path: &Path) -> Option<PathBuf> {
    path.ancestors().find_map(|ancestor| {
        let cargo_toml = ancestor.join("Cargo.toml");
        let git_dir = ancestor.join(".git");
        if cargo_toml.is_file() && git_dir.exists() {
            Some(ancestor.to_path_buf())
        } else {
            None
        }
    })
}

fn is_repo_local_install_executable(executable: &Path) -> bool {
    let Some(executable_name) = executable.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    if executable_name != LOCAL_INSTALL_EXECUTABLE_NAME {
        return false;
    }
    let Some(bin_dir) = executable.parent().and_then(|parent| parent.file_name()) else {
        return false;
    };
    if bin_dir != LOCAL_INSTALL_BIN_DIR_NAME {
        return false;
    }
    let Some(install_dir) = executable
        .parent()
        .and_then(|parent| parent.parent())
        .and_then(|parent| parent.file_name())
    else {
        return false;
    };
    install_dir == LOCAL_INSTALL_DIR_NAME
}

fn local_commit_from_identity(identity: &str) -> Option<&str> {
    let suffix = identity.split_once(LOCAL_INSTALL_IDENTITY_MARKER)?.1;
    let commit = suffix
        .strip_suffix(LOCAL_INSTALL_DIRTY_SUFFIX)
        .unwrap_or(suffix);
    let is_hex = !commit.is_empty() && commit.bytes().all(|byte| byte.is_ascii_hexdigit());
    if !is_hex || commit.len() < 7 || commit.len() > 40 {
        return None;
    }
    Some(commit)
}

fn git_stdout(repo_root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?;
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    Some(value.to_owned())
}

fn git_repo_is_dirty(repo_root: &Path) -> Option<bool> {
    let output = Command::new("git")
        .args(["status", "--short", "--untracked-files=normal"])
        .current_dir(repo_root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?;
    Some(!value.trim().is_empty())
}

fn git_command_succeeds(repo_root: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .args(args)
        .current_dir(repo_root)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn display_version_prefix(version: &str) -> String {
    if version.starts_with('v') {
        version.to_owned()
    } else {
        format!("v{version}")
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{
        active_version, active_version_file_for, discover_repo_root_from_executable,
        display_version_prefix, git_repo_is_dirty, infer_repo_local_version,
        local_commit_from_identity, package_version, read_active_version_env,
        read_active_version_file_for, stale_repo_local_install_for, ACTIVE_VERSION_ENV,
    };

    #[test]
    fn active_version_file_uses_sibling_extension() {
        let executable = std::path::Path::new("/tmp/effigy");
        assert_eq!(
            active_version_file_for(executable),
            std::path::PathBuf::from("/tmp/effigy.active-version")
        );
    }

    #[test]
    fn read_active_version_file_trims_and_ignores_empty_values() {
        let temp = tempfile::tempdir().expect("tempdir");
        let executable = temp.path().join("effigy");
        std::fs::write(&executable, "").expect("write exe");
        std::fs::write(
            active_version_file_for(&executable),
            " v0.3.1+local.abc123 \n",
        )
        .expect("write active version");

        assert_eq!(
            read_active_version_file_for(&executable).as_deref(),
            Some("v0.3.1+local.abc123")
        );

        std::fs::write(active_version_file_for(&executable), "   \n").expect("write empty");
        assert!(read_active_version_file_for(&executable).is_none());
    }

    #[test]
    fn read_active_version_env_trims_and_ignores_empty_values() {
        let _guard = EnvGuard::set(ACTIVE_VERSION_ENV, " v0.3.1+local.abc123 \n");
        assert_eq!(
            read_active_version_env().as_deref(),
            Some("v0.3.1+local.abc123")
        );

        let _guard = EnvGuard::set(ACTIVE_VERSION_ENV, "   ");
        assert!(read_active_version_env().is_none());
    }

    #[test]
    fn active_version_prefers_explicit_env_override() {
        let _guard = EnvGuard::set(ACTIVE_VERSION_ENV, "v9.9.9+local.override");
        assert_eq!(active_version(), "v9.9.9+local.override");
    }

    #[test]
    fn discover_repo_root_from_executable_walks_up_to_cargo_and_git() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(temp.path().join("Cargo.toml"), "[package]\nname=\"x\"\n").expect("cargo");
        std::fs::create_dir(temp.path().join(".git")).expect("git dir");
        let exe = temp.path().join("target/debug/effigy");
        std::fs::create_dir_all(exe.parent().expect("parent")).expect("mkdir exe dir");
        std::fs::write(&exe, "").expect("exe");

        assert_eq!(
            discover_repo_root_from_executable(&exe),
            Some(temp.path().to_path_buf())
        );
    }

    #[test]
    fn infer_repo_local_version_returns_none_without_git_commit() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(temp.path().join("Cargo.toml"), "[package]\nname=\"x\"\n").expect("cargo");
        std::fs::create_dir(temp.path().join(".git")).expect("git dir");
        let exe = temp.path().join("target/debug/effigy");
        std::fs::create_dir_all(exe.parent().expect("parent")).expect("mkdir exe dir");
        std::fs::write(&exe, "").expect("exe");

        assert!(infer_repo_local_version(&exe).is_none());
    }

    #[test]
    fn git_repo_is_dirty_returns_none_without_git_commit() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(temp.path().join("Cargo.toml"), "[package]\nname=\"x\"\n").expect("cargo");
        std::fs::create_dir(temp.path().join(".git")).expect("git dir");
        assert!(git_repo_is_dirty(temp.path()).is_none());
    }

    #[test]
    fn display_version_prefix_is_idempotent_for_prefixed_values() {
        assert_eq!(display_version_prefix("0.3.1"), "v0.3.1");
        assert_eq!(
            display_version_prefix("v0.3.1+local.abc123"),
            "v0.3.1+local.abc123"
        );
    }

    #[test]
    fn stale_repo_local_install_reports_proven_ancestor_install() {
        let temp = tempfile::tempdir().expect("tempdir");
        let (root, ancestor, head) = checkout_with_two_commits(temp.path());
        let installed_identity = format!("v0.12.1+local.{}", &ancestor[..7]);
        let executable = write_local_install(&root, &installed_identity);
        let manifest = root.join("effigy.toml");

        let stale = stale_repo_local_install_for(&executable, &manifest).expect("stale install");
        assert_eq!(stale.executable, executable);
        assert_eq!(stale.repo_root, root);
        assert_eq!(stale.installed_identity, installed_identity);
        assert_eq!(
            stale.current_identity,
            format!(
                "{}+local.{}.dirty",
                display_version_prefix(package_version()),
                &head[..7]
            ),
            "fixture files stay untracked, so the checkout is dirty"
        );
    }

    #[test]
    fn stale_repo_local_install_preserves_recorded_dirty_identity() {
        let temp = tempfile::tempdir().expect("tempdir");
        let (root, ancestor, _head) = checkout_with_two_commits(temp.path());
        let installed_identity = format!("v0.12.1+local.{}.dirty", &ancestor[..7]);
        let executable = write_local_install(&root, &installed_identity);

        let stale =
            stale_repo_local_install_for(&executable, &root.join("effigy.toml")).expect("stale");
        assert_eq!(stale.installed_identity, installed_identity);
    }

    #[test]
    fn stale_repo_local_install_skips_current_install() {
        let temp = tempfile::tempdir().expect("tempdir");
        let (root, _ancestor, head) = checkout_with_two_commits(temp.path());
        let executable = write_local_install(&root, &format!("v0.12.1+local.{}.dirty", &head[..7]));

        assert!(
            stale_repo_local_install_for(&executable, &root.join("effigy.toml")).is_none(),
            "an install recorded at the current checkout revision is not stale"
        );
    }

    #[test]
    fn stale_repo_local_install_skips_descendant_commit() {
        let temp = tempfile::tempdir().expect("tempdir");
        let (root, _ancestor, head) = checkout_with_two_commits(temp.path());
        let descendant = commit_empty(&root, "third");
        assert!(git_ok(&root, &["reset", "-q", "--hard", &head]));
        let executable = write_local_install(&root, &format!("v0.12.1+local.{}", &descendant[..7]));

        assert!(
            stale_repo_local_install_for(&executable, &root.join("effigy.toml")).is_none(),
            "a recorded commit that is not an ancestor of the checkout cannot prove staleness"
        );
    }

    #[test]
    fn stale_repo_local_install_skips_unresolvable_recorded_commit() {
        let temp = tempfile::tempdir().expect("tempdir");
        let (root, _ancestor, _head) = checkout_with_two_commits(temp.path());
        let executable = write_local_install(&root, "v0.12.1+local.deadbee");

        assert!(
            stale_repo_local_install_for(&executable, &root.join("effigy.toml")).is_none(),
            "an unprovable recorded commit keeps the ordinary parse error"
        );
    }

    #[test]
    fn stale_repo_local_install_skips_unknown_local_identity() {
        let temp = tempfile::tempdir().expect("tempdir");
        let (root, _ancestor, _head) = checkout_with_two_commits(temp.path());
        let executable = write_local_install(&root, "v0.12.1+local.unknown");

        assert!(stale_repo_local_install_for(&executable, &root.join("effigy.toml")).is_none());
    }
    #[test]
    fn stale_repo_local_install_skips_missing_stamp() {
        let temp = tempfile::tempdir().expect("tempdir");
        let (root, _ancestor, _head) = checkout_with_two_commits(temp.path());
        let executable = root.join(".local-install/bin/effigy");
        std::fs::create_dir_all(executable.parent().expect("parent")).expect("mkdir");
        std::fs::write(&executable, "").expect("exe");

        assert!(stale_repo_local_install_for(&executable, &root.join("effigy.toml")).is_none());
    }

    #[test]
    fn stale_repo_local_install_skips_non_local_install_placement() {
        let temp = tempfile::tempdir().expect("tempdir");
        let (root, ancestor, _head) = checkout_with_two_commits(temp.path());
        let executable = root.join("target/debug/effigy");
        std::fs::create_dir_all(executable.parent().expect("parent")).expect("mkdir");
        std::fs::write(&executable, "").expect("exe");
        std::fs::write(
            active_version_file_for(&executable),
            format!("v0.12.1+local.{}\n", &ancestor[..7]),
        )
        .expect("stamp");

        assert!(
            stale_repo_local_install_for(&executable, &root.join("effigy.toml")).is_none(),
            "only the checkout's own .local-install/bin/effigy can be diagnosed"
        );
    }

    #[test]
    fn stale_repo_local_install_skips_foreign_or_non_repo_manifest() {
        let temp = tempfile::tempdir().expect("tempdir");
        let (root, ancestor, _head) = checkout_with_two_commits(temp.path());
        let executable = write_local_install(&root, &format!("v0.12.1+local.{}", &ancestor[..7]));

        let foreign = temp.path().join("consumer");
        std::fs::create_dir_all(&foreign).expect("mkdir");
        std::fs::write(foreign.join("Cargo.toml"), "[package]\nname=\"c\"\n").expect("cargo");
        std::fs::create_dir(foreign.join(".git")).expect("git dir");
        std::fs::write(foreign.join("effigy.toml"), "# consumer\n").expect("manifest");
        assert!(
            stale_repo_local_install_for(&executable, &foreign.join("effigy.toml")).is_none(),
            "a manifest outside the executable's checkout cannot prove staleness"
        );

        let plain = temp.path().join("plain-dir");
        std::fs::create_dir_all(&plain).expect("mkdir");
        std::fs::write(plain.join("effigy.toml"), "# plain\n").expect("manifest");
        assert!(
            stale_repo_local_install_for(&executable, &plain.join("effigy.toml")).is_none(),
            "a manifest outside any checkout keeps the ordinary parse error"
        );
    }

    #[test]
    fn local_commit_from_identity_accepts_only_hex_commit_suffixes() {
        assert_eq!(
            local_commit_from_identity("v0.3.1+local.abc1234"),
            Some("abc1234")
        );
        assert_eq!(
            local_commit_from_identity("v0.3.1+local.abc1234.dirty"),
            Some("abc1234")
        );
        assert_eq!(
            local_commit_from_identity("0.3.1+local.abcdef0123456789"),
            Some("abcdef0123456789")
        );
        assert_eq!(local_commit_from_identity("v0.3.1"), None);
        assert_eq!(local_commit_from_identity("v0.3.1+local."), None);
        assert_eq!(local_commit_from_identity("v0.3.1+local.unknown"), None);
        assert_eq!(local_commit_from_identity("v0.3.1+local.abc12"), None);
        assert_eq!(
            local_commit_from_identity(&format!("v0.3.1+local.{}", "a".repeat(41))),
            None
        );
        let forty = "a".repeat(40);
        assert_eq!(
            local_commit_from_identity(&format!("v0.3.1+local.{forty}")),
            Some(forty.as_str())
        );
    }

    struct EnvGuard {
        key: &'static str,
        previous: Option<String>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let previous = std::env::var(key).ok();
            unsafe {
                std::env::set_var(key, value);
            }
            Self { key, previous }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => unsafe {
                    std::env::set_var(self.key, value);
                },
                None => unsafe {
                    std::env::remove_var(self.key);
                },
            }
        }
    }

    /// Creates a checkout with two empty commits and returns the root, the
    /// first commit, and `HEAD`. Fixture files stay untracked so the checkout
    /// reports itself dirty, mirroring a live `.local-install` layout.
    fn checkout_with_two_commits(temp_root: &std::path::Path) -> (PathBuf, String, String) {
        let root = temp_root.join("checkout");
        std::fs::create_dir_all(root.join(".local-install/bin")).expect("mkdir install");
        std::fs::write(root.join("Cargo.toml"), "[package]\nname=\"x\"\n").expect("cargo");
        std::fs::write(root.join("effigy.toml"), "# fixture manifest\n").expect("manifest");
        assert!(git_ok(&root, &["init", "-q"]), "git init");
        let first = commit_empty(&root, "first");
        let head = commit_empty(&root, "second");
        (root, first, head)
    }

    /// Writes an executable and its sibling active-version stamp under the
    /// checkout's `.local-install/bin` and returns the executable path.
    fn write_local_install(root: &Path, recorded_identity: &str) -> PathBuf {
        let executable = root.join(".local-install/bin/effigy");
        std::fs::create_dir_all(executable.parent().expect("parent")).expect("mkdir bin");
        std::fs::write(&executable, "").expect("exe");
        std::fs::write(
            active_version_file_for(&executable),
            format!("{recorded_identity}\n"),
        )
        .expect("stamp");
        executable
    }

    fn commit_empty(repo_root: &Path, message: &str) -> String {
        assert!(git_ok(
            repo_root,
            &[
                "-c",
                "user.name=effigy-test",
                "-c",
                "user.email=effigy-test@example.com",
                "commit",
                "--allow-empty",
                "-q",
                "-m",
                message,
            ],
        ));
        git_out(repo_root, &["rev-parse", "HEAD"]).expect("head sha")
    }

    fn git_ok(repo_root: &Path, args: &[&str]) -> bool {
        std::process::Command::new("git")
            .args(args)
            .current_dir(repo_root)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn git_out(repo_root: &Path, args: &[&str]) -> Option<String> {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(repo_root)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let value = String::from_utf8(output.stdout).ok()?;
        let value = value.trim();
        if value.is_empty() {
            return None;
        }
        Some(value.to_owned())
    }
}
