pub(super) use super::super::prelude::builtin_contracts::*;
pub(super) use super::super::prelude::cases::*;
pub(super) use super::super::prelude::catalog::*;
pub(super) use super::super::prelude::errors::*;
pub(super) use super::super::prelude::fixture_support::*;
pub(super) use super::super::prelude::harness_assertions::*;
pub(super) use super::super::prelude::harness_builtin::*;
pub(super) use super::super::prelude::harness_env::*;
pub(super) use super::super::prelude::harness_workspace::*;
pub(super) use super::super::prelude::output::*;
pub(super) use super::super::prelude::parsing::*;
pub(super) use super::super::prelude::runtime::*;

pub(super) fn run_task(root: &Path, name: &str, args: &[&str]) -> Result<String, RunnerError> {
    run_task_in_workspace(root, name, args)
}

pub(super) fn write_empty_manifest(root: &Path) {
    write_root_manifest(root, "");
}

pub(super) fn assert_run_task_ok_empty(root: &Path, name: &str, args: &[&str]) {
    let out = run_task(root, name, args).expect("task should succeed");
    assert_output_equals(&out, "");
}
