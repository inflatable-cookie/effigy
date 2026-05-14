use super::{
    detect_cargo_version_path, detect_version_file_kind, resolve_version_field_path,
    VersionFileKind,
};

pub fn assert_supported_version_file_kinds() {
    assert_eq!(
        detect_version_file_kind(std::path::Path::new("Cargo.toml")),
        Some(VersionFileKind::CargoToml)
    );
    assert_eq!(
        detect_version_file_kind(std::path::Path::new("package.json")),
        Some(VersionFileKind::PackageJson)
    );
    assert_eq!(
        detect_version_file_kind(std::path::Path::new("pyproject.toml")),
        Some(VersionFileKind::PyProjectToml)
    );
    assert_eq!(
        detect_version_file_kind(std::path::Path::new("VERSION")),
        Some(VersionFileKind::PlainText)
    );
}

pub fn assert_default_version_field_paths() {
    assert_eq!(
        resolve_version_field_path(VersionFileKind::CargoToml, None).expect("default path"),
        Some("package.version".to_owned())
    );
    assert_eq!(
        resolve_version_field_path(VersionFileKind::PackageJson, None).expect("default path"),
        Some("version".to_owned())
    );
    assert_eq!(
        resolve_version_field_path(VersionFileKind::PyProjectToml, None).expect("default path"),
        None
    );
}

pub fn assert_workspace_inherited_cargo_version_path() {
    let direct: toml::Value =
        toml::from_str("[package]\nversion = \"0.2.4\"\n").expect("direct cargo");
    assert_eq!(detect_cargo_version_path(&direct), Some("package.version"));

    let inherited: toml::Value = toml::from_str(
        "[workspace.package]\nversion = \"0.2.4\"\n\n[package]\nversion.workspace = true\n",
    )
    .expect("inherited cargo");
    assert_eq!(
        detect_cargo_version_path(&inherited),
        Some("workspace.package.version")
    );
}
