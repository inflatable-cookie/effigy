use effigy_cli::{BundleArgs, BundleSubcommand};
use effigy_manifest::{export_bundle, get_bundle, list_bundle_default_paths, list_bundles};
use serde_json::json;

use super::error::RunnerError;

pub(super) fn run_bundle(args: BundleArgs) -> Result<String, RunnerError> {
    match args.subcommand {
        BundleSubcommand::List => run_bundle_list(args.output_json),
        BundleSubcommand::Inspect { bundle } => run_bundle_inspect(&bundle, args.output_json),
        BundleSubcommand::Export { bundle, path } => {
            run_bundle_export(&bundle, &path, args.output_json)
        }
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

fn run_bundle_inspect(bundle_name: &str, output_json: bool) -> Result<String, RunnerError> {
    let bundle = get_bundle(bundle_name)
        .ok_or_else(|| RunnerError::task_invocation(format!("unknown bundle `{bundle_name}`")))?;
    let default_paths = list_bundle_default_paths(bundle_name)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;

    if output_json {
        return Ok(json!({
            "schema": "effigy.bundle.inspect.v1",
            "schema_version": 1,
            "ok": true,
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
            }
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

fn run_bundle_export(
    bundle_name: &str,
    path: &std::path::Path,
    output_json: bool,
) -> Result<String, RunnerError> {
    let export = export_bundle(bundle_name, path)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_list_reports_decodelabs() {
        let rendered = run_bundle_list(false).expect("list");
        assert!(rendered.contains("[bundle]"));
        assert!(rendered.contains("decodelabs"));
        assert!(rendered.contains("underlay"));
    }

    #[test]
    fn bundle_inspect_reports_inputs_and_default_paths() {
        let rendered = run_bundle_inspect("decodelabs", false).expect("inspect");
        assert!(rendered.contains("Inputs"));
        assert!(rendered.contains("host"));
        assert!(rendered.contains("Default Paths"));
        assert!(rendered.contains("containers.web.services.app.catalog"));
    }

    #[test]
    fn bundle_inspect_reports_underlay_alias_surface() {
        let rendered = run_bundle_inspect("underlay", false).expect("inspect");
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

        let rendered = run_bundle_export("underlay", &target, false).expect("export");

        assert!(rendered.contains("exported `underlay`"));
        assert!(target.join("bundle.toml").exists());
        assert!(target.join("effigy.toml").exists());
        assert!(target.join("scripts/dev/ui-setup.rhai").exists());
        let _ = std::fs::remove_dir_all(tmp);
    }
}
