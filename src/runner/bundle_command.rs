use super::command_context::resolve_active_repo_root;
use effigy_cli::{BundleArgs, BundleSubcommand};
use effigy_manifest::{
    export_bundle, get_bundle, inspect_bundle_source, list_bundle_default_paths, list_bundles,
    sync_bundle_source, BundleSourceType,
};
use serde_json::json;
use std::path::{Path, PathBuf};

use super::error::RunnerError;

pub(super) fn run_bundle(args: BundleArgs) -> Result<String, RunnerError> {
    match args.subcommand {
        BundleSubcommand::List => run_bundle_list(args.output_json),
        BundleSubcommand::Inspect { bundle } => run_bundle_inspect(
            bundle.as_deref(),
            args.repo_override.as_deref(),
            args.output_json,
        ),
        BundleSubcommand::Export { bundle, path } => run_bundle_export(
            &bundle,
            &path,
            args.repo_override.as_deref(),
            args.output_json,
        ),
        BundleSubcommand::Sync => run_bundle_sync(args.output_json),
    }
}

fn run_bundle_list(output_json: bool) -> Result<String, RunnerError> {
    let bundles = list_bundles();
    if output_json {
        return Ok(json!({
            "schema": "effigy.bundle.list.v1",
            "schema_version": 1,
            "ok": true,
            "bundles": bundles.iter().map(|bundle| json!({
                "name": bundle.name,
                "description": bundle.description,
                "input_count": bundle.inputs.len(),
            })).collect::<Vec<_>>(),
        })
        .to_string());
    }

    if bundles.is_empty() {
        return Ok("[info] no bundles available".to_owned());
    }

    let mut lines = vec![format!("[bundle] {} bundles", bundles.len())];
    lines.extend(
        bundles
            .into_iter()
            .map(|bundle| format!("{} :: {}", bundle.name, bundle.description)),
    );
    Ok(lines.join("\n"))
}

fn run_bundle_inspect(
    bundle_name: Option<&str>,
    repo_override: Option<&Path>,
    output_json: bool,
) -> Result<String, RunnerError> {
    if let Some(bundle_name) = bundle_name {
        return run_shipped_bundle_inspect(bundle_name, output_json);
    }
    run_active_bundle_inspect(repo_override, output_json)
}

fn run_shipped_bundle_inspect(bundle_name: &str, output_json: bool) -> Result<String, RunnerError> {
    let bundle = get_bundle(bundle_name)
        .ok_or_else(|| RunnerError::task_invocation(format!("unknown bundle `{bundle_name}`")))?;
    let default_paths = list_bundle_default_paths(bundle_name)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;

    if output_json {
        return Ok(json!({
            "schema": "effigy.bundle.inspect.v1",
            "schema_version": 1,
            "ok": true,
            "mode": "catalog",
            "bundle": {
                "name": bundle.name,
                "description": bundle.description,
                "inputs": bundle.inputs.iter().map(|input| json!({
                    "name": input.name,
                    "type": input.value_type,
                    "required": input.required,
                    "description": input.description,
                    "default": input.default,
                    "example": input.example,
                })).collect::<Vec<_>>(),
                "default_paths": default_paths,
            },
            "source": null,
        })
        .to_string());
    }

    let mut lines = vec![
        format!("[bundle] {}", bundle.name),
        bundle.description,
        String::new(),
        format!("Inputs ({})", bundle.inputs.len()),
    ];
    for input in &bundle.inputs {
        let mut suffix = vec![format!("{:?}", input.value_type).to_lowercase()];
        suffix.push(
            if input.required {
                "required"
            } else {
                "optional"
            }
            .to_owned(),
        );
        if let Some(default) = &input.default {
            suffix.push(format!("default={default}"));
        }
        if let Some(example) = &input.example {
            suffix.push(format!("example={example}"));
        }
        lines.push(format!(
            "- {} [{}] :: {}",
            input.name,
            suffix.join(", "),
            input.description
        ));
    }

    lines.push(String::new());
    lines.push(format!("Default Paths ({})", default_paths.len()));
    lines.extend(default_paths.into_iter().map(|path| format!("- {path}")));
    Ok(lines.join("\n"))
}

fn run_active_bundle_inspect(
    repo_override: Option<&Path>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let manifest_path = match repo_override {
        Some(repo_root) => discover_bundle_manifest_path(repo_root)?,
        None => {
            let cwd = crate::runner::command_context::active_invocation_cwd()?;
            discover_bundle_manifest_path(&cwd)?
        }
    };
    let Some(report) = inspect_bundle_source(&manifest_path)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?
    else {
        return Err(RunnerError::task_invocation(
            "current repo does not declare a `[bundle]` source".to_owned(),
        ));
    };

    if output_json {
        return Ok(json!({
            "schema": "effigy.bundle.inspect.v1",
            "schema_version": 1,
            "ok": true,
            "mode": "active-source",
            "bundle": null,
            "source": {
                "source_type": report.source_type,
                "source_path": report.source_path,
                "local_path": report.local_path,
                "version_hint": report.version_hint,
                "stale": report.stale,
                "manifest_path": manifest_path,
            }
        })
        .to_string());
    }

    Ok([
        format!("[bundle] source {}", source_type_label(report.source_type)),
        format!("source={}", report.source_path.display()),
        format!("manifest={}", manifest_path.display()),
        format!("local_path={}", report.local_path.display()),
        format!(
            "version={}",
            report.version_hint.as_deref().unwrap_or("unavailable")
        ),
        format!("stale={}", report.stale),
    ]
    .join("\n"))
}

fn run_bundle_export(
    bundle_name: &str,
    path: &std::path::Path,
    repo_override: Option<&Path>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let export_path = resolve_bundle_export_path(repo_override, path)?;
    let export = export_bundle(bundle_name, &export_path)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;

    if output_json {
        return Ok(json!({
            "schema": "effigy.bundle.export.v1",
            "schema_version": 1,
            "ok": true,
            "bundle": export.bundle,
            "path": export.path,
            "files": export.files,
        })
        .to_string());
    }

    let mut lines = vec![
        format!(
            "[bundle] exported `{}` to {}",
            export.bundle,
            export.path.display()
        ),
        "Use it from a manifest with `[bundle].base = { type = \"path\", dir = ... }`.".to_owned(),
        String::new(),
        format!("Files ({})", export.files.len()),
    ];
    lines.extend(export.files.into_iter().map(|file| format!("- {file}")));
    Ok(lines.join("\n"))
}

fn run_bundle_sync(output_json: bool) -> Result<String, RunnerError> {
    let cwd = crate::runner::command_context::active_invocation_cwd()?;
    let manifest_path = discover_bundle_manifest_path(&cwd)?;
    let Some(report) = sync_bundle_source(&manifest_path)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?
    else {
        return Err(RunnerError::task_invocation(
            "current repo does not declare a `[bundle]` source".to_owned(),
        ));
    };

    if output_json {
        return Ok(json!({
            "schema": "effigy.bundle.sync.v1",
            "schema_version": 1,
            "ok": true,
            "source_type": report.source_type,
            "source_path": report.source_path,
            "local_path": report.local_path,
            "version_hint": report.version_hint,
            "changed": report.changed,
            "applicable": report.applicable,
            "manifest_path": manifest_path,
        })
        .to_string());
    }

    let mut lines = vec![format!(
        "[bundle] sync {}",
        source_type_label(report.source_type)
    )];
    lines.push(format!("source={}", report.source_path.display()));
    lines.push(format!("manifest={}", manifest_path.display()));
    if let Some(local_path) = &report.local_path {
        lines.push(format!("local_path={}", local_path.display()));
    }
    if let Some(version_hint) = &report.version_hint {
        lines.push(format!("version={version_hint}"));
    }
    if !report.applicable {
        lines.push("status=not-applicable".to_owned());
        lines.push("current bundle source is local or shipped; nothing to refresh".to_owned());
        return Ok(lines.join("\n"));
    }
    lines.push(format!(
        "status={}",
        if report.changed {
            "refreshed"
        } else {
            "unchanged"
        }
    ));
    Ok(lines.join("\n"))
}

fn discover_bundle_manifest_path(root_or_cwd: impl AsRef<Path>) -> Result<PathBuf, RunnerError> {
    let root_or_cwd = root_or_cwd.as_ref();
    if root_or_cwd.is_dir() {
        let direct = root_or_cwd.join("effigy.toml");
        if direct.is_file() {
            return Ok(direct);
        }
    }
    for ancestor in root_or_cwd.ancestors() {
        let manifest_path = ancestor.join("effigy.toml");
        if manifest_path.is_file() {
            return Ok(manifest_path);
        }
    }
    Err(RunnerError::task_invocation(format!(
        "no `effigy.toml` found at or above {}",
        root_or_cwd.display()
    )))
}

fn resolve_bundle_export_path(
    repo_override: Option<&Path>,
    path: &Path,
) -> Result<PathBuf, RunnerError> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    match repo_override {
        Some(path_override) => {
            let resolved = resolve_active_repo_root(Some(path_override.to_path_buf()))?;
            Ok(resolved.resolved_root.join(path))
        }
        None => Ok(path.to_path_buf()),
    }
}

fn source_type_label(source_type: BundleSourceType) -> &'static str {
    match source_type {
        BundleSourceType::Shipped => "shipped",
        BundleSourceType::Path => "path",
        BundleSourceType::Git => "git",
        BundleSourceType::Oci => "oci",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use effigy_context::{CapturedEnv, EffigyRuntimeContext};

    #[test]
    fn bundle_list_reports_bundles() {
        let rendered = run_bundle_list(false).expect("list");
        assert!(rendered.contains("[bundle]"));
        assert!(rendered.contains("underlay"));
    }

    #[test]
    fn bundle_inspect_reports_inputs_and_default_paths() {
        let rendered = run_bundle_inspect(Some("underlay"), None, false).expect("inspect");
        assert!(rendered.contains("Inputs"));
        assert!(rendered.contains("workspace_subdir"));
        assert!(rendered.contains("Default Paths"));
        assert!(rendered.contains("containers.stack.services.postgres.catalog"));
    }

    #[test]
    fn bundle_inspect_reports_underlay_alias_surface() {
        let rendered = run_bundle_inspect(Some("underlay"), None, false).expect("inspect");
        assert!(rendered.contains("workspace_subdir"));
        assert!(rendered.contains("containers.stack.services.postgres.catalog"));
        assert!(rendered.contains("containers.stack.dns.routes"));
    }

    #[test]
    fn bundle_export_writes_local_bundle_files() {
        let tmp = std::env::temp_dir().join(format!(
            "effigy-bundle-export-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        let target = tmp.join("underlay");

        let rendered = run_bundle_export("underlay", &target, None, false).expect("export");

        assert!(rendered.contains("exported `underlay`"));
        assert!(target.join("bundle.toml").exists());
        assert!(target.join("effigy.toml").exists());
        assert!(target.join("scripts/dev/ui-setup.rhai").exists());
        let _ = std::fs::remove_dir_all(tmp);
    }

    #[test]
    fn bundle_sync_reports_missing_bundle_config() {
        let tmp = std::env::temp_dir().join(format!(
            "effigy-bundle-sync-empty-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir_all(&tmp).expect("mkdir");
        std::fs::write(tmp.join("effigy.toml"), "[tasks.dev]\nrun = \"echo hi\"\n").expect("write");
        let context = EffigyRuntimeContext::builder()
            .cwd_override(Some(tmp.clone()))
            .captured_env(CapturedEnv::default())
            .capture_lossy()
            .expect("capture context");
        let error = crate::runner::command_context::with_runtime_context(&context, || {
            run_bundle_sync(false)
        })
        .expect_err("reject missing bundle");
        assert!(error
            .to_string()
            .contains("does not declare a `[bundle]` source"));
        let _ = std::fs::remove_dir_all(tmp);
    }

    #[test]
    fn bundle_sync_json_reports_not_applicable_for_shipped_bundle() {
        let tmp = std::env::temp_dir().join(format!(
            "effigy-bundle-sync-shipped-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir_all(&tmp).expect("mkdir");
        std::fs::write(
            tmp.join("effigy.toml"),
            "[bundle]\nbase = \"underlay\"\nhost = \"example.test\"\n",
        )
        .expect("write");
        let context = EffigyRuntimeContext::builder()
            .cwd_override(Some(tmp.clone()))
            .captured_env(CapturedEnv::default())
            .capture_lossy()
            .expect("capture context");
        let rendered = crate::runner::command_context::with_runtime_context(&context, || {
            run_bundle_sync(true)
        })
        .expect("bundle sync");
        assert!(rendered.contains("\"schema\":\"effigy.bundle.sync.v1\""));
        assert!(rendered.contains("\"source_type\":\"shipped\""));
        assert!(rendered.contains("\"applicable\":false"));
        let _ = std::fs::remove_dir_all(tmp);
    }

    #[test]
    fn bundle_inspect_without_name_reports_active_source_metadata() {
        let tmp = std::env::temp_dir().join(format!(
            "effigy-bundle-inspect-active-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir_all(&tmp).expect("mkdir");
        std::fs::write(
            tmp.join("effigy.toml"),
            "[bundle]\nbase = \"underlay\"\nhost = \"example.test\"\n",
        )
        .expect("write");
        let context = EffigyRuntimeContext::builder()
            .cwd_override(Some(tmp.clone()))
            .captured_env(CapturedEnv::default())
            .capture_lossy()
            .expect("capture context");
        let rendered = crate::runner::command_context::with_runtime_context(&context, || {
            run_bundle_inspect(None, None, true)
        })
        .expect("bundle inspect");
        assert!(rendered.contains("\"mode\":\"active-source\""));
        assert!(rendered.contains("\"source_type\":\"shipped\""));
        assert!(rendered.contains("\"stale\":false"));
        let _ = std::fs::remove_dir_all(tmp);
    }

    #[test]
    fn bundle_export_repo_override_anchors_relative_path_to_repo_root() {
        let repo = std::env::temp_dir().join(format!(
            "effigy-bundle-export-repo-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir_all(&repo).expect("mkdir repo");
        let target = std::path::Path::new("bundles/underlay");

        let resolved = resolve_bundle_export_path(Some(repo.as_path()), target).expect("resolve");
        assert!(resolved.ends_with(target));
        assert!(resolved.file_name().is_some_and(|name| name == "underlay"));

        let _ = std::fs::remove_dir_all(repo);
    }
}
