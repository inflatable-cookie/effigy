use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::TaskInvocation;

#[path = "migrate/args.rs"]
mod args;
#[path = "migrate/io.rs"]
mod io;
#[path = "migrate/output.rs"]
mod output;

use super::super::{RunnerError, TASK_MANIFEST_FILE};

#[derive(Debug, Clone)]
struct MigrateScript {
    name: String,
    command: String,
}

struct MigrateArgs {
    output_json: bool,
    help: bool,
    apply: bool,
    package_path: Option<PathBuf>,
    script_filter: BTreeSet<String>,
}

struct MigratePlan {
    package_path: PathBuf,
    manifest_path: PathBuf,
    apply: bool,
    added: Vec<MigrateScript>,
    conflicts: Vec<MigrateScript>,
    written: bool,
}

const CONFLICT_REASON_TASK_EXISTS: &str = "task already exists";
const BUILTIN_MIGRATE_NAME: &str = "migrate";

pub(super) fn run_builtin_migrate(
    task: &TaskInvocation,
    args: &[String],
    target_root: &Path,
) -> Result<Option<String>, RunnerError> {
    let parsed = args::parse_migrate_args(task, args)?;

    if parsed.help {
        return output::render_migrate_help(parsed.output_json);
    }

    let plan = build_migrate_plan(&parsed, target_root)?;
    output::render_migrate_output(&plan, parsed.output_json)
}

fn build_migrate_plan(
    parsed: &MigrateArgs,
    target_root: &Path,
) -> Result<MigratePlan, RunnerError> {
    let package = io::resolve_package_path(target_root, parsed.package_path.clone());
    if !package.exists() {
        return Err(RunnerError::TaskInvocation(format!(
            "migration source not found: {}",
            package.display()
        )));
    }

    let selected = io::select_scripts(io::load_package_scripts(&package)?, &parsed.script_filter);

    let manifest_path = target_root.join(TASK_MANIFEST_FILE);
    let (mut manifest_doc, existing_tasks) = io::load_manifest_and_existing_tasks(&manifest_path)?;
    let (added, conflicts) = io::partition_scripts(selected, &existing_tasks);
    let written =
        io::apply_migration_if_requested(parsed.apply, &added, &mut manifest_doc, &manifest_path)?;
    Ok(MigratePlan {
        package_path: package,
        manifest_path,
        apply: parsed.apply,
        added,
        conflicts,
        written,
    })
}
