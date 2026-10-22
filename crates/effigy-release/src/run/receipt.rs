//! The release receipt (contract 054 §5): the durable record of one Run.
//!
//! A receipt is a rendering of the record: its inputs, checkpoints, gate
//! results, artifact digests, approvals, destination IDs and verification
//! results. It never re-reads git or the network, so `release receipt` reports
//! what the Run actually recorded.

use serde::Serialize;

use super::record::{
    RecordApproval, RecordArtifact, RecordCheckpoint, RecordGate, RecordIdentity, RecordTarget,
    RecordTargetState, RecordVerification, ReleaseRecord, ReleaseRunStatus, RELEASE_RECEIPT_SCHEMA,
};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReceiptRelease {
    pub id: String,
    pub invocation: String,
    pub root: String,
    pub stage: String,
    pub status: String,
    pub waiting_on: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReceiptSource {
    pub branch: Option<String>,
    pub commit: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReceiptCandidate {
    pub commit: Option<String>,
    pub identities: Vec<RecordIdentity>,
    pub artifacts: Vec<RecordArtifact>,
}

/// One release Run's receipt.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReleaseReceipt {
    pub schema: String,
    pub release: ReceiptRelease,
    pub source: ReceiptSource,
    pub candidate: ReceiptCandidate,
    pub checkpoints: Vec<RecordCheckpoint>,
    pub gates: Vec<RecordGate>,
    pub approvals: Vec<RecordApproval>,
    pub targets: Vec<RecordTarget>,
    pub verification: Vec<RecordVerification>,
    /// Whether every target published the candidate.
    pub published: bool,
    /// Whether a verify task failed after publishing.
    pub unverified: bool,
    pub errors: Vec<String>,
}

impl ReleaseReceipt {
    pub fn from_record(record: &ReleaseRecord) -> Self {
        let published = !record.targets.is_empty()
            && record
                .targets
                .iter()
                .all(|target| target.state == RecordTargetState::Published);
        let unverified = record
            .verifications
            .iter()
            .any(|verification| !verification.passed);
        Self {
            schema: RELEASE_RECEIPT_SCHEMA.to_owned(),
            release: ReceiptRelease {
                id: record.release_id.clone(),
                invocation: record.run_id.clone(),
                root: record.root.clone(),
                stage: record.stage.clone(),
                status: record.status.label().to_owned(),
                waiting_on: record.waiting_on.clone(),
                created_at: record.created_at.clone(),
                updated_at: record.updated_at.clone(),
            },
            source: ReceiptSource {
                branch: record.branch.clone(),
                commit: record.source_commit.clone(),
            },
            candidate: ReceiptCandidate {
                commit: record.candidate_commit.clone(),
                identities: record.identities.clone(),
                artifacts: record.artifacts.clone(),
            },
            checkpoints: record.checkpoints.clone(),
            gates: record.gates.clone(),
            approvals: record.approvals.clone(),
            targets: record.targets.clone(),
            verification: record.verifications.clone(),
            published,
            unverified,
            errors: record.errors.clone(),
        }
    }

    pub fn complete(&self) -> bool {
        self.release.status == ReleaseRunStatus::Complete.label()
    }
}

/// The `effigy.release.run.v1` payload a `release run` prints: the receipt,
/// where it lives and what the invocation did.
pub fn render_release_run_json(
    receipt: &ReleaseReceipt,
    messages: &[String],
    receipt_path: &str,
) -> String {
    let payload = serde_json::json!({
        "schema": "effigy.release.run.v1",
        "receipt_path": receipt_path,
        "messages": messages,
        "receipt": receipt,
    });
    serde_json::to_string_pretty(&payload).unwrap_or_else(|error| {
        format!("{{\"schema\":\"effigy.release.run.v1\",\"error\":{error:?}}}")
    })
}

pub fn render_release_receipt_json(receipt: &ReleaseReceipt) -> String {
    serde_json::to_string_pretty(receipt).unwrap_or_else(|error| {
        format!("{{\"schema\":\"{RELEASE_RECEIPT_SCHEMA}\",\"error\":{error:?}}}")
    })
}

/// The text receipt: the facts an operator reads at closeout.
pub fn render_release_receipt_text(receipt: &ReleaseReceipt) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "Release receipt {} ({})",
        receipt.release.id, receipt.release.status
    ));
    lines.push(format!("  root: {}", receipt.release.root));
    if let Some(branch) = &receipt.source.branch {
        lines.push(format!(
            "  source: {} at {}",
            branch,
            receipt.source.commit.as_deref().unwrap_or("unknown")
        ));
    }
    lines.push(format!(
        "  candidate commit: {}",
        receipt.candidate.commit.as_deref().unwrap_or("none")
    ));
    if receipt.candidate.identities.is_empty() {
        lines.push("  identities: none".to_owned());
    } else {
        lines.push("  identities:".to_owned());
        for identity in &receipt.candidate.identities {
            lines.push(format!(
                "    {} ({}) {} {}",
                identity.name,
                identity.kind,
                identity.identity.as_deref().unwrap_or("-"),
                identity.tag.as_deref().unwrap_or("")
            ));
        }
    }
    if !receipt.candidate.artifacts.is_empty() {
        lines.push("  artifacts:".to_owned());
        for artifact in &receipt.candidate.artifacts {
            for file in &artifact.files {
                lines.push(format!(
                    "    {} {} ({} bytes) {}",
                    artifact.name, file.sha256, file.bytes, file.path
                ));
            }
        }
    }
    if !receipt.checkpoints.is_empty() {
        lines.push("  checkpoints:".to_owned());
        for checkpoint in &receipt.checkpoints {
            lines.push(format!(
                "    {} at {} ({})",
                checkpoint.stage, checkpoint.at, checkpoint.run
            ));
        }
    }
    if !receipt.gates.is_empty() {
        lines.push("  gates:".to_owned());
        for gate in &receipt.gates {
            lines.push(format!(
                "    {} {} {}",
                gate.stage,
                gate.name,
                if gate.passed { "passed" } else { "failed" }
            ));
        }
    }
    if !receipt.approvals.is_empty() {
        lines.push("  approvals:".to_owned());
        for approval in &receipt.approvals {
            lines.push(format!(
                "    {} approved by {} at {}",
                approval.stage, approval.approved_by, approval.approved_at
            ));
        }
    }
    if !receipt.targets.is_empty() {
        lines.push("  targets:".to_owned());
        for target in &receipt.targets {
            let state = match target.state {
                RecordTargetState::Pending => "pending",
                RecordTargetState::Unknown => "unknown",
                RecordTargetState::Published => "published",
                RecordTargetState::Failed => "failed",
            };
            lines.push(format!("    {} ({}) {}", target.name, target.kind, state));
            for destination in &target.destinations {
                lines.push(format!(
                    "      {} at {} ({})",
                    destination.destination, destination.commit, destination.identity
                ));
            }
        }
    }
    if !receipt.verification.is_empty() {
        lines.push("  verification:".to_owned());
        for verification in &receipt.verification {
            lines.push(format!(
                "    {} {} {}",
                verification.target,
                verification.task,
                if verification.passed {
                    "passed"
                } else {
                    "failed"
                }
            ));
            if let Some(note) = &verification.note {
                lines.push(format!("      {note}"));
            }
        }
    }
    if receipt.published {
        lines.push("  published: yes".to_owned());
    }
    if receipt.unverified {
        lines.push("  unverified: yes".to_owned());
    }
    for error in &receipt.errors {
        lines.push(format!("  error: {error}"));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> ReleaseRecord {
        let mut record = ReleaseRecord::new(
            "run-1",
            &std::path::PathBuf::from("/repo"),
            "2026-10-22T00:00:00Z",
        );
        record.status = ReleaseRunStatus::Complete;
        record.stage = "closeout".into();
        record.candidate_commit = Some("abc".into());
        record.targets.push(RecordTarget {
            name: "tag".into(),
            kind: "tag".into(),
            state: RecordTargetState::Published,
            destinations: Vec::new(),
            note: None,
        });
        record
    }

    #[test]
    fn a_receipt_renders_its_record_without_re_reading_anything() {
        let receipt = ReleaseReceipt::from_record(&record());
        assert_eq!(receipt.schema, RELEASE_RECEIPT_SCHEMA);
        assert!(receipt.published);
        assert!(!receipt.unverified);
        assert!(receipt.complete());
        let json = render_release_receipt_json(&receipt);
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("json");
        assert_eq!(parsed["schema"], RELEASE_RECEIPT_SCHEMA);
        assert_eq!(parsed["candidate"]["commit"], "abc");
        assert!(render_release_receipt_text(&receipt).contains("Release receipt run-1"));
    }
}
