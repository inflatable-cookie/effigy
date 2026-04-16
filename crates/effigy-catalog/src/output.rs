//! Compose file output, checksum tracking, and eject support.
//!
//! Handles writing generated compose files to disk, tracking manifest
//! checksums to avoid unnecessary regeneration, and ejecting from catalog
//! management to full compose ownership.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::assembly::AssemblyResult;
use crate::error::CatalogError;

/// Manages compose file output and lifecycle.
pub struct ComposeOutput {
    /// Directory where generated files are written (e.g., `infra/dev/`).
    output_dir: PathBuf,
}

/// Result of writing compose output to disk.
#[derive(Debug)]
pub struct WriteResult {
    /// Path to the generated compose file.
    pub compose_path: PathBuf,

    /// Paths to written Dockerfiles, keyed by service name.
    pub dockerfile_paths: HashMap<String, PathBuf>,

    /// Paths to written config files.
    pub config_paths: HashMap<String, PathBuf>,

    /// Whether the compose file was regenerated (vs. cached).
    pub regenerated: bool,
}

/// Result of ejecting from catalog management.
#[derive(Debug)]
pub struct EjectResult {
    /// Path to the permanent compose file.
    pub compose_path: PathBuf,

    /// Paths to copied Dockerfiles.
    pub dockerfile_paths: HashMap<String, PathBuf>,

    /// Paths to copied config files.
    pub config_paths: HashMap<String, PathBuf>,
}

impl ComposeOutput {
    /// Create a new output manager for the given directory.
    pub fn new(output_dir: PathBuf) -> Self {
        Self { output_dir }
    }

    /// Write the assembly result to disk.
    ///
    /// If a checksum file exists and matches the current manifest content,
    /// the compose file is not regenerated (returns `regenerated = false`).
    pub fn write(
        &self,
        result: &AssemblyResult,
        manifest_content: &str,
    ) -> Result<WriteResult, CatalogError> {
        std::fs::create_dir_all(&self.output_dir)?;

        let compose_path = self.output_dir.join(".effigy-compose.generated.yml");
        let checksum_path = self.output_dir.join(".effigy-compose.checksum");

        // Check if regeneration is needed.
        let current_checksum = simple_checksum(manifest_content);
        if let Ok(stored) = std::fs::read_to_string(&checksum_path) {
            if stored.trim() == current_checksum && compose_path.exists() {
                return Ok(WriteResult {
                    compose_path,
                    dockerfile_paths: HashMap::new(),
                    config_paths: HashMap::new(),
                    regenerated: false,
                });
            }
        }

        // Write compose file.
        std::fs::write(&compose_path, &result.compose_yaml).map_err(|e| {
            CatalogError::WriteError {
                path: compose_path.clone(),
                reason: e.to_string(),
            }
        })?;

        // Write checksum.
        let _ = std::fs::write(&checksum_path, &current_checksum);

        // Write Dockerfiles to a catalog build context directory.
        let catalog_dir = self.output_dir.join(".effigy-catalog");
        let mut dockerfile_paths = HashMap::new();
        for (service_name, dockerfile_content) in &result.dockerfiles {
            let service_dir = catalog_dir.join(service_name);
            std::fs::create_dir_all(&service_dir)?;
            let path = service_dir.join("Dockerfile");
            std::fs::write(&path, dockerfile_content)?;
            dockerfile_paths.insert(service_name.clone(), path);
        }

        // Write config files.
        let mut config_paths = HashMap::new();
        for (config_name, config_content) in &result.config_files {
            let path = self.output_dir.join(config_name);
            std::fs::write(&path, config_content)?;
            config_paths.insert(config_name.clone(), path);
        }

        Ok(WriteResult {
            compose_path,
            dockerfile_paths,
            config_paths,
            regenerated: true,
        })
    }

    /// Eject from catalog management.
    ///
    /// Copies the generated compose file and supporting files to permanent
    /// locations that the user owns directly. The generated files and
    /// checksum are removed.
    pub fn eject(&self) -> Result<EjectResult, CatalogError> {
        let generated_path = self.output_dir.join(".effigy-compose.generated.yml");
        let permanent_path = self.output_dir.join("docker-compose.yml");

        if !generated_path.exists() {
            return Err(CatalogError::WriteError {
                path: generated_path,
                reason: "no generated compose file to eject — run `effigy container up` first"
                    .to_string(),
            });
        }

        // Copy compose file to permanent location.
        std::fs::copy(&generated_path, &permanent_path).map_err(|e| CatalogError::WriteError {
            path: permanent_path.clone(),
            reason: e.to_string(),
        })?;

        // Copy Dockerfiles from .effigy-catalog/ to a permanent location.
        let catalog_dir = self.output_dir.join(".effigy-catalog");
        let permanent_catalog_dir = self.output_dir.join("catalog");
        let mut dockerfile_paths = HashMap::new();
        if catalog_dir.is_dir() {
            Self::copy_dir_recursive(&catalog_dir, &permanent_catalog_dir)?;
            // Collect paths.
            if let Ok(entries) = std::fs::read_dir(&permanent_catalog_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                            let df_path = path.join("Dockerfile");
                            if df_path.exists() {
                                dockerfile_paths.insert(name.to_string(), df_path);
                            }
                        }
                    }
                }
            }
        }

        // Copy config files (*.conf in output dir that aren't the compose file).
        let mut config_paths = HashMap::new();
        if let Ok(entries) = std::fs::read_dir(&self.output_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(ext) = path.extension() {
                    if ext == "conf" {
                        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                            config_paths.insert(name.to_string(), path);
                        }
                    }
                }
            }
        }

        // Clean up generated files.
        let _ = std::fs::remove_file(&generated_path);
        let _ = std::fs::remove_file(self.output_dir.join(".effigy-compose.checksum"));
        let _ = std::fs::remove_dir_all(&catalog_dir);

        Ok(EjectResult {
            compose_path: permanent_path,
            dockerfile_paths,
            config_paths,
        })
    }

    /// Path to the generated compose file.
    pub fn generated_compose_path(&self) -> PathBuf {
        self.output_dir.join(".effigy-compose.generated.yml")
    }

    /// Path to the override compose file (user-provided).
    pub fn override_compose_path(&self) -> PathBuf {
        self.output_dir.join("compose.override.yml")
    }

    /// Whether an override file exists.
    pub fn has_override(&self) -> bool {
        self.override_compose_path().exists()
    }

    /// Build the Docker Compose file arguments for invocation.
    ///
    /// Returns `["-f", "<generated>"]` or
    /// `["-f", "<generated>", "-f", "<override>"]` if an override exists.
    pub fn compose_file_args(&self) -> Vec<String> {
        let mut args = vec![
            "-f".to_string(),
            self.generated_compose_path().to_string_lossy().to_string(),
        ];
        if self.has_override() {
            args.push("-f".to_string());
            args.push(self.override_compose_path().to_string_lossy().to_string());
        }
        args
    }

    /// Recursively copy a directory.
    fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), CatalogError> {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)?.flatten() {
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());
            if src_path.is_dir() {
                Self::copy_dir_recursive(&src_path, &dst_path)?;
            } else {
                std::fs::copy(&src_path, &dst_path)?;
            }
        }
        Ok(())
    }
}

/// Simple string checksum for cache invalidation.
///
/// Not cryptographic — just needs to detect changes reliably.
fn simple_checksum(content: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    content.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_checksum_is_deterministic() {
        let a = simple_checksum("hello world");
        let b = simple_checksum("hello world");
        assert_eq!(a, b);
    }

    #[test]
    fn simple_checksum_differs_for_different_input() {
        let a = simple_checksum("hello");
        let b = simple_checksum("world");
        assert_ne!(a, b);
    }

    #[test]
    fn write_and_cache_hit() {
        let dir = tempfile::tempdir().unwrap();
        let output = ComposeOutput::new(dir.path().to_path_buf());

        let result = crate::assembly::AssemblyResult {
            compose_yaml: "services:\n  app:\n    image: test\n".to_string(),
            dockerfiles: HashMap::new(),
            config_files: HashMap::new(),
            volumes: Vec::new(),
        };

        // First write — should regenerate.
        let write1 = output.write(&result, "manifest-v1").unwrap();
        assert!(write1.regenerated);
        assert!(write1.compose_path.exists());

        // Second write with same manifest — should be cached.
        let write2 = output.write(&result, "manifest-v1").unwrap();
        assert!(!write2.regenerated);

        // Third write with changed manifest — should regenerate.
        let write3 = output.write(&result, "manifest-v2").unwrap();
        assert!(write3.regenerated);
    }

    #[test]
    fn write_creates_dockerfiles_and_configs() {
        let dir = tempfile::tempdir().unwrap();
        let output = ComposeOutput::new(dir.path().to_path_buf());

        let mut dockerfiles = HashMap::new();
        dockerfiles.insert("app".to_string(), "FROM php:8.3-fpm".to_string());

        let mut config_files = HashMap::new();
        config_files.insert("web.conf".to_string(), "server { }".to_string());

        let result = crate::assembly::AssemblyResult {
            compose_yaml: "services:\n  app:\n    image: test\n".to_string(),
            dockerfiles,
            config_files,
            volumes: Vec::new(),
        };

        let write_result = output.write(&result, "manifest").unwrap();
        assert!(write_result.regenerated);
        assert!(write_result.dockerfile_paths.contains_key("app"));
        assert!(write_result.dockerfile_paths["app"].exists());
        assert!(write_result.config_paths.contains_key("web.conf"));
        assert!(write_result.config_paths["web.conf"].exists());
    }

    #[test]
    fn eject_copies_and_cleans_up() {
        let dir = tempfile::tempdir().unwrap();
        let output = ComposeOutput::new(dir.path().to_path_buf());

        let mut dockerfiles = HashMap::new();
        dockerfiles.insert("app".to_string(), "FROM php:8.3-fpm".to_string());

        let result = crate::assembly::AssemblyResult {
            compose_yaml: "services:\n  app:\n    image: test\n".to_string(),
            dockerfiles,
            config_files: HashMap::new(),
            volumes: Vec::new(),
        };

        // Write first.
        output.write(&result, "manifest").unwrap();

        // Eject.
        let eject_result = output.eject().unwrap();
        assert!(eject_result.compose_path.exists());
        assert_eq!(
            eject_result.compose_path.file_name().unwrap().to_str().unwrap(),
            "docker-compose.yml"
        );

        // Generated files should be cleaned up.
        assert!(!output.generated_compose_path().exists());
        assert!(!dir.path().join(".effigy-compose.checksum").exists());
        assert!(!dir.path().join(".effigy-catalog").exists());

        // Permanent Dockerfile should exist.
        assert!(eject_result.dockerfile_paths.contains_key("app"));
    }

    #[test]
    fn eject_without_generated_file_fails() {
        let dir = tempfile::tempdir().unwrap();
        let output = ComposeOutput::new(dir.path().to_path_buf());
        let result = output.eject();
        assert!(result.is_err());
    }

    #[test]
    fn compose_file_args_without_override() {
        let dir = tempfile::tempdir().unwrap();
        let output = ComposeOutput::new(dir.path().to_path_buf());
        let args = output.compose_file_args();
        assert_eq!(args.len(), 2);
        assert_eq!(args[0], "-f");
    }

    #[test]
    fn compose_file_args_with_override() {
        let dir = tempfile::tempdir().unwrap();
        // Create an override file.
        std::fs::write(dir.path().join("compose.override.yml"), "# override").unwrap();
        let output = ComposeOutput::new(dir.path().to_path_buf());
        let args = output.compose_file_args();
        assert_eq!(args.len(), 4);
        assert_eq!(args[2], "-f");
    }
}
