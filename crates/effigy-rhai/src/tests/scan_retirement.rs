use super::*;

fn scan_retirement_context(name: &str) -> (PathBuf, ScriptContext) {
    let root = temp_root(name);
    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root.clone(),
        task_name: "scan-retirement".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    (root, context)
}

#[test]
fn removed_scan_functions_are_absent_from_the_registered_runtime() {
    let (_root, context) = scan_retirement_context("scan-retirement-absent");

    for call in [
        "scan::generated_assets(#{})",
        "scan::generated_in_src(#{})",
        "scan::comment_ratio(#{})",
        "scan::attention_markers(#{})",
        "scan::stale_suppressions(#{})",
    ] {
        let error = execute_rhai_script(&context, call, &[], &callbacks())
            .expect_err("removed scan call must fail honestly");
        let message = error.to_string().to_lowercase();
        assert!(
            message.contains("scan"),
            "error should name the scan module: {error}"
        );
        assert!(
            message.contains("not found"),
            "error should be an unknown-function failure, not something else: {error}"
        );
    }
}

#[test]
fn kept_scan_functions_stay_exposed() {
    let (_root, context) = scan_retirement_context("scan-retirement-kept");

    for (script, feature) in [
        ("scan::god_files(#{ threshold: 900 })", "scan.god_files"),
        ("scan::duplicate_blocks(#{})", "scan.duplicate_blocks"),
    ] {
        let script = format!(
            "let result = {script}; if result[\"feature\"] != \"{feature}\" {{ throw(\"wrong feature\"); }}"
        );
        execute_rhai_script(&context, &script, &[], &callbacks())
            .expect("kept scan function should still report its feature");
    }
}
