//! Select an owned private gateway root for a fixture's lifetime.
use std::{io, path::Path};

pub const PRIVATE_ROOT_ENV: &str = "EFFIGY_GATEWAY_PRIVATE_STATE_ROOT";

#[derive(Debug)]
pub struct PrivateRoot {
    _environment: super::env::ScopedEnv,
    fixture: super::Fixture,
}
impl PrivateRoot {
    pub fn path(&self) -> &Path {
        self.fixture.path()
    }
}

pub fn private_root(prefix: &str) -> io::Result<PrivateRoot> {
    let fixture = super::Fixture::new(prefix)?;
    let environment = super::env::ScopedEnv::set(PRIVATE_ROOT_ENV, fixture.path());
    Ok(PrivateRoot {
        _environment: environment,
        fixture,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_fixture_root_is_private_and_selected_for_its_lifetime() {
        let previous = std::env::var_os(PRIVATE_ROOT_ENV);
        let root = private_root("g382-").unwrap();
        let owned = root.path().to_owned();
        assert_eq!(
            std::env::var_os(PRIVATE_ROOT_ENV).as_deref(),
            Some(owned.as_os_str())
        );
        assert!(owned.is_dir());
        drop(root);
        assert!(!owned.exists());
        assert_eq!(std::env::var_os(PRIVATE_ROOT_ENV), previous);
    }
}
