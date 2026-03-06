use std::path::Path;

use serde::Serialize;

use crate::runner::error::RunnerError;
use crate::runner::manifest::task_runtime::ManifestTaskCache;

#[path = "fingerprint/digest.rs"]
mod digest;
#[path = "fingerprint/resolve.rs"]
mod resolve;
#[path = "fingerprint/stamp.rs"]
mod stamp;

use digest::fnv1a_hex;
use resolve::{has_glob_magic, render_relative_or_absolute, resolve_declared_matches};
use stamp::stamp_path;

#[derive(Debug)]
pub(super) struct CacheFingerprintSnapshot {
    pub(super) fingerprint: String,
    pub(super) outputs_exist: bool,
}

#[derive(Debug, Serialize)]
struct FingerprintMaterial {
    command: String,
    inputs: Vec<DeclaredInputStamp>,
    env: Vec<DeclaredEnvStamp>,
    outputs: Vec<DeclaredOutputStamp>,
}

#[derive(Debug, Serialize)]
struct DeclaredInputStamp {
    declaration: String,
    matches: Vec<PathStamp>,
}

#[derive(Debug, Serialize)]
struct DeclaredOutputStamp {
    declaration: String,
    exists: bool,
    matched: Vec<String>,
}

#[derive(Debug, Serialize)]
struct DeclaredEnvStamp {
    key: String,
    value: Option<String>,
}

#[derive(Debug, Serialize)]
struct PathStamp {
    path: String,
    kind: &'static str,
    exists: bool,
    size: Option<u64>,
    modified_epoch_ms: Option<u128>,
    digest: Option<String>,
}

pub(super) fn compute_fingerprint_snapshot(
    command: &str,
    config: &ManifestTaskCache,
    catalog_root: &Path,
) -> Result<CacheFingerprintSnapshot, RunnerError> {
    let input_stamps = collect_input_stamps(&config.inputs, catalog_root)?;
    let env_stamps = collect_env_stamps(&config.env);
    let output_stamps = collect_output_stamps(&config.outputs, catalog_root)?;
    let outputs_exist = output_stamps.iter().all(|stamp| stamp.exists);
    let material = FingerprintMaterial {
        command: command.to_owned(),
        inputs: input_stamps,
        env: env_stamps,
        outputs: output_stamps,
    };
    let encoded = serde_json::to_vec(&material)
        .map_err(|error| RunnerError::Ui(format!("failed to encode cache fingerprint: {error}")))?;
    Ok(CacheFingerprintSnapshot {
        fingerprint: fnv1a_hex(&encoded),
        outputs_exist,
    })
}

fn collect_input_stamps(
    declarations: &[String],
    catalog_root: &Path,
) -> Result<Vec<DeclaredInputStamp>, RunnerError> {
    declarations
        .iter()
        .map(|declaration| {
            let matches = resolve_declared_matches(catalog_root, declaration)?;
            let mut stamped = Vec::with_capacity(matches.len());
            for path in matches {
                stamped.push(stamp_path(catalog_root, &path)?);
            }
            stamped.sort_by(|a, b| a.path.cmp(&b.path));
            Ok(DeclaredInputStamp {
                declaration: declaration.clone(),
                matches: stamped,
            })
        })
        .collect()
}

fn collect_output_stamps(
    declarations: &[String],
    catalog_root: &Path,
) -> Result<Vec<DeclaredOutputStamp>, RunnerError> {
    declarations
        .iter()
        .map(|declaration| {
            let matches = resolve_declared_matches(catalog_root, declaration)?;
            let matched = matches
                .iter()
                .map(|path| render_relative_or_absolute(catalog_root, path))
                .collect::<Vec<String>>();
            let exists = if has_glob_magic(declaration) {
                !matched.is_empty()
            } else {
                catalog_root.join(declaration).exists()
            };
            Ok(DeclaredOutputStamp {
                declaration: declaration.clone(),
                exists,
                matched,
            })
        })
        .collect()
}

fn collect_env_stamps(keys: &[String]) -> Vec<DeclaredEnvStamp> {
    let mut env = keys
        .iter()
        .map(|key| DeclaredEnvStamp {
            key: key.clone(),
            value: std::env::var(key).ok(),
        })
        .collect::<Vec<DeclaredEnvStamp>>();
    env.sort_by(|a, b| a.key.cmp(&b.key));
    env
}
