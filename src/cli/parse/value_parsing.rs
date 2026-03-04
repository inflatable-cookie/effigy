use std::path::PathBuf;

use super::CliParseError;

pub(super) fn next_required_value<I>(
    args: &mut I,
    missing: CliParseError,
) -> Result<String, CliParseError>
where
    I: Iterator<Item = String>,
{
    args.next().ok_or(missing)
}

pub(super) fn parse_repo_path<I>(args: &mut I) -> Result<PathBuf, CliParseError>
where
    I: Iterator<Item = String>,
{
    next_required_value(args, CliParseError::MissingRepoValue).map(PathBuf::from)
}

pub(super) fn parse_pretty_bool(value: String) -> Result<bool, CliParseError> {
    parse_bool_literal(value, CliParseError::InvalidPrettyValue)
}

fn parse_bool_literal<F>(value: String, invalid: F) -> Result<bool, CliParseError>
where
    F: FnOnce(String) -> CliParseError,
{
    match value.as_str() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(invalid(value)),
    }
}
