use super::super::render::{
    color_enabled_for_text_output, encode_json, encode_pretty_json_optional,
};
use super::{has_builtin_help_flag, has_builtin_json_flag};
use crate::runner::error::RunnerError;

pub(super) fn builtin_output_color_enabled(output_json: bool) -> bool {
    color_enabled_for_text_output(output_json)
}

pub(super) fn render_optional_text_or_json(
    output_json: bool,
    text: String,
    payload: serde_json::Value,
) -> Result<Option<String>, RunnerError> {
    if output_json {
        return encode_pretty_json_optional(&payload);
    }
    Ok(Some(text))
}

pub(super) fn schema_payload(schema: &str, fields: serde_json::Value) -> serde_json::Value {
    schema_payload_internal(schema, true, fields)
}

pub(super) fn schema_payload_versioned(
    schema: &str,
    fields: serde_json::Value,
) -> serde_json::Value {
    schema_payload_internal(schema, false, fields)
}

fn schema_payload_internal(
    schema: &str,
    include_ok: bool,
    fields: serde_json::Value,
) -> serde_json::Value {
    let mut payload = serde_json::Map::new();
    payload.insert(
        "schema".to_owned(),
        serde_json::Value::String(schema.to_owned()),
    );
    payload.insert(
        "schema_version".to_owned(),
        serde_json::Value::Number(serde_json::Number::from(1)),
    );
    if include_ok {
        payload.insert("ok".to_owned(), serde_json::Value::Bool(true));
    }
    match fields {
        serde_json::Value::Object(fields_map) => payload.extend(fields_map),
        other => {
            payload.insert("data".to_owned(), other);
        }
    }
    serde_json::Value::Object(payload)
}

pub(super) fn render_optional_text_or_schema_json(
    output_json: bool,
    text: String,
    schema: &str,
    fields: serde_json::Value,
) -> Result<Option<String>, RunnerError> {
    render_optional_text_or_json(output_json, text, schema_payload(schema, fields))
}

pub(super) fn render_optional_text_with_schema_fields_lazy<T, P>(
    output_json: bool,
    schema: &str,
    render_text: T,
    render_fields: P,
) -> Result<Option<String>, RunnerError>
where
    T: FnOnce() -> String,
    P: FnOnce(&str) -> serde_json::Value,
{
    let text = render_text();
    if output_json {
        return encode_pretty_json_optional(&schema_payload(schema, render_fields(&text)));
    }
    Ok(Some(text))
}

pub(super) fn render_optional_text_with_schema_text_fields_lazy<T, P>(
    output_json: bool,
    schema: &str,
    render_text: T,
    render_fields: P,
) -> Result<Option<String>, RunnerError>
where
    T: FnOnce() -> String,
    P: FnOnce() -> serde_json::Value,
{
    let text = render_text();
    if output_json {
        let fields = append_text_field(render_fields(), &text);
        return encode_pretty_json_optional(&schema_payload(schema, fields));
    }
    Ok(Some(text))
}

pub(super) fn render_text_or_json_lazy<T, P>(
    output_json: bool,
    render_text: T,
    render_payload: P,
) -> Result<String, RunnerError>
where
    T: FnOnce() -> Result<String, RunnerError>,
    P: FnOnce() -> serde_json::Value,
{
    if output_json {
        return encode_json(&render_payload(), true);
    }
    render_text()
}

pub(super) fn run_help_then<H, R>(
    args: &[String],
    render_help: H,
    run: R,
) -> Result<Option<String>, RunnerError>
where
    H: FnOnce(bool) -> Result<String, RunnerError>,
    R: FnOnce() -> Result<Option<String>, RunnerError>,
{
    if has_builtin_help_flag(args) {
        return render_help(has_builtin_json_flag(args)).map(Some);
    }
    run()
}

fn append_text_field(fields: serde_json::Value, text: &str) -> serde_json::Value {
    match fields {
        serde_json::Value::Object(mut map) => {
            map.insert(
                "text".to_owned(),
                serde_json::Value::String(text.to_owned()),
            );
            serde_json::Value::Object(map)
        }
        other => serde_json::json!({
            "data": other,
            "text": text,
        }),
    }
}
