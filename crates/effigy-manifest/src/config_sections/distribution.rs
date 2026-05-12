#[derive(Debug, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestDistributionConfig {
    #[serde(default)]
    pub package: Option<ManifestDistributionPackageConfig>,
    #[serde(default)]
    pub publish: Option<ManifestDistributionPublishConfig>,
    #[serde(default)]
    pub preflight: Option<ManifestDistributionPreflightConfig>,
    #[serde(default)]
    pub metadata: Option<ManifestDistributionMetadataConfig>,
    #[serde(default)]
    pub closeout: Option<ManifestDistributionCloseoutConfig>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestDistributionPackageConfig {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub repo_url: Option<String>,
    #[serde(default)]
    pub brew_formula: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestDistributionPublishConfig {
    #[serde(default)]
    pub binary_name: Option<String>,
    #[serde(default)]
    pub registry_label: Option<String>,
    #[serde(default)]
    pub verify_tag_install: Option<bool>,
    #[serde(default)]
    pub verify_binary_json_tasks: Option<bool>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestDistributionPreflightConfig {
    #[serde(default)]
    pub docs_task: Option<String>,
    #[serde(default)]
    pub smoke_task: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestDistributionMetadataConfig {
    #[serde(default)]
    pub required_docs: Option<Vec<String>>,
    #[serde(default)]
    pub required_files: Option<Vec<String>>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestDistributionCloseoutConfig {
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub related: Option<String>,
    #[serde(default)]
    pub next_step: Option<String>,
}
