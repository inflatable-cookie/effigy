mod commands;
mod model;
mod parser;
mod values;

pub(super) use model::{ScanCommand, ScanRequest};

pub(super) fn scan_candidate_mode(args: &[String]) -> Option<ScanCommand> {
    commands::scan_candidate_mode(args)
}

pub(super) fn parse_scan_request(
    task: &crate::TaskInvocation,
    args: &[String],
) -> Result<ScanRequest, crate::runner::error::RunnerError> {
    parser::parse_scan_request(task, args)
}
