use std::io::IsTerminal;
use std::path::Path;

use serde::Serialize;

use crate::ui::theme::resolve_color_enabled;
use crate::ui::{
    KeyValue, NoticeLevel, OutputMode, PlainRenderer, Renderer, SummaryCounts, TableSpec,
};
use crate::TaskInvocation;

use super::super::catalog::{discover_catalogs, select_catalog_and_task};
use super::super::util::parse_task_selector;
use super::super::RunnerError;
use super::super::TaskRuntimeArgs;

#[derive(Serialize)]
struct CatalogDiagnosticRow {
    alias: String,
    root: String,
    depth: usize,
    manifest: String,
    has_defer: bool,
}

#[derive(Serialize)]
struct CatalogResolutionProbe {
    selector: String,
    status: String,
    catalog: Option<String>,
    catalog_root: Option<String>,
    task: Option<String>,
    evidence: Vec<String>,
    error: Option<String>,
}

#[derive(Serialize)]
struct CatalogDiagnosticsJson {
    catalogs: Vec<CatalogDiagnosticRow>,
    precedence: Vec<String>,
    resolve: Option<CatalogResolutionProbe>,
}

pub(super) fn run_builtin_catalogs(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    target_root: &Path,
) -> Result<String, RunnerError> {
    if runtime_args.verbose_root {
        return Err(RunnerError::TaskInvocation(format!(
            "`--verbose-root` is not supported for built-in `{}`",
            task.name
        )));
    }

    let mut resolve: Option<String> = None;
    let mut output_json = false;
    let mut pretty_json = true;
    let mut i = 0usize;
    while i < runtime_args.passthrough.len() {
        let arg = &runtime_args.passthrough[i];
        if arg == "--resolve" {
            let Some(value) = runtime_args.passthrough.get(i + 1) else {
                return Err(RunnerError::TaskInvocation(
                    "catalogs argument --resolve requires a value".to_owned(),
                ));
            };
            resolve = Some(value.clone());
            i += 2;
            continue;
        }
        if arg == "--json" {
            output_json = true;
            i += 1;
            continue;
        }
        if arg == "--pretty" {
            let Some(value) = runtime_args.passthrough.get(i + 1) else {
                return Err(RunnerError::TaskInvocation(
                    "catalogs argument --pretty requires a value (`true` or `false`)"
                        .to_owned(),
                ));
            };
            pretty_json = match value.as_str() {
                "true" => true,
                "false" => false,
                _ => {
                    return Err(RunnerError::TaskInvocation(format!(
                        "catalogs argument --pretty value `{value}` is invalid (expected `true` or `false`)"
                    )));
                }
            };
            i += 2;
            continue;
        }
        return Err(RunnerError::TaskInvocation(format!(
            "unknown argument(s) for built-in `{}`: {}",
            task.name,
            runtime_args.passthrough.join(" ")
        )));
    }

    let catalogs = match discover_catalogs(target_root) {
        Ok(catalogs) => catalogs,
        Err(RunnerError::TaskCatalogsMissing { .. }) => Vec::new(),
        Err(error) => return Err(error),
    };

    let mut ordered = catalogs
        .iter()
        .map(|catalog| CatalogDiagnosticRow {
            alias: catalog.alias.clone(),
            root: catalog.catalog_root.display().to_string(),
            depth: catalog.depth,
            manifest: catalog.manifest_path.display().to_string(),
            has_defer: catalog.defer_run.is_some(),
        })
        .collect::<Vec<CatalogDiagnosticRow>>();
    ordered.sort_by(|a, b| a.depth.cmp(&b.depth).then_with(|| a.alias.cmp(&b.alias)));

    let precedence = vec![
        "explicit catalog alias prefix".to_owned(),
        "relative/absolute catalog path prefix".to_owned(),
        "unprefixed nearest in-scope catalog by cwd".to_owned(),
        "unprefixed shallowest catalog from workspace root".to_owned(),
    ];

    let resolve_probe = if let Some(raw_selector) = resolve.clone() {
        let selector = parse_task_selector(&raw_selector)?;
        let cwd = std::env::current_dir().map_err(RunnerError::Cwd)?;
        match select_catalog_and_task(&selector, &catalogs, &cwd) {
            Ok(selection) => Some(CatalogResolutionProbe {
                selector: raw_selector,
                status: "ok".to_owned(),
                catalog: Some(selection.catalog.alias.clone()),
                catalog_root: Some(selection.catalog.catalog_root.display().to_string()),
                task: Some(selector.task_name),
                evidence: selection.evidence.clone(),
                error: None,
            }),
            Err(error) => Some(CatalogResolutionProbe {
                selector: raw_selector,
                status: "error".to_owned(),
                catalog: None,
                catalog_root: None,
                task: Some(selector.task_name),
                evidence: Vec::new(),
                error: Some(error.to_string()),
            }),
        }
    } else {
        None
    };

    if !output_json && !pretty_json {
        return Err(RunnerError::TaskInvocation(
            "`--pretty` is only supported together with `--json` for built-in `catalogs`"
                .to_owned(),
        ));
    }

    if output_json {
        let payload = CatalogDiagnosticsJson {
            catalogs: ordered,
            precedence,
            resolve: resolve_probe,
        };
        return if pretty_json {
            serde_json::to_string_pretty(&payload)
                .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")))
        } else {
            serde_json::to_string(&payload)
                .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")))
        };
    }

    let color_enabled =
        resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal());
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), color_enabled);

    renderer.section("Catalog Diagnostics")?;
    renderer.key_values(&[KeyValue::new("catalogs", catalogs.len().to_string())])?;
    renderer.text("")?;

    let rows = if ordered.is_empty() {
        vec![vec![
            "<none>".to_owned(),
            "<none>".to_owned(),
            "<none>".to_owned(),
            "<none>".to_owned(),
            "<none>".to_owned(),
        ]]
    } else {
        ordered
            .iter()
            .map(|catalog| {
                vec![
                    catalog.alias.clone(),
                    catalog.root.clone(),
                    catalog.depth.to_string(),
                    catalog.manifest.clone(),
                    if catalog.has_defer {
                        "yes".to_owned()
                    } else {
                        "no".to_owned()
                    },
                ]
            })
            .collect::<Vec<Vec<String>>>()
    };
    renderer.table(&TableSpec::new(Vec::new(), rows))?;
    renderer.text("")?;

    renderer.section("Routing Precedence")?;
    renderer.bullet_list(
        "order",
        &precedence
            .iter()
            .enumerate()
            .map(|(idx, line)| format!("{}) {line}", idx + 1))
            .collect::<Vec<String>>(),
    )?;
    renderer.text("")?;

    if let Some(probe) = resolve_probe {
        renderer.section(&format!("Resolution Probe: {}", probe.selector))?;
        if probe.status == "ok" {
            renderer.key_values(&[
                KeyValue::new("catalog", probe.catalog.unwrap_or_else(|| "<none>".to_owned())),
                KeyValue::new(
                    "catalog-root",
                    probe.catalog_root.unwrap_or_else(|| "<none>".to_owned()),
                ),
                KeyValue::new("task", probe.task.unwrap_or_else(|| "<none>".to_owned())),
            ])?;
            renderer.bullet_list("evidence", &probe.evidence)?;
        } else if let Some(error) = probe.error.as_ref() {
            renderer.notice(NoticeLevel::Warning, error)?;
        }
        renderer.text("")?;
    }

    renderer.summary(SummaryCounts {
        ok: 1,
        warn: 0,
        err: 0,
    })?;
    let out = renderer.into_inner();
    String::from_utf8(out)
        .map_err(|error| RunnerError::Ui(format!("invalid utf-8 in rendered output: {error}")))
}
