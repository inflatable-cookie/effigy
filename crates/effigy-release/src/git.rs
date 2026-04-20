use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use crate::ReleaseError;

pub fn git_modified_files(repo_root: &Path) -> Result<Vec<String>, ReleaseError> {
    let repo_check = ProcessCommand::new("git")
        .arg("-C")
        .arg(repo_root)
        .arg("rev-parse")
        .arg("--is-inside-work-tree")
        .output()
        .map_err(|error| {
            ReleaseError::TaskInvocation(format!("failed to inspect git repository: {error}"))
        })?;
    if !repo_check.status.success() || String::from_utf8_lossy(&repo_check.stdout).trim() != "true"
    {
        return Err(ReleaseError::TaskInvocation(format!(
            "release execute requires a git work tree at {}",
            repo_root.display()
        )));
    }

    let output = ProcessCommand::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["status", "--porcelain=v1", "--untracked-files=all"])
        .output()
        .map_err(|error| {
            ReleaseError::TaskInvocation(format!("failed to inspect git working tree: {error}"))
        })?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(ReleaseError::TaskInvocation(if detail.is_empty() {
            "failed to inspect git working tree".to_owned()
        } else {
            format!("failed to inspect git working tree: {detail}")
        }));
    }

    let stdout = String::from_utf8(output.stdout).map_err(|error| {
        ReleaseError::TaskInvocation(format!("git status output was not utf-8: {error}"))
    })?;
    let mut paths = stdout
        .lines()
        .filter_map(parse_git_status_path)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    paths.sort();
    Ok(paths)
}

pub fn git_current_branch(repo_root: &Path) -> Result<String, ReleaseError> {
    let output = ProcessCommand::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["symbolic-ref", "--quiet", "--short", "HEAD"])
        .output()
        .map_err(|error| {
            ReleaseError::TaskInvocation(format!("failed to resolve current branch: {error}"))
        })?;
    if !output.status.success() {
        return Err(ReleaseError::TaskInvocation(
            "release execute requires a checked-out branch".to_owned(),
        ));
    }
    let branch = String::from_utf8(output.stdout).map_err(|error| {
        ReleaseError::TaskInvocation(format!("git branch output was not utf-8: {error}"))
    })?;
    let trimmed = branch.trim();
    if trimmed.is_empty() {
        Err(ReleaseError::TaskInvocation(
            "release execute requires a checked-out branch".to_owned(),
        ))
    } else {
        Ok(trimmed.to_owned())
    }
}

pub fn git_head_sha(repo_root: &Path) -> Result<String, ReleaseError> {
    let output = ProcessCommand::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(|error| {
            ReleaseError::TaskInvocation(format!("failed to resolve current HEAD: {error}"))
        })?;
    if !output.status.success() {
        return Err(ReleaseError::TaskInvocation(
            "release execute requires a readable current HEAD".to_owned(),
        ));
    }
    let sha = String::from_utf8(output.stdout).map_err(|error| {
        ReleaseError::TaskInvocation(format!("git HEAD output was not utf-8: {error}"))
    })?;
    let trimmed = sha.trim();
    if trimmed.is_empty() {
        Err(ReleaseError::TaskInvocation(
            "release execute requires a readable current HEAD".to_owned(),
        ))
    } else {
        Ok(trimmed.to_owned())
    }
}

pub fn git_remote_url(repo_root: &Path, remote: &str) -> Result<String, ReleaseError> {
    let output = ProcessCommand::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["remote", "get-url", remote])
        .output()
        .map_err(|error| {
            ReleaseError::TaskInvocation(format!(
                "failed to inspect git remote `{remote}`: {error}"
            ))
        })?;
    if !output.status.success() {
        return Err(ReleaseError::TaskInvocation(format!(
            "release execute requires a configured `{remote}` remote"
        )));
    }
    let url = String::from_utf8(output.stdout).map_err(|error| {
        ReleaseError::TaskInvocation(format!("git remote output was not utf-8: {error}"))
    })?;
    let trimmed = url.trim();
    if trimmed.is_empty() {
        Err(ReleaseError::TaskInvocation(format!(
            "release execute requires a configured `{remote}` remote"
        )))
    } else {
        Ok(trimmed.to_owned())
    }
}

pub fn git_tag_exists(repo_root: &Path, tag: &str) -> Result<bool, ReleaseError> {
    let output = ProcessCommand::new("git")
        .arg("-C")
        .arg(repo_root)
        .args([
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/tags/{tag}"),
        ])
        .output()
        .map_err(|error| {
            ReleaseError::TaskInvocation(format!("failed to inspect local git tags: {error}"))
        })?;
    Ok(output.status.success())
}

pub fn git_add_release_files(repo_root: &Path, files: &[PathBuf]) -> Result<(), ReleaseError> {
    let mut command = ProcessCommand::new("git");
    command.arg("-C").arg(repo_root).arg("add");
    for path in files {
        let relative = path.strip_prefix(repo_root).unwrap_or(path);
        command.arg(relative);
    }
    let output = command.output().map_err(|error| {
        ReleaseError::TaskInvocation(format!("failed to stage release files: {error}"))
    })?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(ReleaseError::TaskInvocation(if stderr.is_empty() {
            "failed to stage release files".to_owned()
        } else {
            format!("failed to stage release files: {stderr}")
        }))
    }
}

pub fn git_commit_release(repo_root: &Path, message: &str) -> Result<String, ReleaseError> {
    let output = ProcessCommand::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["commit", "-m", message])
        .output()
        .map_err(|error| {
            ReleaseError::TaskInvocation(format!("failed to create release commit: {error}"))
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(ReleaseError::TaskInvocation(if stderr.is_empty() {
            "failed to create release commit".to_owned()
        } else {
            format!("failed to create release commit: {stderr}")
        }));
    }

    let rev = ProcessCommand::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(|error| {
            ReleaseError::TaskInvocation(format!("failed to read release commit sha: {error}"))
        })?;
    if !rev.status.success() {
        return Err(ReleaseError::TaskInvocation(
            "failed to read release commit sha".to_owned(),
        ));
    }
    let sha = String::from_utf8(rev.stdout).map_err(|error| {
        ReleaseError::TaskInvocation(format!("git rev-parse output was not utf-8: {error}"))
    })?;
    Ok(sha.trim().to_owned())
}

pub fn git_create_tag(repo_root: &Path, tag: &str) -> Result<(), ReleaseError> {
    let output = ProcessCommand::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["tag", tag])
        .output()
        .map_err(|error| {
            ReleaseError::TaskInvocation(format!("failed to create release tag `{tag}`: {error}"))
        })?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(ReleaseError::TaskInvocation(if stderr.is_empty() {
            format!("failed to create release tag `{tag}`")
        } else {
            format!("failed to create release tag `{tag}`: {stderr}")
        }))
    }
}

pub fn git_push_release(
    repo_root: &Path,
    branch: &str,
    remote: &str,
    tag: Option<&str>,
) -> Result<(), ReleaseError> {
    let branch_output = ProcessCommand::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["push", remote, branch])
        .output()
        .map_err(|error| {
            ReleaseError::TaskInvocation(format!(
                "failed to push release branch to `{remote}`: {error}"
            ))
        })?;
    if !branch_output.status.success() {
        let stderr = String::from_utf8_lossy(&branch_output.stderr)
            .trim()
            .to_owned();
        return Err(ReleaseError::TaskInvocation(if stderr.is_empty() {
            format!("failed to push release branch to `{remote}`")
        } else {
            format!("failed to push release branch to `{remote}`: {stderr}")
        }));
    }

    if let Some(tag) = tag {
        let tag_output = ProcessCommand::new("git")
            .arg("-C")
            .arg(repo_root)
            .args(["push", remote, tag])
            .output()
            .map_err(|error| {
                ReleaseError::TaskInvocation(format!(
                    "failed to push release tag `{tag}` to `{remote}`: {error}"
                ))
            })?;
        if !tag_output.status.success() {
            let stderr = String::from_utf8_lossy(&tag_output.stderr)
                .trim()
                .to_owned();
            return Err(ReleaseError::TaskInvocation(if stderr.is_empty() {
                format!("failed to push release tag `{tag}` to `{remote}`")
            } else {
                format!("failed to push release tag `{tag}` to `{remote}`: {stderr}")
            }));
        }
    }

    Ok(())
}

fn parse_git_status_path(line: &str) -> Option<String> {
    let raw_path = line.get(3..)?.trim();
    if raw_path.is_empty() {
        return None;
    }
    let path = raw_path
        .split_once(" -> ")
        .map(|(_, new_path)| new_path)
        .unwrap_or(raw_path)
        .trim();
    if path.is_empty() {
        None
    } else {
        Some(path.to_owned())
    }
}
