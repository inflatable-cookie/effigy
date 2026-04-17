use super::BuiltinArgParser;

impl BuiltinArgParser<'_> {
    pub(crate) fn context_argument_requires_value_message(context: &str, flag: &str) -> String {
        format!("{context} argument {flag} requires a value")
    }

    pub(crate) fn context_argument_requires_bool_literal_value_message(
        context: &str,
        flag: &str,
    ) -> String {
        format!("{context} argument {flag} requires a value (`true` or `false`)")
    }

    pub(crate) fn context_argument_invalid_bool_literal_value_message(
        context: &str,
        flag: &str,
        value: &str,
    ) -> String {
        format!("{context} argument {flag} value `{value}` is invalid (expected `true` or `false`)")
    }

    pub(crate) fn builtin_flag_requires_value_message(builtin: &str, flag: &str) -> String {
        format!("`{flag}` requires a value for built-in `{builtin}`")
    }

    pub(crate) fn flag_requires_value_message(flag: &str, expected: &str) -> String {
        format!("`{flag}` requires {expected}")
    }

    pub(crate) fn builtin_requires_subcommand_message(builtin: &str, expected: &str) -> String {
        format!("`{builtin}` requires a subcommand: {expected}")
    }

    pub(crate) fn builtin_unknown_subcommand_message(
        builtin: &str,
        subcommand: &str,
        expected: &str,
    ) -> String {
        format!("unknown {builtin} subcommand `{subcommand}` (expected {expected})")
    }
}
