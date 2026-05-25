use super::*;

#[test]
fn execute_rhai_script_rejects_prompt_helpers_without_interactive_tty() {
    let root = temp_root("prompt-non-interactive");
    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root,
        task_name: "prompt".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };

    let error = execute_rhai_script(
        &context,
        r#"prompt::confirm("Continue?", true);"#,
        &[],
        &callbacks(),
    )
    .expect_err("prompt should require tty");
    assert!(
        error
            .to_string()
            .contains("prompt helpers require interactive stdin and stdout"),
        "{error}"
    );
}

#[test]
fn rhai_surface_registry_lists_prompt_module() {
    let surface = crate::surface::rhai_surface_json();
    assert!(surface["modules"]
        .as_array()
        .expect("modules")
        .iter()
        .any(|module| module.as_str() == Some("prompt")));
    assert!(surface["functions"]
        .as_array()
        .expect("functions")
        .iter()
        .any(|function| function["module"] == "prompt" && function["name"] == "confirm"));
}
