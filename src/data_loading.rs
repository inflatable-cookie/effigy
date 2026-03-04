use std::path::Path;

pub(crate) fn read_utf8(path: &Path) -> Result<String, std::io::Error> {
    std::fs::read_to_string(path)
}

pub(crate) fn read_utf8_or_default(path: &Path) -> String {
    read_utf8(path).unwrap_or_default()
}

pub(crate) fn parse_json<T>(raw: &str) -> Result<T, serde_json::Error>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_str::<T>(raw)
}

pub(crate) fn parse_toml<T>(raw: &str) -> Result<T, toml::de::Error>
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
