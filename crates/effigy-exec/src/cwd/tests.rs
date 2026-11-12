use super::*;

fn mapper() -> CwdMapper {
    CwdMapper::new(
        PathBuf::from("/Users/tom/projects/client"),
        PathBuf::from("/var/www/html"),
    )
}

#[test]
fn map_repo_root_does_not_emit_trailing_slash() {
    // PathBuf::join("") otherwise emits "/var/www/html/" which nerdctl/runc
    // rejects via "current working directory is outside of container mount
    // namespace root" because it does not match the container's WORKDIR
    // string exactly. PathBuf equality does not catch this: a trailing-slash
    // mutation still passes PathBuf == PathBuf.
    let m = mapper();
    let result = m
        .host_to_container(Path::new("/Users/tom/projects/client"))
        .unwrap();
    assert_eq!(result.to_string_lossy(), "/var/www/html");
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
    // Same-prefix sibling is the starts_with trap: "client-evil" shares the
    // repo-root string prefix but must not map.
    let result = m.host_to_container(Path::new("/Users/tom/projects/client-evil"));
    assert!(matches!(
        result.unwrap_err(),
        ExecError::CwdOutsideRepo { .. }
    ));
}

#[test]
fn reject_parent_directory_of_repo() {
    let m = mapper();
    let result = m.host_to_container(Path::new("/Users/tom/projects"));
    assert!(matches!(
        result.unwrap_err(),
        ExecError::CwdOutsideRepo { .. }
    ));
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
fn container_to_host_root_does_not_emit_trailing_slash() {
    let m = mapper();
    let host = m.container_to_host(Path::new("/var/www/html")).unwrap();
    // PathBuf equality does not catch a trailing separator, so the root case
    // is asserted as text on both directions (see
    // map_repo_root_does_not_emit_trailing_slash).
    assert_eq!(host.to_string_lossy(), "/Users/tom/projects/client");
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

    // A `.`-laden spelling of the same directory maps by canonicalization,
    // not by string prefixing.
    let result = m
        .host_to_container(&dir.path().join("./src/./app"))
        .unwrap();
    assert_eq!(result, PathBuf::from("/workspace/src/app"));
    // The container path maps back onto the same real directory, even when
    // the fixture root itself passes through a symlink.
    let back = m.container_to_host(&result).unwrap();
    assert_eq!(back.canonicalize().unwrap(), subdir.canonicalize().unwrap());
}
