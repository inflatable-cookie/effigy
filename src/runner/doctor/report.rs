#[path = "report/snapshot.rs"]
mod snapshot;
#[path = "report/state.rs"]
mod state;

pub(in crate::runner) use effigy_doctor::{
    DoctorFinding, DoctorFixAction, DoctorFixStatus, DoctorReport, DoctorSeverity, DoctorSummary,
};
pub(in crate::runner) use snapshot::ManifestSnapshot;
pub(in crate::runner) use state::DoctorState;
