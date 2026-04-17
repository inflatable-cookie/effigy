use super::*;

fn mapper() -> CwdMapper {
    CwdMapper::new(
        PathBuf::from("/Users/tom/projects/client"),
        PathBuf::from("/var/www/html"),
    )
}

#[test]
fn map_repo_root_to_container_root() {
    let m = mapper();
    let result = m
        .host_to_container(Path::new("/Users/tom/projects/client"))
        .unwrap();
    assert_eq!(result, PathBuf::from("/var/www/html"));
}

#[test]
fn map_subdirectory() {
    let m = mapper();
    let result = m
        .host_to_container(Path::new("/Users/tom/projects/client/app/Models"))
        .unwrap();
    assert_eq!(result, PathBuf::from("/var/www/html/app/Models"));
}

#[test]
fn map_deep_subdirectory() {
    let m = mapper();
    let result = m
        .host_to_container(Path::new(
            "/Users/tom/projects/client/app/Http/Controllers/Api",
        ))
        .unwrap();
    assert_eq!(
        result,
        PathBuf::from("/var/www/html/app/Http/Controllers/Api")
    );
}

#[test]
fn reject_path_outside_repo() {
    let m = mapper();
    let result = m.host_to_container(Path::new("/Users/tom/other-project"));
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        ExecError::CwdOutsideRepo { .. }
    ));
}

#[test]
fn reject_parent_directory_of_repo() {
    let m = mapper();
    let result = m.host_to_container(Path::new("/Users/tom/projects"));
    assert!(result.is_err());
}

#[test]
fn container_to_host_roundtrip() {
    let m = mapper();
    let container_path = Path::new("/var/www/html/app/Models");
    let host_path = m.container_to_host(container_path).unwrap();
    assert_eq!(
        host_path,
        PathBuf::from("/Users/tom/projects/client/app/Models")
    );
}

#[test]
fn container_to_host_root() {
    let m = mapper();
    let host = m.container_to_host(Path::new("/var/www/html")).unwrap();
    assert_eq!(host, PathBuf::from("/Users/tom/projects/client"));
}

#[test]
fn container_to_host_outside_working_dir() {
    let m = mapper();
    let result = m.container_to_host(Path::new("/tmp/something"));
    assert!(result.is_err());
}

#[test]
fn works_with_real_filesystem_paths() {
    let dir = tempfile::tempdir().unwrap();
    let subdir = dir.path().join("src/app");
    std::fs::create_dir_all(&subdir).unwrap();

    let m = CwdMapper::new(dir.path().to_path_buf(), PathBuf::from("/workspace"));

    let result = m.host_to_container(&subdir).unwrap();
    assert_eq!(result, PathBuf::from("/workspace/src/app"));
}
