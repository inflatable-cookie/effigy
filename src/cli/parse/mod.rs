use crate::Command;

#[path = "command_parsing.rs"]
mod command_parsing;
#[path = "global_json.rs"]
mod global_json;
#[path = "value_parsing.rs"]
mod value_parsing;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliParseError {
    MissingRepoValue,
    MissingTaskNameValue,
    MissingResolveSelectorValue,
    MissingPrettyValue,
    InvalidPrettyValue(String),
    UnknownArgument(String),
}

impl std::fmt::Display for CliParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CliParseError::MissingRepoValue => write!(f, "--repo requires a value"),
            CliParseError::MissingTaskNameValue => write!(f, "--task requires a value"),
            CliParseError::MissingResolveSelectorValue => write!(f, "--resolve requires a value"),
            CliParseError::MissingPrettyValue => {
                write!(f, "--pretty requires a value (`true` or `false`)")
            }
            CliParseError::InvalidPrettyValue(value) => write!(
                f,
                "--pretty value `{value}` is invalid (expected `true` or `false`)"
            ),
            CliParseError::UnknownArgument(arg) => write!(f, "unknown argument: {arg}"),
        }
    }
}

impl std::error::Error for CliParseError {}

pub fn strip_global_json_flags(args: Vec<String>) -> (Vec<String>, bool) {
    global_json::strip_global_json_flags(args)
}

pub fn strip_global_json_flag(args: Vec<String>) -> (Vec<String>, bool) {
    strip_global_json_flags(args)
}

pub fn apply_global_json_flag(cmd: Command, json_mode: bool) -> Command {
    global_json::apply_global_json_flag(cmd, json_mode)
}

pub fn command_requests_json(cmd: &Command, global_json_mode: bool) -> bool {
    global_json::command_requests_json(cmd, global_json_mode)
}

pub fn parse_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    command_parsing::parse_command(args)
}

fn unknown_argument(arg: impl Into<String>) -> CliParseError {
    CliParseError::UnknownArgument(arg.into())
}
