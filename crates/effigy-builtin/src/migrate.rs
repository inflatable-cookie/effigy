use effigy_cli::TaskInvocation;
use std::path::Path;

#[path = "migrate/io.rs"]
mod io;
#[path = "migrate/model.rs"]
mod model;
#[path = "migrate/output.rs"]
mod output;
#[path = "migrate/plan.rs"]
mod plan;
#[path = "migrate/request.rs"]
mod request;

use super::command_spec::run_builtin_command;
use super::help_text::{render_titled_help, HelpSection};
use super::render_builtin_help_text;
use crate::BuiltinError;

const CONFLICT_REASON_DESTINATION_EXISTS: &str = "destination already exists";
const BUILTIN_MIGRATE_NAME: &str = "migrate";

pub(super) fn run_builtin_migrate(
    task: &TaskInvocation,
    args: &[String],
    target_root: &Path,
) -> Result<Option<String>, BuiltinError> {
    run_builtin_command(
        args,
        |output_json| render_builtin_help_text("tasks-migrate", render_migrate_help(), output_json),
        || request::parse_migrate_request(task, args),
        |request: model::MigrateRequest| {
            let plan = plan::build_migrate_plan(&request, target_root)?;
            output::render_migrate_output(&plan, request.output_json)
        },
    )
}

fn render_migrate_help() -> String {
    render_titled_help(
        "tasks migrate",
        &[
            HelpSection::Plain {
                heading: "Usage",
                lines: &["effigy tasks migrate [--from <PATH>] [--script <NAME>]... [--apply] [--json]"],
            },
            HelpSection::Bulleted {
                heading: "Notes",
                items: &[
                    "import `package.json` scripts with preview-first, explicit apply flow; `test` maps to `[test.suites].js` and other scripts map to `[tasks]`",
                    "package.json remains unchanged; existing destination conflicts require manual remediation",
                ],
            },
            HelpSection::Bulleted {
                heading: "Examples",
                items: &[
                    "effigy tasks migrate",
                    "effigy tasks migrate --script build --script test",
                    "effigy tasks migrate --apply",
                    "effigy tasks migrate --json",
                ],
            },
        ],
    )
}
