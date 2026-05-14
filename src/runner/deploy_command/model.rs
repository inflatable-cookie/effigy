use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize)]
pub(crate) struct DeployModel {
    pub(crate) schema: String,
    pub(crate) schema_version: u64,
    pub(crate) app: DeployApp,
    pub(crate) services: Vec<DeployService>,
    pub(crate) backing_services: Vec<DeployBackingService>,
    pub(crate) domains: Vec<DeployDomain>,
    pub(crate) secrets: Vec<DeploySecret>,
    pub(crate) warnings: Vec<DeployWarning>,
}

#[derive(Clone, Serialize)]
pub(crate) struct DeployApp {
    pub(crate) name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) bundle: Option<String>,
    pub(crate) project_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) source_root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) notes: Option<String>,
}

#[derive(Clone, Serialize)]
pub(crate) struct DeployService {
    pub(crate) name: String,
    pub(crate) role: String,
    pub(crate) runtime: String,
    pub(crate) source_root: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) build: Option<DeployCommandStep>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) start: Option<DeployCommandStep>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) release: Option<DeployCommandStep>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) health: Option<DeployHealth>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) output: Option<DeployOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) port: Option<u16>,
    pub(crate) domains: Vec<String>,
    pub(crate) env: BTreeMap<String, String>,
    pub(crate) secret_refs: Vec<String>,
    pub(crate) volumes: Vec<String>,
    pub(crate) warnings: Vec<DeployWarning>,
}

#[derive(Clone, Serialize)]
pub(crate) struct DeployCommandStep {
    pub(crate) command: String,
}

#[derive(Clone, Serialize)]
pub(crate) struct DeployHealth {
    pub(crate) kind: String,
    pub(crate) path: String,
}

#[derive(Clone, Serialize)]
pub(crate) struct DeployOutput {
    pub(crate) kind: String,
    pub(crate) path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) fallback: Option<String>,
}

#[derive(Clone, Serialize)]
pub(crate) struct DeployBackingService {
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) mode: String,
    pub(crate) required: bool,
    pub(crate) consumers: Vec<String>,
    pub(crate) warnings: Vec<DeployWarning>,
}

#[derive(Clone, Serialize)]
pub(crate) struct DeployDomain {
    pub(crate) host: String,
    pub(crate) service: String,
    pub(crate) tls: String,
}

#[derive(Clone, Serialize)]
pub(crate) struct DeploySecret {
    pub(crate) name: String,
    pub(crate) services: Vec<String>,
    pub(crate) required: bool,
    pub(crate) source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) notes: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct DeployWarning {
    pub(crate) code: String,
    pub(crate) scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) target: Option<String>,
    pub(crate) message: String,
    pub(crate) severity: String,
}

#[derive(Serialize)]
pub(crate) struct DeployExportResult {
    pub(crate) schema: String,
    pub(crate) schema_version: u64,
    pub(crate) provider: String,
    pub(crate) plan: bool,
    pub(crate) path: String,
    pub(crate) files: Vec<String>,
    pub(crate) warnings: Vec<DeployWarning>,
}

pub(crate) fn detect_static_fallback(repo_root: &Path, dir: &str) -> Option<String> {
    let service_root = repo_root.join(dir);
    let config_names = [
        "svelte.config.js",
        "svelte.config.ts",
        "svelte.config.mjs",
        "svelte.config.cjs",
    ];
    let fallback_regex = Regex::new(r#"fallback\s*:\s*["']([^"']+)["']"#).ok()?;

    for config_name in config_names {
        let path = service_root.join(config_name);
        let Ok(contents) = fs::read_to_string(&path) else {
            continue;
        };
        if let Some(captures) = fallback_regex.captures(&contents) {
            if let Some(value) = captures.get(1) {
                return Some(value.as_str().to_owned());
            }
        }
    }

    None
}

pub(crate) fn missing_static_fallback_warning(target: &str, missing: bool) -> Vec<DeployWarning> {
    if !missing {
        return Vec::new();
    }

    vec![DeployWarning {
        code: "missing-static-fallback".to_owned(),
        scope: "service".to_owned(),
        target: Some(target.to_owned()),
        message: "No static fallback file is declared yet for provider rewrite generation"
            .to_owned(),
        severity: "warn".to_owned(),
    }]
}

pub(crate) fn collect_model_warnings(model: &DeployModel) -> Vec<DeployWarning> {
    model
        .warnings
        .iter()
        .cloned()
        .chain(
            model
                .services
                .iter()
                .flat_map(|service| service.warnings.clone()),
        )
        .collect()
}
