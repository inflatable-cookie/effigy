use toml::{map::Map, Value};

use super::diagnostics::SchemaContext;
use super::tables::validate_allowed_keys;
use super::values::{
    validate_non_empty_string_or_array_of_non_empty_strings, validate_optional_boolean_field,
    validate_optional_float_field, validate_optional_integer_field,
    validate_optional_non_empty_string_field,
};

const THRESHOLD_SCAN_KEYS: &[&str] = &[
    "threshold",
    "warn",
    "high",
    "critical",
    "min_occurrences",
    "fail_on_findings",
    "respect_gitignore",
    "doctor",
    "include",
    "exclude",
    "format",
    "out",
];

const GENERATED_IN_SRC_KEYS: &[&str] = &[
    "threshold",
    "warn",
    "warn_bytes",
    "high",
    "high_bytes",
    "critical",
    "critical_bytes",
    "source_root",
    "source_roots",
    "fail_on_findings",
    "respect_gitignore",
    "doctor",
    "include",
    "exclude",
    "format",
    "out",
];

const COMMENT_RATIO_KEYS: &[&str] = &[
    "threshold",
    "warn",
    "high",
    "critical",
    "min_code_lines",
    "fail_on_findings",
    "respect_gitignore",
    "doctor",
    "include",
    "exclude",
    "format",
    "out",
];

const ATTENTION_MARKER_KEYS: &[&str] = &[
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
];

const STALE_SUPPRESSION_KEYS: &[&str] = &[
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
];

pub(super) fn validate_scan_section(context: &mut SchemaContext<'_, '_>, scan: &Value) {
    let Some(scan_table) = scan.as_table() else {
        context.unsupported_value("scan", SchemaContext::value_type(scan), "expected table");
        return;
    };
    validate_allowed_keys(
        context,
        "scan",
        scan_table,
        &[
            "god_files",
            "duplicate_blocks",
            "comment_ratio",
            "generated_assets",
        "generated_in_src",
        "attention_markers",
        "stale_suppressions",
        ],
    );
    validate_threshold_scan_section(context, scan_table.get("god_files"), "scan.god_files");
    validate_duplicate_blocks_section(
        context,
        scan_table.get("duplicate_blocks"),
        "scan.duplicate_blocks",
    );
    validate_comment_ratio_section(
        context,
        scan_table.get("comment_ratio"),
        "scan.comment_ratio",
    );
    validate_threshold_scan_section(
        context,
        scan_table.get("generated_assets"),
        "scan.generated_assets",
    );
    validate_generated_in_src_section(
        context,
        scan_table.get("generated_in_src"),
        "scan.generated_in_src",
    );
    validate_attention_markers_section(
        context,
        scan_table.get("attention_markers"),
        "scan.attention_markers",
    );
    validate_stale_suppressions_section(
        context,
        scan_table.get("stale_suppressions"),
        "scan.stale_suppressions",
    );
}

fn validate_threshold_scan_section(
    context: &mut SchemaContext<'_, '_>,
    value: Option<&Value>,
    section_path: &str,
) {
    let Some(table) = section_table(context, value, section_path, THRESHOLD_SCAN_KEYS) else {
        return;
    };

    for field in ["threshold", "warn", "high", "critical"] {
        validate_optional_integer_field(
            context,
            table.get(field),
            &field_path(section_path, field),
        );
    }

    validate_common_scan_fields(context, table, section_path);
}

fn validate_duplicate_blocks_section(
    context: &mut SchemaContext<'_, '_>,
    value: Option<&Value>,
    section_path: &str,
) {
    let Some(table) = section_table(context, value, section_path, THRESHOLD_SCAN_KEYS) else {
        return;
    };

    for field in ["threshold", "warn", "high", "critical", "min_occurrences"] {
        validate_optional_integer_field(
            context,
            table.get(field),
            &field_path(section_path, field),
        );
    }

    validate_common_scan_fields(context, table, section_path);
}

fn validate_attention_markers_section(
    context: &mut SchemaContext<'_, '_>,
    value: Option<&Value>,
    section_path: &str,
) {
    let Some(table) = section_table(context, value, section_path, ATTENTION_MARKER_KEYS) else {
        return;
    };

    for field in ["warning", "high", "critical"] {
        validate_string_or_array_field(context, table.get(field), &field_path(section_path, field));
    }

    validate_common_scan_fields(context, table, section_path);
}

fn validate_stale_suppressions_section(
    context: &mut SchemaContext<'_, '_>,
    value: Option<&Value>,
    section_path: &str,
) {
    let Some(table) = section_table(context, value, section_path, STALE_SUPPRESSION_KEYS) else {
        return;
    };

    for field in ["warning", "high", "critical"] {
        validate_string_or_array_field(context, table.get(field), &field_path(section_path, field));
    }

    validate_common_scan_fields(context, table, section_path);
}

fn validate_generated_in_src_section(
    context: &mut SchemaContext<'_, '_>,
    value: Option<&Value>,
    section_path: &str,
) {
    let Some(table) = section_table(context, value, section_path, GENERATED_IN_SRC_KEYS) else {
        return;
    };

    for field in [
        "threshold",
        "warn",
        "warn_bytes",
        "high",
        "high_bytes",
        "critical",
        "critical_bytes",
    ] {
        validate_optional_integer_field(
            context,
            table.get(field),
            &field_path(section_path, field),
        );
    }
    for field in ["source_root", "source_roots"] {
        validate_string_or_array_field(context, table.get(field), &field_path(section_path, field));
    }

    validate_common_scan_fields(context, table, section_path);
}

fn validate_comment_ratio_section(
    context: &mut SchemaContext<'_, '_>,
    value: Option<&Value>,
    section_path: &str,
) {
    let Some(table) = section_table(context, value, section_path, COMMENT_RATIO_KEYS) else {
        return;
    };

    for field in ["threshold", "warn", "high", "critical"] {
        validate_optional_float_field(context, table.get(field), &field_path(section_path, field));
    }
    validate_optional_integer_field(
        context,
        table.get("min_code_lines"),
        &field_path(section_path, "min_code_lines"),
    );

    validate_common_scan_fields(context, table, section_path);
}

fn section_table<'a>(
    context: &mut SchemaContext<'_, '_>,
    value: Option<&'a Value>,
    section_path: &str,
    allowed_keys: &[&str],
) -> Option<&'a Map<String, Value>> {
    let value = value?;
    let Some(table) = value.as_table() else {
        context.unsupported_value(
            section_path,
            SchemaContext::value_type(value),
            "expected table",
        );
        return None;
    };
    validate_allowed_keys(context, section_path, table, allowed_keys);
    Some(table)
}

fn validate_common_scan_fields(
    context: &mut SchemaContext<'_, '_>,
    table: &Map<String, Value>,
    section_path: &str,
) {
    for field in ["fail_on_findings", "respect_gitignore", "doctor"] {
        validate_optional_boolean_field(
            context,
            table.get(field),
            &field_path(section_path, field),
        );
    }

    for field in ["include", "exclude"] {
        validate_string_or_array_field(context, table.get(field), &field_path(section_path, field));
    }

    validate_format_field(
        context,
        table.get("format"),
        &field_path(section_path, "format"),
    );
    validate_out_field(context, table.get("out"), &field_path(section_path, "out"));
}

fn validate_string_or_array_field(
    context: &mut SchemaContext<'_, '_>,
    value: Option<&Value>,
    key_path: &str,
) {
    if let Some(value) = value {
        validate_non_empty_string_or_array_of_non_empty_strings(
            context,
            key_path,
            value,
            "expected string or array of strings",
        );
    }
}

fn validate_format_field(
    context: &mut SchemaContext<'_, '_>,
    value: Option<&Value>,
    key_path: &str,
) {
    let Some(value) = value else {
        return;
    };
    let Some(raw) = value.as_str() else {
        context.unsupported_value(
            key_path,
            SchemaContext::value_type(value),
            "expected one of: text, markdown",
        );
        return;
    };
    if !matches!(raw, "text" | "markdown") {
        context.unsupported_value(key_path, raw, "expected one of: text, markdown");
    }
}

fn validate_out_field(context: &mut SchemaContext<'_, '_>, value: Option<&Value>, key_path: &str) {
    validate_optional_non_empty_string_field(context, value, key_path);
}

fn field_path(section_path: &str, field: &str) -> String {
    format!("{section_path}.{field}")
}
