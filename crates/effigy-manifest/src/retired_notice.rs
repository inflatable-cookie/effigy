//! Where one invocation delivers a manifest's retired v0.15 config notice.
//!
//! The notice is not printed once per run any more: a repository that still
//! carries one retired section saw it on every command and every nested
//! selector. Now [`emit`] remembers, per checkout, which manifest digest it
//! already named (a small marker under `<checkout>/.effigy/`), and the
//! invocation decides the channel:
//!
//! - an ordinary command prints it to stderr once per checkout per manifest
//!   digest;
//! - `effigy doctor` prints it every time it runs, because a doctor report is
//!   where an operator looks for configuration drift;
//! - a `--json` invocation never writes it to stderr and carries it in the
//!   [`crate::retired_config`] notice list the command envelope reports as
//!   `warnings`;
//! - a nested Effigy invocation stays quiet, because its parent already said
//!   it. Parents mark nested invocations by setting the same
//!   `EFFIGY_INTERNAL_SUPPRESS_HEADER` marker they use to keep the CLI header
//!   from repeating. The marker routes this notice and the header only: a
//!   nested invocation that asked for `--json` still gets its
//!   `effigy.command.v1` envelope (effigy#330).
//!
//! The notice content is unchanged; only its delivery is throttled and routed.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use sha2::{Digest, Sha256};

use crate::retired_config::RetiredConfigDeclaration;

/// The marker a parent sets on a nested Effigy invocation.
const NESTED_INVOCATION_ENV: &str = "EFFIGY_INTERNAL_SUPPRESS_HEADER";

/// Checkout-local directory holding one marker per manifest digest already
/// named. Contract 052's store does not own it yet; this file owns it until
/// the store grows the notice record.
const NOTICE_DIR: &str = ".effigy/retired-config-notice";

/// Where this invocation delivers a retired-config notice.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RetiredNoticeDelivery {
    /// stderr, once per checkout per manifest digest.
    #[default]
    OncePerCheckout,
    /// stderr on every run (`effigy doctor`).
    Always,
    /// The JSON envelope's `warnings`; never stderr.
    JsonEnvelope,
}

#[derive(Debug, Default)]
struct Ledger {
    delivery: RetiredNoticeDelivery,
    /// Notices this process already printed or collected.
    seen: HashSet<String>,
    /// Notices waiting for the JSON envelope.
    pending: Vec<String>,
}

fn ledger() -> &'static Mutex<Ledger> {
    static LEDGER: OnceLock<Mutex<Ledger>> = OnceLock::new();
    LEDGER.get_or_init(|| Mutex::new(Ledger::default()))
}

fn lock_ledger() -> std::sync::MutexGuard<'static, Ledger> {
    match ledger().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Choose this invocation's notice channel. The binary calls this before it
/// answers a command; the default is the ordinary stderr notice.
pub fn set_delivery(delivery: RetiredNoticeDelivery) {
    lock_ledger().delivery = delivery;
}

/// Notices collected for the JSON envelope, drained once.
pub fn take_pending() -> Vec<String> {
    std::mem::take(&mut lock_ledger().pending)
}

/// Deliver one manifest's notice on this invocation's channel.
pub(crate) fn emit(manifest_path: &Path, declaration: &RetiredConfigDeclaration) {
    let Some(notice) = declaration.notice(manifest_path) else {
        return;
    };
    if is_nested_invocation() {
        return;
    }
    let mut ledger = lock_ledger();
    if ledger.seen.contains(&notice) {
        return;
    }
    match ledger.delivery {
        RetiredNoticeDelivery::JsonEnvelope => {
            ledger.seen.insert(notice.clone());
            ledger.pending.push(notice);
        }
        RetiredNoticeDelivery::OncePerCheckout => {
            if notice_already_named(manifest_path, &notice) {
                return;
            }
            record_notice(manifest_path, &notice);
            ledger.seen.insert(notice.clone());
            eprintln!("{notice}");
        }
        RetiredNoticeDelivery::Always => {
            ledger.seen.insert(notice.clone());
            eprintln!("{notice}");
        }
    }
}

fn is_nested_invocation() -> bool {
    std::env::var(NESTED_INVOCATION_ENV)
        .map(|value| matches!(value.trim(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

/// One marker per (checkout, manifest digest), so an edited manifest is named
/// again while an unchanged one stays quiet.
fn notice_marker_path(manifest_path: &Path, notice: &str) -> PathBuf {
    let digest = manifest_digest(manifest_path, notice);
    checkout_root(manifest_path).join(NOTICE_DIR).join(digest)
}

fn manifest_digest(manifest_path: &Path, notice: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(manifest_path.to_string_lossy().as_bytes());
    hasher.update([0]);
    match std::fs::read(manifest_path) {
        Ok(source) => hasher.update(source),
        Err(_) => hasher.update(notice.as_bytes()),
    }
    hasher.update([0]);
    hasher.update(notice.as_bytes());
    let digest = hasher.finalize();
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

/// The checkout that owns this manifest: the nearest ancestor with a Git
/// marker, else the directory the manifest lives in.
fn checkout_root(manifest_path: &Path) -> PathBuf {
    let mut current = manifest_path.parent();
    while let Some(directory) = current {
        if directory.join(".git").exists() {
            return directory.to_path_buf();
        }
        current = directory.parent();
    }
    manifest_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn notice_already_named(manifest_path: &Path, notice: &str) -> bool {
    notice_marker_path(manifest_path, notice).is_file()
}

/// Remember that this checkout has named the notice. A failure to write keeps
/// the notice printing rather than losing it.
fn record_notice(manifest_path: &Path, notice: &str) {
    let marker = notice_marker_path(manifest_path, notice);
    let Some(directory) = marker.parent() else {
        return;
    };
    if let Err(error) = std::fs::create_dir_all(directory) {
        if error.kind() != std::io::ErrorKind::AlreadyExists {
            return;
        }
    }
    let _ = std::fs::write(&marker, notice);
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{checkout_root, manifest_digest};

    #[test]
    fn the_checkout_walk_stops_at_the_git_marker_or_the_manifest_directory() {
        let root = tempfile::Builder::new()
            .prefix("effigy-retired-notice-")
            .tempdir()
            .expect("owned fixture root");
        let manifest = root.path().join("effigy.toml");
        std::fs::write(&manifest, "[tasks]\nbuild = \"true\"\n").expect("manifest");
        assert_eq!(checkout_root(&manifest), root.path());
        std::fs::create_dir(root.path().join(".git")).expect("git marker");
        let nested = root.path().join("apps/web");
        std::fs::create_dir_all(&nested).expect("nested manifest directory");
        assert_eq!(checkout_root(&nested.join("effigy.toml")), root.path());
    }

    #[test]
    fn one_manifest_digest_covers_its_path_its_bytes_and_its_notice() {
        let first = manifest_digest(Path::new("/repo/effigy.toml"), "notice one");
        assert_eq!(
            first,
            manifest_digest(Path::new("/repo/effigy.toml"), "notice one")
        );
        assert_ne!(
            first,
            manifest_digest(Path::new("/repo/effigy.toml"), "notice two")
        );
        assert_ne!(
            first,
            manifest_digest(Path::new("/repo/other.toml"), "notice one")
        );
    }
}
