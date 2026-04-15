mod contracts;
mod finding;
pub mod manifest_schema;
mod projection;
mod report;
pub mod task_references;

pub use contracts::{check_id, install_tool, remediation, schema_supported_value, ALL_CHECK_IDS};
pub use finding::{DoctorFinding, DoctorSeverity, FindingSink};
pub use projection::{
    doctor_finding_sections, doctor_fixes_table_rows, group_max_severity, grouped_findings,
    summarize_group, DoctorFindingSection, DoctorSectionFinding,
};
pub use report::{
    finalize_fix_actions, DoctorFixAction, DoctorFixStatus, DoctorReport, DoctorState,
    DoctorSummary,
};
