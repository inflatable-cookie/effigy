//! Fragment requirements: the `[requires]` table in `service.toml`.
//!
//! A fragment names the Effigy release, fragment schema version and renderer
//! capabilities it needs. Requirements are read from the raw TOML before the
//! full schema parse, so a fragment written for a newer Effigy is refused
//! with the version it needs rather than a parse error about fields this
//! release does not know.

use semver::{Version, VersionReq};

use crate::error::CatalogError;

/// Fragment schema version this Effigy renders.
pub const SUPPORTED_FRAGMENT_SCHEMA: u32 = 1;

/// Renderer capabilities this Effigy provides to fragments.
pub const SUPPORTED_FRAGMENT_CAPABILITIES: &[&str] = &[
    "config-variants",
    "dockerfile",
    "exec-target",
    "installs-mkcert-ca",
    "loopback-alias",
    "optional-dependencies",
    "param-variants",
    "shared-service",
    "workspace-host-integration",
];

/// The running Effigy release, used when a resolver is not told otherwise.
pub fn current_effigy_version() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).expect("workspace version is semver")
}

/// Parsed `[requires]` table.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FragmentRequirements {
    /// Effigy release range the fragment supports.
    pub effigy: Option<VersionReq>,
    /// Fragment schema version the fragment is written against.
    pub schema: Option<u32>,
    /// Renderer capabilities the fragment relies on.
    pub capabilities: Vec<String>,
}

impl FragmentRequirements {
    /// Read `[requires]` from raw `service.toml` text. Other tables are not
    /// interpreted here.
    pub fn parse(service_toml: &str, fragment: &str) -> Result<Self, CatalogError> {
        let invalid = |reason: String| CatalogError::InvalidServiceToml {
            name: fragment.to_owned(),
            reason,
        };
        let table = toml::from_str::<toml::Table>(service_toml)
            .map_err(|error| invalid(error.to_string()))?;
        let Some(requires) = table.get("requires") else {
            return Ok(Self::default());
        };
        let requires = requires
            .as_table()
            .ok_or_else(|| invalid("`requires` must be a table".to_owned()))?;
        let mut parsed = Self::default();
        for (key, value) in requires {
            match key.as_str() {
                "effigy" => {
                    let text = value.as_str().ok_or_else(|| {
                        invalid("`requires.effigy` must be a semver range string".to_owned())
                    })?;
                    parsed.effigy = Some(VersionReq::parse(text).map_err(|error| {
                        invalid(format!("`requires.effigy` is not a semver range: {error}"))
                    })?);
                }
                "schema" => {
                    let schema = value
                        .as_integer()
                        .and_then(|schema| u32::try_from(schema).ok())
                        .filter(|schema| *schema > 0)
                        .ok_or_else(|| {
                            invalid("`requires.schema` must be a positive integer".to_owned())
                        })?;
                    parsed.schema = Some(schema);
                }
                "capabilities" => {
                    let list = value.as_array().ok_or_else(|| {
                        invalid("`requires.capabilities` must be a list of strings".to_owned())
                    })?;
                    for capability in list {
                        let capability = capability.as_str().ok_or_else(|| {
                            invalid("`requires.capabilities` must be a list of strings".to_owned())
                        })?;
                        parsed.capabilities.push(capability.to_owned());
                    }
                }
                // A newer Effigy may add requirement kinds. An unknown kind is
                // itself an unmet requirement, reported with the release range.
                other => parsed.capabilities.push(format!("requires.{other}")),
            }
        }
        Ok(parsed)
    }

    /// Refuse a fragment this Effigy cannot honour, naming what it needs.
    pub fn check(
        &self,
        fragment: &str,
        source: &str,
        effigy: &Version,
    ) -> Result<(), CatalogError> {
        let mut unmet = Vec::new();
        if let Some(range) = &self.effigy {
            if !range.matches(effigy) {
                unmet.push(format!("Effigy {range}"));
            }
        }
        if let Some(schema) = self.schema {
            if schema > SUPPORTED_FRAGMENT_SCHEMA {
                unmet.push(format!(
                    "fragment schema {schema} (this release renders schema {SUPPORTED_FRAGMENT_SCHEMA})"
                ));
            }
        }
        let missing = self
            .capabilities
            .iter()
            .filter(|capability| !SUPPORTED_FRAGMENT_CAPABILITIES.contains(&capability.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            unmet.push(format!("capabilities {}", missing.join(", ")));
        }
        if unmet.is_empty() {
            return Ok(());
        }
        let needs = self
            .effigy
            .as_ref()
            .map(|range| format!("Effigy {range}"))
            .unwrap_or_else(|| "a newer Effigy".to_owned());
        Err(CatalogError::UnsupportedFragment {
            name: fragment.to_owned(),
            source_handle: source.to_owned(),
            needs,
            unmet: unmet.join("; "),
            running: effigy.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(toml: &str, effigy: &str) -> Result<(), CatalogError> {
        FragmentRequirements::parse(toml, "svc").unwrap().check(
            "svc",
            "acme",
            &Version::parse(effigy).unwrap(),
        )
    }

    #[test]
    fn absent_requirements_accept_any_release() {
        assert!(check("[service]\nname = \"svc\"\n", "0.15.0").is_ok());
    }

    #[test]
    fn met_requirements_accept() {
        let toml =
            "[requires]\neffigy = \">=0.15\"\nschema = 1\ncapabilities = [\"loopback-alias\"]\n";
        assert!(check(toml, "0.15.0").is_ok());
    }

        #[test]
    fn newer_schema_and_unknown_capability_are_refused() {
        let error = check(
            "[requires]\neffigy = \">=0.17\"\nschema = 2\ncapabilities = [\"gpu\"]\n",
            "0.15.0",
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("fragment schema 2"), "{error}");
        assert!(error.contains("capabilities gpu"), "{error}");
        assert!(error.contains("needs Effigy >=0.17"), "{error}");
    }

    #[test]
    fn unknown_requirement_kinds_are_unmet() {
        let error = check("[requires]\nruntime = \"wasm\"\n", "0.15.0")
            .unwrap_err()
            .to_string();
        assert!(error.contains("requires.runtime"), "{error}");
        assert!(error.contains("a newer Effigy"), "{error}");
    }

    #[test]
    fn malformed_requirements_are_invalid() {
        for toml in [
            "requires = 1\n",
            "[requires]\neffigy = \"not a range\"\n",
            "[requires]\nschema = 0\n",
            "[requires]\ncapabilities = \"loopback-alias\"\n",
        ] {
            assert!(
                FragmentRequirements::parse(toml, "svc").is_err(),
                "{toml} should be invalid"
            );
        }
    }
}
