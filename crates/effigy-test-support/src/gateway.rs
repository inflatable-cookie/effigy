//! Private gateway roots for fixtures. A fixture never reaches the machine
//! gateway: every root here is an owned directory outside the checkout and
//! selects itself through the environment override Effigy reads.
use std::ffi::OsString;
use std::io;
use std::path::Path;

/// The override that selects a private gateway state root.
pub const PRIVATE_ROOT_ENV: &str = "EFFIGY_GATEWAY_PRIVATE_STATE_ROOT";

/// An owned private gateway root, selected for this process until it drops.
/// Environment mutation is process-wide, so a caller that runs in parallel
/// holds its own serialization, as every fixture env guard does.
#[derive(Debug)]
pub struct PrivateRoot {
    fixture: super::Fixture,
    previous: Option<OsString>,
}

impl PrivateRoot {
    pub fn path(&self) -> &Path {
        self.fixture.path()
    }
}

impl Drop for PrivateRoot {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(value) => std::env::set_var(PRIVATE_ROOT_ENV, value),
            None => std::env::remove_var(PRIVATE_ROOT_ENV),
        }
    }
}

/// A fresh private gateway root whose path stays short enough for the
/// gateway's Unix control socket.
pub fn private_root(prefix: &str) -> io::Result<PrivateRoot> {
    let fixture = super::Fixture::short_home(prefix)?;
    let previous = std::env::var_os(PRIVATE_ROOT_ENV);
    std::env::set_var(PRIVATE_ROOT_ENV, fixture.path());
    Ok(PrivateRoot { fixture, previous })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fixture_root_is_private_and_selected_for_its_lifetime() {
        std::env::remove_var(PRIVATE_ROOT_ENV);
        let root = private_root("effigy-gateway-root-").expect("owned private root");
        let selected = std::env::var_os(PRIVATE_ROOT_ENV).expect("root selected");
        assert_eq!(Path::new(&selected), root.path());
        assert!(root.path().is_dir());
        assert!(
            root.path().as_os_str().len() <= super::super::SHORT_PATH_MAX,
            "{} is too long for a gateway control socket",
            root.path().display()
        );
        let owned = root.path().to_path_buf();
        drop(root);
        assert!(!owned.exists(), "the root is removed with its fixture");
        assert!(
            std::env::var_os(PRIVATE_ROOT_ENV).is_none(),
            "the override is restored"
        );
    }
}
