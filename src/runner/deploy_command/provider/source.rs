use std::path::{Path, PathBuf};

use effigy_core::git_exec::run_git_output;
use effigy_core::git_source::{canonical_git_cache_identity, sanitize_cache_segment, sha256_hex};

use super::{DeployProviderFetch, DeployProviderProvenance};
use crate::runner::error::RunnerError;

pub(super) fn resolve_git_provider_source(
    repo_root: &Path,
    provider_name: &str,
    url: &str,
    reference: Option<&str>,
    rev: Option<&str>,
    fetch: DeployProviderFetch,
    warnings: &mut Vec<String>,
) -> Result<(PathBuf, DeployProviderProvenance), RunnerError> {
    let reference = reference.map(str::trim).filter(|value| !value.is_empty());
    let pinned = rev.map(|rev| pinned_rev(provider_name, rev)).transpose()?;
    let cache_key = match (&pinned, reference) {
        (Some(rev), _) => rev.clone(),
        (None, Some(reference)) => reference.to_owned(),
        (None, None) => "main".to_owned(),
    };
    if pinned.is_none() {
        let floating = reference.unwrap_or("main");
        warnings.push(format!(
            "deploy provider `{provider_name}` source uses floating ref `{floating}`; pin `rev` to a full commit SHA so plans and exports are reproducible"
        ));
    }
    let local_path = repo_root
        .join(".effigy")
        .join("cache/providers/git")
        .join(sha256_hex(canonical_git_cache_identity(url).as_bytes()))
        .join(sanitize_cache_segment(&cache_key));
    let resolution = ensure_git_checkout(GitCheckoutRequest {
        repo_root,
        provider_name,
        url,
        reference,
        pinned: pinned.as_deref(),
        fetch,
        local_path: &local_path,
    })?;
    let commit = git_head(&local_path)?;
    if let Some(pinned) = &pinned {
        if commit.as_deref() != Some(pinned.as_str()) {
            return Err(RunnerError::task_invocation(format!(
                "deploy provider `{provider_name}` checkout at {} resolved commit {} instead of pinned rev {pinned}",
                local_path.display(),
                commit.as_deref().unwrap_or("<none>")
            )));
        }
    }
    Ok((
        local_path,
        DeployProviderProvenance {
            kind: "git".to_owned(),
            url: Some(url.to_owned()),
            requested_ref: reference.map(str::to_owned),
            requested_rev: pinned,
            commit,
            resolution: resolution.as_str().to_owned(),
        },
    ))
}

fn pinned_rev(provider_name: &str, rev: &str) -> Result<String, RunnerError> {
    let rev = rev.trim().to_ascii_lowercase();
    let full_sha = matches!(rev.len(), 40 | 64) && rev.bytes().all(|byte| byte.is_ascii_hexdigit());
    if full_sha {
        return Ok(rev);
    }
    Err(RunnerError::task_invocation(format!(
        "deploy provider `{provider_name}` source `rev` must be a full commit SHA (40 hex characters), not `{rev}`; use `ref` for a moving branch or tag"
    )))
}

/// How a cached checkout was obtained for this resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GitCheckoutResolution {
    Cached,
    Fetched,
}

impl GitCheckoutResolution {
    fn as_str(self) -> &'static str {
        match self {
            Self::Cached => "cached",
            Self::Fetched => "fetched",
        }
    }
}

struct GitCheckoutRequest<'a> {
    repo_root: &'a Path,
    provider_name: &'a str,
    url: &'a str,
    reference: Option<&'a str>,
    pinned: Option<&'a str>,
    fetch: DeployProviderFetch,
    local_path: &'a Path,
}

fn ensure_git_checkout(
    request: GitCheckoutRequest<'_>,
) -> Result<GitCheckoutResolution, RunnerError> {
    let GitCheckoutRequest {
        repo_root,
        provider_name,
        url,
        reference,
        pinned,
        fetch,
        local_path,
    } = request;
    if local_path.exists() && !local_path.is_dir() {
        return Err(RunnerError::task_invocation(format!(
            "deploy provider `{provider_name}` cache path exists but is not a directory: {}",
            local_path.display()
        )));
    }
    let fresh_clone = !local_path.join(".git").is_dir();
    if fetch == DeployProviderFetch::Offline {
        if fresh_clone {
            return Err(RunnerError::task_invocation(offline_provider_message(
                provider_name,
                local_path,
                "there is no cached checkout for it",
            )));
        }
        let Some(pinned) = pinned else {
            // A floating preview uses whatever the cache holds. Materializing the
            // working tree from local objects needs no network.
            materialize_checkout(local_path, None)?;
            return Ok(GitCheckoutResolution::Cached);
        };
        let head = git_head(local_path)?;
        if head.as_deref() == Some(pinned) {
            materialize_checkout(local_path, None)?;
            return Ok(GitCheckoutResolution::Cached);
        }
        if git_rev_exists(local_path, pinned) {
            materialize_checkout(local_path, Some(pinned))?;
            return Ok(GitCheckoutResolution::Cached);
        }
        return Err(RunnerError::task_invocation(offline_provider_message(
            provider_name,
            local_path,
            &format!(
                "the cached checkout is at commit {}, not the pinned rev {pinned}",
                head.as_deref().unwrap_or("<none>")
            ),
        )));
    }
    if fresh_clone {
        if let Some(parent) = local_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                RunnerError::task_invocation(format!(
                    "failed to create deploy provider cache directory {}: {error}",
                    parent.display()
                ))
            })?;
        }
        run_git(
            repo_root,
            None,
            &["clone", "--no-checkout", url, &local_path.to_string_lossy()],
        )?;
    } else {
        run_git(
            local_path,
            Some(local_path),
            &["remote", "set-url", "origin", url],
        )?;
    }
    let Some(pinned) = pinned else {
        let reference = reference.unwrap_or("main");
        run_git(
            local_path,
            Some(local_path),
            &["fetch", "--depth", "1", "origin", reference],
        )?;
        run_git(
            local_path,
            Some(local_path),
            &["checkout", "--detach", "FETCH_HEAD"],
        )?;
        return Ok(GitCheckoutResolution::Fetched);
    };
    if !fresh_clone && git_head(local_path)?.as_deref() == Some(pinned) {
        materialize_checkout(local_path, None)?;
        return Ok(GitCheckoutResolution::Cached);
    }
    // A fresh clone carries every object, so the pin needs no fetch. Otherwise
    // fetch the configured ref when it can carry the pin, then the pin itself;
    // a server that refuses an unadvertised SHA still succeeds after the ref
    // fetch brought the commit with it.
    if !git_rev_exists(local_path, pinned) {
        let fetch_source = reference.unwrap_or(pinned);
        let fetched_ref = run_git(
            local_path,
            Some(local_path),
            &["fetch", "--depth", "1", "origin", fetch_source],
        )
        .is_ok();
        if !fetched_ref || !git_rev_exists(local_path, pinned) {
            run_git(
                local_path,
                Some(local_path),
                &["fetch", "--depth", "1", "origin", pinned],
            )?;
        }
    }
    materialize_checkout(local_path, Some(pinned))?;
    Ok(GitCheckoutResolution::Fetched)
}

/// Materialize the working tree for `commit` (or the cached HEAD) when a
/// previous run left the checkout empty. `provider.toml` is the package root's
/// required file, so for an already-correct HEAD its absence proves the working
/// tree is not materialized. An explicit commit is always checked out because
/// the cache may hold another commit's files.
fn materialize_checkout(local_path: &Path, commit: Option<&str>) -> Result<(), RunnerError> {
    if commit.is_none() && local_path.join("provider.toml").is_file() {
        return Ok(());
    }
    let target = commit.unwrap_or("HEAD");
    run_git(
        local_path,
        Some(local_path),
        &["checkout", "--detach", target],
    )
}

fn offline_provider_message(provider_name: &str, local_path: &Path, detail: &str) -> String {
    format!(
        "deploy provider `{provider_name}` cannot be resolved offline: {detail} at {}. Plan previews never fetch. Populate the cache with a networked `effigy deploy export {provider_name} --path <DIR>` (without `--plan`) or `effigy deploy status <ENV>` run, or configure a `path` source",
        local_path.display()
    )
}

fn git_head(local_path: &Path) -> Result<Option<String>, RunnerError> {
    let output = run_git_output(
        Some(local_path),
        &["rev-parse", "--verify", "--quiet", "HEAD"],
        |error| {
            RunnerError::task_invocation(format!(
                "failed to run git for deploy provider package in {}: {error}",
                local_path.display()
            ))
        },
        |detail| RunnerError::task_invocation(format!("{detail} for deploy provider package")),
    );
    match output {
        Ok(output) => {
            let head = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            Ok((!head.is_empty()).then_some(head))
        }
        Err(_) => Ok(None),
    }
}

fn git_rev_exists(local_path: &Path, reference: &str) -> bool {
    run_git_output(
        Some(local_path),
        &["rev-parse", "--verify", "--quiet", reference],
        |_| (),
        |_| (),
    )
    .is_ok()
}

fn run_git(manifest_root: &Path, cwd: Option<&Path>, args: &[&str]) -> Result<(), RunnerError> {
    run_git_output(
        cwd,
        args,
        |error| {
            RunnerError::task_invocation(format!(
                "failed to run git for deploy provider package in {}: {error}",
                manifest_root.display()
            ))
        },
        |detail| RunnerError::task_invocation(format!("{detail} for deploy provider package")),
    )
    .map(|_| ())
}


#[cfg(test)]
mod tests {
    use super::pinned_rev;

    #[test]
    fn a_pinned_rev_must_be_a_full_commit_sha() {
        let full = "0123456789abcdef0123456789abcdef01234567";
        assert_eq!(
            pinned_rev("acme", &full.to_ascii_uppercase()).expect("a full sha is a pin"),
            full
        );
        let error = pinned_rev("acme", "0123456").expect_err("a short sha is not a pin");
        let message = error.to_string();
        assert!(
            message.contains("source `rev` must be a full commit SHA"),
            "got: {message}"
        );
        assert!(
            message.contains("use `ref` for a moving branch or tag"),
            "the refusal names the alternative: {message}"
        );
    }
}
