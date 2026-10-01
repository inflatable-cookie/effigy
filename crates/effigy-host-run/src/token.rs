use crate::secure_fs::Authority;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
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
        if !claims_are_canonical(&token) {
            return Err(TokenError::Invalid("invalid token claims"));
        }
        let exp =
            DateTime::from_timestamp_millis(token.exp).ok_or(TokenError::Invalid(
                "invalid token claims",
            ))?;
        // Valid only while now_ms < exp: a token expires at exp exactly.
        if exp <= now {
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
    /// Integer UTC Unix milliseconds. RFC 3339 text, fractions and other
    /// spellings are refused by the typed deserialization itself.
    exp: i64,
}

/// Contract 010 at 16fcb59: claims are exactly `runId` (1–160 chars), `epoch`
/// (positive integer), `class` (`heavy`|`light`), `root` (nonempty canonical
/// path) and `exp` (positive integer UTC milliseconds). `deny_unknown_fields`
/// refuses unknown claims; serde refuses missing and wrong-typed ones.
fn claims_are_canonical(token: &ParentTokenWire) -> bool {
    (1..=160).contains(&token.run_id.chars().count())
        && token.epoch > 0
        && matches!(token.class.as_str(), "heavy" | "light")
        && !token.root.as_os_str().is_empty()
        && token.exp > 0
}

/// `keyB64` is RFC 4648 §4 standard padded base64 of exactly 32 bytes (44
/// characters), contract 010 at 16fcb59. URL-safe, unpadded, whitespace and
/// non-canonical spellings are refused: the standard engine requires the
/// standard alphabet, canonical padding, and zero trailing bits.
fn decode_key_bytes(encoded: &str) -> Option<Vec<u8>> {
    let bytes = STANDARD.decode(encoded).ok()?;
    (bytes.len() == 32).then_some(bytes)
}

fn decode_key(key: KeyEpoch) -> Result<KeyMaterial, TokenError> {
    let bytes =
        decode_key_bytes(&key.key_b64).ok_or(TokenError::Invalid("invalid key encoding"))?;
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
    use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
    use base64::Engine;
    use chrono::{TimeZone, Utc};
    use serde_json::{json, Value};
    use std::path::PathBuf;

    fn signed_token(payload: &Value, key: &[u8]) -> String {
        let part = URL_SAFE_NO_PAD.encode(serde_json::to_vec(payload).unwrap());
        token_from_parts(&part, key)
    }

    fn token_from_parts(payload_part: &str, key: &[u8]) -> String {
        format!(
            "{payload_part}.{}",
            URL_SAFE_NO_PAD.encode(hmac_sha256(key, payload_part.as_bytes()))
        )
    }

    /// Key bytes whose standard spelling contains `+`, `/` and padding, as
    /// Node's `Buffer.toString("base64")` writes in the scheduler's key file.
    fn key_bytes() -> Vec<u8> {
        (0..32u8)
            .map(|byte| 0xfb_u8.wrapping_add(byte * 7))
            .collect()
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    fn authority(epoch: u64) -> crate::Authority {
        crate::Authority {
            format: "host.run.authority".into(),
            version: 1,
            holder: "queue".into(),
            endpoint: PathBuf::from("/unused"),
            epoch,
            pid: 1,
            start_identity: "1@x".into(),
        }
    }

    fn keys_for(epoch: u64, key: &[u8]) -> TokenKeys {
        let json = json!({"format":"host.run.keys","version":1,
            "current":{"epoch":epoch,"keyB64":STANDARD.encode(key)}});
        TokenKeys::from_json(&serde_json::to_vec(&json).unwrap()).unwrap()
    }

    fn accepts_key(spelling: &str) -> Result<TokenKeys, crate::TokenError> {
        TokenKeys::from_json(
            &serde_json::to_vec(&json!({"format":"host.run.keys","version":1,
                "current":{"epoch":1,"keyB64":spelling}}))
            .unwrap(),
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

    /// The pinned contract-010 vectors (planner-recomputed, 2026-10-01), byte
    /// for byte: 32 key bytes, their 44-character RFC 4648 §4 spelling, the
    /// base64url payload part, and its HMAC-SHA256 over the transmitted ASCII.
    #[test]
    fn pinned_key_payload_and_token_vectors_are_reproduced_exactly() {
        let key = key_bytes();
        assert_eq!(
            hex(&key),
            "fb020910171e252c333a41484f565d646b727980878e959ca3aab1b8bfc6cdd4"
        );
        assert_eq!(
            STANDARD.encode(&key),
            "+wIJEBceJSwzOkFIT1ZdZGtyeYCHjpWco6qxuL/GzdQ="
        );
        let payload = "eyJjbGFzcyI6ImhlYXZ5IiwiZXBvY2giOjcsImV4cCI6MTc5ODg3OTIwMDAwMCwicm9vdCI6Ii9ob3N0LXJ1biIsInJ1bklkIjoic3ludGhldGljLXJ1biJ9";
        assert_eq!(
            URL_SAFE_NO_PAD.decode(payload).unwrap(),
            br#"{"class":"heavy","epoch":7,"exp":1798879200000,"root":"/host-run","runId":"synthetic-run"}"#
        );
        assert_eq!(
            hex(&hmac_sha256(&key, payload.as_bytes())),
            "f1da0e9935b91e109f7bc9e90ccf40876616bb1e288865da9c50fe8adbe33baa"
        );
        assert_eq!(
            token_from_parts(payload, &key),
            format!("{payload}.8doOmTW5HhCfe8npDM9Ah2YWux4oiGXanFD-itvjO6o")
        );
        // The same claims in a different key order carry their own MAC over
        // their own transmitted bytes.
        let reordered = "eyJlcG9jaCI6NywiZXhwIjoxNzk4ODc5MjAwMDAwLCJjbGFzcyI6ImhlYXZ5IiwicnVuSWQiOiJzeW50aGV0aWMtcnVuIiwicm9vdCI6Ii9ob3N0LXJ1biJ9";
        assert_eq!(
            URL_SAFE_NO_PAD.encode(hmac_sha256(&key, reordered.as_bytes())),
            "yzF6WZ0Op5KWKUjnycbOq7SsLSzfnkhJ_QzEXtEo1SU"
        );
    }

    #[test]
    fn only_the_canonical_key_spelling_is_accepted() {
        let key = key_bytes();
        let canonical = STANDARD.encode(&key);
        assert_eq!(canonical.len(), 44);
        accepts_key(&canonical).unwrap();
        for refused in [
            STANDARD_NO_PAD.encode(&key),
            URL_SAFE.encode(&key),
            URL_SAFE_NO_PAD.encode(&key),
            format!(" {canonical}"),
            format!("{canonical}\n"),
            // Same decoded bytes with non-zero trailing bits: non-canonical.
            format!("{}R{}", &canonical[..42], &canonical[43..]),
            STANDARD.encode([1u8; 31]),
            STANDARD.encode([1u8; 33]),
            String::new(),
        ] {
            assert!(accepts_key(&refused).is_err(), "refuse spelling {refused:?}");
        }
        // Even a canonically spelled key needs a positive epoch.
        assert!(TokenKeys::from_json(
            &serde_json::to_vec(&json!({"format":"host.run.keys","version":1,
                "current":{"epoch":0,"keyB64":canonical}}))
            .unwrap()
        )
        .is_err());
    }

    #[test]
    fn expiry_boundary_is_strict_at_one_millisecond() {
        let root = tempfile::tempdir().unwrap();
        let key = vec![3u8; 32];
        let keys = keys_for(1, &key);
        let authority = authority(1);
        let exp = Utc.with_ymd_and_hms(2026, 10, 1, 15, 0, 0)
            .unwrap()
            .timestamp_millis();
        let claims =
            json!({"runId":"run1","epoch":1,"class":"heavy","root":root.path(),"exp":exp});
        // now_ms < exp: one millisecond before expiry is still valid.
        keys
            .validate(
                &signed_token(&claims, &key),
                root.path(),
                &authority,
                Utc.timestamp_millis_opt(exp - 1).unwrap(),
                |_| unreachable!(),
            )
            .unwrap();
        // A token expires at exp exactly.
        assert_eq!(
            keys
                .validate(
                    &signed_token(&claims, &key),
                    root.path(),
                    &authority,
                    Utc.timestamp_millis_opt(exp).unwrap(),
                    |_| unreachable!(),
                )
                .unwrap_err(),
            crate::TokenError::Invalid("expired or incomplete token")
        );
    }

    #[test]
    fn expiry_spellings_beyond_canonical_milliseconds_are_refused() {
        let root = tempfile::tempdir().unwrap();
        let key = vec![3u8; 32];
        let keys = keys_for(1, &key);
        let authority = authority(1);
        let now = Utc.with_ymd_and_hms(2026, 10, 1, 14, 0, 0).unwrap();
        let valid_ms = now.timestamp_millis() + 60_000;
        let claims_with = |exp: Value| {
            json!({"runId":"run1","epoch":1,"class":"heavy","root":root.path(),"exp":exp})
        };
        for (refused, reason) in [
            (json!("2026-10-01T15:00:00Z"), "invalid token payload"), // RFC 3339 text
            (json!(valid_ms.to_string()), "invalid token payload"),   // string digits
            (json!(1.5), "invalid token payload"), // fractional milliseconds
            (json!(true), "invalid token payload"),
            (json!(null), "invalid token payload"),
            (json!(0), "invalid token claims"),
            (json!(-1), "invalid token claims"),
        ] {
            assert_eq!(
                keys
                    .validate(
                        &signed_token(&claims_with(refused), &key),
                        root.path(),
                        &authority,
                        now,
                        |_| unreachable!(),
                    )
                    .unwrap_err(),
                crate::TokenError::Invalid(reason)
            );
        }
        keys
            .validate(
                &signed_token(&claims_with(json!(valid_ms)), &key),
                root.path(),
                &authority,
                now,
                |_| unreachable!(),
            )
            .unwrap();
    }

    #[test]
    fn claims_are_exactly_the_canonical_shape() {
        let root = tempfile::tempdir().unwrap();
        let key = vec![3u8; 32];
        let keys = keys_for(1, &key);
        let authority = authority(1);
        let now = Utc.with_ymd_and_hms(2026, 10, 1, 14, 0, 0).unwrap();
        let exp = now.timestamp_millis() + 60_000;
        // 160 characters is the maximum accepted runId.
        let accepted = json!({"runId":"r".repeat(160),"epoch":1,"class":"light","root":root.path(),"exp":exp});
        keys.validate(&signed_token(&accepted, &key), root.path(), &authority, now, |_| {
            unreachable!()
        })
        .unwrap();
        let refused = [
            ("empty runId", json!({"runId":"","epoch":1,"class":"heavy","root":root.path(),"exp":exp})),
            ("161-char runId", json!({"runId":"r".repeat(161),"epoch":1,"class":"heavy","root":root.path(),"exp":exp})),
            ("zero epoch", json!({"runId":"r","epoch":0,"class":"heavy","root":root.path(),"exp":exp})),
            ("negative epoch", json!({"runId":"r","epoch":-1,"class":"heavy","root":root.path(),"exp":exp})),
            ("string epoch", json!({"runId":"r","epoch":"1","class":"heavy","root":root.path(),"exp":exp})),
            ("unknown class", json!({"runId":"r","epoch":1,"class":"median","root":root.path(),"exp":exp})),
            ("capitalised class", json!({"runId":"r","epoch":1,"class":"Heavy","root":root.path(),"exp":exp})),
            ("empty root", json!({"runId":"r","epoch":1,"class":"heavy","root":"","exp":exp})),
            ("unknown claim", json!({"runId":"r","epoch":1,"class":"heavy","root":root.path(),"exp":exp,"iss":"queue"})),
            ("missing class", json!({"runId":"r","epoch":1,"root":root.path(),"exp":exp})),
            ("numeric runId", json!({"runId":7,"epoch":1,"class":"heavy","root":root.path(),"exp":exp})),
        ];
        for (label, payload) in refused {
            assert!(
                matches!(
                    keys.validate(&signed_token(&payload, &key), root.path(), &authority, now, |_| {
                        unreachable!()
                    }),
                    Err(crate::TokenError::Invalid(_))
                ),
                "{label} must be refused"
            );
        }
    }

    #[test]
    fn token_structure_is_exactly_two_unpadded_base64url_parts() {
        let root = tempfile::tempdir().unwrap();
        let key = vec![3u8; 32];
        let keys = keys_for(1, &key);
        let authority = authority(1);
        let now = Utc.with_ymd_and_hms(2026, 10, 1, 14, 0, 0).unwrap();
        let claims = json!({"runId":"run1","epoch":1,"class":"heavy","root":root.path(),"exp":now.timestamp_millis() + 60_000});
        let valid = signed_token(&claims, &key);
        keys.validate(&valid, root.path(), &authority, now, |_| unreachable!())
            .unwrap();
        let (payload_part, mac_part) = valid.split_once('.').unwrap();
        for (label, refused) in [
            ("padded payload", format!("{payload_part}=.{mac_part}")),
            ("padded mac", format!("{payload_part}.{mac_part}=")),
            ("standard alphabet", format!("{payload_part}.+wIJEBceJSwzOkFIT1ZdZGtyeYCHjpWco6qxuL/GzdQ=")),
            ("extra component", format!("{valid}.extra")),
            ("empty payload", format!(".{mac_part}")),
            ("empty mac", format!("{payload_part}.")),
            ("single part", "onlyonepart".to_owned()),
            ("no parts", String::new()),
        ] {
            assert!(
                matches!(
                    keys.validate(&refused, root.path(), &authority, now, |_| unreachable!()),
                    Err(crate::TokenError::Invalid(_))
                ),
                "{label} must be refused"
            );
        }
    }

    #[test]
    fn reordered_claims_signed_over_their_own_bytes_stay_valid() {
        let root = tempfile::tempdir().unwrap();
        let key = vec![3u8; 32];
        let keys = keys_for(1, &key);
        let authority = authority(1);
        let now = Utc.with_ymd_and_hms(2026, 10, 1, 14, 0, 0).unwrap();
        // A key order serde_json would never emit, signed exactly as transmitted.
        let payload = format!(
            r#"{{"exp":{},"root":{},"class":"light","epoch":1,"runId":"run1"}}"#,
            now.timestamp_millis() + 60_000,
            serde_json::to_string(root.path()).unwrap(),
        );
        let token = token_from_parts(&URL_SAFE_NO_PAD.encode(payload.as_bytes()), &key);
        let accepted = keys
            .validate(&token, root.path(), &authority, now, |_| unreachable!())
            .unwrap();
        assert_eq!(accepted.run_id, "run1");
        assert_eq!(accepted.class, "light");
    }

    #[test]
    fn current_and_previous_epoch_tokens_are_verified_without_key_disclosure() {
        let root = tempfile::tempdir().unwrap();
        let cwd = root.path().join("child");
        std::fs::create_dir(&cwd).unwrap();
        let key = vec![7u8; 32];
        let keys_json = json!({
            "format":"host.run.keys", "version":1,
            "current":{"epoch":4,"keyB64":STANDARD.encode([8u8;32])},
            "previous":{"epoch":3,"keyB64":STANDARD.encode(&key)}
        });
        let keys = TokenKeys::from_json(&serde_json::to_vec(&keys_json).unwrap()).unwrap();
        let authority = authority(4);
        let now = Utc.with_ymd_and_hms(2026, 10, 1, 14, 0, 0).unwrap();
        let exp = Utc.with_ymd_and_hms(2026, 10, 1, 15, 0, 0)
            .unwrap()
            .timestamp_millis();
        let payload = json!({"runId":"run1","epoch":3,"class":"heavy","root":root.path(),"exp":exp});
        let encoded = signed_token(&payload, &key);
        let accepted = keys
            .validate(&encoded, &cwd, &authority, now, |_| {
                Ok(json!({"runId":"run1","state":"running","epoch":4}))
            })
            .unwrap();
        assert_eq!(accepted.run_id, "run1");
        let current_payload = json!({"runId":"run2","epoch":4,"class":"heavy","root":root.path(),"exp":exp});
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
        let now = Utc.with_ymd_and_hms(2026, 10, 1, 14, 0, 0).unwrap();
        let exp = now.timestamp_millis() + 60_000;
        let payload = json!({"runId":"r","epoch":4,"class":"heavy","root":temp.path(),"exp":exp});
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
        assert_eq!(
            keys.validate(&invalid, temp.path(), &authority, now, |_| unreachable!())
                .unwrap_err(),
            crate::TokenError::Invalid("token signature mismatch")
        );
        // A different valid payload under the original MAC binds the exact
        // received bytes: swapping the payload part is detected.
        let other_payload = URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&json!({"runId":"r2","epoch":4,"class":"heavy","root":temp.path(),"exp":exp}))
                .unwrap(),
        );
        let swapped = format!("{other_payload}.{signature_part}");
        assert_eq!(
            keys.validate(&swapped, temp.path(), &authority, now, |_| unreachable!())
                .unwrap_err(),
            crate::TokenError::Invalid("token signature mismatch")
        );
        let expired = signed_token(
            &json!({"runId":"r","epoch":4,"class":"heavy","root":temp.path(),"exp":now.timestamp_millis() - 1_000}),
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
