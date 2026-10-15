use toml::Value;

use super::diagnostics::SchemaContext;

pub(super) fn validate_top_level_keys(
    context: &mut SchemaContext<'_, '_>,
    table: &toml::map::Map<String, Value>,
) {
    for key in table.keys() {
        if !effigy_manifest::MANIFEST_TOP_LEVEL_KEYS.contains(&key.as_str()) {
            context.unsupported_key(key);
        }
    }
}
