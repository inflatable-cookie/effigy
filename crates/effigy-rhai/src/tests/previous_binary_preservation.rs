use super::*;
use crate::call_rhai_function;
use std::os::unix::fs::PermissionsExt;

const PRODUCTION_STAGING: &str = include_str!("../../../../scripts/build-local-bin.rhai");

/// Install state under a temporary install root: the active path, its receipts,
/// the rollback pair, and the staged build the first install step leaves behind.
struct InstallRoot {
    root: PathBuf,
    target: PathBuf,
    staged: PathBuf,
    previous: PathBuf,
    active_version: PathBuf,
    previous_version: PathBuf,
    source: PathBuf,
}

impl InstallRoot {
    fn new(name: &str) -> Self {
        let root = temp_root(name);
        let install_dir = root.join("bin");
        fs::create_dir_all(&install_dir).expect("install dir");
        Self {
            target: install_dir.join("effigy"),
            staged: install_dir.join("effigy.new"),
            previous: install_dir.join("effigy.previous"),
            active_version: install_dir.join("effigy.active-version"),
            previous_version: install_dir.join("effigy.previous.version"),
            source: root.join("source-effigy"),
            root,
        }
    }

    fn write_executable(path: &Path, bytes: &[u8]) {
        fs::write(path, bytes).expect("write binary");
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("binary mode");
    }

    /// Runs the first install step: the same calls `build-local-bin.rhai` makes
    /// after its build, with no signing identity (fixture-tested separately).
    fn stage(&self) {
        let context = script_context(&self.root);
        call_rhai_function(
            &context,
            PRODUCTION_STAGING,
            "stage_local_binary",
            (
                self.source.display().to_string(),
                self.staged.display().to_string(),
                String::new(),
            ),
            &callbacks(),
        )
        .expect("stage release binary");
        call_rhai_function(
            &context,
            PRODUCTION_STAGING,
            "preserve_previous_local_install",
            (
                self.target.display().to_string(),
                self.staged.display().to_string(),
                self.previous.display().to_string(),
                self.active_version.display().to_string(),
                self.previous_version.display().to_string(),
            ),
            &callbacks(),
        )
        .expect("preserve previous install");
        call_rhai_function(
            &context,
            PRODUCTION_STAGING,
            "stage_local_receipts",
            (
                self.target.display().to_string(),
                self.staged.display().to_string(),
                self.active_version.display().to_string(),
                "v0.14.0+local.test".to_owned(),
            ),
            &callbacks(),
        )
        .expect("stage receipts");
    }
}

#[cfg(unix)]
#[test]
fn install_previous_binary_preservation_stages_owner_only_copy() {
    let install = InstallRoot::new("previous-binary-preservation");
    InstallRoot::write_executable(&install.target, b"previous-binary");
    fs::write(&install.active_version, b"v0.13.1+local.old\n").expect("current version");
    InstallRoot::write_executable(&install.source, b"replacement-binary");

    install.stage();

    // Staging publishes nothing: the active binary and its receipt are untouched
    // until the activation step renames the staged build over them.
    assert_eq!(
        fs::read(&install.target).expect("active bytes before activation"),
        b"previous-binary"
    );
    assert_eq!(
        fs::read(&install.active_version).expect("active version before activation"),
        b"v0.13.1+local.old\n"
    );
    assert_eq!(
        fs::read(&install.staged).expect("staged bytes"),
        b"replacement-binary"
    );
    let staged_receipt = install.root.join("bin/effigy.source-receipt.new");
    let staged_version = install.root.join("bin/effigy.active-version.new");
    assert!(
        fs::read_to_string(&staged_receipt)
            .expect("staged source receipt")
            .starts_with("source_head="),
        "the staged receipt is a source receipt"
    );
    assert_eq!(
        fs::read_to_string(&staged_version).expect("staged version"),
        "v0.14.0+local.test\n"
    );

    assert_eq!(
        fs::read(&install.previous).expect("previous bytes"),
        b"previous-binary"
    );
    assert_eq!(
        fs::metadata(&install.previous)
            .expect("previous metadata")
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        fs::read(&install.previous_version).expect("previous version"),
        b"v0.13.1+local.old\n"
    );
    assert_eq!(
        fs::metadata(&install.previous_version)
            .expect("previous version metadata")
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}

#[cfg(unix)]
#[test]
fn install_previous_binary_preservation_keeps_rollback_when_build_is_already_active() {
    let install = InstallRoot::new("previous-binary-retry");
    // The first attempt preserved the real previous install and swapped in this
    // build, then failed later. The retry must not copy the new build over the
    // rollback binary.
    InstallRoot::write_executable(&install.previous, b"released-previous");
    fs::write(&install.previous_version, b"v0.14.1+local.released\n").expect("rollback version");
    fs::write(&install.active_version, b"v0.15.0+local.new\n").expect("active version");
    InstallRoot::write_executable(&install.target, b"replacement-binary");
    InstallRoot::write_executable(&install.source, b"replacement-binary");

    install.stage();

    assert_eq!(
        fs::read(&install.previous).expect("rollback bytes after retry"),
        b"released-previous"
    );
    assert_eq!(
        fs::read(&install.previous_version).expect("rollback version after retry"),
        b"v0.14.1+local.released\n"
    );
    assert_eq!(
        fs::read(&install.target).expect("active bytes after retry"),
        b"replacement-binary"
    );
}
