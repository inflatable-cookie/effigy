use super::*;

#[test]
fn execute_rhai_script_exposes_semver_helpers() {
    let root = temp_root("semver");
    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root,
        task_name: "semver".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    let script = r#"
        let parsed = semver::parse("v1.2.3-alpha.1+build.5");
        if parsed["major"] != 1 { throw("major"); }
        if parsed["minor"] != 2 { throw("minor"); }
        if parsed["patch"] != 3 { throw("patch"); }
        if parsed["pre"] != "alpha.1" { throw("pre"); }
        if parsed["build"] != "build.5" { throw("build"); }
        if parsed["normalized"] != "1.2.3-alpha.1+build.5" { throw("normalized"); }
        if !semver::valid("1.2.3") { throw("valid"); }
        if semver::valid("1.2") { throw("invalid"); }
        if semver::compare("1.2.3", "1.2.4") != -1 { throw("compare less"); }
        if semver::compare("1.2.3", "1.2.3") != 0 { throw("compare equal"); }
        if semver::compare("1.3.0", "1.2.9") != 1 { throw("compare greater"); }
        if !semver::satisfies("1.4.0", ">=1.2, <2.0") { throw("satisfies"); }
        if semver::bump_major("1.2.3") != "2.0.0" { throw("bump major"); }
        if semver::bump_minor("1.2.3") != "1.3.0" { throw("bump minor"); }
        if semver::bump_patch("1.2.3") != "1.2.4" { throw("bump patch"); }
    "#;

    execute_rhai_script(&context, script, &[], &callbacks()).expect("execute");
}

#[test]
fn rhai_surface_registry_lists_semver_module() {
    let surface = crate::surface::rhai_surface_json();
    assert!(surface["modules"]
        .as_array()
        .expect("modules")
        .iter()
        .any(|module| module.as_str() == Some("semver")));
    assert!(surface["functions"]
        .as_array()
        .expect("functions")
        .iter()
        .any(|function| function["module"] == "semver" && function["name"] == "satisfies"));
}
