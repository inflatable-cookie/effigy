//! Own a fresh fixture containing sealed kernel objects. Unseal only this
//! directory on unwind; symlinks are never followed into other checkouts.
use std::path::Path;

#[derive(Debug)]
pub(crate) struct SealedFixture(pub(crate) tempfile::TempDir);
impl SealedFixture {
    pub(crate) fn path(&self) -> &Path {
        self.0.path()
    }
}
impl Drop for SealedFixture {
    fn drop(&mut self) {
        unseal(self.path());
    }
}
fn unseal(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if std::fs::symlink_metadata(path).is_ok_and(|meta| meta.is_dir()) {
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700));
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                unseal(&entry.path());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};

    #[test]
    fn cleanup_removes_sealed_objects_without_unsealing_a_link_target() {
        let owner = tempfile::tempdir().unwrap();
        let sibling = owner.path().join("sibling");
        std::fs::create_dir(&sibling).unwrap();
        std::fs::write(sibling.join("sentinel"), "retained").unwrap();
        std::fs::set_permissions(&sibling, std::fs::Permissions::from_mode(0o500)).unwrap();
        let fixture = SealedFixture(tempfile::tempdir_in(owner.path()).unwrap());
        let root = fixture.path().to_path_buf();
        let sealed = root.join("objects/digest");
        std::fs::create_dir_all(&sealed).unwrap();
        std::fs::write(sealed.join("compose.yml"), "original").unwrap();
        std::fs::set_permissions(&sealed, std::fs::Permissions::from_mode(0o500)).unwrap();
        symlink(&sibling, root.join("external")).unwrap();
        drop(fixture);
        assert!(!root.exists());
        assert_eq!(
            std::fs::read_to_string(sibling.join("sentinel")).unwrap(),
            "retained"
        );
        assert_eq!(
            std::fs::metadata(&sibling).unwrap().permissions().mode() & 0o777,
            0o500
        );
        std::fs::set_permissions(&sibling, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
}
