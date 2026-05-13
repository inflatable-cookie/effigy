use std::path::{Path, PathBuf};
use std::process::Command;

const ACTIVE_VERSION_ENV: &str = "EFFIGY_ACTIVE_VERSION";
const LOCAL_ACTIVE_VERSION_EXTENSION: &str = "active-version";

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
    executable.ancestors().find_map(|ancestor| {
        let cargo_toml = ancestor.join("Cargo.toml");
        let git_dir = ancestor.join(".git");
        if cargo_toml.is_file() && git_dir.exists() {
            Some(ancestor.to_path_buf())
        } else {
            None
        }
    })
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

fn display_version_prefix(version: &str) -> String {
    if version.starts_with('v') {
        version.to_owned()
    } else {
        format!("v{version}")
    }
}

#[cfg(test)]
mod tests {
    use super::{
        active_version, active_version_file_for, discover_repo_root_from_executable,
        display_version_prefix, git_repo_is_dirty, infer_repo_local_version,
        read_active_version_env, read_active_version_file_for, ACTIVE_VERSION_ENV,
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
}
