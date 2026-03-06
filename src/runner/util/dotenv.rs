use std::collections::BTreeMap;

pub(in crate::runner) fn parse_dotenv_entries(src: &str) -> BTreeMap<String, String> {
    let mut entries = BTreeMap::new();
    for raw_line in src.lines() {
        let mut line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(exported) = line.strip_prefix("export ") {
            line = exported.trim_start();
        }
        let Some((key_raw, value_raw)) = line.split_once('=') else {
            continue;
        };
        let key = key_raw.trim();
        if key.is_empty() {
            continue;
        }
        let value = strip_matching_quotes(value_raw.trim());
        entries.insert(key.to_owned(), value.to_owned());
    }
    entries
}

fn strip_matching_quotes(value: &str) -> &str {
    if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        &value[1..value.len() - 1]
    } else {
        value
    }
}

#[cfg(test)]
#[path = "dotenv/tests.rs"]
mod tests;
