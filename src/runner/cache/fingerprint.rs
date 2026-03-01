use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use globset::Glob;
use serde::Serialize;
use walkdir::WalkDir;

use crate::runner::{ManifestTaskCache, RunnerError};

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

fn resolve_declared_matches(
    catalog_root: &Path,
    declaration: &str,
) -> Result<Vec<PathBuf>, RunnerError> {
    if has_glob_magic(declaration) {
        return resolve_glob_matches(catalog_root, declaration);
    }
    Ok(vec![catalog_root.join(declaration)])
}

fn resolve_glob_matches(catalog_root: &Path, pattern: &str) -> Result<Vec<PathBuf>, RunnerError> {
    let glob = Glob::new(pattern).map_err(|error| {
        RunnerError::TaskInvocation(format!(
            "invalid cache declaration glob `{pattern}`: {error}"
        ))
    })?;
    let matcher = glob.compile_matcher();
    let mut matches = WalkDir::new(catalog_root)
        .sort_by_file_name()
        .into_iter()
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            if path == catalog_root {
                return None;
            }
            let relative = path.strip_prefix(catalog_root).ok()?;
            let relative_rendered = relative.to_string_lossy().replace('\\', "/");
            matcher
                .is_match(&relative_rendered)
                .then_some(path.to_path_buf())
        })
        .collect::<Vec<PathBuf>>();
    matches.sort();
    Ok(matches)
}

fn stamp_path(catalog_root: &Path, path: &Path) -> Result<PathStamp, RunnerError> {
    let rendered = render_relative_or_absolute(catalog_root, path);
    let Ok(metadata) = fs::metadata(path) else {
        return Ok(PathStamp {
            path: rendered,
            kind: "missing",
            exists: false,
            size: None,
            modified_epoch_ms: None,
            digest: None,
        });
    };

    if metadata.is_file() {
        let body = fs::read(path).map_err(|error| {
            RunnerError::TaskInvocation(format!(
                "failed reading cache input {}: {error}",
                path.display()
            ))
        })?;
        return Ok(PathStamp {
            path: rendered,
            kind: "file",
            exists: true,
            size: Some(metadata.len()),
            modified_epoch_ms: metadata_modified_epoch_ms(&metadata),
            digest: Some(fnv1a_hex(&body)),
        });
    }

    if metadata.is_dir() {
        let digest = digest_directory(path)?;
        return Ok(PathStamp {
            path: rendered,
            kind: "dir",
            exists: true,
            size: None,
            modified_epoch_ms: metadata_modified_epoch_ms(&metadata),
            digest: Some(digest),
        });
    }

    Ok(PathStamp {
        path: rendered,
        kind: "other",
        exists: true,
        size: None,
        modified_epoch_ms: metadata_modified_epoch_ms(&metadata),
        digest: None,
    })
}

fn digest_directory(root: &Path) -> Result<String, RunnerError> {
    let mut hasher = Fnv1a64::new();
    for entry in WalkDir::new(root)
        .sort_by_file_name()
        .into_iter()
        .filter_map(Result::ok)
    {
        let path = entry.path();
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let rel_rendered = relative.to_string_lossy().replace('\\', "/");
        hasher.update(rel_rendered.as_bytes());

        let Ok(metadata) = fs::metadata(path) else {
            continue;
        };
        if metadata.is_file() {
            hasher.update(b"f");
            let body = fs::read(path).map_err(|error| {
                RunnerError::TaskInvocation(format!(
                    "failed reading cache directory input {}: {error}",
                    path.display()
                ))
            })?;
            hasher.update(&body);
        } else if metadata.is_dir() {
            hasher.update(b"d");
        }
    }
    Ok(hasher.finish_hex())
}

fn metadata_modified_epoch_ms(metadata: &fs::Metadata) -> Option<u128> {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis())
}

fn render_relative_or_absolute(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| path.display().to_string())
}

fn has_glob_magic(value: &str) -> bool {
    value.contains('*') || value.contains('?') || value.contains('[') || value.contains('{')
}

fn fnv1a_hex(bytes: &[u8]) -> String {
    let mut hasher = Fnv1a64::new();
    hasher.update(bytes);
    hasher.finish_hex()
}

struct Fnv1a64 {
    state: u64,
}

impl Fnv1a64 {
    fn new() -> Self {
        Self {
            state: 0xcbf29ce484222325,
        }
    }

    fn update(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.state ^= u64::from(*byte);
            self.state = self.state.wrapping_mul(0x100000001b3);
        }
    }

    fn finish_hex(&self) -> String {
        format!("{:016x}", self.state)
    }
}
