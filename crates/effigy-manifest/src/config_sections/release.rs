use indexmap::IndexMap;

#[derive(Debug, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestReleaseConfig {
    #[serde(default)]
    pub version_file: Option<String>,
    #[serde(default)]
    pub version_path: Option<String>,
    #[serde(default)]
    pub changelog: Option<String>,
    #[serde(default, rename = "pre-1-0")]
    pub pre_1_0: Option<bool>,
    #[serde(default)]
    pub initial_tag_current_version: Option<bool>,
    #[serde(default)]
    pub sync_files: Vec<String>,
    #[serde(default)]
    pub gates: IndexMap<String, ManifestReleaseGateConfig>,
    #[serde(default)]
    pub tag_format: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
pub enum ManifestReleaseGateConfig {
    Command(String),
    Detailed(ManifestReleaseGateDetails),
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestReleaseGateDetails {
    pub command: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::ManifestReleaseConfig;

    #[test]
    fn release_gates_preserve_declaration_order() {
        let parsed: ManifestReleaseConfig = toml::from_str(
            r#"
[gates]
zoo = "printf zoo"
alpha = "printf alpha"
"#,
        )
        .expect("parse release gates");
        assert_eq!(
            parsed.gates.keys().cloned().collect::<Vec<_>>(),
            vec!["zoo".to_owned(), "alpha".to_owned()]
        );
    }
}
