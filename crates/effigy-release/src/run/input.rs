//! The release task protocol (contract 054 §6).
//!
//! A task target, artifact task or verify task receives the candidate as JSON
//! at `$EFFIGY_RELEASE_INPUT` and writes its result to `$EFFIGY_RELEASE_OUTPUT`.
//! The input carries the exact certified artifacts, so a task that rebuilds
//! instead of using them is visible in the digest re-check before publish.

use serde_json::{json, Value};

use super::record::{ReleaseRecord, ReleaseStage};

pub const RELEASE_TASK_INPUT_SCHEMA: &str = "effigy.release.input.v1";
pub const RELEASE_INPUT_ENV: &str = "EFFIGY_RELEASE_INPUT";
pub const RELEASE_OUTPUT_ENV: &str = "EFFIGY_RELEASE_OUTPUT";

/// The `effigy.release.input.v1` document a release task reads.
pub fn release_input_value(
    record: &ReleaseRecord,
    stage: ReleaseStage,
    subject: Option<&str>,
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
    let mut document = json!({
        "schema": RELEASE_TASK_INPUT_SCHEMA,
        "run": record.release_id,
        "invocation": record.run_id,
        "stage": stage.label(),
        "source_commit": record.source_commit,
        "candidate_commit": record.candidate_commit,
        "identities": identities,
        "artifacts": artifacts,
    });
    if let Some(subject) = subject {
        document["subject"] = Value::String(subject.to_owned());
    }
    document
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_input_names_the_candidate_and_its_artifacts() {
        let mut record = ReleaseRecord::new("run-1", &std::path::PathBuf::from("/repo"), "now");
        record.candidate_commit = Some("abc".into());
        record.identities.push(super::super::record::RecordIdentity {
            name: "core".into(),
            kind: "semver".into(),
            identity: Some("1.2.3".into()),
            tag: Some("core-v1.2.3".into()),
        });
        let value = release_input_value(&record, ReleaseStage::Certify, Some("core"));
        assert_eq!(value["schema"], RELEASE_TASK_INPUT_SCHEMA);
        assert_eq!(value["candidate_commit"], "abc");
        assert_eq!(value["identities"][0]["tag"], "core-v1.2.3");
        assert_eq!(value["subject"], "core");
    }
}
