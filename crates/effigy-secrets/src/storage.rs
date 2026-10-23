//! Vault persistence. The machine vault gate covers the complete mutation;
//! ciphertext and its passphrase/local-dev wraps are one generation part.
//! The optional local-dev key is stable until `secrets lock` removes it.
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use effigy_kernel::ctx::{
    CancelToken, Deadline, EffectClass, EffectContext, Limits, OperationContext, Placement,
};
use effigy_kernel::fs::{DirFd, FileName, FsError};
use effigy_kernel::gate::{Gate, GateError, GateGuard, Wait};
use effigy_kernel::id::{Generation, ResourceId, Scope};
use effigy_kernel::observe::{AbsenceProof, Observed};
use effigy_kernel::record::{self, Record, ReplaceOpts};
use effigy_kernel::root::StateRoot;
use effigy_kernel::store::StoreError;

use crate::{
    local_dev_unlock_key_path, LocalDevUnlockKey, VaultEnvelope, VaultFileError,
    VaultPlaintextPayload,
};

const MAX_VAULT_BYTES: u64 = 16 * 1024 * 1024;
const BUDGET: Duration = Duration::from_secs(60);

#[derive(Debug, thiserror::Error)]
pub enum VaultStorageError {
    #[error("vault I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    File(#[from] VaultFileError),
    #[error(transparent)]
    Fs(#[from] FsError),
    #[error(transparent)]
    Gate(#[from] GateError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("vault state unavailable: {0}")]
    Unavailable(String),
}

fn context() -> OperationContext {
    OperationContext::new(
        Deadline::At(Instant::now() + BUDGET),
        CancelToken::default(),
        Limits {
            capture_tail: 4096,
            log_rotate: 4096,
            log_keep: 2,
            line_max: 4096,
            queue_bytes: 4096,
            probe_concurrency: 1,
        },
        EffectClass::Mutate,
        Placement::Host,
    )
}
fn file_name(path: &Path) -> Result<FileName, VaultStorageError> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| VaultStorageError::Unavailable("invalid vault file name".into()))?;
    Ok(FileName::new(name)?)
}
fn parent(path: &Path) -> &Path {
    path.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
}
#[derive(Debug)]
struct Document(Vec<u8>);
impl Record for Document {
    const MAX_BYTES: u64 = MAX_VAULT_BYTES;
    fn decode(bytes: &[u8]) -> Result<Self, String> {
        Ok(Self(bytes.to_vec()))
    }
}

/// Reads either the legacy envelope or exactly one committed generation.
/// It never falls back from an unreadable pointer to an older generation.
pub fn read_envelope(path: &Path) -> Result<VaultEnvelope, VaultStorageError> {
    let dir = DirFd::open(&std::fs::canonicalize(parent(path))?)?;
    read_selected(&dir, &file_name(path)?).map(|(_, envelope)| envelope)
}
fn read_selected(dir: &DirFd, name: &FileName) -> Result<(u64, VaultEnvelope), VaultStorageError> {
    let observe = context()
        .observe()
        .map_err(|e| VaultStorageError::Unavailable(format!("{e:?}")))?;
    let raw = match record::read_record::<Document>(&observe, dir, name) {
        Observed::Verified(raw, _) => raw.0,
        Observed::Absent(AbsenceProof::NoFile { .. }) => {
            return Err(std::io::Error::from(std::io::ErrorKind::NotFound).into())
        }
        other => return Err(VaultStorageError::Unavailable(format!("{other:?}"))),
    };
    let json =
        std::str::from_utf8(&raw).map_err(|e| VaultStorageError::Unavailable(e.to_string()))?;
    // A legacy document is identified by its schema, not by a failed decrypt.
    let shape: serde_json::Value = serde_json::from_str(json).map_err(VaultFileError::from)?;
    if shape.get("schema").is_some() {
        return Ok((0, VaultEnvelope::from_json(json)?));
    }
    match record::read_generation(&observe, dir, name, MAX_VAULT_BYTES) {
        Observed::Verified(set, _) if set.parts.len() == 1 => {
            let (_, bytes) = &set.parts[0];
            let json = std::str::from_utf8(bytes)
                .map_err(|e| VaultStorageError::Unavailable(e.to_string()))?;
            Ok((set.generation.get(), VaultEnvelope::from_json(json)?))
        }
        other => Err(VaultStorageError::Unavailable(format!("{other:?}"))),
    }
}

/// A bounded, cross-process mutation. Acquire after prompts and before reading
/// the payload; retain through commit. The same gate is used by CLI and Rhai,
/// including callers that resolve the vault through a linked worktree.
#[derive(Debug)]
pub struct VaultWrite {
    cx: EffectContext,
    guard: GateGuard,
    dir: DirFd,
    name: FileName,
    path: PathBuf,
}
impl VaultWrite {
    pub fn acquire(path: &Path) -> Result<Self, VaultStorageError> {
        let home = StateRoot::home_from_env()
            .ok_or_else(|| VaultStorageError::Unavailable("an absolute HOME is required".into()))?;
        Self::acquire_in(&home, path)
    }
    /// Explicit HOME for isolated fixtures and embedders.
    pub fn acquire_in(home: &Path, path: &Path) -> Result<Self, VaultStorageError> {
        let cx = context()
            .effects()
            .map_err(|e| VaultStorageError::Unavailable(format!("{e:?}")))?;
        let root = StateRoot::machine_effects(&cx, home)?;
        let id = ResourceId::store(Scope::Machine, "vault").map_err(StoreError::from)?;
        let guard = Gate::acquire(&cx, &root, &[id], Wait::Until(Instant::now() + BUDGET))?;
        std::fs::create_dir_all(parent(path))?;
        let dir = DirFd::open(&std::fs::canonicalize(parent(path))?)?;
        Ok(Self {
            cx,
            guard,
            dir,
            name: file_name(path)?,
            path: path.to_path_buf(),
        })
    }
    pub fn read(&self) -> Result<VaultEnvelope, VaultStorageError> {
        read_selected(&self.dir, &self.name).map(|(_, envelope)| envelope)
    }
    /// Private auxiliary state uses the same gate and atomic replacement.
    pub fn write_private(&self, path: &Path, bytes: &[u8]) -> Result<(), VaultStorageError> {
        if parent(path) != parent(&self.path) {
            return Err(VaultStorageError::Unavailable(
                "auxiliary vault state must share its directory".into(),
            ));
        }
        record::atomic_replace(
            &self.cx,
            &self.guard,
            &self.dir,
            &file_name(path)?,
            bytes,
            ReplaceOpts::default(),
        )?;
        Ok(())
    }
    pub fn commit(&self, envelope: &VaultEnvelope) -> Result<(), VaultStorageError> {
        let generation = match read_selected(&self.dir, &self.name) {
            Ok((generation, _)) => generation.checked_add(1).ok_or_else(|| {
                VaultStorageError::Unavailable("vault generation exhausted".into())
            })?,
            Err(VaultStorageError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => 1,
            Err(error) => return Err(error),
        };
        let rendered = envelope.to_json_pretty()?;
        let stem = FileName::new(&format!("{}.ciphertext", self.name))?;
        record::commit_generation(
            &self.cx,
            &self.guard,
            &self.dir,
            &self.name,
            Generation::new(generation),
            &[(&stem, rendered.as_bytes())],
            MAX_VAULT_BYTES,
        )?;
        Ok(())
    }
    /// CLI writes keep the existing local-dev key. A missing key is installed
    /// atomically before selecting the new envelope; the old passphrase
    /// ciphertext stays readable even if that first installation is killed.
    pub fn write_payload(
        &self,
        payload: &VaultPlaintextPayload,
        passphrase: &str,
    ) -> Result<(), VaultStorageError> {
        let key_path = local_dev_unlock_key_path(&self.path);
        let key = match std::fs::read(&key_path) {
            Ok(bytes) => LocalDevUnlockKey::from_bytes(bytes)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let key = LocalDevUnlockKey::generate()?;
                self.write_private(&key_path, key.expose())?;
                key
            }
            Err(e) => return Err(e.into()),
        };
        self.commit(&payload.encrypt_with_passphrase_and_local_dev_key(passphrase, &key)?)
    }
}

#[cfg(test)]
mod tests;
