use super::*;
use crate::call_rhai_function;
use std::os::unix::fs::PermissionsExt;

const PRODUCTION_STAGING: &str = include_str!("../../../../scripts/build-local-bin.rhai");

#[cfg(unix)]
#[test]
fn install_previous_binary_preservation_stages_owner_only_copy() {
    let root = temp_root("previous-binary-preservation");
    let install_dir = root.join("bin");
    fs::create_dir_all(&install_dir).expect("install dir");
    let target = install_dir.join("effigy");
    let staged = install_dir.join("effigy.new");
    let previous = install_dir.join("effigy.previous");
    let active_version = install_dir.join("effigy.active-version");
    let previous_version = install_dir.join("effigy.previous.version");
    let source = root.join("source-effigy");
    fs::write(&target, b"previous-binary").expect("current binary");
    fs::set_permissions(&target, fs::Permissions::from_mode(0o755)).expect("current mode");
    fs::write(&active_version, b"v0.13.1+local.old\n").expect("current version");
    fs::write(&source, b"replacement-binary").expect("source binary");
    fs::set_permissions(&source, fs::Permissions::from_mode(0o755)).expect("source mode");

    let context = script_context(&root);
    call_rhai_function(
        &context,
        PRODUCTION_STAGING,
        "preserve_previous_local_install",
        (
            target.display().to_string(),
            previous.display().to_string(),
            active_version.display().to_string(),
            previous_version.display().to_string(),
        ),
        &callbacks(),
    )
    .expect("preserve production staging");
    call_rhai_function(
        &context,
        PRODUCTION_STAGING,
        "activate_local_install",
        (
            source.display().to_string(),
            staged.display().to_string(),
            target.display().to_string(),
            active_version.display().to_string(),
            "v0.14.0+local.test".to_owned(),
        ),
        &callbacks(),
    )
    .expect("activate production staging");

    assert_eq!(
        fs::read(&previous).expect("previous bytes"),
        b"previous-binary"
    );
    assert_eq!(
        fs::metadata(&previous)
            .expect("previous metadata")
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        fs::read(&previous_version).expect("previous version"),
        b"v0.13.1+local.old\n"
    );
    assert_eq!(
        fs::metadata(&previous_version)
            .expect("previous version metadata")
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(
        fs::read(&target).expect("activated bytes"),
        b"replacement-binary"
    );
    assert_eq!(
        fs::metadata(&target)
            .expect("activated metadata")
            .permissions()
            .mode()
            & 0o777,
        0o755
    );
    assert_eq!(
        fs::read(&active_version).expect("active version"),
        b"v0.14.0+local.test\n"
    );
    assert!(!staged.exists(), "staged name must be consumed by mv");
}
