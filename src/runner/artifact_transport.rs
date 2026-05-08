use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use effigy_artifacts::{
    ArtifactKind, OciArtifactAdapter, OciArtifactDescriptor, OciArtifactError,
    OciArtifactInspectRequest, OciArtifactPullReport, OciArtifactPullRequest,
    OciArtifactPushReport, OciArtifactPushRequest,
};
use serde_json::Value;

#[derive(Debug, Clone)]
pub(super) struct OrasCliArtifactAdapter {
    executable: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OciOperation {
    Inspect,
    Pull,
    Push,
}

impl Default for OrasCliArtifactAdapter {
    fn default() -> Self {
        Self {
            executable: PathBuf::from("oras"),
        }
    }
}

impl OciArtifactAdapter for OrasCliArtifactAdapter {
    fn inspect(
        &self,
        request: &OciArtifactInspectRequest,
    ) -> Result<OciArtifactDescriptor, OciArtifactError> {
        let output = ProcessCommand::new(&self.executable)
            .args([
                "manifest",
                "fetch",
                "--descriptor",
                "--format",
                "json",
                request.reference.reference(),
            ])
            .output()
            .map_err(|error| OciArtifactError::InspectFailed {
                reference: request.reference.redacted(),
                message: oras_invocation_error_message(OciOperation::Inspect, error),
            })?;
        if !output.status.success() {
            return Err(OciArtifactError::InspectFailed {
                reference: request.reference.redacted(),
                message: remediate_oras_failure(
                    OciOperation::Inspect,
                    &request.reference,
                    &output.stderr,
                ),
            });
        }
        parse_oras_descriptor(&request.reference, &output.stdout).map_err(|message| {
            OciArtifactError::InspectFailed {
                reference: request.reference.redacted(),
                message,
            }
        })
    }

    fn pull(
        &self,
        request: &OciArtifactPullRequest,
    ) -> Result<OciArtifactPullReport, OciArtifactError> {
        let pulled_root = request
            .destination_root
            .join(safe_oci_pull_dir_name(request.reference.reference()));
        fs::create_dir_all(&pulled_root).map_err(|error| OciArtifactError::PullFailed {
            reference: request.reference.redacted(),
            message: format!(
                "failed to create pull directory {}: {error}",
                pulled_root.display()
            ),
        })?;

        let output = ProcessCommand::new(&self.executable)
            .arg("pull")
            .arg(request.reference.reference())
            .arg("--output")
            .arg(&pulled_root)
            .arg("--format")
            .arg("json")
            .output()
            .map_err(|error| OciArtifactError::PullFailed {
                reference: request.reference.redacted(),
                message: oras_invocation_error_message(OciOperation::Pull, error),
            })?;
        if !output.status.success() {
            return Err(OciArtifactError::PullFailed {
                reference: request.reference.redacted(),
                message: remediate_oras_failure(
                    OciOperation::Pull,
                    &request.reference,
                    &output.stderr,
                ),
            });
        }

        let descriptor = self.inspect(&OciArtifactInspectRequest {
            reference: request.reference.clone(),
        })?;
        let primary_files = parse_oras_pull_files(&pulled_root, &output.stdout)
            .or_else(|| discover_pulled_files(&pulled_root).ok())
            .filter(|files| !files.is_empty())
            .ok_or_else(|| OciArtifactError::PullFailed {
                reference: request.reference.redacted(),
                message: "pull completed but no files were reported or discovered".to_owned(),
            })?;

        Ok(OciArtifactPullReport {
            descriptor,
            pulled_root,
            primary_files,
        })
    }

    fn push(
        &self,
        request: &OciArtifactPushRequest,
    ) -> Result<OciArtifactPushReport, OciArtifactError> {
        if request.primary_files.is_empty() {
            return Err(OciArtifactError::PushFailed {
                reference: request.reference.redacted(),
                message: "push request has no primary files".to_owned(),
            });
        }

        let mut command = ProcessCommand::new(&self.executable);
        command
            .arg("push")
            .arg(request.reference.reference())
            .arg(&request.metadata_path);
        for file in &request.primary_files {
            command.arg(file);
        }
        command.arg("--format").arg("json");
        let output = command
            .output()
            .map_err(|error| OciArtifactError::PushFailed {
                reference: request.reference.redacted(),
                message: oras_invocation_error_message(OciOperation::Push, error),
            })?;
        if !output.status.success() {
            return Err(OciArtifactError::PushFailed {
                reference: request.reference.redacted(),
                message: remediate_oras_failure(
                    OciOperation::Push,
                    &request.reference,
                    &output.stderr,
                ),
            });
        }

        let mut descriptor = parse_oras_descriptor(&request.reference, &output.stdout)
            .unwrap_or_else(|_| OciArtifactDescriptor::new(&request.reference));
        if descriptor.digest.is_none() {
            descriptor = self.inspect(&OciArtifactInspectRequest {
                reference: request.reference.clone(),
            })?;
        }
        Ok(OciArtifactPushReport {
            pushed_ref: request.reference.redacted(),
            digest: descriptor.digest.clone(),
            descriptor,
        })
    }
}

pub(super) fn infer_kind_from_primary_files(primary_files: &[PathBuf]) -> ArtifactKind {
    primary_files
        .iter()
        .find_map(|path| ArtifactKind::from_path(path))
        .unwrap_or(ArtifactKind::AppSpecific)
}

pub(super) fn parse_oras_descriptor(
    reference: &effigy_artifacts::OciArtifactRef,
    stdout: &[u8],
) -> Result<OciArtifactDescriptor, String> {
    let value: Value = serde_json::from_slice(stdout)
        .map_err(|error| format!("failed to parse `oras manifest fetch --format json`: {error}"))?;
    let mut descriptor = OciArtifactDescriptor::new(reference);
    if let Some(digest) = value.get("digest").and_then(Value::as_str) {
        descriptor = descriptor.with_digest(digest);
    }
    if let Some(media_type) = value
        .get("mediaType")
        .or_else(|| value.get("media_type"))
        .and_then(Value::as_str)
    {
        descriptor = descriptor.with_media_type(media_type);
    }
    if let Some(size) = value.get("size").and_then(Value::as_u64) {
        descriptor = descriptor.with_size(size);
    }
    Ok(descriptor)
}

pub(super) fn parse_oras_pull_files(pulled_root: &Path, stdout: &[u8]) -> Option<Vec<PathBuf>> {
    let value: Value = serde_json::from_slice(stdout).ok()?;
    let files = value.get("files")?.as_array()?;
    let mut paths = Vec::new();
    for file in files {
        let path = file.get("path").and_then(Value::as_str)?;
        paths.push(path_relative_to_root(pulled_root, Path::new(path)));
    }
    Some(paths)
}

pub(super) fn sanitize_process_output(
    reference: &effigy_artifacts::OciArtifactRef,
    bytes: &[u8],
) -> String {
    let output = String::from_utf8_lossy(bytes);
    let redacted = reference.redacted();
    output
        .replace(reference.reference(), &redacted)
        .trim()
        .to_owned()
}

fn oras_invocation_error_message(operation: OciOperation, error: std::io::Error) -> String {
    format!(
        "failed to run `oras`: {error}; install ORAS and authenticate with the registry before {} OCI artifacts",
        operation_verb(operation)
    )
}

fn remediate_oras_failure(
    operation: OciOperation,
    reference: &effigy_artifacts::OciArtifactRef,
    bytes: &[u8],
) -> String {
    let detail = sanitize_process_output(reference, bytes);
    let lower = detail.to_ascii_lowercase();

    let remediation = if looks_like_auth_failure(&lower) {
        let registry = registry_host(reference.reference()).unwrap_or("the target registry");
        format!("authenticate first with `oras login {registry}` and retry")
    } else if operation == OciOperation::Push && looks_like_push_denial(&lower) {
        "authenticate with push access for the target repository tag and retry".to_owned()
    } else if looks_like_registry_reachability_failure(&lower) {
        "check the registry hostname, network reachability, and TLS setup, then retry".to_owned()
    } else if looks_like_reference_shape_failure(&lower) {
        "check the explicit `oci://` reference shape and verify it manually with `oras manifest fetch <ref>`".to_owned()
    } else {
        format!(
            "check the registry response above and retry the {} once the underlying issue is fixed",
            operation_label(operation)
        )
    };

    format!("{detail}; {remediation}")
}

fn operation_verb(operation: OciOperation) -> &'static str {
    match operation {
        OciOperation::Inspect => "inspecting",
        OciOperation::Pull => "pulling",
        OciOperation::Push => "pushing",
    }
}

fn operation_label(operation: OciOperation) -> &'static str {
    match operation {
        OciOperation::Inspect => "inspect",
        OciOperation::Pull => "pull",
        OciOperation::Push => "push",
    }
}

fn looks_like_auth_failure(lower: &str) -> bool {
    lower.contains("unauthorized")
        || lower.contains("authentication required")
        || lower.contains("not logged in")
        || lower.contains("no basic auth credentials")
        || lower.contains("token has expired")
}

fn looks_like_push_denial(lower: &str) -> bool {
    lower.contains("denied")
        || lower.contains("requested access to the resource is denied")
        || lower.contains("insufficient_scope")
        || lower.contains("permission_denied")
}

fn looks_like_registry_reachability_failure(lower: &str) -> bool {
    lower.contains("no such host")
        || lower.contains("dial tcp")
        || lower.contains("connection refused")
        || lower.contains("i/o timeout")
        || lower.contains("timeout")
        || lower.contains("tls handshake timeout")
        || lower.contains("server misbehaving")
        || lower.contains("temporary failure in name resolution")
}

fn looks_like_reference_shape_failure(lower: &str) -> bool {
    lower.contains("invalid reference")
        || lower.contains("invalid repository")
        || lower.contains("invalid tag")
        || lower.contains("bad reference")
        || lower.contains("invalid checksum digest")
        || lower.contains("invalid artifact reference")
}

fn registry_host(reference: &str) -> Option<&str> {
    reference.split('/').next().and_then(|authority| {
        authority
            .rsplit_once('@')
            .map(|(_, host)| host)
            .or(Some(authority))
    })
}

fn discover_pulled_files(root: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_files(root, root, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_files(root: &Path, dir: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, files)?;
        } else if path.is_file() {
            files.push(path_relative_to_root(root, &path));
        }
    }
    Ok(())
}

fn path_relative_to_root(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root).unwrap_or(path).to_path_buf()
}

fn safe_oci_pull_dir_name(reference: &str) -> String {
    let mut slug = String::new();
    for ch in reference.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "artifact".to_owned()
    } else {
        slug.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use effigy_artifacts::ArtifactSourceRef;

    #[test]
    fn oras_stderr_sanitizer_removes_userinfo_and_raw_reference() {
        let parsed = ArtifactSourceRef::parse("oci://token:secret@ghcr.io/acowtancy/private:uat")
            .expect("parse");
        let ArtifactSourceRef::Oci(oci) = parsed else {
            panic!("expected oci");
        };

        let sanitized = sanitize_process_output(
            &oci,
            b"pull token:secret@ghcr.io/acowtancy/private:uat denied",
        );

        assert!(!sanitized.contains("token:secret"));
        assert!(sanitized.contains("***@ghcr.io/acowtancy/private:uat"));
    }

    #[test]
    fn oras_failure_remediation_points_to_registry_login_for_auth_errors() {
        let parsed =
            ArtifactSourceRef::parse("oci://ghcr.io/acowtancy/private:uat").expect("parse");
        let ArtifactSourceRef::Oci(oci) = parsed else {
            panic!("expected oci");
        };

        let message = remediate_oras_failure(
            OciOperation::Pull,
            &oci,
            b"Error response from registry: unauthorized",
        );

        assert!(message.contains("unauthorized"));
        assert!(message.contains("oras login ghcr.io"));
    }

    #[test]
    fn oras_failure_remediation_calls_out_push_access_for_denied_pushes() {
        let parsed =
            ArtifactSourceRef::parse("oci://ghcr.io/acowtancy/private:uat").expect("parse");
        let ArtifactSourceRef::Oci(oci) = parsed else {
            panic!("expected oci");
        };

        let message = remediate_oras_failure(
            OciOperation::Push,
            &oci,
            b"push denied: requested access to the resource is denied",
        );

        assert!(message.contains("requested access to the resource is denied"));
        assert!(message.contains("push access"));
    }

    #[test]
    fn oras_failure_remediation_calls_out_reference_shape_errors() {
        let parsed =
            ArtifactSourceRef::parse("oci://ghcr.io/acowtancy/private:uat").expect("parse");
        let ArtifactSourceRef::Oci(oci) = parsed else {
            panic!("expected oci");
        };

        let message = remediate_oras_failure(
            OciOperation::Inspect,
            &oci,
            b"invalid reference: invalid tag",
        );

        assert!(message.contains("invalid reference"));
        assert!(message.contains("oci://"));
        assert!(message.contains("oras manifest fetch <ref>"));
    }

    #[test]
    fn oras_failure_remediation_calls_out_registry_reachability_errors() {
        let parsed =
            ArtifactSourceRef::parse("oci://ghcr.io/acowtancy/private:uat").expect("parse");
        let ArtifactSourceRef::Oci(oci) = parsed else {
            panic!("expected oci");
        };

        let message = remediate_oras_failure(
            OciOperation::Inspect,
            &oci,
            b"dial tcp: lookup ghcr.io: no such host",
        );

        assert!(message.contains("no such host"));
        assert!(message.contains("network reachability"));
    }

    #[test]
    fn oras_invocation_error_mentions_install_and_authenticate() {
        let message = oras_invocation_error_message(
            OciOperation::Push,
            std::io::Error::new(std::io::ErrorKind::NotFound, "No such file or directory"),
        );

        assert!(message.contains("failed to run `oras`"));
        assert!(message.contains("install ORAS"));
        assert!(message.contains("authenticate with the registry"));
        assert!(message.contains("pushing OCI artifacts"));
    }
}
