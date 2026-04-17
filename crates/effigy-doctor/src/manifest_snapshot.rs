use effigy_manifest::config_sections::ManifestJsPackageManager;
use effigy_manifest::LoadedCatalog;

pub(crate) struct ManifestSnapshot {
    pub(crate) manifest_paths: Vec<std::path::PathBuf>,
    pub(crate) parsed_catalogs: Vec<LoadedCatalog>,
    pub(crate) preferred_js_pm: Option<ManifestJsPackageManager>,
    pub(crate) parse_ok_any: bool,
}
