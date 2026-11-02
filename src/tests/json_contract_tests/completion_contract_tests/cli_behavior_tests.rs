use crate::runner::json_contract_tests::prelude::{
    fs, lock_test, run_completion_task, temp_workspace, with_completion_cache_default,
    write_manifest,
};

#[test]
fn builtin_completion_candidates_text_includes_builtin_and_task_selectors() {
    let _guard = lock_test();
    let _env = with_completion_cache_default();
    let root = temp_workspace("completion-candidates-text-contract");
    let catalog_a = root.join("catalog_a");
    fs::create_dir_all(&catalog_a).expect("mkdir catalog_a");
    write_manifest(
        &root.join("effigy.toml"),
        "[catalog.members]\ncatalog_a = \"catalog_a\"\n\n[tasks.build]\nrun = \"printf root\"\n",
    );
    write_manifest(
        &catalog_a.join("effigy.toml"),
        "[catalog]\nalias = \"catalog_a\"\n[tasks.api]\nrun = \"printf api\"\n",
    );

    let out = run_completion_task(root, &["candidates"]).expect("run completion candidates");
    assert!(out.lines().any(|line| line == "help"));
    assert!(out.lines().any(|line| line == "build"));
    assert!(out.lines().any(|line| line == "catalog_a/api"));
}

// The bash-script dynamic-candidates probe line is pinned by
// help_tests' completion ok-contract table (`builtin-completion-bash`).
// The candidates `--prefix`/`--repo`/unknown-argument refusals are pinned by
// help_tests' completion argument-validation table, tasks_and_doctor_command_tests'
// argument matrix (global `--repo`), and the builtin argument contract matrix
// (`candidates --wat`) respectively — the JSON-side duplicates were removed.
