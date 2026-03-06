#[path = "report/finalize.rs"]
mod finalize;
#[path = "report/snapshot.rs"]
mod snapshot;
#[path = "report/state.rs"]
mod state;
#[path = "report/summary.rs"]
mod summary;
#[path = "report/types.rs"]
mod types;

pub(in crate::runner) use snapshot::ManifestSnapshot;
pub(in crate::runner) use state::DoctorState;
pub(in crate::runner) use types::{
    DoctorFinding, DoctorFixAction, DoctorFixStatus, DoctorReport, DoctorSeverity,
};
