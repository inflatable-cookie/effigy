//! Secret vault domain models.
//!
//! This crate owns the storage and redaction types for Effigy's built-in
//! secret vault. It deliberately does not expose CLI behavior or runtime
//! injection. Encryption is added in the next `g05.003` card.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::Path;

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use zeroize::Zeroizing;

pub const VAULT_SCHEMA: &str = "effigy.secrets.vault.v1";
pub const VAULT_SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_MAX_UNIX_MODE: u32 = 0o600;
pub const DEFAULT_KDF: &str = "argon2id";
pub const DEFAULT_CIPHER: &str = "xchacha20poly1305";
pub const DEFAULT_SALT_LEN: usize = 32;
pub const XCHACHA20POLY1305_NONCE_LEN: usize = 24;
pub const VAULT_KEY_LEN: usize = 32;
pub const ARGON2_M_COST_KIB: u32 = 19_456;
pub const ARGON2_T_COST: u32 = 2;
pub const ARGON2_P_COST: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum VaultFileError {
    #[error("failed to parse vault document: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported vault schema `{actual}` (expected `{expected}`)")]
    UnsupportedSchema {
        expected: &'static str,
        actual: String,
    },
    #[error("unsupported vault schema version `{actual}` (expected `{expected}`)")]
    UnsupportedSchemaVersion { expected: u32, actual: u32 },
    #[error("vault ciphertext is empty")]
    EmptyCiphertext,
    #[error("vault salt is empty")]
    EmptySalt,
    #[error("vault nonce is empty")]
    EmptyNonce,
    #[error("unsupported vault kdf `{actual}` (expected `{expected}`)")]
    UnsupportedKdf {
        expected: &'static str,
        actual: String,
    },
    #[error("unsupported vault cipher `{actual}` (expected `{expected}`)")]
    UnsupportedCipher {
        expected: &'static str,
        actual: String,
    },
    #[error("invalid vault nonce length `{actual}` (expected `{expected}`)")]
    InvalidNonceLength { expected: usize, actual: usize },
    #[error("failed to derive vault key")]
    KeyDerivation,
    #[error("failed to encrypt vault payload")]
    Encrypt,
    #[error("failed to decrypt vault payload")]
    Decrypt,
    #[error("failed to decode vault payload: {0}")]
    PayloadJson(serde_json::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultEnvelope {
    pub schema: String,
    pub schema_version: u32,
    pub algorithms: VaultAlgorithms,
    pub kdf: VaultKdf,
    pub cipher: VaultCipher,
    pub payload: EncryptedVaultPayload,
}

impl VaultEnvelope {
    pub fn new(
        algorithms: VaultAlgorithms,
        kdf: VaultKdf,
        cipher: VaultCipher,
        payload: EncryptedVaultPayload,
    ) -> Self {
        Self {
            schema: VAULT_SCHEMA.to_owned(),
            schema_version: VAULT_SCHEMA_VERSION,
            algorithms,
            kdf,
            cipher,
            payload,
        }
    }

    pub fn from_json(input: &str) -> Result<Self, VaultFileError> {
        let envelope: Self = serde_json::from_str(input)?;
        envelope.validate()?;
        Ok(envelope)
    }

    pub fn to_json_pretty(&self) -> Result<String, VaultFileError> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn validate(&self) -> Result<(), VaultFileError> {
        if self.schema != VAULT_SCHEMA {
            return Err(VaultFileError::UnsupportedSchema {
                expected: VAULT_SCHEMA,
                actual: self.schema.clone(),
            });
        }
        if self.schema_version != VAULT_SCHEMA_VERSION {
            return Err(VaultFileError::UnsupportedSchemaVersion {
                expected: VAULT_SCHEMA_VERSION,
                actual: self.schema_version,
            });
        }
        if self.kdf.salt.is_empty() {
            return Err(VaultFileError::EmptySalt);
        }
        if self.cipher.nonce.is_empty() {
            return Err(VaultFileError::EmptyNonce);
        }
        if self.payload.ciphertext.is_empty() {
            return Err(VaultFileError::EmptyCiphertext);
        }
        if self.algorithms.kdf != DEFAULT_KDF || self.kdf.name != DEFAULT_KDF {
            return Err(VaultFileError::UnsupportedKdf {
                expected: DEFAULT_KDF,
                actual: self.kdf.name.clone(),
            });
        }
        if self.algorithms.cipher != DEFAULT_CIPHER || self.cipher.name != DEFAULT_CIPHER {
            return Err(VaultFileError::UnsupportedCipher {
                expected: DEFAULT_CIPHER,
                actual: self.cipher.name.clone(),
            });
        }
        if self.cipher.nonce.len() != XCHACHA20POLY1305_NONCE_LEN {
            return Err(VaultFileError::InvalidNonceLength {
                expected: XCHACHA20POLY1305_NONCE_LEN,
                actual: self.cipher.nonce.len(),
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultAlgorithms {
    pub kdf: String,
    pub cipher: String,
}

impl VaultAlgorithms {
    pub fn new(kdf: impl Into<String>, cipher: impl Into<String>) -> Self {
        Self {
            kdf: kdf.into(),
            cipher: cipher.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultKdf {
    pub name: String,
    #[serde(default)]
    pub params: BTreeMap<String, serde_json::Value>,
    pub salt: Vec<u8>,
}

impl VaultKdf {
    pub fn new(name: impl Into<String>, salt: Vec<u8>) -> Self {
        Self {
            name: name.into(),
            params: BTreeMap::new(),
            salt,
        }
    }

    pub fn argon2id_default(salt: Vec<u8>) -> Self {
        let mut params = BTreeMap::new();
        params.insert(
            "m_cost_kib".to_owned(),
            serde_json::Value::from(ARGON2_M_COST_KIB),
        );
        params.insert("t_cost".to_owned(), serde_json::Value::from(ARGON2_T_COST));
        params.insert("p_cost".to_owned(), serde_json::Value::from(ARGON2_P_COST));
        Self {
            name: DEFAULT_KDF.to_owned(),
            params,
            salt,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultCipher {
    pub name: String,
    pub nonce: Vec<u8>,
}

impl VaultCipher {
    pub fn new(name: impl Into<String>, nonce: Vec<u8>) -> Self {
        Self {
            name: name.into(),
            nonce,
        }
    }

    pub fn xchacha20poly1305(nonce: Vec<u8>) -> Self {
        Self {
            name: DEFAULT_CIPHER.to_owned(),
            nonce,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncryptedVaultPayload {
    pub ciphertext: Vec<u8>,
}

impl EncryptedVaultPayload {
    pub fn new(ciphertext: Vec<u8>) -> Self {
        Self { ciphertext }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct SecretValue {
    inner: Zeroizing<String>,
}

impl SecretValue {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            inner: Zeroizing::new(value.into()),
        }
    }

    pub fn expose(&self) -> &str {
        &self.inner
    }
}

impl fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretValue([REDACTED])")
    }
}

impl fmt::Display for SecretValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

impl Serialize for SecretValue {
    /// Serializes as `[REDACTED]`, never the plaintext.
    ///
    /// This is deliberate: a `SecretValue` reachable through a generic
    /// `serde_json::to_string`/`to_vec` of some enclosing struct (logs, JSON
    /// output, debug dumps) must not leak the secret. The only place that needs
    /// the real bytes serialized — the encrypted vault payload — opts in
    /// explicitly via [`secret_plaintext_serde`] on `VaultSecretRecord::value`.
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str("[REDACTED]")
    }
}

/// Explicit plaintext (de)serialization for a `SecretValue`.
///
/// Used **only** by the vault payload, where the real secret bytes must be
/// serialized so they can be encrypted. Every other serialization of a
/// `SecretValue` goes through its redacting [`Serialize`] impl, so a secret can
/// only reach serialized output through this deliberate, vault-internal path.
pub(crate) mod secret_plaintext_serde {
    use super::SecretValue;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(value: &SecretValue, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(value.expose())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<SecretValue, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer).map(SecretValue::new)
    }
}

impl<'de> Deserialize<'de> for SecretValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer).map(Self::new)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultPlaintextPayload {
    pub records: BTreeMap<String, VaultSecretRecord>,
}

impl VaultPlaintextPayload {
    pub fn empty() -> Self {
        Self {
            records: BTreeMap::new(),
        }
    }

    pub fn encrypt_with_passphrase(
        &self,
        passphrase: &str,
    ) -> Result<VaultEnvelope, VaultFileError> {
        let mut salt = vec![0; DEFAULT_SALT_LEN];
        OsRng.fill_bytes(&mut salt);
        let mut nonce = vec![0; XCHACHA20POLY1305_NONCE_LEN];
        OsRng.fill_bytes(&mut nonce);
        self.encrypt_with_material(passphrase, salt, nonce)
    }

    pub fn encrypt_with_material(
        &self,
        passphrase: &str,
        salt: Vec<u8>,
        nonce: Vec<u8>,
    ) -> Result<VaultEnvelope, VaultFileError> {
        let kdf = VaultKdf::argon2id_default(salt);
        let cipher = VaultCipher::xchacha20poly1305(nonce);
        let key = derive_vault_key(passphrase, &kdf)?;
        let plaintext = serde_json::to_vec(self).map_err(VaultFileError::PayloadJson)?;
        let nonce = xnonce_from_cipher(&cipher)?;
        let key = Key::from(key);
        let aead = XChaCha20Poly1305::new(&key);
        let ciphertext = aead
            .encrypt(nonce, plaintext.as_ref())
            .map_err(|_| VaultFileError::Encrypt)?;
        Ok(VaultEnvelope::new(
            VaultAlgorithms::new(DEFAULT_KDF, DEFAULT_CIPHER),
            kdf,
            cipher,
            EncryptedVaultPayload::new(ciphertext),
        ))
    }
}

impl VaultEnvelope {
    pub fn decrypt_with_passphrase(
        &self,
        passphrase: &str,
    ) -> Result<VaultPlaintextPayload, VaultFileError> {
        self.validate()?;
        let key = derive_vault_key(passphrase, &self.kdf)?;
        let nonce = xnonce_from_cipher(&self.cipher)?;
        let key = Key::from(key);
        let aead = XChaCha20Poly1305::new(&key);
        let plaintext = aead
            .decrypt(nonce, self.payload.ciphertext.as_ref())
            .map_err(|_| VaultFileError::Decrypt)?;
        serde_json::from_slice(&plaintext).map_err(VaultFileError::PayloadJson)
    }
}

fn derive_vault_key(
    passphrase: &str,
    kdf: &VaultKdf,
) -> Result<[u8; VAULT_KEY_LEN], VaultFileError> {
    if kdf.name != DEFAULT_KDF {
        return Err(VaultFileError::UnsupportedKdf {
            expected: DEFAULT_KDF,
            actual: kdf.name.clone(),
        });
    }
    if kdf.salt.is_empty() {
        return Err(VaultFileError::EmptySalt);
    }
    let params = Params::new(
        ARGON2_M_COST_KIB,
        ARGON2_T_COST,
        ARGON2_P_COST,
        Some(VAULT_KEY_LEN),
    )
    .map_err(|_| VaultFileError::KeyDerivation)?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = [0_u8; VAULT_KEY_LEN];
    argon2
        .hash_password_into(passphrase.as_bytes(), &kdf.salt, &mut key)
        .map_err(|_| VaultFileError::KeyDerivation)?;
    Ok(key)
}

fn xnonce_from_cipher(cipher: &VaultCipher) -> Result<&XNonce, VaultFileError> {
    if cipher.name != DEFAULT_CIPHER {
        return Err(VaultFileError::UnsupportedCipher {
            expected: DEFAULT_CIPHER,
            actual: cipher.name.clone(),
        });
    }
    <&XNonce>::try_from(cipher.nonce.as_slice()).map_err(|_| VaultFileError::InvalidNonceLength {
        expected: XCHACHA20POLY1305_NONCE_LEN,
        actual: cipher.nonce.len(),
    })
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultSecretRecord {
    #[serde(with = "secret_plaintext_serde")]
    pub value: SecretValue,
}

impl VaultSecretRecord {
    pub fn new(value: SecretValue) -> Self {
        Self { value }
    }
}

impl fmt::Debug for VaultSecretRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VaultSecretRecord")
            .field("value", &"[REDACTED]")
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VaultPermissionStatus {
    Safe,
    Unsafe { mode: u32, max_mode: u32 },
    UnsupportedPlatform,
}

impl VaultPermissionStatus {
    pub fn is_safe(&self) -> bool {
        matches!(self, Self::Safe | Self::UnsupportedPlatform)
    }
}

#[cfg(unix)]
pub fn inspect_vault_permissions(path: &Path) -> std::io::Result<VaultPermissionStatus> {
    use std::os::unix::fs::PermissionsExt;

    let mode = fs::metadata(path)?.permissions().mode() & 0o777;
    if mode & !DEFAULT_MAX_UNIX_MODE == 0 {
        Ok(VaultPermissionStatus::Safe)
    } else {
        Ok(VaultPermissionStatus::Unsafe {
            mode,
            max_mode: DEFAULT_MAX_UNIX_MODE,
        })
    }
}

#[cfg(not(unix))]
pub fn inspect_vault_permissions(_path: &Path) -> std::io::Result<VaultPermissionStatus> {
    Ok(VaultPermissionStatus::UnsupportedPlatform)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope_fixture() -> VaultEnvelope {
        VaultEnvelope::new(
            VaultAlgorithms::new(DEFAULT_KDF, DEFAULT_CIPHER),
            VaultKdf::argon2id_default(vec![1, 2, 3, 4]),
            VaultCipher::xchacha20poly1305(vec![5; XCHACHA20POLY1305_NONCE_LEN]),
            EncryptedVaultPayload::new(vec![8, 9, 10]),
        )
    }

    #[test]
    fn vault_envelope_round_trips_through_json() {
        let envelope = envelope_fixture();
        let json = envelope.to_json_pretty().expect("serialize");
        let parsed = VaultEnvelope::from_json(&json).expect("parse");
        assert_eq!(parsed, envelope);
    }

    #[test]
    fn vault_envelope_rejects_wrong_schema() {
        let mut envelope = envelope_fixture();
        envelope.schema = "wrong.schema".to_owned();
        let json = envelope.to_json_pretty().expect("serialize");
        let error = VaultEnvelope::from_json(&json).expect_err("schema should fail");
        assert_eq!(
            error.to_string(),
            "unsupported vault schema `wrong.schema` (expected `effigy.secrets.vault.v1`)"
        );
    }

    #[test]
    fn vault_envelope_rejects_empty_ciphertext() {
        let mut envelope = envelope_fixture();
        envelope.payload.ciphertext.clear();
        let error = envelope.validate().expect_err("ciphertext should fail");
        assert_eq!(error.to_string(), "vault ciphertext is empty");
    }

    #[test]
    fn secret_values_redact_display_and_debug() {
        let secret = SecretValue::new("postgres://secret-value");
        assert_eq!(secret.to_string(), "[REDACTED]");
        assert_eq!(format!("{secret:?}"), "SecretValue([REDACTED])");
        assert_eq!(secret.expose(), "postgres://secret-value");
    }

    #[test]
    fn vault_record_debug_redacts_value() {
        let record = VaultSecretRecord::new(SecretValue::new("hunter2"));
        let debug = format!("{record:?}");
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("hunter2"));
    }

    #[test]
    fn secret_value_serialize_redacts_plaintext() {
        let secret = SecretValue::new("sk-live-do-not-leak");
        let json = serde_json::to_string(&secret).expect("serialize");
        assert_eq!(json, "\"[REDACTED]\"");
        assert!(!json.contains("sk-live-do-not-leak"));
    }

    #[test]
    fn enclosing_struct_serialize_does_not_leak_secret() {
        // A struct that derives Serialize and happens to hold a SecretValue —
        // the exact accidental-egress shape the redacting impl guards against.
        #[derive(Serialize)]
        struct Diagnostic {
            name: String,
            token: SecretValue,
        }

        let payload = Diagnostic {
            name: "api".to_owned(),
            token: SecretValue::new("sk-live-do-not-leak"),
        };
        let json = serde_json::to_string(&payload).expect("serialize");
        assert!(json.contains("[REDACTED]"));
        assert!(
            !json.contains("sk-live-do-not-leak"),
            "generic serialization must not emit plaintext: {json}"
        );
    }

    #[test]
    fn vault_secret_record_serializes_plaintext_for_encryption() {
        // The one deliberate exposure path: the vault payload must serialize the
        // real bytes (so they can be encrypted), via the explicit field serde.
        let record = VaultSecretRecord::new(SecretValue::new("plaintext-for-vault"));
        let json = serde_json::to_string(&record).expect("serialize");
        assert!(
            json.contains("plaintext-for-vault"),
            "vault record must expose plaintext for encryption: {json}"
        );
        let back: VaultSecretRecord = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.value.expose(), "plaintext-for-vault");
    }

    #[test]
    fn vault_payload_encrypts_and_decrypts_with_correct_passphrase() {
        let mut payload = VaultPlaintextPayload::empty();
        payload.records.insert(
            "database_url".to_owned(),
            VaultSecretRecord::new(SecretValue::new("postgres://secret-value")),
        );

        let envelope = payload
            .encrypt_with_material(
                "correct horse battery staple",
                vec![42; DEFAULT_SALT_LEN],
                vec![7; XCHACHA20POLY1305_NONCE_LEN],
            )
            .expect("encrypt");
        let decrypted = envelope
            .decrypt_with_passphrase("correct horse battery staple")
            .expect("decrypt");

        assert_eq!(
            decrypted
                .records
                .get("database_url")
                .expect("record")
                .value
                .expose(),
            "postgres://secret-value"
        );
        assert_ne!(
            envelope.payload.ciphertext,
            serde_json::to_vec(&payload).expect("plaintext json")
        );
    }

    #[test]
    fn vault_payload_rejects_wrong_passphrase_without_plaintext() {
        let mut payload = VaultPlaintextPayload::empty();
        payload.records.insert(
            "api_key".to_owned(),
            VaultSecretRecord::new(SecretValue::new("sk-live-secret")),
        );
        let envelope = payload
            .encrypt_with_material(
                "right passphrase",
                vec![11; DEFAULT_SALT_LEN],
                vec![12; XCHACHA20POLY1305_NONCE_LEN],
            )
            .expect("encrypt");

        let error = envelope
            .decrypt_with_passphrase("wrong passphrase")
            .expect_err("wrong passphrase should fail");

        assert_eq!(error.to_string(), "failed to decrypt vault payload");
        assert!(!error.to_string().contains("sk-live-secret"));
    }

    #[test]
    fn vault_payload_rejects_corrupt_ciphertext() {
        let payload = VaultPlaintextPayload::empty();
        let mut envelope = payload
            .encrypt_with_material(
                "passphrase",
                vec![21; DEFAULT_SALT_LEN],
                vec![22; XCHACHA20POLY1305_NONCE_LEN],
            )
            .expect("encrypt");
        envelope.payload.ciphertext[0] ^= 0xff;

        let error = envelope
            .decrypt_with_passphrase("passphrase")
            .expect_err("corrupt payload should fail");

        assert_eq!(error.to_string(), "failed to decrypt vault payload");
    }

    #[test]
    fn vault_payload_rejects_unsupported_cipher() {
        let payload = VaultPlaintextPayload::empty();
        let mut envelope = payload
            .encrypt_with_material(
                "passphrase",
                vec![31; DEFAULT_SALT_LEN],
                vec![32; XCHACHA20POLY1305_NONCE_LEN],
            )
            .expect("encrypt");
        envelope.cipher.name = "rot13".to_owned();

        let error = envelope
            .decrypt_with_passphrase("passphrase")
            .expect_err("unsupported cipher should fail");

        assert_eq!(
            error.to_string(),
            "unsupported vault cipher `rot13` (expected `xchacha20poly1305`)"
        );
    }

    #[cfg(unix)]
    #[test]
    fn permission_check_flags_group_or_world_bits() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("local.vault");
        fs::write(&path, "{}").expect("write vault");

        let mut permissions = fs::metadata(&path).expect("metadata").permissions();
        permissions.set_mode(0o644);
        fs::set_permissions(&path, permissions).expect("chmod");

        assert_eq!(
            inspect_vault_permissions(&path).expect("inspect"),
            VaultPermissionStatus::Unsafe {
                mode: 0o644,
                max_mode: DEFAULT_MAX_UNIX_MODE,
            }
        );
    }
}
