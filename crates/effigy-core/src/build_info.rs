use std::path::{Path, PathBuf};
use std::process::Command;

const LOCAL_ACTIVE_VERSION_EXTENSION: &str = "active-version";

pub fn package_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn active_version() -> String {
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
    Some(format!(
        "{}+local.{hash}",
        display_version_prefix(package_version())
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
        active_version_file_for, discover_repo_root_from_executable, display_version_prefix,
        infer_repo_local_version, read_active_version_file_for,
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
    fn display_version_prefix_is_idempotent_for_prefixed_values() {
        assert_eq!(display_version_prefix("0.3.1"), "v0.3.1");
        assert_eq!(
            display_version_prefix("v0.3.1+local.abc123"),
            "v0.3.1+local.abc123"
        );
    }
}
