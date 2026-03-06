use toml::Value;

use super::diagnostics::SchemaContext;
use super::tables::validate_allowed_keys;
use super::values::{
    validate_non_empty_string_or_array_of_non_empty_strings, validate_optional_boolean_field,
    validate_optional_integer_field,
};

pub(super) fn validate_scan_section(context: &mut SchemaContext<'_, '_>, scan: &Value) {
    let Some(scan_table) = scan.as_table() else {
        context.unsupported_value("scan", SchemaContext::value_type(scan), "expected table");
        return;
    };
    validate_allowed_keys(
        context,
        "scan",
        scan_table,
        &["god_files", "generated_assets", "attention_markers"],
    );
    if let Some(god_files) = scan_table.get("god_files") {
        validate_god_files_section(context, god_files);
    }
    if let Some(generated_assets) = scan_table.get("generated_assets") {
        validate_generated_assets_section(context, generated_assets);
    }
    if let Some(attention_markers) = scan_table.get("attention_markers") {
        validate_attention_markers_section(context, attention_markers);
    }
}

fn validate_god_files_section(context: &mut SchemaContext<'_, '_>, value: &Value) {
    let Some(table) = value.as_table() else {
        context.unsupported_value(
            "scan.god_files",
            SchemaContext::value_type(value),
            "expected table",
        );
        return;
    };
    validate_allowed_keys(
        context,
        "scan.god_files",
        table,
        &[
            "threshold",
            "warn",
            "high",
            "critical",
            "fail_on_findings",
            "respect_gitignore",
            "doctor",
            "include",
            "exclude",
            "format",
            "out",
        ],
    );
    validate_optional_integer_field(context, table.get("threshold"), "scan.god_files.threshold");
    validate_optional_integer_field(context, table.get("warn"), "scan.god_files.warn");
    validate_optional_integer_field(context, table.get("high"), "scan.god_files.high");
    validate_optional_integer_field(context, table.get("critical"), "scan.god_files.critical");
    validate_optional_boolean_field(
        context,
        table.get("fail_on_findings"),
        "scan.god_files.fail_on_findings",
    );
    validate_optional_boolean_field(
        context,
        table.get("respect_gitignore"),
        "scan.god_files.respect_gitignore",
    );
    validate_optional_boolean_field(context, table.get("doctor"), "scan.god_files.doctor");
    if let Some(include) = table.get("include") {
        validate_non_empty_string_or_array_of_non_empty_strings(
            context,
            "scan.god_files.include",
            include,
            "expected string or array of strings",
        );
    }
    if let Some(exclude) = table.get("exclude") {
        validate_non_empty_string_or_array_of_non_empty_strings(
            context,
            "scan.god_files.exclude",
            exclude,
            "expected string or array of strings",
        );
    }
    if let Some(format) = table.get("format") {
        validate_format(context, format);
    }
    if let Some(out) = table.get("out") {
        validate_out_path(context, out);
    }
}

fn validate_generated_assets_section(context: &mut SchemaContext<'_, '_>, value: &Value) {
    let Some(table) = value.as_table() else {
        context.unsupported_value(
            "scan.generated_assets",
            SchemaContext::value_type(value),
            "expected table",
        );
        return;
    };
    validate_allowed_keys(
        context,
        "scan.generated_assets",
        table,
        &[
            "threshold",
            "warn",
            "high",
            "critical",
            "fail_on_findings",
            "respect_gitignore",
            "doctor",
            "include",
            "exclude",
            "format",
            "out",
        ],
    );
    validate_optional_integer_field(
        context,
        table.get("threshold"),
        "scan.generated_assets.threshold",
    );
    validate_optional_integer_field(context, table.get("warn"), "scan.generated_assets.warn");
    validate_optional_integer_field(context, table.get("high"), "scan.generated_assets.high");
    validate_optional_integer_field(
        context,
        table.get("critical"),
        "scan.generated_assets.critical",
    );
    validate_optional_boolean_field(
        context,
        table.get("fail_on_findings"),
        "scan.generated_assets.fail_on_findings",
    );
    validate_optional_boolean_field(
        context,
        table.get("respect_gitignore"),
        "scan.generated_assets.respect_gitignore",
    );
    validate_optional_boolean_field(context, table.get("doctor"), "scan.generated_assets.doctor");
    if let Some(include) = table.get("include") {
        validate_non_empty_string_or_array_of_non_empty_strings(
            context,
            "scan.generated_assets.include",
            include,
            "expected string or array of strings",
        );
    }
    if let Some(exclude) = table.get("exclude") {
        validate_non_empty_string_or_array_of_non_empty_strings(
            context,
            "scan.generated_assets.exclude",
            exclude,
            "expected string or array of strings",
        );
    }
    if let Some(format) = table.get("format") {
        validate_generated_assets_format(context, format);
    }
    if let Some(out) = table.get("out") {
        validate_generated_assets_out_path(context, out);
    }
}

fn validate_format(context: &mut SchemaContext<'_, '_>, value: &Value) {
    let Some(raw) = value.as_str() else {
        context.unsupported_value(
            "scan.god_files.format",
            SchemaContext::value_type(value),
            "expected one of: text, markdown",
        );
        return;
    };
    if !matches!(raw, "text" | "markdown") {
        context.unsupported_value(
            "scan.god_files.format",
            raw,
            "expected one of: text, markdown",
        );
    }
}

fn validate_out_path(context: &mut SchemaContext<'_, '_>, value: &Value) {
    let Some(raw) = value.as_str() else {
        context.unsupported_value(
            "scan.god_files.out",
            SchemaContext::value_type(value),
            "expected string",
        );
        return;
    };
    if raw.trim().is_empty() {
        context.unsupported_value(
            "scan.god_files.out",
            "empty string",
            "expected non-empty string",
        );
    }
}

fn validate_generated_assets_format(context: &mut SchemaContext<'_, '_>, value: &Value) {
    let Some(raw) = value.as_str() else {
        context.unsupported_value(
            "scan.generated_assets.format",
            SchemaContext::value_type(value),
            "expected one of: text, markdown",
        );
        return;
    };
    if !matches!(raw, "text" | "markdown") {
        context.unsupported_value(
            "scan.generated_assets.format",
            raw,
            "expected one of: text, markdown",
        );
    }
}

fn validate_generated_assets_out_path(context: &mut SchemaContext<'_, '_>, value: &Value) {
    let Some(raw) = value.as_str() else {
        context.unsupported_value(
            "scan.generated_assets.out",
            SchemaContext::value_type(value),
            "expected string",
        );
        return;
    };
    if raw.trim().is_empty() {
        context.unsupported_value(
            "scan.generated_assets.out",
            "empty string",
            "expected non-empty string",
        );
    }
}

fn validate_attention_markers_section(context: &mut SchemaContext<'_, '_>, value: &Value) {
    let Some(table) = value.as_table() else {
        context.unsupported_value(
            "scan.attention_markers",
            SchemaContext::value_type(value),
            "expected table",
        );
        return;
    };
    validate_allowed_keys(
        context,
        "scan.attention_markers",
        table,
        &[
            "warning",
            "high",
            "critical",
            "fail_on_findings",
            "respect_gitignore",
            "doctor",
            "include",
            "exclude",
            "format",
            "out",
        ],
    );
    if let Some(warning) = table.get("warning") {
        validate_non_empty_string_or_array_of_non_empty_strings(
            context,
            "scan.attention_markers.warning",
            warning,
            "expected string or array of strings",
        );
    }
    if let Some(high) = table.get("high") {
        validate_non_empty_string_or_array_of_non_empty_strings(
            context,
            "scan.attention_markers.high",
            high,
            "expected string or array of strings",
        );
    }
    if let Some(critical) = table.get("critical") {
        validate_non_empty_string_or_array_of_non_empty_strings(
            context,
            "scan.attention_markers.critical",
            critical,
            "expected string or array of strings",
        );
    }
    validate_optional_boolean_field(
        context,
        table.get("fail_on_findings"),
        "scan.attention_markers.fail_on_findings",
    );
    validate_optional_boolean_field(
        context,
        table.get("respect_gitignore"),
        "scan.attention_markers.respect_gitignore",
    );
    validate_optional_boolean_field(
        context,
        table.get("doctor"),
        "scan.attention_markers.doctor",
    );
    if let Some(include) = table.get("include") {
        validate_non_empty_string_or_array_of_non_empty_strings(
            context,
            "scan.attention_markers.include",
            include,
            "expected string or array of strings",
        );
    }
    if let Some(exclude) = table.get("exclude") {
        validate_non_empty_string_or_array_of_non_empty_strings(
            context,
            "scan.attention_markers.exclude",
            exclude,
            "expected string or array of strings",
        );
    }
    if let Some(format) = table.get("format") {
        let Some(raw) = format.as_str() else {
            context.unsupported_value(
                "scan.attention_markers.format",
                SchemaContext::value_type(format),
                "expected one of: text, markdown",
            );
            return;
        };
        if !matches!(raw, "text" | "markdown") {
            context.unsupported_value(
                "scan.attention_markers.format",
                raw,
                "expected one of: text, markdown",
            );
        }
    }
    if let Some(out) = table.get("out") {
        let Some(raw) = out.as_str() else {
            context.unsupported_value(
                "scan.attention_markers.out",
                SchemaContext::value_type(out),
                "expected string",
            );
            return;
        };
        if raw.trim().is_empty() {
            context.unsupported_value(
                "scan.attention_markers.out",
                "empty string",
                "expected non-empty string",
            );
        }
    }
}
