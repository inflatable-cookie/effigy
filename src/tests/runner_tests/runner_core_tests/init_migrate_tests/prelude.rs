pub(super) use super::super::prelude::*;

pub(super) fn write_package_json_scripts(root: &Path, scripts: &[(&str, &str)]) {
    let entries = scripts
        .iter()
        .map(|(name, command)| format!("    \"{name}\": \"{command}\""))
        .collect::<Vec<_>>()
        .join(",\n");
    let package_json = format!("{{\n  \"scripts\": {{\n{entries}\n  }}\n}}\n");
    fs::write(root.join("package.json"), package_json).expect("write package scripts");
}
