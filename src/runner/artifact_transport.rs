use std::path::PathBuf;

use effigy_artifacts::ArtifactKind;

pub(super) use effigy_artifacts::OrasCliArtifactAdapter;

pub(super) fn infer_kind_from_primary_files(primary_files: &[PathBuf]) -> ArtifactKind {
    primary_files
        .iter()
        .find_map(|path| ArtifactKind::from_path(path))
        .unwrap_or(ArtifactKind::AppSpecific)
}
