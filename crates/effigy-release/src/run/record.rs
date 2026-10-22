//! The durable release record: one checkpoint after every stage boundary.
//!
//! Contract 054 §3 keeps one Run's progress as a checkpoint at each stage
//! boundary. The kernel journal holds the checkpoint pointer (§10 ruling 2:
//! state lives outside the repo); this record holds the facts a resume needs:
//! the candidate commit, each identity's version, each artifact's digest and
//! each destination already published. Every field is restored from here, so a
//! resumed Run never re-derives a version or rebuilds an artifact.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The record schema. Versioned separately from the receipt, which renders it.
pub const RELEASE_RECORD_SCHEMA: &str = "effigy.release.record.v1";
/// The checkpoint pointer the kernel engine record carries.
pub const RELEASE_CHECKPOINT_SCHEMA: &str = "effigy.release.checkpoint.v1";
/// The receipt schema contract 054 §5 prints.
pub const RELEASE_RECEIPT_SCHEMA: &str = "effigy.release.receipt.v1";

/// One stage boundary of the release Run (contract 054 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReleaseStage {
    Validate,
    PreGates,
    Prepare,
    Certify,
    Publish,
    Verify,
    Closeout,
}

impl ReleaseStage {
    pub const ALL: [Self; 7] = [
        Self::Validate,
        Self::PreGates,
        Self::Prepare,
        Self::Certify,
        Self::Publish,
        Self::Verify,
        Self::Closeout,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Validate => "validate",
            Self::PreGates => "pre-gates",
            Self::Prepare => "prepare",
            Self::Certify => "certify",
            Self::Publish => "publish",
            Self::Verify => "verify",
            Self::Closeout => "closeout",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|stage| stage.label() == value.trim())
    }

    /// The boundary after this one. `None` for `closeout`.
    pub fn next(self) -> Option<Self> {
        let index = Self::ALL.iter().position(|stage| *stage == self)?;
        Self::ALL.get(index + 1).copied()
    }
}

/// What the Run is doing between invocations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReleaseRunStatus {
    /// The last invocation stopped at a checkpoint before its target stage.
    Paused,
    /// The last invocation stopped at an approval checkpoint.
    AwaitingApproval,
    /// The last invocation failed. A resume may continue from its checkpoint.
    Failed,
    /// Every stage completed.
    Complete,
}

impl ReleaseRunStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Paused => "paused",
            Self::AwaitingApproval => "awaiting-approval",
            Self::Failed => "failed",
            Self::Complete => "complete",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordIdentity {
    pub name: String,
    pub kind: String,
    /// The version (`semver`), the artifact digest (`digest`), the commit
    /// (`source`) or the task's printed value (`task`).
    pub identity: Option<String>,
    pub tag: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordArtifactFile {
    /// Absolute path in the Run's candidate directory.
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordArtifact {
    pub name: String,
    pub task: String,
    /// The configured path pattern the files were resolved from.
    pub pattern: String,
    pub files: Vec<RecordArtifactFile>,
}

impl RecordArtifact {
    /// The identity a `digest` identity reports: one file's digest, or the
    /// digest of the sorted file digests when the artifact holds several.
    pub fn identity_digest(&self) -> Option<String> {
        match self.files.as_slice() {
            [] => None,
            [file] => Some(file.sha256.clone()),
            files => {
                let mut canonical = String::new();
                for file in files {
                    canonical.push_str(&file.sha256);
                    canonical.push('\n');
                }
                Some(crate::run::artifacts::sha256_bytes(canonical.as_bytes()))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordGate {
    pub stage: String,
    pub name: String,
    pub passed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RecordTargetState {
    Pending,
    /// A publish was attempted and its outcome is not known: reconcile before
    /// any retry (contract 054 §7).
    Unknown,
    Published,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordDestination {
    /// The identity whose tag this destination is.
    pub identity: String,
    pub destination: String,
    pub commit: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordTarget {
    pub name: String,
    pub kind: String,
    pub state: RecordTargetState,
    pub destinations: Vec<RecordDestination>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordApproval {
    pub stage: String,
    pub approved_by: String,
    pub approved_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordCheckpoint {
    pub stage: String,
    pub run: String,
    pub status: String,
    pub at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordVerification {
    pub target: String,
    pub task: String,
    pub passed: bool,
    pub note: Option<String>,
}

/// One release Run, durable between invocations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseRecord {
    pub schema: String,
    /// The first invocation's kernel run id: the release's identity.
    pub release_id: String,
    /// The invocation that wrote this record last.
    pub run_id: String,
    pub root: String,
    /// The last completed stage boundary.
    pub stage: String,
    pub status: ReleaseRunStatus,
    pub waiting_on: Option<String>,
    pub branch: Option<String>,
    /// The source commit the release started from.
    pub source_commit: Option<String>,
    /// The prepared commit: the candidate's source (contract 054 §2).
    pub candidate_commit: Option<String>,
    pub identities: Vec<RecordIdentity>,
    pub artifacts: Vec<RecordArtifact>,
    pub gates: Vec<RecordGate>,
    pub targets: Vec<RecordTarget>,
    pub verifications: Vec<RecordVerification>,
    pub approvals: Vec<RecordApproval>,
    pub checkpoints: Vec<RecordCheckpoint>,
    pub pushed_commit: bool,
    pub errors: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl ReleaseRecord {
    pub fn new(release_id: &str, root: &PathBuf, now: &str) -> Self {
        Self {
            schema: RELEASE_RECORD_SCHEMA.to_owned(),
            release_id: release_id.to_owned(),
            run_id: release_id.to_owned(),
            root: root.display().to_string(),
            stage: String::new(),
            status: ReleaseRunStatus::Paused,
            waiting_on: None,
            branch: None,
            source_commit: None,
            candidate_commit: None,
            identities: Vec::new(),
            artifacts: Vec::new(),
            gates: Vec::new(),
            targets: Vec::new(),
            verifications: Vec::new(),
            approvals: Vec::new(),
            checkpoints: Vec::new(),
            pushed_commit: false,
            errors: Vec::new(),
            created_at: now.to_owned(),
            updated_at: now.to_owned(),
        }
    }

    pub fn stage(&self) -> Option<ReleaseStage> {
        ReleaseStage::parse(&self.stage)
    }

    /// Whether the action a checkpoint awaits is already approved.
    pub fn approved(&self, stage: ReleaseStage) -> bool {
        self.approvals
            .iter()
            .any(|approval| approval.stage == stage.label())
    }

    pub fn identity(&self, name: &str) -> Option<&RecordIdentity> {
        self.identities
            .iter()
            .find(|identity| identity.name == name)
    }

    pub fn target(&self, name: &str) -> Option<&RecordTarget> {
        self.targets.iter().find(|target| target.name == name)
    }

    /// The kernel checkpoint pointer: stage and status only, so the store row
    /// stays a small policy document and the record file stays the data.
    pub fn checkpoint(&self, record_path: &std::path::Path) -> ReleaseCheckpoint {
        ReleaseCheckpoint {
            schema: RELEASE_CHECKPOINT_SCHEMA.to_owned(),
            release_id: self.release_id.clone(),
            run_id: self.run_id.clone(),
            stage: self.stage.clone(),
            status: self.status.label().to_owned(),
            waiting_on: self.waiting_on.clone(),
            record: record_path.display().to_string(),
        }
    }
}

/// What the kernel engine record carries for the release owner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseCheckpoint {
    pub schema: String,
    pub release_id: String,
    pub run_id: String,
    pub stage: String,
    pub status: String,
    pub waiting_on: Option<String>,
    /// The record file's absolute path under the Effigy state root.
    pub record: String,
}

impl ReleaseCheckpoint {
    pub fn status(&self) -> Option<ReleaseRunStatus> {
        match self.status.as_str() {
            "paused" => Some(ReleaseRunStatus::Paused),
            "awaiting-approval" => Some(ReleaseRunStatus::AwaitingApproval),
            "failed" => Some(ReleaseRunStatus::Failed),
            "complete" => Some(ReleaseRunStatus::Complete),
            _ => None,
        }
    }

    pub fn stage(&self) -> Option<ReleaseStage> {
        ReleaseStage::parse(&self.stage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_order_matches_the_contract() {
        let labels = ReleaseStage::ALL.map(ReleaseStage::label);
        assert_eq!(
            labels,
            [
                "validate",
                "pre-gates",
                "prepare",
                "certify",
                "publish",
                "verify",
                "closeout"
            ]
        );
        assert_eq!(ReleaseStage::Publish.next(), Some(ReleaseStage::Verify));
        assert_eq!(ReleaseStage::Closeout.next(), None);
    }

    #[test]
    fn every_stage_label_round_trips() {
        for stage in ReleaseStage::ALL {
            assert_eq!(ReleaseStage::parse(stage.label()), Some(stage));
        }
        assert_eq!(ReleaseStage::parse("simulate"), None);
    }

    #[test]
    fn record_round_trips_and_denies_unknown_fields() {
        let record = ReleaseRecord::new("run-1", &PathBuf::from("/repo"), "2026-10-22T00:00:00Z");
        let encoded = serde_json::to_string(&record).expect("encode");
        let decoded: ReleaseRecord = serde_json::from_str(&encoded).expect("decode");
        assert_eq!(decoded, record);

        let mut value: serde_json::Value = serde_json::from_str(&encoded).expect("value");
        value["unexpected"] = serde_json::json!(true);
        assert!(serde_json::from_value::<ReleaseRecord>(value).is_err());
    }

    #[test]
    fn a_digest_identity_with_several_files_has_one_stable_digest() {
        let artifact = RecordArtifact {
            name: "site".into(),
            task: "build".into(),
            pattern: "dist/".into(),
            files: vec![
                RecordArtifactFile {
                    path: "/candidate/site/a".into(),
                    sha256: "aa".into(),
                    bytes: 1,
                },
                RecordArtifactFile {
                    path: "/candidate/site/b".into(),
                    sha256: "bb".into(),
                    bytes: 2,
                },
            ],
        };
        assert_eq!(
            artifact.identity_digest().as_deref(),
            Some(crate::run::artifacts::sha256_bytes(b"aa\nbb\n").as_str())
        );
    }
}
