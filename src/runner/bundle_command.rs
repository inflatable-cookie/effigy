use effigy_cli::{BundleArgs, BundleSubcommand};
use effigy_manifest::{
    inspect_bundle_source, sync_bundle_source, BundleSourceType, TASK_MANIFEST_FILE,
};
use serde_json::json;
use std::path::{Path, PathBuf};

use super::error::RunnerError;

pub(super) fn run_bundle(args: BundleArgs) -> Result<String, RunnerError> {
    match args.subcommand {
        BundleSubcommand::Inspect => {
            run_active_bundle_inspect(args.repo_override.as_deref(), args.output_json)
        }
        BundleSubcommand::Sync => run_bundle_sync(args.output_json),
    }
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

fn run_bundle_sync(output_json: bool) -> Result<String, RunnerError> {
    let cwd = crate::runner::command_context::active_invocation_cwd()?;
    let manifest_path = discover_bundle_manifest_path(&cwd)?;
    let Some(report) = sync_bundle_source(&manifest_path, &source_preparation()?)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?
    else {
        return Err(RunnerError::task_invocation(
            "current repo does not declare a `[bundle]` source or catalog sources".to_owned(),
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
            "sources": report.sources,
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
        lines.push("current bundle source is local; nothing to refresh".to_owned());
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

static SOURCE_CANCEL: std::sync::OnceLock<effigy_kernel::ctx::CancelToken> =
    std::sync::OnceLock::new();

pub(crate) fn set_source_cancel(token: effigy_kernel::ctx::CancelToken) {
    let _ = SOURCE_CANCEL.set(token);
}
fn source_preparation() -> Result<effigy_manifest::SourcePreparation, RunnerError> {
    let home = match crate::runner::command_context::active_runtime_context() {
        Some(context) => context.paths().home.clone(),
        None => effigy_kernel::root::StateRoot::home_from_env(),
    }
    .ok_or_else(|| {
        RunnerError::task_invocation("source preparation requires an absolute HOME".to_owned())
    })?;
    let executable =
        std::env::current_exe().map_err(|err| RunnerError::task_invocation(err.to_string()))?;
    let next = effigy_kernel::outcome::Invocation::validated(
        vec!["bundle".into(), "sync".into()],
        |args| args == ["bundle", "sync"],
    )
    .map_err(|err| RunnerError::task_invocation(format!("{err:?}")))?;
    Ok(effigy_manifest::SourcePreparation {
        home,
        executable,
        cancel: SOURCE_CANCEL.get().cloned().unwrap_or_default(),
        next,
        budget: std::time::Duration::from_secs(120),
    })
}
pub(in crate::runner) fn prepare_sources_for_manifest(manifest: &Path) -> Result<(), RunnerError> {
    if !effigy_manifest::bundle_sources_need_preparation(manifest)
        .map_err(|err| RunnerError::task_invocation(err.to_string()))?
    {
        return Ok(());
    }
    effigy_manifest::prepare_bundle_sources(manifest, &source_preparation()?)
        .map_err(|err| RunnerError::task_invocation(err.to_string()))?;
    Ok(())
}

fn discover_bundle_manifest_path(root_or_cwd: impl AsRef<Path>) -> Result<PathBuf, RunnerError> {
    let root_or_cwd = root_or_cwd.as_ref();
    if root_or_cwd.is_dir() {
        let direct = root_or_cwd.join(TASK_MANIFEST_FILE);
        if direct.is_file() {
            return Ok(direct);
        }
    }
    for ancestor in root_or_cwd.ancestors() {
        let manifest_path = ancestor.join(TASK_MANIFEST_FILE);
        if manifest_path.is_file() {
            return Ok(manifest_path);
        }
    }
    Err(RunnerError::task_invocation(format!(
        "no `effigy.toml` found at or above {}",
        root_or_cwd.display()
    )))
}

fn source_type_label(source_type: BundleSourceType) -> &'static str {
    match source_type {
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
    fn bundle_sync_reports_missing_bundle_config() {
        let directory = tempfile::tempdir().expect("fixture directory");
        let tmp = directory.path().to_path_buf();
        std::fs::create_dir_all(&tmp).expect("mkdir");
        std::fs::write(tmp.join("effigy.toml"), "[tasks.dev]\nrun = \"echo hi\"\n").expect("write");
        let context = EffigyRuntimeContext::builder()
            .cwd_override(Some(tmp.clone()))
            .captured_env(CapturedEnv {
                home: Some(tmp.as_os_str().into()),
                ..CapturedEnv::default()
            })
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
    fn bundle_sync_json_prepares_path_bundle() {
        let directory = tempfile::tempdir().expect("fixture directory");
        let tmp = directory.path().to_path_buf();
        std::fs::create_dir_all(&tmp).expect("mkdir");
        std::fs::write(
            tmp.join("effigy.toml"),
            "[bundle]\nbase = { type = \"path\", dir = \"bundles/acme\" }\n",
        )
        .expect("write");
        let bundle = tmp.join("bundles/acme");
        std::fs::create_dir_all(&bundle).expect("bundle directory");
        std::fs::write(bundle.join("bundle.toml"), "[bundle]\nname = \"test\"\n")
            .expect("descriptor");
        std::fs::write(
            bundle.join("effigy.toml"),
            "[tasks.hello]\nrun = \"echo hello\"\n",
        )
        .expect("defaults");
        let context = EffigyRuntimeContext::builder()
            .cwd_override(Some(tmp.clone()))
            .captured_env(CapturedEnv {
                home: Some(tmp.as_os_str().into()),
                ..CapturedEnv::default()
            })
            .capture_lossy()
            .expect("capture context");
        let rendered = crate::runner::command_context::with_runtime_context(&context, || {
            run_bundle_sync(true)
        })
        .expect("bundle sync");
        assert!(rendered.contains("\"schema\":\"effigy.bundle.sync.v1\""));
        assert!(rendered.contains("\"source_type\":\"path\""));
        assert!(rendered.contains("\"applicable\":true"));
        let _ = std::fs::remove_dir_all(tmp);
    }

    #[test]
    fn bundle_inspect_reports_active_source_metadata() {
        let directory = tempfile::tempdir().expect("fixture directory");
        let tmp = directory.path().to_path_buf();
        std::fs::create_dir_all(&tmp).expect("mkdir");
        std::fs::write(
            tmp.join("effigy.toml"),
            "[bundle]\nbase = { type = \"path\", dir = \"bundles/acme\" }\n",
        )
        .expect("write");
        let context = EffigyRuntimeContext::builder()
            .cwd_override(Some(tmp.clone()))
            .captured_env(CapturedEnv {
                home: Some(tmp.as_os_str().into()),
                ..CapturedEnv::default()
            })
            .capture_lossy()
            .expect("capture context");
        let rendered = crate::runner::command_context::with_runtime_context(&context, || {
            run_active_bundle_inspect(None, true)
        })
        .expect("bundle inspect");
        assert!(rendered.contains("\"mode\":\"active-source\""));
        assert!(rendered.contains("\"source_type\":\"path\""));
        assert!(rendered.contains("\"stale\":false"));
        let _ = std::fs::remove_dir_all(tmp);
    }
}
