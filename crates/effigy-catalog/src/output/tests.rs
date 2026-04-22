use super::*;

#[test]
fn simple_checksum_is_deterministic() {
    let a = simple_checksum("hello world");
    let b = simple_checksum("hello world");
    assert_eq!(a, b);
}

#[test]
fn simple_checksum_differs_for_different_input() {
    let a = simple_checksum("hello");
    let b = simple_checksum("world");
    assert_ne!(a, b);
}

#[test]
fn write_and_cache_hit() {
    let dir = tempfile::tempdir().unwrap();
    let output = ComposeOutput::new(dir.path().to_path_buf());

    let result = crate::assembly::AssemblyResult {
        compose_yaml: "services:\n  app:\n    image: test\n".to_string(),
        dockerfiles: HashMap::new(),
        config_files: HashMap::new(),
        volumes: Vec::new(),
    };

    // First write — should regenerate.
    let write1 = output.write(&result, "manifest-v1").unwrap();
    assert!(write1.regenerated);
    assert!(write1.compose_path.exists());

    // Second write with same manifest — should be cached.
    let write2 = output.write(&result, "manifest-v1").unwrap();
    assert!(!write2.regenerated);

    // Third write with changed manifest — should regenerate.
    let write3 = output.write(&result, "manifest-v2").unwrap();
    assert!(write3.regenerated);
}

#[test]
fn write_regenerates_when_rendered_compose_changes_under_same_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let output = ComposeOutput::new(dir.path().to_path_buf());

    let first = crate::assembly::AssemblyResult {
        compose_yaml: "services:\n  app:\n    image: test:v1\n".to_string(),
        dockerfiles: HashMap::new(),
        config_files: HashMap::new(),
        volumes: Vec::new(),
    };
    let second = crate::assembly::AssemblyResult {
        compose_yaml: "services:\n  app:\n    image: test:v2\n".to_string(),
        dockerfiles: HashMap::new(),
        config_files: HashMap::new(),
        volumes: Vec::new(),
    };

    let write1 = output.write(&first, "manifest-v1").unwrap();
    assert!(write1.regenerated);

    let write2 = output.write(&second, "manifest-v1").unwrap();
    assert!(write2.regenerated);

    let stored = std::fs::read_to_string(output.generated_compose_path()).unwrap();
    assert_eq!(stored, second.compose_yaml);
}

#[test]
fn write_creates_dockerfiles_and_configs() {
    let dir = tempfile::tempdir().unwrap();
    let output = ComposeOutput::new(dir.path().to_path_buf());

    let mut dockerfiles = HashMap::new();
    dockerfiles.insert("app".to_string(), "FROM php:8.3-fpm".to_string());

    let mut config_files = HashMap::new();
    config_files.insert("web.conf".to_string(), "server { }".to_string());

    let result = crate::assembly::AssemblyResult {
        compose_yaml: "services:\n  app:\n    image: test\n".to_string(),
        dockerfiles,
        config_files,
        volumes: Vec::new(),
    };

    let write_result = output.write(&result, "manifest").unwrap();
    assert!(write_result.regenerated);
    assert!(write_result.dockerfile_paths.contains_key("app"));
    assert!(write_result.dockerfile_paths["app"].exists());
    assert!(write_result.config_paths.contains_key("web.conf"));
    assert!(write_result.config_paths["web.conf"].exists());
}

#[test]
fn write_regenerates_when_dockerfile_changes_under_same_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let output = ComposeOutput::new(dir.path().to_path_buf());

    let mut first_dockerfiles = HashMap::new();
    first_dockerfiles.insert("app".to_string(), "FROM php:8.3-fpm".to_string());
    let first = crate::assembly::AssemblyResult {
        compose_yaml: "services:\n  app:\n    image: test\n".to_string(),
        dockerfiles: first_dockerfiles,
        config_files: HashMap::new(),
        volumes: Vec::new(),
    };

    let mut second_dockerfiles = HashMap::new();
    second_dockerfiles.insert(
        "app".to_string(),
        "FROM php:8.3-fpm\nRUN useradd dev".to_string(),
    );
    let second = crate::assembly::AssemblyResult {
        compose_yaml: "services:\n  app:\n    image: test\n".to_string(),
        dockerfiles: second_dockerfiles,
        config_files: HashMap::new(),
        volumes: Vec::new(),
    };

    let write1 = output.write(&first, "manifest-v1").unwrap();
    assert!(write1.regenerated);

    let write2 = output.write(&second, "manifest-v1").unwrap();
    assert!(write2.regenerated);

    let stored =
        std::fs::read_to_string(dir.path().join(".effigy-catalog/app/Dockerfile")).unwrap();
    assert_eq!(stored, second.dockerfiles["app"]);
}

#[test]
fn write_regenerates_when_config_changes_under_same_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let output = ComposeOutput::new(dir.path().to_path_buf());

    let mut first_configs = HashMap::new();
    first_configs.insert("web.conf".to_string(), "server { return 200; }".to_string());
    let first = crate::assembly::AssemblyResult {
        compose_yaml: "services:\n  web:\n    image: nginx\n".to_string(),
        dockerfiles: HashMap::new(),
        config_files: first_configs,
        volumes: Vec::new(),
    };

    let mut second_configs = HashMap::new();
    second_configs.insert("web.conf".to_string(), "server { return 204; }".to_string());
    let second = crate::assembly::AssemblyResult {
        compose_yaml: "services:\n  web:\n    image: nginx\n".to_string(),
        dockerfiles: HashMap::new(),
        config_files: second_configs,
        volumes: Vec::new(),
    };

    let write1 = output.write(&first, "manifest-v1").unwrap();
    assert!(write1.regenerated);

    let write2 = output.write(&second, "manifest-v1").unwrap();
    assert!(write2.regenerated);

    let stored = std::fs::read_to_string(dir.path().join("web.conf")).unwrap();
    assert_eq!(stored, second.config_files["web.conf"]);
}

#[test]
fn eject_copies_and_cleans_up() {
    let dir = tempfile::tempdir().unwrap();
    let output = ComposeOutput::new(dir.path().to_path_buf());

    let mut dockerfiles = HashMap::new();
    dockerfiles.insert("app".to_string(), "FROM php:8.3-fpm".to_string());

    let result = crate::assembly::AssemblyResult {
        compose_yaml: "services:\n  app:\n    image: test\n".to_string(),
        dockerfiles,
        config_files: HashMap::new(),
        volumes: Vec::new(),
    };

    // Write first.
    output.write(&result, "manifest").unwrap();

    // Eject.
    let eject_result = output.eject().unwrap();
    assert!(eject_result.compose_path.exists());
    assert_eq!(
        eject_result
            .compose_path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap(),
        "docker-compose.yml"
    );

    // Generated files should be cleaned up.
    assert!(!output.generated_compose_path().exists());
    assert!(!dir.path().join(".effigy-compose.checksum").exists());
    assert!(!dir.path().join(".effigy-catalog").exists());

    // Permanent Dockerfile should exist.
    assert!(eject_result.dockerfile_paths.contains_key("app"));
}

#[test]
fn eject_without_generated_file_fails() {
    let dir = tempfile::tempdir().unwrap();
    let output = ComposeOutput::new(dir.path().to_path_buf());
    let result = output.eject();
    assert!(result.is_err());
}

#[test]
fn eject_to_promotes_into_explicit_target_dir() {
    let dir = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let output = ComposeOutput::new(dir.path().to_path_buf());

    let mut dockerfiles = HashMap::new();
    dockerfiles.insert("app".to_string(), "FROM php:8.3-fpm".to_string());

    let mut config_files = HashMap::new();
    config_files.insert("web.conf".to_string(), "server { }".to_string());

    let result = crate::assembly::AssemblyResult {
        compose_yaml: "services:\n  app:\n    image: test\n".to_string(),
        dockerfiles,
        config_files,
        volumes: Vec::new(),
    };

    output.write(&result, "manifest").unwrap();
    let eject_result = output.eject_to(target.path()).unwrap();

    assert_eq!(
        eject_result.compose_path,
        target.path().join("docker-compose.yml")
    );
    assert!(target.path().join("catalog/app/Dockerfile").exists());
    assert!(target.path().join("web.conf").exists());
    assert!(!output.generated_compose_path().exists());
}

#[test]
fn compose_file_args_without_override() {
    let dir = tempfile::tempdir().unwrap();
    let output = ComposeOutput::new(dir.path().to_path_buf());
    let args = output.compose_file_args();
    assert_eq!(args.len(), 2);
    assert_eq!(args[0], "-f");
}

#[test]
fn compose_file_args_with_override() {
    let dir = tempfile::tempdir().unwrap();
    // Create an override file.
    std::fs::write(dir.path().join("compose.override.yml"), "# override").unwrap();
    let output = ComposeOutput::new(dir.path().to_path_buf());
    let args = output.compose_file_args();
    assert_eq!(args.len(), 4);
    assert_eq!(args[2], "-f");
}
