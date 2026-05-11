use std::path::{Path, PathBuf};

use toml::Value;

use effigy_core::runtime_dir::ensure_effigy_ignored_in_git_root;

use super::{bundle_source_path, get_bundle, BundleInputType, BundleSpec, ManifestError};

#[derive(Clone, Copy)]
pub(super) struct EmbeddedBundleAsset {
    pub(super) path: &'static str,
    pub(super) contents: &'static str,
}

pub(super) struct BundleExportFile {
    pub(super) path: &'static str,
    pub(super) contents: String,
}

pub(super) fn shipped_bundle_export_files(
    name: &str,
) -> Result<Vec<BundleExportFile>, ManifestError> {
    Err(ManifestError::Compose {
        path: bundle_source_path(name),
        detail: format!("unknown bundle `{name}`"),
    })
}

pub(super) fn render_export_descriptor(spec: &BundleSpec) -> String {
    let mut out = String::new();
    out.push_str("[bundle]\n");
    out.push_str(&format!("name = {}\n", toml_string(&spec.name)));
    out.push_str(&format!(
        "description = {}\n",
        toml_string(&spec.description)
    ));
    out.push_str("defaults = \"effigy.toml\"\n");
    for input in &spec.inputs {
        out.push_str("\n[[inputs]]\n");
        out.push_str(&format!("name = {}\n", toml_string(&input.name)));
        out.push_str(&format!(
            "type = \"{}\"\n",
            bundle_input_type_literal(input.value_type)
        ));
        if input.required {
            out.push_str("required = true\n");
        }
        out.push_str(&format!(
            "description = {}\n",
            toml_string(&input.description)
        ));
        if let Some(default) = &input.default {
            out.push_str(&format!("default = {default}\n"));
        }
        if let Some(example) = &input.example {
            out.push_str(&format!("example = {example}\n"));
        }
    }
    out
}

pub(super) fn render_export_readme(spec: &BundleSpec) -> String {
    format!(
        "# {} bundle\n\n{}\n\nUse from a consuming manifest with:\n\n```toml\n[bundle]\nbase = {{ type = \"path\", dir = \"path/to/{}\" }}\n# set the inputs from bundle.toml here\n```\n",
        spec.name, spec.description, spec.name
    )
}

pub(super) fn toml_string(value: &str) -> String {
    toml::Value::String(value.to_owned()).to_string()
}

pub(super) fn bundle_input_type_literal(value_type: BundleInputType) -> &'static str {
    match value_type {
        BundleInputType::String => "string",
        BundleInputType::Integer => "integer",
        BundleInputType::Bool => "bool",
        BundleInputType::List => "list",
    }
}

pub(super) fn materialize_shipped_bundle_assets(
    manifest_path: &Path,
    bundle_name: &str,
) -> Result<PathBuf, ManifestError> {
    let assets = embedded_bundle_assets(bundle_name);
    if assets.is_empty() || is_virtual_bundle_manifest_path(manifest_path) {
        return Ok(bundle_source_path(bundle_name));
    }

    let manifest_dir = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    ensure_effigy_ignored_in_git_root(manifest_dir).map_err(|error| ManifestError::Read {
        path: manifest_dir.join(".gitignore"),
        error,
    })?;
    let hash = embedded_bundle_assets_hash(bundle_name, assets);
    let bundle_cache_dir = manifest_dir
        .join(".effigy")
        .join("runtime")
        .join("bundles")
        .join(bundle_name);
    let bundle_root = bundle_cache_dir.join(&hash);
    prune_stale_materialized_bundle_roots(&bundle_cache_dir, &hash)?;

    for asset in assets {
        let asset_path = bundle_root.join(asset.path);
        if let Some(parent) = asset_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| ManifestError::Read {
                path: parent.to_path_buf(),
                error,
            })?;
        }
        let should_write = match std::fs::read_to_string(&asset_path) {
            Ok(existing) => existing != asset.contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
            Err(error) => {
                return Err(ManifestError::Read {
                    path: asset_path,
                    error,
                });
            }
        };
        if should_write {
            std::fs::write(&asset_path, asset.contents).map_err(|error| ManifestError::Read {
                path: asset_path,
                error,
            })?;
        }
    }

    Ok(bundle_root)
}

pub(super) fn prune_stale_materialized_bundle_roots(
    bundle_cache_dir: &Path,
    active_hash: &str,
) -> Result<(), ManifestError> {
    let entries = match std::fs::read_dir(bundle_cache_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(ManifestError::Read {
                path: bundle_cache_dir.to_path_buf(),
                error,
            });
        }
    };

    for entry in entries {
        let entry = entry.map_err(|error| ManifestError::Read {
            path: bundle_cache_dir.to_path_buf(),
            error,
        })?;
        let path = entry.path();
        if entry.file_name().to_string_lossy() == active_hash {
            continue;
        }
        let file_type = entry.file_type().map_err(|error| ManifestError::Read {
            path: path.clone(),
            error,
        })?;
        if file_type.is_dir() {
            std::fs::remove_dir_all(&path).map_err(|error| ManifestError::Read {
                path: path.clone(),
                error,
            })?;
        }
    }

    Ok(())
}

pub(super) fn embedded_bundle_assets(_bundle_name: &str) -> &'static [EmbeddedBundleAsset] {
    &[]
}

pub(super) fn is_virtual_bundle_manifest_path(manifest_path: &Path) -> bool {
    manifest_path.to_string_lossy().starts_with("<bundle:")
}

pub(super) fn embedded_bundle_assets_hash(
    bundle_name: &str,
    assets: &[EmbeddedBundleAsset],
) -> String {
    let mut hash = Fnv64::new();
    hash.write(bundle_name.as_bytes());
    for asset in assets {
        hash.write(asset.path.as_bytes());
        hash.write(asset.contents.as_bytes());
    }
    format!("{:016x}", hash.finish())
}

struct Fnv64(u64);

impl Fnv64 {
    fn new() -> Self {
        Self(0xcbf29ce484222325)
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x100000001b3);
        }
    }

    fn finish(self) -> u64 {
        self.0
    }
}
