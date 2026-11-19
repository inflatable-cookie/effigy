//! `[contracts]` — the project's own JSON contract paths.
//!
//! `effigy contracts check-json` reads these instead of assuming Effigy's
//! documentation tree, so any project validates its JSON artifacts against its
//! own schema index and selection contract (contract 055).

#[derive(Debug, Clone, serde::Deserialize, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ManifestContractsConfig {
    /// Project-relative path to the JSON schema index.
    #[serde(default)]
    pub schema_index: Option<String>,
    /// Project-relative path to the selection payload contract.
    #[serde(default)]
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
