use super::errors::{OciArtifactError, OciFailureClass};
use super::refs::OciArtifactRef;
use super::util::{digest_from_ref, path_relative_to_root, safe_oci_pull_dir_name};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OciArtifactInspectRequest {
    pub reference: OciArtifactRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OciArtifactPullRequest {
    pub reference: OciArtifactRef,
    pub destination_root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OciArtifactPushRequest {
    pub reference: OciArtifactRef,
    pub staged_root: PathBuf,
    pub metadata_path: PathBuf,
    pub primary_files: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OciArtifactDescriptor {
    pub reference: String,
    pub redacted_reference: String,
    pub digest: Option<String>,
    pub media_type: Option<String>,
    pub size: Option<u64>,
}

impl OciArtifactDescriptor {
    pub fn new(reference: &OciArtifactRef) -> Self {
        let redacted_reference = reference.redacted();
        Self {
            reference: redacted_reference.clone(),
            redacted_reference,
            digest: digest_from_ref(reference.reference()),
            media_type: None,
            size: None,
        }
    }

    pub fn with_digest(mut self, digest: impl Into<String>) -> Self {
        self.digest = Some(digest.into());
        self
    }

    pub fn with_media_type(mut self, media_type: impl Into<String>) -> Self {
        self.media_type = Some(media_type.into());
        self
    }

    pub fn with_size(mut self, size: u64) -> Self {
        self.size = Some(size);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OciArtifactPullReport {
    pub descriptor: OciArtifactDescriptor,
    pub pulled_root: PathBuf,
    pub primary_files: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OciArtifactPushReport {
    pub descriptor: OciArtifactDescriptor,
    pub pushed_ref: String,
    pub digest: Option<String>,
}

pub trait OciArtifactAdapter {
    fn inspect(
        &self,
        request: &OciArtifactInspectRequest,
    ) -> Result<OciArtifactDescriptor, OciArtifactError>;

    fn pull(
        &self,
        request: &OciArtifactPullRequest,
    ) -> Result<OciArtifactPullReport, OciArtifactError>;

    fn push(
        &self,
        request: &OciArtifactPushRequest,
    ) -> Result<OciArtifactPushReport, OciArtifactError>;
}

#[derive(Debug, Clone)]
pub struct OrasCliArtifactAdapter {
    executable: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OciOperation {
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
                request.reference.reference(),
            ])
            .output()
            .map_err(|error| {
                let (class, message) = oras_invocation_failure(OciOperation::Inspect, error);
                OciArtifactError::InspectFailed {
                    reference: request.reference.redacted(),
                    message,
                    class,
                }
            })?;
        if !output.status.success() {
            let (class, message) =
                oras_failure(OciOperation::Inspect, &request.reference, &output.stderr);
            return Err(OciArtifactError::InspectFailed {
                reference: request.reference.redacted(),
                message,
                class,
            });
        }
        parse_oras_descriptor(&request.reference, &output.stdout).map_err(|message| {
            OciArtifactError::InspectFailed {
                reference: request.reference.redacted(),
                message,
                class: OciFailureClass::Unclassified,
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
            class: OciFailureClass::Unclassified,
        })?;

        let output = ProcessCommand::new(&self.executable)
            .arg("pull")
            .arg(request.reference.reference())
            .arg("--output")
            .arg(&pulled_root)
            .arg("--format")
            .arg("json")
            .output()
            .map_err(|error| {
                let (class, message) = oras_invocation_failure(OciOperation::Pull, error);
                OciArtifactError::PullFailed {
                    reference: request.reference.redacted(),
                    message,
                    class,
                }
            })?;
        if !output.status.success() {
            let (class, message) =
                oras_failure(OciOperation::Pull, &request.reference, &output.stderr);
            return Err(OciArtifactError::PullFailed {
                reference: request.reference.redacted(),
                message,
                class,
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
                class: OciFailureClass::Unclassified,
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
                class: OciFailureClass::Unclassified,
            });
        }

        let mut command = ProcessCommand::new(&self.executable);
        command
            .arg("push")
            .arg(request.reference.reference())
            .current_dir(&request.staged_root)
            .arg(path_relative_to_root(
                &request.staged_root,
                &request.metadata_path,
            ));
        for file in &request.primary_files {
            command.arg(path_relative_to_root(&request.staged_root, file));
        }
        command.arg("--format").arg("json");
        let output = command.output().map_err(|error| {
            let (class, message) = oras_invocation_failure(OciOperation::Push, error);
            OciArtifactError::PushFailed {
                reference: request.reference.redacted(),
                message,
                class,
            }
        })?;
        if !output.status.success() {
            let (class, message) =
                oras_failure(OciOperation::Push, &request.reference, &output.stderr);
            return Err(OciArtifactError::PushFailed {
                reference: request.reference.redacted(),
                message,
                class,
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

pub fn parse_oras_descriptor(
    reference: &OciArtifactRef,
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
    bind_pinned_digest(reference, descriptor)
}

pub(crate) fn bind_pinned_digest(
    reference: &OciArtifactRef,
    descriptor: OciArtifactDescriptor,
) -> Result<OciArtifactDescriptor, String> {
    let Some(pinned) = digest_from_ref(reference.reference()) else {
        return Ok(descriptor);
    };
    match descriptor.digest.as_deref() {
        Some(observed) if observed != pinned => Err(format!(
            "OCI digest `{observed}` does not match digest-pinned reference `{pinned}`"
        )),
        Some(_) => Ok(descriptor),
        None => Ok(descriptor.with_digest(pinned)),
    }
}

pub fn parse_oras_pull_files(pulled_root: &Path, stdout: &[u8]) -> Option<Vec<PathBuf>> {
    let value: Value = serde_json::from_slice(stdout).ok()?;
    let files = value.get("files")?.as_array()?;
    let mut paths = Vec::new();
    for file in files {
        let path = file.get("path").and_then(Value::as_str)?;
        paths.push(path_relative_to_root(pulled_root, Path::new(path)));
    }
    Some(paths)
}

pub fn sanitize_process_output(reference: &OciArtifactRef, bytes: &[u8]) -> String {
    let output = String::from_utf8_lossy(bytes);
    let redacted = reference.redacted();
    output
        .replace(reference.reference(), &redacted)
        .trim()
        .to_owned()
}

/// The markers `oras` prints, in precedence order: an expired token is an
/// authentication failure even though the registry also denied the request, and
/// a reachability failure never means the reference was wrong.
const FAILURE_MARKERS: &[(&str, OciFailureClass)] = &[
    ("unauthorized", OciFailureClass::NotAuthenticated),
    ("authentication required", OciFailureClass::NotAuthenticated),
    ("not logged in", OciFailureClass::NotAuthenticated),
    (
        "no basic auth credentials",
        OciFailureClass::NotAuthenticated,
    ),
    ("token has expired", OciFailureClass::NotAuthenticated),
    ("denied", OciFailureClass::NotPermitted),
    ("insufficient_scope", OciFailureClass::NotPermitted),
    ("permission_denied", OciFailureClass::NotPermitted),
    ("no such host", OciFailureClass::Unreachable),
    ("dial tcp", OciFailureClass::Unreachable),
    ("connection refused", OciFailureClass::Unreachable),
    ("timeout", OciFailureClass::Unreachable),
    ("server misbehaving", OciFailureClass::Unreachable),
    (
        "temporary failure in name resolution",
        OciFailureClass::Unreachable,
    ),
    (
        "invalid artifact reference",
        OciFailureClass::InvalidReference,
    ),
    ("invalid reference", OciFailureClass::InvalidReference),
    ("invalid repository", OciFailureClass::InvalidReference),
    ("invalid tag", OciFailureClass::InvalidReference),
    ("bad reference", OciFailureClass::InvalidReference),
    ("invalid checksum digest", OciFailureClass::InvalidReference),
];

/// The one classifier for `oras` output: every class this crate reports is
/// decoded here, once, into a type.
pub(crate) fn classify_oras_output(detail: &str) -> OciFailureClass {
    let lower = detail.to_ascii_lowercase();
    FAILURE_MARKERS
        .iter()
        .find(|(marker, _)| lower.contains(marker))
        .map_or(OciFailureClass::Unclassified, |(_, class)| *class)
}

fn oras_invocation_failure(
    operation: OciOperation,
    error: std::io::Error,
) -> (OciFailureClass, String) {
    let message = format!(
        "failed to run `oras`: {error}; install ORAS and authenticate with the registry before {} OCI artifacts",
        operation_verb(operation)
    );
    (OciFailureClass::ClientMissing, message)
}

/// The registry's own detail plus the next step its class asks for.
fn oras_failure(
    operation: OciOperation,
    reference: &OciArtifactRef,
    bytes: &[u8],
) -> (OciFailureClass, String) {
    let detail = sanitize_process_output(reference, bytes);
    let class = classify_oras_output(&detail);
    let next = next_action(class, operation, reference);
    (class, format!("{detail}; {next}"))
}

/// The next step each class asks for, said after the registry's own detail.
pub(crate) fn next_action(
    class: OciFailureClass,
    operation: OciOperation,
    reference: &OciArtifactRef,
) -> String {
    match class {
        OciFailureClass::NotAuthenticated => {
            let registry = registry_host(reference.reference()).unwrap_or("the target registry");
            format!("authenticate first with `oras login {registry}` and retry")
        }
        OciFailureClass::NotPermitted if operation == OciOperation::Push => {
            "authenticate with push access for the target repository tag and retry".to_owned()
        }
        OciFailureClass::NotPermitted => {
            "authenticate with access to the target repository and retry".to_owned()
        }
        OciFailureClass::Unreachable => {
            "check the registry hostname, network reachability, and TLS setup, then retry".to_owned()
        }
        OciFailureClass::InvalidReference => {
            "check the explicit `oci://` reference shape and verify it manually with `oras manifest fetch <ref>`".to_owned()
        }
        OciFailureClass::ClientMissing => {
            "install ORAS and authenticate with the registry, then retry".to_owned()
        }
        OciFailureClass::Unclassified => format!(
            "check the registry response above and retry the {} once the underlying issue is fixed",
            operation_label(operation)
        ),
    }
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
