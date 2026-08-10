use super::{fs, write_manifest_shared, Path, PathBuf, PermissionsExt};

pub(crate) fn write_manifest(path: &Path, body: &str) {
    write_manifest_shared(path, body);
    register_test_catalog_members(path);
}

fn register_test_catalog_members(manifest_path: &Path) {
    if manifest_path.file_name().and_then(|name| name.to_str()) != Some("effigy.toml") {
        return;
    }
    let Some(root) = manifest_path
        .ancestors()
        .filter(|ancestor| ancestor.join("package.json").is_file())
        .last()
        .map(Path::to_path_buf)
    else {
        return;
    };
    let root_manifest = root.join("effigy.toml");
    if !root_manifest.is_file() {
        return;
    }
    let candidates = if manifest_path == root_manifest {
        walkdir::WalkDir::new(&root)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file() && entry.file_name() == "effigy.toml")
            .map(|entry| entry.into_path())
            .collect::<Vec<_>>()
    } else {
        vec![manifest_path.to_path_buf()]
    };
    let Ok(source) = fs::read_to_string(&root_manifest) else {
        return;
    };
    let Ok(mut document) = source.parse::<toml_edit::DocumentMut>() else {
        return;
    };
    let catalog = document
        .entry("catalog")
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
    let Some(catalog) = catalog.as_table_mut() else {
        return;
    };
    let members = catalog
        .entry("members")
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
    let Some(members) = members.as_table_mut() else {
        return;
    };
    let mut changed = false;
    for candidate in candidates {
        if candidate == root_manifest {
            continue;
        }
        let Some(catalog_root) = candidate.parent() else {
            continue;
        };
        let Ok(relative) = catalog_root.strip_prefix(&root) else {
            continue;
        };
        if relative.components().any(|component| {
            matches!(
                component.as_os_str().to_str(),
                Some(".effigy" | "external" | "node_modules" | "vendor" | "target")
            )
        }) {
            continue;
        }
        let relative = relative.to_string_lossy().replace('\\', "/");
        let handle = format!(
            "fixture_{}",
            relative
                .chars()
                .map(|character| if character.is_ascii_alphanumeric() {
                    character
                } else {
                    '_'
                })
                .collect::<String>()
        );
        if !members.contains_key(&handle) {
            members.insert(&handle, toml_edit::value(relative));
            changed = true;
        }
    }
    if changed {
        fs::write(root_manifest, document.to_string()).expect("register explicit test catalogs");
    }
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
