//! The durable release record: one checkpoint after every stage boundary.
//!
//! Contract 054 §3 keeps one Run's progress as a checkpoint at each stage
//! boundary. The checkout journal holds the checkpoint pointer (Q-010 and
//! 054 §10 ruling 2); this record holds the facts a resume needs:
//! the candidate commit, each identity's version, each artifact's digest and
//! each destination already published. Every field is restored from here, so a
//! resumed Run never re-derives a version or rebuilds an artifact.

use std::path::Path;

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
    /// The last invocation stopped waiting for hosted evidence for one gate.
    /// The Run holds no process open; a resume re-checks the evidence.
    AwaitingHostedEvidence,
    /// The last invocation stopped because a target's publish outcome is
    /// unknown and it declares no `check` task. The operator records whether
    /// the candidate is published before the Run continues (054 §7).
    AwaitingDecision,
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
            Self::AwaitingHostedEvidence => "awaiting-hosted-evidence",
            Self::AwaitingDecision => "awaiting-decision",
            Self::Failed => "failed",
            Self::Complete => "complete",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [
            Self::Paused,
            Self::AwaitingApproval,
            Self::AwaitingHostedEvidence,
            Self::AwaitingDecision,
            Self::Failed,
            Self::Complete,
        ]
        .into_iter()
        .find(|status| status.label() == value.trim())
    }
}

/// What an operator records for a target whose publish outcome is unknown
/// (contract 054 §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetDecision {
    Published,
    NotPublished,
}

impl TargetDecision {
    pub fn label(self) -> &'static str {
        match self {
            Self::Published => "published",
            Self::NotPublished => "not-published",
        }
    }
}

/// One operator decision about an unknown publish outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordDecision {
    pub target: String,
    /// `published` or `not-published`.
    pub decision: String,
    pub decided_by: String,
    pub decided_at: String,
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
    /// The hosted GitHub Actions run a hosted wait reused, when this gate was
    /// satisfied by exact-commit evidence instead of running its command.
    #[serde(default)]
    pub hosted_run: Option<String>,
    /// The bound run's GitHub id, so the receipt names the one run this release
    /// dispatched rather than whichever run was newest.
    #[serde(default)]
    pub hosted_run_id: Option<u64>,
}

/// One hosted-evidence wait: the dispatch window a release Run opened and the
/// one run it bound (contract 054 §3).
///
/// The window is what stops a wait from taking a sibling run. A run created
/// before it belongs to an earlier dispatch — another release, an operator's
/// re-dispatch — and is never this Run's evidence. Once bound, the run id is
/// what a resume re-checks: the release never re-binds to something newer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordHostedWait {
    pub gate: String,
    pub workflow: String,
    /// The commit the evidence must be for.
    pub commit: String,
    /// When this Run opened its dispatch window (RFC 3339, UTC).
    pub created_after: String,
    /// The dispatched run this Run bound, once it has one.
    #[serde(default)]
    pub run_id: Option<u64>,
    #[serde(default)]
    pub run_url: Option<String>,
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
    /// The identity whose tag this destination is. A `task` target reports
    /// destination IDs without naming an identity (contract 054 §6).
    #[serde(default)]
    pub identity: Option<String>,
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
    /// Hosted-evidence waits: the dispatch window each opened and the run it
    /// bound (contract 054 §3).
    #[serde(default)]
    pub hosted_waits: Vec<RecordHostedWait>,
    pub targets: Vec<RecordTarget>,
    pub verifications: Vec<RecordVerification>,
    pub approvals: Vec<RecordApproval>,
    /// Operator rulings on unknown publish outcomes (contract 054 §7).
    #[serde(default)]
    pub decisions: Vec<RecordDecision>,
    pub checkpoints: Vec<RecordCheckpoint>,
    pub pushed_commit: bool,
    pub errors: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl ReleaseRecord {
    pub fn new(release_id: &str, root: &Path, now: &str) -> Self {
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
            hosted_waits: Vec::new(),
            targets: Vec::new(),
            verifications: Vec::new(),
            approvals: Vec::new(),
            decisions: Vec::new(),
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

    /// A gate that already passed in this stage, so a resumed gate stage never
    /// re-runs it (a hosted wait resumes in the middle of one).
    pub fn gate_passed(&self, stage: &str, name: &str) -> bool {
        self.gates
            .iter()
            .any(|gate| gate.stage == stage && gate.name == name && gate.passed)
    }

    pub fn identity(&self, name: &str) -> Option<&RecordIdentity> {
        self.identities
            .iter()
            .find(|identity| identity.name == name)
    }

    pub fn target(&self, name: &str) -> Option<&RecordTarget> {
        self.targets.iter().find(|target| target.name == name)
    }

    pub fn target_mut(&mut self, name: &str) -> Option<&mut RecordTarget> {
        self.targets.iter_mut().find(|target| target.name == name)
    }

    /// The hosted wait this gate already opened, if any.
    pub fn hosted_wait(&self, gate: &str) -> Option<&RecordHostedWait> {
        self.hosted_waits.iter().find(|wait| wait.gate == gate)
    }

    /// Open a dispatch window for a gate, unless it already has one for this
    /// commit.
    pub fn open_hosted_wait(
        &mut self,
        gate: &str,
        workflow: &str,
        commit: &str,
        created_after: &str,
    ) -> &RecordHostedWait {
        let fresh = || RecordHostedWait {
            gate: gate.to_owned(),
            workflow: workflow.to_owned(),
            commit: commit.to_owned(),
            created_after: created_after.to_owned(),
            run_id: None,
            run_url: None,
        };
        match self.hosted_waits.iter().position(|wait| wait.gate == gate) {
            Some(index) if self.hosted_waits[index].commit == commit => &self.hosted_waits[index],
            Some(index) => {
                // The gate now asks about another commit, so its old window
                // and binding describe a different dispatch.
                self.hosted_waits[index] = fresh();
                &self.hosted_waits[index]
            }
            None => {
                self.hosted_waits.push(fresh());
                self.hosted_waits
                    .last()
                    .expect("a hosted wait was just pushed")
            }
        }
    }

    /// Record the one run this Run bound, so a resume re-checks it.
    pub fn bind_hosted_run(&mut self, gate: &str, run_id: Option<u64>, run_url: &str) {
        if let Some(wait) = self.hosted_waits.iter_mut().find(|wait| wait.gate == gate) {
            wait.run_id = run_id;
            wait.run_url = Some(run_url.to_owned());
        }
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
        ReleaseRunStatus::parse(&self.status)
    }

    pub fn stage(&self) -> Option<ReleaseStage> {
        ReleaseStage::parse(&self.stage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hosted_wait_journals_its_window_and_bound_run() {
        let mut record = ReleaseRecord::new("run-1", Path::new("/repo"), "2026-10-22T00:00:00Z");
        let opened = record.open_hosted_wait("ci", "ci.yml", "abc123", "2026-10-22T00:10:00Z");
        assert_eq!(opened.created_after, "2026-10-22T00:10:00Z");
        assert_eq!(opened.run_id, None);

        // Reopening keeps the window this Run opened: a resume re-checks the
        // same dispatch, never a newer one.
        let again = record.open_hosted_wait("ci", "ci.yml", "abc123", "2026-10-22T00:20:00Z");
        assert_eq!(again.created_after, "2026-10-22T00:10:00Z");

        record.bind_hosted_run("ci", Some(7), "https://example.test/runs/7");
        assert_eq!(record.hosted_wait("ci").expect("wait").run_id, Some(7));

        // Another commit is another dispatch, so its window opens fresh.
        let moved = record.open_hosted_wait("ci", "ci.yml", "def456", "2026-10-22T00:30:00Z");
        assert_eq!(moved.created_after, "2026-10-22T00:30:00Z");
        assert_eq!(moved.run_id, None);

        let encoded = serde_json::to_string(&record).expect("encode");
        let decoded: ReleaseRecord = serde_json::from_str(&encoded).expect("decode");
        assert_eq!(decoded, record);
    }

    #[test]
    fn a_record_without_hosted_waits_still_decodes() {
        let record = ReleaseRecord::new("run-1", Path::new("/repo"), "2026-10-22T00:00:00Z");
        let mut value: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&record).expect("encode")).expect("value");
        value
            .as_object_mut()
            .expect("record object")
            .remove("hosted_waits");
        let decoded: ReleaseRecord = serde_json::from_value(value).expect("decode");
        assert!(decoded.hosted_waits.is_empty());
    }

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
        let record = ReleaseRecord::new("run-1", Path::new("/repo"), "2026-10-22T00:00:00Z");
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
