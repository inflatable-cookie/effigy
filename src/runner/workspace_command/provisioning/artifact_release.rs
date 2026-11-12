//! Fetch and authenticate published release artifacts under a per-target cache lock.

use super::artifact_acquisition::ArtifactFileLock;
use super::artifact_plan::LinuxWorkspaceTarget;
use super::artifact_receipt::{
    digest_matches, read_trimmed_identity_file, verify_guest_artifact,
    workspace_effigy_active_version_file, workspace_effigy_install_identity_file,
    workspace_effigy_release_install_identity, write_artifact_identity,
    write_workspace_artifact_receipt,
};
use crate::runner::workspace_client::context::{render_digest, sha256};
use crate::runner::workspace_command::{emit_workspace_info, WorkspaceOpProgress};
use crate::runner::RunnerError;
use effigy_ui::steps::ProgressFrame;
#[cfg(test)]
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
/// The published checksum the release download authenticates its bytes with.
const RELEASE_CHECKSUM_SUFFIX: &str = ".sha256";
const EFFIGY_RELEASE_REPO_BASE_URL: &str = "https://github.com/inflatable-cookie/effigy";

pub(super) fn ensure_downloaded_linux_workspace_effigy_artifact(
    target: LinuxWorkspaceTarget,
) -> Result<PathBuf, RunnerError> {
    let cache_path = linux_workspace_effigy_cache_path(target)?;
    let install_identity = workspace_effigy_release_install_identity();
    if let Some(cached) = current_release_cache(&cache_path, target, &install_identity)? {
        return Ok(cached);
    }

    let parent = cache_path.parent().ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "workspace linux effigy cache path has no parent: `{}`",
            cache_path.display()
        ))
    })?;
    std::fs::create_dir_all(parent).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to create workspace linux effigy cache directory `{}`: {error}",
            parent.display()
        ))
    })?;
    // One fetch per cache: a concurrent preparation waits here, then reuses the
    // bytes the first one verified and published, instead of racing its rename.
    let _fetch = ArtifactFileLock::acquire(
        &cache_path.with_extension("lock"),
        &format!(
            "linux workspace release download of `{}`",
            target.release_triple()
        ),
    )?;
    if let Some(cached) = current_release_cache(&cache_path, target, &install_identity)? {
        return Ok(cached);
    }

    let url = linux_workspace_effigy_release_url(target);
    emit_workspace_info(
        &format!("downloading linux effigy release artifact from `{url}`"),
        false,
    );
    let progress = WorkspaceOpProgress::start(false, format!("GET `{url}`"), false);
    progress.update(|_| {
        ProgressFrame::new(format!(
            "downloading linux effigy `{}`",
            target.release_triple()
        ))
    });
    let result = (|| {
        let digest = download_linux_workspace_effigy_release(&url, &cache_path, &progress)?;
        // The download records the receipt its consumer authenticates against, so
        // a cached artifact is never trusted on a version sidecar alone.
        progress.update(|_| {
            ProgressFrame::new("recording linux effigy artifact identity").waiting_for(format!(
                "write artifact receipt and identity in `{}`",
                parent.display()
            ))
        });
        write_workspace_artifact_receipt(&cache_path, target, &digest, "release-download")?;
        write_artifact_identity(&cache_path, &install_identity)?;
        Ok(())
    })();
    match &result {
        Ok(_) => progress.finish_clear(),
        Err(error) => progress.fail_error(error),
    }
    result.map(|()| cache_path)
}

/// The cached release artifact when its identity and receipt are current and its
/// bytes verify. Binds the sidecars to the verified bytes; never stamps stale ones.
fn current_release_cache(
    cache_path: &Path,
    target: LinuxWorkspaceTarget,
    install_identity: &str,
) -> Result<Option<PathBuf>, RunnerError> {
    if !linux_workspace_release_cache_is_current(cache_path, install_identity)
        || verify_guest_artifact(cache_path, target).is_err()
    {
        return Ok(None);
    }
    write_artifact_identity(cache_path, install_identity)?;
    Ok(Some(cache_path.to_path_buf()))
}

/// A cached release binary is reused only when its identity sidecar names this
/// release. Unlabelled bytes are downloaded again instead of being stamped current.
pub(super) fn linux_workspace_release_cache_is_current(
    cache_path: &Path,
    install_identity: &str,
) -> bool {
    cache_path.is_file()
        && read_trimmed_identity_file(&workspace_effigy_active_version_file(cache_path)).as_deref()
            == Some(effigy_core::build_info::active_version().as_str())
        && read_trimmed_identity_file(&workspace_effigy_install_identity_file(cache_path))
            .as_deref()
            == Some(install_identity)
}

pub(super) fn linux_workspace_effigy_cache_path(
    target: LinuxWorkspaceTarget,
) -> Result<PathBuf, RunnerError> {
    let home = std::env::var_os("HOME").ok_or_else(|| {
        RunnerError::task_invocation(
            "HOME is not set; cannot resolve workspace linux effigy cache path",
        )
    })?;
    Ok(Path::new(&home)
        .join(target.cache_relative_dir())
        .join(format!("v{}", env!("CARGO_PKG_VERSION")))
        .join(format!("effigy-{}", target.release_triple())))
}

pub(super) fn linux_workspace_effigy_release_url(target: LinuxWorkspaceTarget) -> String {
    #[cfg(test)]
    let base = RELEASE_FIXTURE_BASE_URL
        .with(|base| base.borrow().clone())
        .unwrap_or_else(|| EFFIGY_RELEASE_REPO_BASE_URL.to_owned());
    #[cfg(not(test))]
    let base = EFFIGY_RELEASE_REPO_BASE_URL.to_owned();
    format!(
        "{base}/releases/download/v{}/effigy-{}",
        env!("CARGO_PKG_VERSION"),
        target.release_triple()
    )
}

/// Download one release artifact and authenticate it against the checksum the
/// release publishes beside it. Returns the `sha256:<hex>` the bytes matched.
pub(super) fn download_linux_workspace_effigy_release(
    url: &str,
    dest: &Path,
    progress: &WorkspaceOpProgress,
) -> Result<String, RunnerError> {
    let client = reqwest::blocking::Client::builder()
        .build()
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to initialize release download client for `{url}`: {error}"
            ))
        })?;
    let bytes = release_get(&client, url).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to download linux effigy release artifact from `{url}`: {error}"
        ))
    })?;
    progress.update(|_| {
        ProgressFrame::new("fetching linux effigy release checksum")
            .waiting_for(format!("GET `{url}{RELEASE_CHECKSUM_SUFFIX}`"))
    });
    let expected = published_release_checksum(&client, url)?;
    progress.update(|_| {
        ProgressFrame::new("verifying linux effigy release checksum")
            .waiting_for("SHA-256 artifact digest comparison")
    });
    let digest = render_digest(sha256(&bytes));
    if !digest_matches(&digest, &expected) {
        return Err(RunnerError::task_invocation(format!(
            "linux effigy release artifact from `{url}` is {digest} but the published checksum is \
             sha256:{expected}; refusing to install bytes the release does not name"
        )));
    }

    let tmp_path = dest.with_extension("tmp");
    progress.update(|_| {
        ProgressFrame::new("caching verified linux effigy artifact")
            .waiting_for(format!("write and rename `{}`", tmp_path.display()))
    });
    std::fs::write(&tmp_path, &bytes).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to write linux effigy release artifact to `{}`: {error}",
            tmp_path.display()
        ))
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&tmp_path)
            .map_err(|error| {
                RunnerError::task_invocation(format!(
                    "failed to stat linux effigy release artifact `{}`: {error}",
                    tmp_path.display()
                ))
            })?
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&tmp_path, permissions).map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to mark linux effigy release artifact executable `{}`: {error}",
                tmp_path.display()
            ))
        })?;
    }
    std::fs::rename(&tmp_path, dest).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to move linux effigy release artifact into cache `{}`: {error}",
            dest.display()
        ))
    })?;
    Ok(digest)
}

/// One GET of a published release asset: its body, or the transport's refusal.
#[cfg(not(test))]
fn release_get(client: &reqwest::blocking::Client, url: &str) -> Result<Vec<u8>, String> {
    client
        .get(url)
        .send()
        .and_then(|response| response.error_for_status())
        .and_then(|response| response.bytes())
        .map(|bytes| bytes.to_vec())
        .map_err(|error| error.to_string())
}

// Tests serve release bodies from this table, so no download reaches the
// network. A URL the table does not name refuses like a failed transport.
#[cfg(test)]
thread_local! {
    pub(super) static RELEASE_FIXTURE_BASE_URL: std::cell::RefCell<Option<String>> =
        const { std::cell::RefCell::new(None) };
    pub(super) static RELEASE_FIXTURES: std::cell::RefCell<BTreeMap<String, Vec<u8>>> =
        const { std::cell::RefCell::new(BTreeMap::new()) };
}

#[cfg(test)]
fn release_get(client: &reqwest::blocking::Client, url: &str) -> Result<Vec<u8>, String> {
    // The checksum sidecar is a second GET of the same download, not a fetch.
    if !url.ends_with(RELEASE_CHECKSUM_SUFFIX) {
        RELEASE_FETCHES.with(|fetches| fetches.set(fetches.get() + 1));
    }
    if url.starts_with("http://127.0.0.1:") {
        return client
            .get(url)
            .send()
            .and_then(|response| response.error_for_status())
            .and_then(|response| response.bytes())
            .map(|bytes| bytes.to_vec())
            .map_err(|error| error.to_string());
    }
    let delay = RELEASE_FETCH_DELAY.with(std::cell::Cell::get);
    if !delay.is_zero() {
        std::thread::sleep(delay);
    }
    RELEASE_FIXTURES.with(|fixtures| {
        fixtures
            .borrow()
            .get(url)
            .cloned()
            .ok_or_else(|| format!("no test release fixture serves `{url}`"))
    })
}

// Test-only transport accounting: how many asset GETs this thread issued, and how long
// each takes, so a concurrency test can observe duplicate fetches.
#[cfg(test)]
thread_local! {
    pub(super) static RELEASE_FETCHES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    pub(super) static RELEASE_FETCH_DELAY: std::cell::Cell<std::time::Duration> =
        const { std::cell::Cell::new(std::time::Duration::ZERO) };
}

/// Authenticate against the published `<asset>.sha256`. Missing checksums refuse;
/// downloaded bytes and a self-written version sidecar never establish provenance.
fn published_release_checksum(
    client: &reqwest::blocking::Client,
    url: &str,
) -> Result<String, RunnerError> {
    let checksum_url = format!("{url}{RELEASE_CHECKSUM_SUFFIX}");
    let body = release_get(client, &checksum_url)
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "the release publishes no checksum at `{checksum_url}` ({error}); the workspace \
                 guest artifact cannot be authenticated. Rebuild it locally with \
                 `effigy workspace:linux:artifact`, or publish a checksum beside the release \
                 artifact"
            ))
        })
        .and_then(|bytes| {
            String::from_utf8(bytes).map_err(|error| {
                RunnerError::task_invocation(format!(
                    "the published checksum at `{checksum_url}` is not text: {error}"
                ))
            })
        })?;
    let asset = url.rsplit('/').next().unwrap_or_default();
    let line = body.strip_suffix('\n').ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "the published checksum at `{checksum_url}` is not one newline-terminated asset line"
        ))
    })?;
    let (digest, named_asset) = line.split_once("  ").ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "the published checksum at `{checksum_url}` does not use the release sidecar format"
        ))
    })?;
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || named_asset != asset
    {
        return Err(RunnerError::task_invocation(format!(
            "the published checksum at `{checksum_url}` does not name `{asset}` with a lowercase SHA-256 digest"
        )));
    }
    Ok(digest.to_owned())
}
