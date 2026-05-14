mod apply;
mod capture;
mod history;
mod lineage;
mod model;
mod paths;
#[cfg(test)]
mod tests;
mod validation;

pub use apply::{
    StateApplyHookContext, StateApplyHookLayerContext, StateStackApplyHookStatus,
    StateStackApplyLayerReport, StateStackApplyLayerStatus, StateStackApplyReport,
};
pub use capture::{
    capture_produced_layer, parse_capture_role, StateCaptureArtifactOperation, StateCaptureMode,
    StateCapturePlanRequest, StateCapturePlanningError, StateCaptureSetEntry,
    StateCaptureSetReport, StateCaptureTaskContext, StateStackCaptureArtifact,
    StateStackCaptureProducedLayer, StateStackCaptureReport, StateStackCaptureTask,
    StateStackCaptureTaskStatus,
};
pub use history::{StateHistoryKind, StateStackHistoryItem, StateStackHistoryReport};
pub use lineage::{
    StateStackArtifactOperation, StateStackArtifactReportRef, StateStackLineageLayer,
    StateStackLineagePlan, StateStackLineageReport,
};
pub use model::{
    plain_state_environment, plain_state_layer_apply_mode, plain_state_layer_role,
    StateEnvironment, StateLayerApplyMode, StateLayerEnvironmentPolicy, StateLayerRole,
    StateStackLayer, StateStackManifest, STATE_STACK_APPLY_CONTEXT_SCHEMA,
    STATE_STACK_APPLY_SCHEMA, STATE_STACK_CAPTURE_CONTEXT_SCHEMA, STATE_STACK_CAPTURE_SCHEMA,
    STATE_STACK_CAPTURE_SET_SCHEMA, STATE_STACK_HISTORY_SCHEMA, STATE_STACK_LINEAGE_SCHEMA,
    STATE_STACK_SCHEMA,
};
pub use paths::{
    build_state_apply_hook_context, build_state_capture_task_context, path_display,
    safe_path_component, state_capture_set_report_write_paths, state_report_write_paths,
    StateContextFile, StateReportWritePaths,
};
pub use validation::{validate_state_stack, StateStackParseError, StateStackValidationError};
