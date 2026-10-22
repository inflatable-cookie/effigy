use std::collections::BTreeMap;

use crate::ManifestInlineTaskDefinition;

#[derive(Debug, Clone, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestSecretsConfig {
    #[serde(default)]
    pub backend: Option<ManifestSecretsBackend>,
    #[serde(default)]
    pub vault: Option<ManifestSecretsVaultConfig>,
    #[serde(default)]
    pub external: Option<ManifestSecretsExternalConfig>,
    #[serde(default)]
    pub keys: BTreeMap<String, ManifestSecretKeyConfig>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ManifestSecretsBackend {
    EffigyVault,
    External,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
#[serde(from = "VaultConfig")]
pub struct ManifestSecretsVaultConfig {
    pub path: Option<String>,
    pub generate: Option<ManifestInlineTaskDefinition>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct VaultConfig {
    path: Option<String>,
    generate: Option<ManifestInlineTaskDefinition>,
    #[serde(
        default,
        rename = "identity",
        alias = "unlock",
        alias = "external",
        deserialize_with = "reject_vault_policy"
    )]
    _retired_policy: Option<toml::Value>,
}

impl From<VaultConfig> for ManifestSecretsVaultConfig {
    fn from(config: VaultConfig) -> Self {
        Self {
            path: config.path,
            generate: config.generate,
        }
    }
}

/// The `[secrets.vault]` policy keys retired in v0.15 (#351), in the order an
/// error names them.
pub(crate) const RETIRED_VAULT_POLICY_KEYS: [&str; 3] = ["identity", "unlock", "external"];

/// The first retired `[secrets.vault]` policy key declared in `raw`, whether it
/// is written as a table, a dotted key or an inline table.
pub(crate) fn retired_vault_policy_key(raw: &toml::Value) -> Option<&'static str> {
    let vault = raw.get("secrets")?.get("vault")?;
    RETIRED_VAULT_POLICY_KEYS
        .iter()
        .copied()
        .find(|key| vault.get(*key).is_some())
}

/// Check before composition discards source spans or overrides a retired key.
pub(crate) fn reject_retired_vault_keys(
    manifest_path: &std::path::Path,
    source: &str,
    raw: &toml::Value,
) -> Result<(), crate::ManifestError> {
    if retired_vault_policy_key(raw).is_none() {
        return Ok(());
    }
    probe_retired_vault_policy(source).map_err(|error| crate::ManifestError::Parse {
        path: manifest_path.to_path_buf(),
        error,
    })
}

/// The 1-based line of a retired `[secrets.vault]` policy key, taken from the
/// probe error's span so a bundle export names the exact line.
pub(crate) fn retired_vault_policy_line(source: &str) -> Option<usize> {
    let span = probe_retired_vault_policy(source).err()?.span()?;
    Some(source.get(..span.start)?.matches('\n').count() + 1)
}

/// Deserialize the real config shape so a retired key keeps the declaring
/// file's line and column instead of only a field name.
fn probe_retired_vault_policy(source: &str) -> Result<(), toml::de::Error> {
    #[derive(serde::Deserialize)]
    struct SecretsProbe {
        #[serde(rename = "secrets")]
        _secrets: ManifestSecretsConfig,
    }
    toml::from_str::<SecretsProbe>(source).map(|_| ())
}

fn reject_vault_policy<'de, D: serde::Deserializer<'de>>(
    _deserializer: D,
) -> Result<Option<toml::Value>, D::Error> {
    Err(serde::de::Error::custom(
        "`[secrets.vault]` identity, unlock and external keys are retired; delete this line; it only restated the passphrase default",
    ))
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestSecretsExternalConfig {
    #[serde(default)]
    pub adapter: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestSecretKeyConfig {
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub targets: Vec<ManifestSecretTarget>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ManifestSecretTarget {
    Tasks,
    Containers,
    Rhai,
    Deploy,
    State,
    Artifacts,
}
