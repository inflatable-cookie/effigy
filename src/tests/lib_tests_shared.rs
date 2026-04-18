pub(super) use crate::render_cli_header;
pub(super) use effigy_cli::help::ui::render_help;
pub(super) use effigy_cli::{
    apply_global_json_flag, command_requests_json, parse_command, strip_global_json_flag,
    strip_global_json_flags, BootstrapArgs, Command, ContainerArgs, ContainerSubcommand,
    ContractsArgs, ContractsCheckMode, ContractsSelectionPrintMode, ContractsSubcommand, DemoArgs,
    DemoListGap, DemoListGroupBy, DemoListMode, DemoListQuery, DemoListStatus, DemoSubcommand,
    DistributionArgs, DistributionSubcommand, DocsArgs, DocsBlockRequirement, DocsSubcommand,
    DoctorArgs, ExecArgs, GatewayArgs, GatewaySubcommand, HelpTopic, ReleaseArgs,
    ReleaseSubcommand, ServiceArgs, ServiceSubcommand, TaskInvocation, TasksArgs,
};
pub(super) use effigy_ui::PlainRenderer;
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
