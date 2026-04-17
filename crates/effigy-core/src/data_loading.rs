//! Small filesystem + parsing helpers shared across Effigy crates.
//!
//! These are intentionally thin wrappers around `std::fs`, `serde_json`,
//! and `toml`. They exist so domain crates can reach for a named
//! function instead of scattering `std::fs::read_to_string` / `from_str`
//! invocations. Behaviour matches the underlying crates exactly.

use std::path::Path;

pub fn read_utf8(path: &Path) -> Result<String, std::io::Error> {
    std::fs::read_to_string(path)
}

pub fn parse_json<T>(raw: &str) -> Result<T, serde_json::Error>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_str::<T>(raw)
}

pub fn parse_toml<T>(raw: &str) -> Result<T, toml::de::Error>
where
    T: serde::de::DeserializeOwned,
{
    toml::from_str::<T>(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_helpers_produce_expected_values() {
        let json = parse_json::<serde_json::Value>(r#"{"ok":true}"#).expect("parse json");
        assert_eq!(
            json.get("ok").and_then(serde_json::Value::as_bool),
            Some(true)
        );

        let toml =
            parse_toml::<toml::Value>("[tasks]\nping = \"printf ok\"\n").expect("parse toml");
        assert!(toml.get("tasks").is_some());
    }
}
