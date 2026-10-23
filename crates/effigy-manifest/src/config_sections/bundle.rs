use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum ManifestBundleBase {
    Path { dir: String },
    Git { url: String, r#ref: Option<String> },
    Oci { url: String },
}

/// How the Prepare paths treat a pin that is behind its declaration or its
/// upstream (contract 020). `Auto` is the default: `dev`, `bootstrap` and
/// `container up` refresh it themselves. `Manual` restores the strict rule
/// where only `effigy bundle sync` moves selected bytes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ManifestSourceUpdate {
    #[default]
    Auto,
    Manual,
}

impl ManifestSourceUpdate {
    pub fn is_manual(self) -> bool {
        matches!(self, Self::Manual)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ManifestBundleConfig {
    pub base: Option<ManifestBundleBase>,
    pub update: ManifestSourceUpdate,
    pub inputs: BTreeMap<String, toml::Value>,
}

/// One `[catalog.sources.<name>]` table: the typed source plus its own update
/// policy, so a project can keep one catalog source on explicit sync only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestCatalogSourceConfig {
    pub base: ManifestBundleBase,
    pub update: ManifestSourceUpdate,
}

impl serde::Serialize for ManifestCatalogSourceConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let value = toml::Value::try_from(&self.base).map_err(serde::ser::Error::custom)?;
        let toml::Value::Table(table) = value else {
            return Err(serde::ser::Error::custom(
                "a source declaration serializes as a table",
            ));
        };
        let mut map = serializer.serialize_map(None)?;
        for (key, value) in table {
            map.serialize_entry(&key, &value)?;
        }
        map.serialize_entry("update", &self.update)?;
        map.end()
    }
}

impl<'de> serde::Deserialize<'de> for ManifestCatalogSourceConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let mut table = <toml::Table>::deserialize(deserializer)?;
        let update = match table.remove("update") {
            Some(value) => value
                .try_into::<ManifestSourceUpdate>()
                .map_err(serde::de::Error::custom)?,
            None => ManifestSourceUpdate::default(),
        };
        let base: ManifestBundleBase = toml::Value::Table(table)
            .try_into()
            .map_err(serde::de::Error::custom)?;
        Ok(Self { base, update })
    }
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
    update: Option<ManifestSourceUpdate>,
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
            update: repr.update.unwrap_or_default(),
            inputs: repr.inputs,
        })
    }
}
