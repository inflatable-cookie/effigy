use std::path::Path;

pub(crate) fn read_utf8(path: &Path) -> Result<String, std::io::Error> {
    std::fs::read_to_string(path)
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
#[path = "data_loading/tests.rs"]
mod tests;
