use crate::secure_fs::Authority;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KeyFile {
    format: String,
    version: u32,
    current: KeyEpoch,
    previous: Option<KeyEpoch>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KeyEpoch {
    epoch: u64,
    #[serde(rename = "keyB64")]
    key_b64: String,
}

#[derive(Clone)]
pub struct TokenKeys {
    current: KeyMaterial,
    previous: Option<KeyMaterial>,
}

#[derive(Clone)]
struct KeyMaterial {
    epoch: u64,
    bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParentToken {
    pub run_id: String,
    pub epoch: u64,
    pub class: String,
    pub root: PathBuf,
    pub exp: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenError {
    Invalid(&'static str),
    SchedulerUnreachable,
}

impl std::fmt::Display for TokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(reason) => write!(f, "invalid_parent_token: {reason}"),
            Self::SchedulerUnreachable => f.write_str("scheduler_unreachable"),
        }
    }
}
impl std::error::Error for TokenError {}

impl TokenKeys {
    pub fn from_json(bytes: &[u8]) -> Result<Self, TokenError> {
        let file: KeyFile =
            serde_json::from_slice(bytes).map_err(|_| TokenError::Invalid("invalid key file"))?;
        if file.format != "host.run.keys" || file.version != 1 {
            return Err(TokenError::Invalid(
                "unsupported key file format or version",
            ));
        }
        let current = decode_key(file.current)?;
        let previous = file.previous.map(decode_key).transpose()?;
        if previous
            .as_ref()
            .is_some_and(|key| key.epoch.checked_add(1) != Some(current.epoch))
        {
            return Err(TokenError::Invalid(
                "previous key epoch is not immediately prior",
            ));
        }
        Ok(Self { current, previous })
    }

    pub fn from_root(root: &crate::HostRunRoot) -> Result<Self, TokenError> {
        Self::from_json(
            &root
                .open_token_key()
                .map_err(|_| TokenError::Invalid("untrusted token key file"))?,
        )
    }

    /// Validate a present token. Previous-epoch tokens require an explicit
    /// scheduler lookup whose record is running at the current authority epoch.
    pub fn validate<F>(
        &self,
        encoded: &str,
        cwd: &Path,
        authority: &Authority,
        now: DateTime<Utc>,
        mut status: F,
    ) -> Result<ParentToken, TokenError>
    where
        F: FnMut(&str) -> Result<Value, TokenError>,
    {
        let (payload_part, mac_part) = encoded
            .split_once('.')
            .ok_or(TokenError::Invalid("malformed token"))?;
        if payload_part.is_empty() || mac_part.is_empty() || mac_part.contains('.') {
            return Err(TokenError::Invalid("malformed token"));
        }
        let mac = URL_SAFE_NO_PAD
            .decode(mac_part)
            .map_err(|_| TokenError::Invalid("malformed token MAC"))?;
        let payload_bytes = URL_SAFE_NO_PAD
            .decode(payload_part)
            .map_err(|_| TokenError::Invalid("malformed token payload"))?;
        let token: ParentTokenWire = serde_json::from_slice(&payload_bytes)
            .map_err(|_| TokenError::Invalid("invalid token payload"))?;
        let exp = DateTime::parse_from_rfc3339(&token.exp)
            .map_err(|_| TokenError::Invalid("invalid token expiry"))?
            .with_timezone(&Utc);
        if token.run_id.is_empty() || token.epoch == 0 || token.class.is_empty() || exp <= now {
            return Err(TokenError::Invalid("expired or incomplete token"));
        }
        if !cwd.is_absolute() || !token.root.is_absolute() {
            return Err(TokenError::Invalid("cwd and token root must be absolute"));
        }
        let current_cwd = std::fs::canonicalize(cwd)
            .map_err(|_| TokenError::Invalid("cwd cannot be canonicalized"))?;
        let token_root = std::fs::canonicalize(&token.root)
            .map_err(|_| TokenError::Invalid("token root cannot be canonicalized"))?;
        if !current_cwd.is_dir() || !token_root.is_dir() {
            return Err(TokenError::Invalid("cwd or token root is not a directory"));
        }
        if current_cwd != token_root && !current_cwd.starts_with(&token_root) {
            return Err(TokenError::Invalid("cwd is outside token root"));
        }
        let key = if token.epoch == authority.epoch && token.epoch == self.current.epoch {
            &self.current
        } else if token.epoch.checked_add(1) == Some(authority.epoch)
            && self
                .previous
                .as_ref()
                .is_some_and(|previous| previous.epoch == token.epoch)
        {
            self.previous.as_ref().expect("validated previous key")
        } else {
            return Err(TokenError::Invalid(
                "token epoch is not current or immediately previous",
            ));
        };
        if !constant_time_eq(&hmac_sha256(&key.bytes, payload_part.as_bytes()), &mac) {
            return Err(TokenError::Invalid("token signature mismatch"));
        }
        if token.epoch != authority.epoch {
            let run = status(&token.run_id)?;
            if run.get("state").and_then(Value::as_str) != Some("running")
                || run.get("epoch").and_then(Value::as_u64) != Some(authority.epoch)
                || run.get("runId").and_then(Value::as_str) != Some(token.run_id.as_str())
            {
                return Err(TokenError::Invalid("previous-epoch run was not taken over"));
            }
        }
        Ok(ParentToken {
            run_id: token.run_id,
            epoch: token.epoch,
            class: token.class,
            root: token_root,
            exp,
        })
    }

    #[cfg(test)]
    pub(crate) fn for_test(epoch: u64, bytes: Vec<u8>) -> Self {
        Self {
            current: KeyMaterial { epoch, bytes },
            previous: None,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
struct ParentTokenWire {
    run_id: String,
    epoch: u64,
    class: String,
    root: PathBuf,
    exp: String,
}

fn decode_key(key: KeyEpoch) -> Result<KeyMaterial, TokenError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(key.key_b64)
        .map_err(|_| TokenError::Invalid("invalid key encoding"))?;
    if key.epoch == 0 || bytes.len() != 32 {
        return Err(TokenError::Invalid(
            "key must contain 32 bytes and a positive epoch",
        ));
    }
    Ok(KeyMaterial {
        epoch: key.epoch,
        bytes,
    })
}

pub(crate) fn hmac_sha256(key: &[u8], message: &[u8]) -> Vec<u8> {
    const BLOCK: usize = 64;
    let mut key_block = [0u8; BLOCK];
    if key.len() > BLOCK {
        let digest = Sha256::digest(key);
        key_block[..digest.len()].copy_from_slice(&digest);
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }
    let mut inner = Vec::with_capacity(BLOCK + message.len());
    inner.extend(key_block.iter().map(|byte| *byte ^ 0x36));
    inner.extend_from_slice(message);
    let inner_hash = Sha256::digest(&inner);
    let mut outer = Vec::with_capacity(BLOCK + inner_hash.len());
    outer.extend(key_block.iter().map(|byte| *byte ^ 0x5c));
    outer.extend_from_slice(&inner_hash);
    Sha256::digest(&outer).to_vec()
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0u8, |diff, (a, b)| diff | (*a ^ *b))
        == 0
}

#[cfg(test)]
mod tests {
    use super::{hmac_sha256, TokenKeys};
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    use chrono::{TimeZone, Utc};
    use serde_json::{json, Value};
    use std::path::PathBuf;

    fn signed_token(payload: &Value, key: &[u8]) -> String {
        let part = URL_SAFE_NO_PAD.encode(serde_json::to_vec(payload).unwrap());
        format!(
            "{part}.{}",
            URL_SAFE_NO_PAD.encode(hmac_sha256(key, part.as_bytes()))
        )
    }

    #[test]
    fn hmac_sha256_matches_the_rfc_4231_vector() {
        let output = hmac_sha256(&[0x0b; 20], b"Hi There");
        let hex = output
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(
            hex,
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
    }

    #[test]
    fn current_and_previous_epoch_tokens_are_verified_without_key_disclosure() {
        let root = tempfile::tempdir().unwrap();
        let cwd = root.path().join("child");
        std::fs::create_dir(&cwd).unwrap();
        let key = vec![7u8; 32];
        let keys_json = json!({
            "format":"host.run.keys", "version":1,
            "current":{"epoch":4,"keyB64":URL_SAFE_NO_PAD.encode([8u8;32])},
            "previous":{"epoch":3,"keyB64":URL_SAFE_NO_PAD.encode(&key)}
        });
        let keys = TokenKeys::from_json(&serde_json::to_vec(&keys_json).unwrap()).unwrap();
        let authority = crate::Authority {
            format: "host.run.authority".into(),
            version: 1,
            holder: "queue".into(),
            endpoint: PathBuf::from("/unused"),
            epoch: 4,
            pid: 1,
            start_identity: "1@x".into(),
        };
        let payload = json!({"runId":"run1","epoch":3,"class":"heavy","root":root.path(),"exp":"2026-10-01T15:00:00Z"});
        let encoded = signed_token(&payload, &key);
        let now = Utc.with_ymd_and_hms(2026, 10, 1, 14, 0, 0).unwrap();
        let accepted = keys
            .validate(&encoded, &cwd, &authority, now, |_| {
                Ok(json!({"runId":"run1","state":"running","epoch":4}))
            })
            .unwrap();
        assert_eq!(accepted.run_id, "run1");
        let current_payload = json!({"runId":"run2","epoch":4,"class":"heavy","root":root.path(),"exp":"2026-10-01T15:00:00Z"});
        let current = signed_token(&current_payload, &[8u8; 32]);
        assert_eq!(
            keys.validate(&current, &cwd, &authority, now, |_| unreachable!())
                .unwrap()
                .run_id,
            "run2"
        );
        assert_eq!(
            keys.validate(&encoded, &cwd, &authority, now, |_| Err(
                crate::TokenError::SchedulerUnreachable
            ))
            .unwrap_err(),
            crate::TokenError::SchedulerUnreachable
        );
    }

    #[test]
    fn invalid_signature_expiry_and_outside_root_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let payload = json!({"runId":"r","epoch":4,"class":"heavy","root":temp.path(),"exp":"2026-10-01T15:00:00Z"});
        let token = signed_token(&payload, &[7u8; 32]);
        let (payload_part, signature_part) = token.split_once('.').unwrap();
        let invalid = format!("{payload_part}.{}", URL_SAFE_NO_PAD.encode([0u8; 32]));
        let keys = TokenKeys::for_test(4, vec![7u8; 32]);
        let authority = crate::Authority {
            format: "host.run.authority".into(),
            version: 1,
            holder: "q".into(),
            endpoint: PathBuf::new(),
            epoch: 4,
            pid: 1,
            start_identity: "i".into(),
        };
        let now = Utc.with_ymd_and_hms(2026, 10, 1, 14, 0, 0).unwrap();
        assert_eq!(
            keys.validate(&invalid, temp.path(), &authority, now, |_| unreachable!())
                .unwrap_err(),
            crate::TokenError::Invalid("token signature mismatch")
        );
        let expired = signed_token(
            &json!({"runId":"r","epoch":4,"class":"heavy","root":temp.path(),"exp":"2026-10-01T13:00:00Z"}),
            &[7u8; 32],
        );
        assert_eq!(
            keys.validate(&expired, temp.path(), &authority, now, |_| unreachable!())
                .unwrap_err(),
            crate::TokenError::Invalid("expired or incomplete token")
        );
        assert_eq!(
            keys.validate(&token, outside.path(), &authority, now, |_| unreachable!())
                .unwrap_err(),
            crate::TokenError::Invalid("cwd is outside token root")
        );
        assert!(!signature_part.is_empty());
        assert_eq!(
            crate::transport::ClientError::InvalidParentToken(crate::TokenError::Invalid("bad"))
                .exit_code(),
            Some(77)
        );
        assert_eq!(
            crate::transport::ClientError::SchedulerUnreachable.exit_code(),
            Some(75)
        );
    }
}
