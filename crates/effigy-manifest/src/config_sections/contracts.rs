//! `[contracts]` — the project's own JSON contract paths.
//!
//! `effigy contracts check-json` reads these instead of assuming Effigy's
//! documentation tree, so any project validates its JSON artifacts against its
//! own schema index and selection contract (contract 054).

#[derive(Debug, Clone, serde::Deserialize, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ManifestContractsConfig {
    /// Project-relative path to the JSON schema index.
    #[serde(default, alias = "schema-index")]
    pub schema_index: Option<String>,
    /// Project-relative path to the selection payload contract.
    #[serde(default, alias = "selection-contract")]
    pub selection_contract: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::ManifestContractsConfig;

    #[test]
    fn contracts_config_accepts_declared_paths() {
        let parsed: ManifestContractsConfig = toml::from_str(
            r#"
schema_index = "docs/knowledge/contracts/json-schema-index.json"
selection_contract = "docs/knowledge/contracts/json-selection-contract.json"
"#,
        )
        .expect("parse contracts");

        assert_eq!(
            parsed.schema_index.as_deref(),
            Some("docs/knowledge/contracts/json-schema-index.json")
        );
        assert_eq!(
            parsed.selection_contract.as_deref(),
            Some("docs/knowledge/contracts/json-selection-contract.json")
        );
    }

    #[test]
    fn contracts_config_accepts_kebab_case_keys() {
        let parsed: ManifestContractsConfig = toml::from_str(
            r#"
schema-index = "index.json"
selection-contract = "selection.json"
"#,
        )
        .expect("parse kebab-case contracts");

        assert_eq!(parsed.schema_index.as_deref(), Some("index.json"));
        assert_eq!(parsed.selection_contract.as_deref(), Some("selection.json"));
    }

    #[test]
    fn contracts_config_rejects_unknown_keys() {
        let result: Result<ManifestContractsConfig, _> = toml::from_str(
            r#"
schema_index = "index.json"
artifact = "selected.json"
"#,
        );

        assert!(result.is_err(), "expected unknown contracts key rejection");
    }
}
