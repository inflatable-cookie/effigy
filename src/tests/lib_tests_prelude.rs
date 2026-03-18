pub(super) use crate::ui::PlainRenderer;
pub(super) use crate::{
    apply_global_json_flag, command_requests_json, parse_command, render_cli_header, render_help,
    strip_global_json_flag, strip_global_json_flags, BootstrapArgs, Command, ContractsArgs,
    ContractsCheckMode, ContractsSelectionPrintMode, ContractsSubcommand, DistributionArgs,
    DistributionSubcommand, DocsArgs, DocsBlockRequirement, DocsSubcommand, DoctorArgs, HelpTopic,
    ReleaseArgs, ReleaseSubcommand, TaskInvocation, TasksArgs,
};
pub(super) use std::path::PathBuf;

pub(crate) fn render_help_text(topic: HelpTopic) -> String {
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), false);
    render_help(&mut renderer, topic).expect("help render");
    String::from_utf8(renderer.into_inner()).expect("utf8")
}

pub(crate) fn render_cli_header_text(root: &str) -> String {
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), false);
    render_cli_header(&mut renderer, PathBuf::from(root).as_path()).expect("header");
    String::from_utf8(renderer.into_inner()).expect("utf8")
}
