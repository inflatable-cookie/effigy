use std::collections::BTreeMap;

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
#[serde(deny_unknown_fields)]
pub struct ManifestSecretsVaultConfig {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub identity: Option<ManifestSecretsVaultIdentity>,
    #[serde(default)]
    pub unlock: Option<ManifestSecretsUnlockPolicy>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ManifestSecretsVaultIdentity {
    SshAgent,
    Passphrase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ManifestSecretsUnlockPolicy {
    Passphrase,
    KeyAndPassphrase,
    External,
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
