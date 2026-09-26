use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use effigy_cli::BootstrapDepsSyncMode;
use effigy_manifest::{ManifestJsPackageManager, TASK_MANIFEST_FILE};
use serde::Serialize;
use serde_json::json;

use crate::runner::manifest::load_task_manifest_with_inspection;

use crate::runner::error::RunnerError;

#[derive(Debug, Serialize)]
struct BootstrapDepsOperation {
    path: String,
    absolute_path: String,
    kind: &'static str,
    command: String,
    manifest_path: Option<String>,
}

#[derive(Debug, Serialize)]
struct BootstrapDepsSkippedPath {
    path: String,
    reason: &'static str,
}

pub(in crate::runner) fn run_bootstrap_deps_sync(
    repo_root: &Path,
    mode: BootstrapDepsSyncMode,
    refresh_lock: bool,
    paths: &[String],
    output_json: bool,
) -> Result<String, RunnerError> {
    let root_parent = repo_root.parent().unwrap_or(repo_root);
    let mut manifest_cache = BTreeMap::<PathBuf, Option<ManifestJsPackageManager>>::new();
    preflight_bun_installs(
        repo_root,
        root_parent,
        mode,
        refresh_lock,
        paths,
        &mut manifest_cache,
    )?;
    let mut operations = Vec::<BootstrapDepsOperation>::new();
    let mut skipped = Vec::<BootstrapDepsSkippedPath>::new();

    for path_raw in paths {
        let Some(resolved) = resolve_bootstrap_sync_path(repo_root, root_parent, path_raw)? else {
            skipped.push(BootstrapDepsSkippedPath {
                path: path_raw.clone(),
                reason: "missing directory",
            });
            continue;
        };
        let package_json = resolved.join("package.json");
        let cargo_toml = resolved.join("Cargo.toml");
        let wants_js = matches!(
            mode,
            BootstrapDepsSyncMode::Both | BootstrapDepsSyncMode::JsOnly
        );
        let wants_rust = matches!(
            mode,
            BootstrapDepsSyncMode::Both | BootstrapDepsSyncMode::RustOnly
        );
        let mut matched = false;

        if wants_js && package_json.is_file() {
            let manifest_path = find_nearest_manifest_path(&resolved, root_parent);
            let package_manager =
                js_package_manager_for_manifest(manifest_path.as_deref(), &mut manifest_cache)?;
            let package_manager = package_manager.ok_or_else(|| {
                RunnerError::task_invocation(format!(
                    "`bootstrap deps sync {}` found package.json but no `[package_manager].js` is configured",
                    path_raw
                ))
            })?;
            let bun_install_root = if package_manager == ManifestJsPackageManager::Bun {
                Some(find_bun_workspace_root(&resolved, root_parent))
            } else {
                None
            };
            let (program, args, command) = js_install_invocation(
                package_manager,
                path_raw,
                refresh_lock,
                bun_install_root.as_deref(),
            )?;
            run_sync_command(program, &args, &resolved, &command)?;
            if let Some(install_root) = bun_install_root {
                remove_macos_metadata_from_node_modules(&install_root)?;
            }
            operations.push(BootstrapDepsOperation {
                path: path_raw.clone(),
                absolute_path: resolved.display().to_string(),
                kind: "js",
                command,
                manifest_path: manifest_path.map(|path| path.display().to_string()),
            });
            matched = true;
        }

        if wants_rust && cargo_toml.is_file() {
            let command = "cargo fetch --manifest-path Cargo.toml".to_owned();
            run_sync_command(
                "cargo",
                &["fetch", "--manifest-path", "Cargo.toml"],
                &resolved,
                &command,
            )?;
            operations.push(BootstrapDepsOperation {
                path: path_raw.clone(),
                absolute_path: resolved.display().to_string(),
                kind: "rust",
                command,
                manifest_path: None,
            });
            matched = true;
        }

        if !matched {
            let detail = match mode {
                BootstrapDepsSyncMode::Both => "expected package.json and/or Cargo.toml",
                BootstrapDepsSyncMode::JsOnly => "expected package.json",
                BootstrapDepsSyncMode::RustOnly => "expected Cargo.toml",
            };
            return Err(RunnerError::task_invocation(format!(
                "`bootstrap deps sync {}` found no supported dependency manifest ({detail})",
                path_raw
            )));
        }
    }

    if output_json {
        return serde_json::to_string_pretty(&json!({
            "schema": "effigy.bootstrap.deps.v1",
            "schema_version": 1,
            "ok": true,
            "mode": match mode {
                BootstrapDepsSyncMode::Both => "both",
                BootstrapDepsSyncMode::JsOnly => "js",
                BootstrapDepsSyncMode::RustOnly => "rust",
            },
            "operations": operations,
            "skipped": skipped,
        }))
        .map_err(|error| RunnerError::task_invocation(error.to_string()));
    }

    let mut text = format!("bootstrap deps sync completed ({})", operations.len());
    for operation in operations {
        text.push_str(&format!(
            "\n- {} [{}]: {}",
            operation.path, operation.kind, operation.command
        ));
    }
    for skipped_path in skipped {
        text.push_str(&format!(
            "\n- {} [skip]: {}",
            skipped_path.path, skipped_path.reason
        ));
    }
    Ok(text)
}

fn resolve_bootstrap_sync_path(
    repo_root: &Path,
    root_parent: &Path,
    path_raw: &str,
) -> Result<Option<PathBuf>, RunnerError> {
    let canonical_repo_root = repo_root
        .canonicalize()
        .map_err(|error| RunnerError::task_invocation_failed_read(repo_root, error))?;
    let canonical_root_parent = root_parent
        .canonicalize()
        .map_err(|error| RunnerError::task_invocation_failed_read(root_parent, error))?;
    let candidate = repo_root.join(path_raw);
    let metadata = match std::fs::metadata(&candidate) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(RunnerError::task_invocation_failed_read(&candidate, error)),
    };
    if !metadata.is_dir() {
        return Err(RunnerError::task_invocation(format!(
            "`bootstrap deps sync {path_raw}` must target a directory",
        )));
    }
    let canonical = candidate
        .canonicalize()
        .map_err(|error| RunnerError::task_invocation_failed_read(&candidate, error))?;
    if canonical.starts_with(&canonical_repo_root) || canonical.starts_with(&canonical_root_parent)
    {
        return Ok(Some(canonical));
    }
    Err(RunnerError::task_invocation(format!(
        "`bootstrap deps sync {path_raw}` cannot escape the repo parent directory",
    )))
}

fn find_nearest_manifest_path(start: &Path, root_parent: &Path) -> Option<PathBuf> {
    let canonical_root_parent = root_parent.canonicalize().ok()?;
    for ancestor in start.ancestors() {
        if !ancestor.starts_with(&canonical_root_parent) {
            break;
        }
        let manifest_path = ancestor.join(TASK_MANIFEST_FILE);
        if manifest_path.is_file() {
            return Some(manifest_path);
        }
        if ancestor == canonical_root_parent {
            break;
        }
    }
    None
}

fn js_package_manager_for_manifest(
    manifest_path: Option<&Path>,
    cache: &mut BTreeMap<PathBuf, Option<ManifestJsPackageManager>>,
) -> Result<Option<ManifestJsPackageManager>, RunnerError> {
    let Some(manifest_path) = manifest_path else {
        return Ok(None);
    };
    if let Some(existing) = cache.get(manifest_path) {
        return Ok(*existing);
    }
    let loaded = load_task_manifest_with_inspection(manifest_path)?;
    let package_manager = loaded.manifest.package_manager.and_then(|config| config.js);
    cache.insert(manifest_path.to_path_buf(), package_manager);
    Ok(package_manager)
}

fn js_install_invocation(
    package_manager: ManifestJsPackageManager,
    path_raw: &str,
    refresh_lock: bool,
    bun_install_root: Option<&Path>,
) -> Result<(&'static str, Vec<&'static str>, String), RunnerError> {
    match package_manager {
        ManifestJsPackageManager::Bun if refresh_lock => Ok((
            "bun",
            vec!["install", "--save-text-lockfile"],
            "bun install --save-text-lockfile".to_owned(),
        )),
        ManifestJsPackageManager::Bun => {
            let install_root = bun_install_root.expect("Bun install root is resolved first");
            ensure_bun_lock_exists(path_raw, install_root)?;
            Ok((
                "bun",
                vec!["install", "--frozen-lockfile"],
                "bun install --frozen-lockfile".to_owned(),
            ))
        }
        ManifestJsPackageManager::Pnpm if refresh_lock => Err(RunnerError::task_invocation(
            format!(
                "`bootstrap deps sync --refresh-lock {path_raw}` is only supported when `[package_manager].js = \"bun\"`"
            ),
        )),
        ManifestJsPackageManager::Npm if refresh_lock => Err(RunnerError::task_invocation(
            format!(
                "`bootstrap deps sync --refresh-lock {path_raw}` is only supported when `[package_manager].js = \"bun\"`"
            ),
        )),
        ManifestJsPackageManager::Pnpm => {
            Ok(("pnpm", vec!["install"], "pnpm install".to_owned()))
        }
        ManifestJsPackageManager::Npm => Ok(("npm", vec!["install"], "npm install".to_owned())),
        ManifestJsPackageManager::Direct => Err(RunnerError::task_invocation(format!(
            "`bootstrap deps sync {path_raw}` cannot hydrate JS dependencies with `[package_manager].js = \"direct\"`",
        ))),
    }
}

fn preflight_bun_installs(
    repo_root: &Path,
    root_parent: &Path,
    mode: BootstrapDepsSyncMode,
    refresh_lock: bool,
    paths: &[String],
    manifest_cache: &mut BTreeMap<PathBuf, Option<ManifestJsPackageManager>>,
) -> Result<(), RunnerError> {
    if mode == BootstrapDepsSyncMode::RustOnly {
        return Ok(());
    }

    let mut bun_target_found = false;
    for path_raw in paths {
        let Some(resolved) = resolve_bootstrap_sync_path(repo_root, root_parent, path_raw)? else {
            continue;
        };
        if !resolved.join("package.json").is_file() {
            continue;
        }

        let manifest_path = find_nearest_manifest_path(&resolved, root_parent);
        let package_manager = js_package_manager_for_manifest(manifest_path.as_deref(), manifest_cache)?
            .ok_or_else(|| {
                RunnerError::task_invocation(format!(
                    "`bootstrap deps sync {path_raw}` found package.json but no `[package_manager].js` is configured"
                ))
            })?;
        if package_manager == ManifestJsPackageManager::Bun {
            bun_target_found = true;
            if !refresh_lock {
                let install_root = find_bun_workspace_root(&resolved, root_parent);
                ensure_bun_lock_exists(path_raw, &install_root)?;
            }
        } else if refresh_lock {
            return Err(RunnerError::task_invocation(format!(
                "`bootstrap deps sync --refresh-lock {path_raw}` is only supported when `[package_manager].js = \"bun\"`"
            )));
        }
    }

    if refresh_lock && !bun_target_found {
        return Err(RunnerError::task_invocation(
            "`bootstrap deps sync --refresh-lock` requires at least one target with a Bun package.json and `[package_manager].js = \"bun\"`"
        ));
    }
    Ok(())
}

fn ensure_bun_lock_exists(path_raw: &str, install_root: &Path) -> Result<(), RunnerError> {
    let has_lock = ["bun.lock", "bun.lockb"]
        .iter()
        .any(|name| install_root.join(name).is_file());
    if has_lock {
        return Ok(());
    }
    Err(RunnerError::task_invocation(format!(
        "`bootstrap deps sync {path_raw}` requires `bun.lock` or `bun.lockb` at {} for a frozen Bun install; run `effigy bootstrap deps sync --refresh-lock {path_raw}` to generate or refresh `bun.lock`, review and commit it, then rerun without `--refresh-lock`",
        install_root.display()
    )))
}

fn find_bun_workspace_root(project_dir: &Path, root_parent: &Path) -> PathBuf {
    let root_parent = root_parent
        .canonicalize()
        .unwrap_or_else(|_| root_parent.to_path_buf());
    for ancestor in project_dir.ancestors() {
        if !ancestor.starts_with(&root_parent) {
            break;
        }
        let manifest_path = ancestor.join("package.json");
        let Ok(contents) = std::fs::read_to_string(manifest_path) else {
            continue;
        };
        let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&contents) else {
            continue;
        };
        if manifest.get("workspaces").is_some() {
            return ancestor.to_path_buf();
        }
    }
    project_dir.to_path_buf()
}

fn remove_macos_metadata_from_node_modules(install_root: &Path) -> Result<(), RunnerError> {
    let node_modules = install_root.join("node_modules");
    let metadata = match std::fs::symlink_metadata(&node_modules) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(RunnerError::task_invocation_failed_read(
                &node_modules,
                error,
            ))
        }
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Ok(());
    }
    remove_macos_metadata_entries(&node_modules)
}

fn remove_macos_metadata_entries(directory: &Path) -> Result<(), RunnerError> {
    let entries = std::fs::read_dir(directory)
        .map_err(|error| RunnerError::task_invocation_failed_read(directory, error))?;
    for entry in entries {
        let entry =
            entry.map_err(|error| RunnerError::task_invocation_failed_read(directory, error))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| RunnerError::task_invocation_failed_read(&path, error))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == ".DS_Store" || name.starts_with("._") || name == "__MACOSX" {
            let result = if file_type.is_dir() {
                std::fs::remove_dir_all(&path)
            } else {
                std::fs::remove_file(&path)
            };
            if let Err(error) = result {
                if error.kind() != std::io::ErrorKind::NotFound {
                    return Err(RunnerError::task_invocation_failed_write(&path, error));
                }
            }
        } else if file_type.is_dir() {
            remove_macos_metadata_entries(&path)?;
        }
    }
    Ok(())
}

fn run_sync_command(
    program: &str,
    args: &[&str],
    cwd: &Path,
    command_label: &str,
) -> Result<(), RunnerError> {
    let output = ProcessCommand::new(program)
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: command_label.to_owned(),
            error,
        })?;
    if output.status.success() {
        return Ok(());
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    let mut detail = format!(
        "`{command_label}` failed in {} (code={:?})",
        cwd.display(),
        output.status.code()
    );
    if !stdout.is_empty() {
        detail.push_str(&format!("\nstdout:\n{stdout}"));
    }
    if !stderr.is_empty() {
        detail.push_str(&format!("\nstderr:\n{stderr}"));
    }
    Err(RunnerError::task_invocation(detail))
}
