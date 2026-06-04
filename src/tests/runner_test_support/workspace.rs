use super::{fs, write_manifest_shared, Path, PathBuf, PermissionsExt};

pub(crate) fn write_manifest(path: &Path, body: &str) {
    write_manifest_shared(path, body);
}

pub(crate) fn write_root_manifest(root: &Path, body: &str) {
    let manifest = if body.contains("[catalog]") {
        body.to_owned()
    } else if body.is_empty() {
        "[catalog]\nalias = \"root\"\n".to_owned()
    } else {
        format!("[catalog]\nalias = \"root\"\n\n{body}")
    };
    write_manifest(&root.join("effigy.toml"), &manifest);
}

pub(crate) fn create_workspace_dir(root: &Path, name: &str) -> PathBuf {
    let dir = root.join(name);
    fs::create_dir_all(&dir).expect("mkdir workspace dir");
    dir
}

pub(crate) fn write_catalog_tasks(dir: &Path, alias: Option<&str>, tasks: &[(&str, &str)]) {
    let mut manifest = String::new();
    if let Some(alias) = alias {
        manifest.push_str(&format!("[catalog]\nalias = \"{alias}\"\n"));
    }
    for (task, run) in tasks {
        manifest.push_str(&format!("[tasks.{task}]\nrun = \"{run}\"\n"));
    }
    write_manifest(&dir.join("effigy.toml"), &manifest);
}

pub(crate) fn write_executable(path: &Path, script: &str) {
    fs::write(path, script).expect("write executable");
    let mut perms = fs::metadata(path).expect("stat").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).expect("chmod");
}

pub(crate) fn write_package_json_with_test_script(root: &Path) {
    fs::write(
        root.join("package.json"),
        "{ \"scripts\": { \"test\": \"vitest\" } }\n",
    )
    .expect("write package");
}

pub(crate) fn write_package_json_with_vitest_dev_dependency(root: &Path) {
    fs::write(
        root.join("package.json"),
        r#"{
  "devDependencies": {
    "vitest": "^2.0.0"
  }
}"#,
    )
    .expect("write package");
}

pub(crate) fn write_multi_suite_cargo_manifest(root: &Path) {
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"multi\"\nversion = \"0.1.0\"\n",
    )
    .expect("write cargo toml");
}

pub(crate) fn setup_multi_suite_repo(root: &Path) {
    write_package_json_with_test_script(root);
    write_multi_suite_cargo_manifest(root);
}

pub(crate) fn write_test_suites_manifest(root: &Path, suites: &[(&str, &str)]) {
    let mut manifest = "[test.suites]\n".to_owned();
    for (suite, cmd) in suites {
        manifest.push_str(&format!("{suite} = \"{cmd}\"\n"));
    }
    write_root_manifest(root, &manifest);
}

pub(crate) fn install_local_vitest(root: &Path, script: &str) {
    let local_bin = root.join("node_modules/.bin");
    fs::create_dir_all(&local_bin).expect("mkdir local bin");
    write_executable(&local_bin.join("vitest"), script);
}

pub(crate) fn install_local_vitest_marker(root: &Path, marker: &Path) {
    install_local_vitest(
        root,
        &format!(
            "#!/bin/sh\nprintf called > \"{}\"\nexit 0\n",
            marker.display()
        ),
    );
}

pub(crate) fn write_js_package_manager_manifest(root: &Path, package_manager: &str) {
    write_root_manifest(
        root,
        &format!(
            r#"[package_manager]
js = "{package_manager}"
"#
        ),
    );
}
