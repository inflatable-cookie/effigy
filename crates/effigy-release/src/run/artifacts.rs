//! Artifact resolution and digests (contract 054 §3 certify, §6 continuity).
//!
//! Certify runs each artifact task and copies what it built into the Run's
//! candidate directory in Effigy-owned state (054 §10 ruling 2), recording each
//! file's path and sha256. Publish re-hashes those exact files and refuses if
//! any changed: the candidate is what gets published, never a rebuild.

use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest as _, Sha256};

use super::record::{RecordArtifact, RecordArtifactFile};
use crate::ReleaseError;

pub fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex(&hasher.finalize())
}

fn sha256_file(path: &Path) -> Result<(String, u64), ReleaseError> {
    let bytes = fs::read(path).map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "failed to read release artifact {}: {error}",
            path.display()
        ))
    })?;
    Ok((sha256_bytes(&bytes), bytes.len() as u64))
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn has_glob(pattern: &str) -> bool {
    pattern.contains(['*', '?', '[', '{'])
}

/// The directory a pattern's matches must start in: everything before the
/// first glob metacharacter's path segment.
fn pattern_prefix(pattern: &str) -> PathBuf {
    let mut prefix = PathBuf::new();
    for segment in pattern.split('/') {
        if has_glob(segment) {
            break;
        }
        prefix.push(segment);
    }
    prefix
}

fn collect_files(directory: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut paths = entries
        .flatten()
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            if name == ".git" || name == "node_modules" {
                continue;
            }
            collect_files(&path, out);
        } else if path.is_file() {
            out.push(path);
        }
    }
}

/// The files an artifact `path` pattern names, relative to the release root.
///
/// A plain path names one file or every file under one directory; a glob is
/// matched below its fixed prefix. Sorted, so a digest over several files is
/// stable.
pub fn resolve_artifact_files(root: &Path, pattern: &str) -> Result<Vec<PathBuf>, ReleaseError> {
    let trimmed = pattern.trim();
    if trimmed.is_empty() {
        return Err(ReleaseError::TaskInvocation(
            "release artifact path must not be empty".to_owned(),
        ));
    }
    let mut files = Vec::new();
    if has_glob(trimmed) {
        let matcher = globset::Glob::new(trimmed)
            .map_err(|error| {
                ReleaseError::TaskInvocation(format!(
                    "release artifact path `{trimmed}` is not a valid pattern: {error}"
                ))
            })?
            .compile_matcher();
        let prefix = pattern_prefix(trimmed);
        let search_root = if prefix.as_os_str().is_empty() {
            root.to_path_buf()
        } else {
            root.join(&prefix)
        };
        let mut candidates = Vec::new();
        if search_root.is_file() {
            candidates.push(search_root);
        } else {
            collect_files(&search_root, &mut candidates);
        }
        for candidate in candidates {
            let relative = candidate
                .strip_prefix(root)
                .unwrap_or(&candidate)
                .to_string_lossy()
                .into_owned();
            if matcher.is_match(&relative) {
                files.push(candidate);
            }
        }
    } else {
        let path = root.join(trimmed);
        if path.is_file() {
            files.push(path);
        } else if path.is_dir() {
            collect_files(&path, &mut files);
        }
    }
    files.sort();
    files.dedup();
    if files.is_empty() {
        return Err(ReleaseError::TaskInvocation(format!(
            "release artifact path `{trimmed}` matched no file under {}",
            root.display()
        )));
    }
    Ok(files)
}

/// Copy one artifact's files into the candidate directory and record them.
pub fn capture_artifact(
    root: &Path,
    candidate_dir: &Path,
    name: &str,
    task: &str,
    pattern: &str,
) -> Result<RecordArtifact, ReleaseError> {
    let sources = resolve_artifact_files(root, pattern)?;
    let artifact_dir = candidate_dir.join(name);
    fs::create_dir_all(&artifact_dir).map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "failed to create release candidate directory {}: {error}",
            artifact_dir.display()
        ))
    })?;
    let mut recorded = Vec::with_capacity(sources.len());
    for source in sources {
        let file_name = source
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .ok_or_else(|| {
                ReleaseError::TaskInvocation(format!(
                    "release artifact path `{pattern}` resolved an unnamed file"
                ))
            })?;
        let destination = artifact_dir.join(&file_name);
        let (sha256, bytes) = sha256_file(&source)?;
        copy_atomically(&source, &destination)?;
        recorded.push(RecordArtifactFile {
            path: destination.display().to_string(),
            sha256,
            bytes,
        });
    }
    Ok(RecordArtifact {
        name: name.to_owned(),
        task: task.to_owned(),
        pattern: pattern.to_owned(),
        files: recorded,
    })
}

fn copy_atomically(source: &Path, destination: &Path) -> Result<(), ReleaseError> {
    let temporary = destination.with_extension("candidate-tmp");
    fs::copy(source, &temporary).map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "failed to stage release artifact {}: {error}",
            source.display()
        ))
    })?;
    fs::rename(&temporary, destination).map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "failed to publish release artifact {}: {error}",
            destination.display()
        ))
    })
}

/// Re-hash the recorded candidate files. Any difference refuses the publish: a
/// task that rebuilt instead of using the certified candidate breaks §6.
pub fn verify_artifact_digests(artifacts: &[RecordArtifact]) -> Result<(), ReleaseError> {
    let mut problems = Vec::new();
    for artifact in artifacts {
        if artifact.files.is_empty() {
            problems.push(format!(
                "release artifact `{}` is recorded but has no candidate file",
                artifact.name
            ));
            continue;
        }
        for file in &artifact.files {
            let path = Path::new(&file.path);
            match sha256_file(path) {
                Ok((sha256, bytes)) => {
                    if sha256 != file.sha256 || bytes != file.bytes {
                        problems.push(format!(
                            "release artifact `{}` changed after certify: {} was {} bytes sha256 {}, now {} bytes sha256 {}",
                            artifact.name,
                            file.path,
                            file.bytes,
                            file.sha256,
                            bytes,
                            sha256
                        ));
                    }
                }
                Err(error) => problems.push(error.to_string()),
            }
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(ReleaseError::TaskInvocation(format!(
            "release publish refused: the certified artifact digests no longer match (contract 054 §6). {}",
            problems.join("; ")
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digests_are_the_sha256_hex_of_the_bytes() {
        assert_eq!(
            sha256_bytes(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn a_glob_resolves_below_its_fixed_prefix() {
        let fixture = tempfile::tempdir().expect("fixture");
        let root = fixture.path();
        fs::create_dir_all(root.join("dist/nested")).expect("dist");
        fs::write(root.join("dist/core-1.tgz"), b"core").expect("core");
        fs::write(root.join("dist/react-1.tgz"), b"react").expect("react");
        fs::write(root.join("dist/nested/core-2.tgz"), b"nested").expect("nested");
        let files = resolve_artifact_files(root, "dist/core-*.tgz").expect("resolve");
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("dist/core-1.tgz"));

        let all = resolve_artifact_files(root, "dist/").expect("directory");
        assert_eq!(all.len(), 3, "got {all:?}");
    }

    #[test]
    fn a_changed_candidate_file_refuses_its_digest() {
        let fixture = tempfile::tempdir().expect("fixture");
        let root = fixture.path();
        fs::write(root.join("app.tgz"), b"artifact").expect("artifact");
        let artifact = capture_artifact(root, &root.join("candidate"), "app", "pack", "app.tgz")
            .expect("capture");
        verify_artifact_digests(std::slice::from_ref(&artifact)).expect("unchanged");

        fs::write(root.join("candidate/app/app.tgz"), b"rebuilt").expect("rebuild");
        let error = verify_artifact_digests(std::slice::from_ref(&artifact))
            .expect_err("changed artifact");
        assert!(error.to_string().contains("changed after certify"), "{error}");
    }
}
