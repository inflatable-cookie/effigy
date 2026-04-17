use crate::runner::manifest::config_sections::ManifestJsPackageManager;
use effigy_manifest::LoadedCatalog;

pub(in crate::runner) struct ManifestSnapshot {
    pub(in crate::runner) manifest_paths: Vec<std::path::PathBuf>,
    pub(in crate::runner) parsed_catalogs: Vec<LoadedCatalog>,
    pub(in crate::runner) preferred_js_pm: Option<ManifestJsPackageManager>,
    pub(in crate::runner) parse_ok_any: bool,
}
