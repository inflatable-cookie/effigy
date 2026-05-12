use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestBundleBase {
    Path { dir: String },
    Git { url: String, r#ref: Option<String> },
    Oci { url: String },
}

#[derive(Debug, Clone, Default)]
pub struct ManifestBundleConfig {
    pub base: Option<ManifestBundleBase>,
    pub inputs: BTreeMap<String, toml::Value>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
enum ManifestBundleBaseRepr {
    Typed(ManifestBundleBaseTable),
    LegacyString(String),
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
enum ManifestBundleBaseTable {
    Path {
        dir: String,
    },
    Git {
        url: String,
        #[serde(default)]
        r#ref: Option<String>,
    },
    Oci {
        url: String,
    },
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
struct ManifestBundleConfigRepr {
    #[serde(default)]
    base: Option<ManifestBundleBaseRepr>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    base_path: Option<String>,
    #[serde(flatten)]
    inputs: BTreeMap<String, toml::Value>,
}

impl From<ManifestBundleBaseRepr> for ManifestBundleBase {
    fn from(value: ManifestBundleBaseRepr) -> Self {
        match value {
            ManifestBundleBaseRepr::LegacyString(_) => {
                unreachable!("legacy string rejected earlier")
            }
            ManifestBundleBaseRepr::Typed(table) => match table {
                ManifestBundleBaseTable::Path { dir } => Self::Path { dir },
                ManifestBundleBaseTable::Git { url, r#ref } => Self::Git { url, r#ref },
                ManifestBundleBaseTable::Oci { url } => Self::Oci { url },
            },
        }
    }
}

impl<'de> serde::Deserialize<'de> for ManifestBundleConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let repr = ManifestBundleConfigRepr::deserialize(deserializer)?;
        if repr.base_path.is_some() {
            return Err(serde::de::Error::custom(
                "`[bundle].base_path` has been removed. Use `base = { type = \"path\", dir = \"...\" }` instead.",
            ));
        }
        if repr.name.is_some() {
            return Err(serde::de::Error::custom(
                "legacy `[bundle].name` has been removed. Use `base = { type = \"path\" | \"git\" | \"oci\", ... }` instead.",
            ));
        }

        let base = match repr.base {
            Some(ManifestBundleBaseRepr::LegacyString(value)) => {
                return Err(serde::de::Error::custom(format!(
                    "string `[bundle].base` value `{value}` has been removed. Use `base = {{ type = \"path\" | \"git\" | \"oci\", ... }}` instead."
                )));
            }
            Some(base) => Some(base.into()),
            None => None,
        };

        Ok(Self {
            base,
            inputs: repr.inputs,
        })
    }
}
