//! The release task protocol (contract 054 §6).
//!
//! A task target, artifact task or verify task receives the candidate as JSON
//! at `$EFFIGY_RELEASE_INPUT` and writes its result to `$EFFIGY_RELEASE_OUTPUT`.
//! The input carries the exact certified artifacts, so a task that rebuilds
//! instead of using them is visible in the digest re-check before publish.

use std::path::Path;

use serde_json::{json, Value};

use super::record::{ReleaseRecord, ReleaseStage};

pub const RELEASE_TASK_INPUT_SCHEMA: &str = "effigy.release.input.v1";
pub const RELEASE_TASK_OUTPUT_SCHEMA: &str = "effigy.release.output.v1";
pub const RELEASE_INPUT_ENV: &str = "EFFIGY_RELEASE_INPUT";
pub const RELEASE_OUTPUT_ENV: &str = "EFFIGY_RELEASE_OUTPUT";

/// What a release task is run for: one target, one artifact, one identity, or
/// the release itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseTaskSubject<'a> {
    Target(&'a str),
    Artifact(&'a str),
    Identity(&'a str),
    Release,
}

impl<'a> ReleaseTaskSubject<'a> {
    pub fn kind(self) -> &'static str {
        match self {
            Self::Target(_) => "target",
            Self::Artifact(_) => "artifact",
            Self::Identity(_) => "identity",
            Self::Release => "release",
        }
    }

    pub fn name(self) -> Option<&'a str> {
        match self {
            Self::Target(name) | Self::Artifact(name) | Self::Identity(name) => Some(name),
            Self::Release => None,
        }
    }

    fn target(self) -> Option<&'a str> {
        match self {
            Self::Target(name) => Some(name),
            _ => None,
        }
    }

    fn document(self) -> Value {
        match self.name() {
            Some(name) => json!({ "kind": self.kind(), "name": name }),
            None => json!({ "kind": self.kind() }),
        }
    }
}

/// The `effigy.release.input.v1` document a release task reads.
pub fn release_input_value(
    record: &ReleaseRecord,
    stage: ReleaseStage,
    subject: ReleaseTaskSubject<'_>,
) -> Value {
    let identities: Vec<Value> = record
        .identities
        .iter()
        .map(|identity| {
            json!({
                "name": identity.name,
                "kind": identity.kind,
                "identity": identity.identity,
                "tag": identity.tag,
            })
        })
        .collect();
    let artifacts: Vec<Value> = record
        .artifacts
        .iter()
        .map(|artifact| {
            json!({
                "name": artifact.name,
                "pattern": artifact.pattern,
                "files": artifact.files.iter().map(|file| json!({
                    "path": file.path,
                    "sha256": file.sha256,
                    "bytes": file.bytes,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    json!({
        "schema": RELEASE_TASK_INPUT_SCHEMA,
        "schema_version": 1,
        "run": record.release_id,
        "invocation": record.run_id,
        "stage": stage.label(),
        "source_commit": record.source_commit,
        "candidate_commit": record.candidate_commit,
        "identities": identities,
        "artifacts": artifacts,
        "target": subject.target(),
        "subject": subject.document(),
    })
}

/// What a release task reported about its destination (contract 054 §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Committed {
    /// The task published the candidate.
    Yes,
    /// The task did not publish the candidate.
    No,
    /// The task ran and did not say, or could not be read: the publish outcome
    /// is unknown and is never retried blind (054 §7).
    Unknown,
}

/// The `effigy.release.output.v1` document a task target writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseTaskOutput {
    pub committed: Committed,
    /// Destination IDs: a release URL, a package@version, an image digest or a
    /// deploy ID.
    pub destinations: Vec<String>,
    pub note: Option<String>,
}

impl ReleaseTaskOutput {
    /// A task that exits without writing an output committed nothing it can
    /// vouch for (contract 054 §6).
    pub fn unknown() -> Self {
        Self {
            committed: Committed::Unknown,
            destinations: Vec::new(),
            note: None,
        }
    }
}

/// Parse one `effigy.release.output.v1` document. `Err` carries why the
/// document is unusable; the caller treats that as an unknown outcome, never as
/// "published" or "not published".
pub fn parse_release_task_output(raw: &[u8]) -> Result<ReleaseTaskOutput, String> {
    let value: Value =
        serde_json::from_slice(raw).map_err(|error| format!("output is not JSON: {error}"))?;
    let Some(object) = value.as_object() else {
        return Err("output is not a JSON object".to_owned());
    };
    const FIELDS: [&str; 5] = [
        "schema",
        "schema_version",
        "committed",
        "destinations",
        "note",
    ];
    if let Some(unknown) = object.keys().find(|key| !FIELDS.contains(&key.as_str())) {
        return Err(format!(
            "output field `{unknown}` is not part of `{RELEASE_TASK_OUTPUT_SCHEMA}`"
        ));
    }
    if let Some(version) = object.get("schema_version") {
        if version != &serde_json::json!(1) {
            return Err(format!("output `schema_version` is {version}, not 1"));
        }
    }
    if let Some(schema) = object.get("schema").and_then(Value::as_str) {
        if schema != RELEASE_TASK_OUTPUT_SCHEMA {
            return Err(format!(
                "output schema `{schema}` is not `{RELEASE_TASK_OUTPUT_SCHEMA}`"
            ));
        }
    }
    let committed = match object.get("committed") {
        Some(Value::Bool(true)) => Committed::Yes,
        Some(Value::Bool(false)) => Committed::No,
        Some(Value::String(value)) if value.trim() == "unknown" => Committed::Unknown,
        Some(other) => {
            return Err(format!(
                "output `committed` is {other}, not true, false or \"unknown\""
            ))
        }
        None => return Err("output has no `committed` field".to_owned()),
    };
    let destinations = match object.get("destinations") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(values)) => {
            let mut destinations = Vec::with_capacity(values.len());
            for value in values {
                let Some(destination) = value.as_str().map(str::trim) else {
                    return Err("output `destinations` must be an array of strings".to_owned());
                };
                if !destination.is_empty() {
                    destinations.push(destination.to_owned());
                }
            }
            destinations
        }
        Some(_) => return Err("output `destinations` must be an array of strings".to_owned()),
    };
    let note = object
        .get("note")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|note| !note.is_empty())
        .map(str::to_owned);
    Ok(ReleaseTaskOutput {
        committed,
        destinations,
        note,
    })
}

/// Read a task's output file. A missing file is `Ok(None)`: the task exited
/// without writing one.
pub fn read_release_task_output(path: &Path) -> Result<Option<ReleaseTaskOutput>, String> {
    match std::fs::read(path) {
        Ok(raw) => parse_release_task_output(&raw).map(Some),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("output could not be read: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_input_names_the_candidate_and_its_artifacts() {
        let mut record = ReleaseRecord::new("run-1", std::path::Path::new("/repo"), "now");
        record.candidate_commit = Some("abc".into());
        record
            .identities
            .push(super::super::record::RecordIdentity {
                name: "core".into(),
                kind: "semver".into(),
                identity: Some("1.2.3".into()),
                tag: Some("core-v1.2.3".into()),
            });
        let value = release_input_value(
            &record,
            ReleaseStage::Certify,
            ReleaseTaskSubject::Artifact("core"),
        );
        assert_eq!(value["schema"], RELEASE_TASK_INPUT_SCHEMA);
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["candidate_commit"], "abc");
        assert_eq!(value["identities"][0]["tag"], "core-v1.2.3");
        assert_eq!(value["subject"]["kind"], "artifact");
        assert_eq!(value["subject"]["name"], "core");
        assert_eq!(value["target"], serde_json::Value::Null);

        let target = release_input_value(
            &record,
            ReleaseStage::Publish,
            ReleaseTaskSubject::Target("npm"),
        );
        assert_eq!(target["target"], "npm");
        assert_eq!(target["subject"]["kind"], "target");
    }

    #[test]
    fn an_output_reads_committed_destinations_and_note() {
        let output = parse_release_task_output(
            br#"{"schema":"effigy.release.output.v1","committed":true,"destinations":["core@1.1.0","https://example.test/1"],"note":"uploaded"}"#,
        )
        .expect("output");
        assert_eq!(output.committed, Committed::Yes);
        assert_eq!(
            output.destinations,
            ["core@1.1.0", "https://example.test/1"]
        );
        assert_eq!(output.note.as_deref(), Some("uploaded"));

        let not_published = parse_release_task_output(br#"{"committed":false}"#).expect("output");
        assert_eq!(not_published.committed, Committed::No);
        assert!(not_published.destinations.is_empty());

        let unknown = parse_release_task_output(br#"{"committed":"unknown"}"#).expect("output");
        assert_eq!(unknown.committed, Committed::Unknown);
    }

    #[test]
    fn an_unusable_output_is_an_error_never_an_assumption() {
        assert!(parse_release_task_output(br#"{"committed":"yes"}"#).is_err());
        assert!(parse_release_task_output(br#"{"destinations":["x"]}"#).is_err());
        assert!(parse_release_task_output(br#"not json"#).is_err());
        assert!(parse_release_task_output(
            br#"{"schema":"effigy.release.output.v2","committed":true}"#
        )
        .is_err());
        assert!(
            parse_release_task_output(br#"{"committed":true,"destination":["typo"]}"#).is_err()
        );
        assert!(parse_release_task_output(br#"{"schema_version":2,"committed":true}"#).is_err());
    }
}
