//! Doctor policy contracts, report model, and runner orchestration
//! for Effigy.
//!
//! Card 250 established the extraction-pattern playbook: narrow
//! `*Error` boundary, types move with the domain, runtime reach-ins
//! behind a small port trait. Card 254 grew this crate from a
//! pure-library surface (reports, findings, projections, contracts)
//! into the full doctor domain crate — workflow, checks, render,
//! explain, health, manifest scan, and the doctor command entry
//! point all live here.

mod contracts;
mod error;
mod finding;
pub mod manifest_schema;
mod ports;
mod projection;
mod report;
pub mod task_references;

// Orchestration tree — moved from `src/runner/doctor/**` under card
// 254.
mod attention_markers;
mod checks;
mod command;
mod comment_ratio;
mod conflicts;
mod duplicate_blocks;
mod environment;
mod explain;
mod finding_templates;
mod generated_assets;
mod generated_in_src;
mod god_files;
mod health;
mod manifest;
mod manifest_snapshot;
mod progress;
mod references;
mod render;
mod render_support;
mod scan_checks;
mod stale_suppressions;
mod task_graph;
mod text_blocks;
mod util;
mod workflow;

pub use command::run_doctor;
pub use contracts::{check_id, install_tool, remediation, schema_supported_value, ALL_CHECK_IDS};
pub use error::DoctorError;
pub use finding::{DoctorFinding, DoctorSeverity, FindingSink};
pub use ports::{DoctorRuntimeDiagnostics, DoctorRuntimePorts};
pub use projection::{
    doctor_finding_sections, doctor_fixes_table_rows, group_max_severity, grouped_findings,
    summarize_group, DoctorFindingSection, DoctorSectionFinding,
};
pub use report::{
    finalize_fix_actions, DoctorFixAction, DoctorFixStatus, DoctorReport, DoctorState,
    DoctorSummary,
};
