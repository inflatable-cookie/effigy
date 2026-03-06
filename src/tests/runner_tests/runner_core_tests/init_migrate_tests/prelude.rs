pub(super) use super::super::prelude::{
    assert_builtin_argument_contract_case_table, assert_file_text_contains_all,
    assert_file_text_equals, assert_file_text_excludes_all, assert_output_contains_all,
    assert_path_exists, assert_path_missing, assert_task_invocation_error_contains, fs,
    run_builtin_err, run_builtin_ok, run_tasks, temp_workspace, write_root_manifest,
    BuiltinArgumentContractCase, Path, TasksArgs,
};

pub(super) fn write_package_json_scripts(root: &Path, scripts: &[(&str, &str)]) {
    let entries = scripts
        .iter()
        .map(|(name, command)| format!("    \"{name}\": \"{command}\""))
        .collect::<Vec<_>>()
        .join(",\n");
    let package_json = format!("{{\n  \"scripts\": {{\n{entries}\n  }}\n}}\n");
    fs::write(root.join("package.json"), package_json).expect("write package scripts");
}
