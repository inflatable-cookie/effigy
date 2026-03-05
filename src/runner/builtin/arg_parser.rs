use super::super::RunnerError;
use super::ensure_no_unknown_builtin_args;
use crate::TaskInvocation;

mod flags;
mod messages;
mod values;

pub(super) struct BuiltinArgParser<'a> {
    args: &'a [String],
    index: usize,
}

pub(in super::super) enum ParseLoopAction {
    Handled,
    Unknown,
    Break,
}

impl<'a> BuiltinArgParser<'a> {
    pub(super) fn new(args: &'a [String]) -> Self {
        Self { args, index: 0 }
    }

    pub(super) fn next(&mut self) -> Option<&'a str> {
        let arg = self.args.get(self.index)?;
        self.index += 1;
        Some(arg.as_str())
    }

    pub(super) fn first_positional_arg(args: &[String]) -> Option<&str> {
        args.iter()
            .find(|arg| !arg.starts_with('-'))
            .map(String::as_str)
    }

    pub(super) fn next_value(&mut self, missing_message: &str) -> Result<&'a str, RunnerError> {
        let value = self
            .args
            .get(self.index)
            .ok_or_else(|| RunnerError::task_invocation(missing_message))?;
        self.index += 1;
        Ok(value.as_str())
    }

    pub(super) fn remaining(&self) -> &'a [String] {
        &self.args[self.index..]
    }

    pub(in super::super) fn positional_task_invocation(&self, name: &str) -> TaskInvocation {
        TaskInvocation {
            name: name.to_owned(),
            args: self.remaining().to_vec(),
        }
    }

    pub(in super::super) fn parse_loop_collect_unknown<F>(
        &mut self,
        mut on_arg: F,
    ) -> Result<Vec<String>, RunnerError>
    where
        F: FnMut(&mut Self, &str) -> Result<ParseLoopAction, RunnerError>,
    {
        let mut unknown = Vec::<String>::new();
        while let Some(arg) = self.next() {
            match on_arg(self, arg)? {
                ParseLoopAction::Handled => {}
                ParseLoopAction::Unknown => unknown.push(arg.to_owned()),
                ParseLoopAction::Break => break,
            }
        }
        Ok(unknown)
    }

    pub(in super::super) fn parse_loop_require_no_unknown<F>(
        &mut self,
        task_name: &str,
        on_arg: F,
    ) -> Result<(), RunnerError>
    where
        F: FnMut(&mut Self, &str) -> Result<ParseLoopAction, RunnerError>,
    {
        let unknown = self.parse_loop_collect_unknown(on_arg)?;
        ensure_no_unknown_builtin_args(task_name, &unknown)
    }

    pub(in super::super) fn unknown_if_flag_or<F>(
        &self,
        arg: &str,
        on_positional: F,
    ) -> Result<ParseLoopAction, RunnerError>
    where
        F: FnOnce(&str) -> Result<ParseLoopAction, RunnerError>,
    {
        if arg.starts_with('-') {
            return Ok(ParseLoopAction::Unknown);
        }
        on_positional(arg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::RunnerError;

    #[test]
    fn context_string_flag_value_missing_message_contract_is_stable() {
        let mut parser = BuiltinArgParser::new(&[]);
        let err = parser
            .context_string_flag_value("task", "--task")
            .expect_err("missing flag value should fail");
        assert_task_invocation(err, "task argument --task requires a value");
    }

    #[test]
    fn context_bool_literal_flag_value_message_contract_is_stable() {
        let mut parser = BuiltinArgParser::new(&[]);
        let err = parser
            .context_bool_literal_flag_value("catalogs", "--pretty")
            .expect_err("missing bool literal should fail");
        assert_task_invocation(
            err,
            "catalogs argument --pretty requires a value (`true` or `false`)",
        );

        let args = vec!["invalid".to_owned()];
        let mut invalid_parser = BuiltinArgParser::new(&args);
        let err = invalid_parser
            .context_bool_literal_flag_value("catalogs", "--pretty")
            .expect_err("invalid bool literal should fail");
        assert_task_invocation(
            err,
            "catalogs argument --pretty value `invalid` is invalid (expected `true` or `false`)",
        );
    }

    #[test]
    fn builtin_string_flag_value_missing_message_contract_is_stable() {
        let mut parser = BuiltinArgParser::new(&[]);
        let err = parser
            .builtin_string_flag_value("config", "--target")
            .expect_err("missing builtin string value should fail");
        assert_task_invocation(err, "`--target` requires a value for built-in `config`");
    }

    #[test]
    fn quoted_choice_flag_value_message_contract_is_stable() {
        let mut parser = BuiltinArgParser::new(&[]);
        let err = parser
            .quoted_choice_flag_value("--owner", "`effigy` or `external`", |_| None::<()>)
            .expect_err("missing enum choice should fail");
        assert_task_invocation(err, "`--owner` requires a value (`effigy` or `external`)");

        let args = vec!["wat".to_owned()];
        let mut invalid_parser = BuiltinArgParser::new(&args);
        let err = invalid_parser
            .quoted_choice_flag_value("--owner", "`effigy` or `external`", |value| match value {
                "effigy" | "external" => Some(()),
                _ => None,
            })
            .expect_err("invalid enum choice should fail");
        assert_task_invocation(
            err,
            "invalid `--owner` value `wat` (expected `effigy` or `external`)",
        );
    }

    #[test]
    fn flag_string_value_missing_message_contract_is_stable() {
        let mut parser = BuiltinArgParser::new(&[]);
        let err = parser
            .flag_string_value("--from", "a file path")
            .expect_err("missing flag value should fail");
        assert_task_invocation(err, "`--from` requires a file path");
    }

    #[test]
    fn first_positional_arg_contract_is_stable() {
        let args = vec![
            "--json".to_owned(),
            "--pretty".to_owned(),
            "candidates".to_owned(),
            "--prefix".to_owned(),
        ];
        assert_eq!(
            BuiltinArgParser::first_positional_arg(&args),
            Some("candidates")
        );
    }

    #[test]
    fn required_and_unknown_subcommand_message_contracts_are_stable() {
        let mut parser = BuiltinArgParser::new(&[]);
        let err = parser
            .required_subcommand("cache", "`inspect` or `invalidate`")
            .expect_err("missing subcommand should fail");
        assert_task_invocation(
            err,
            "`cache` requires a subcommand: `inspect` or `invalidate`",
        );

        assert_eq!(
            BuiltinArgParser::builtin_unknown_subcommand_message(
                "cache",
                "drop",
                "`inspect` or `invalidate`",
            ),
            "unknown cache subcommand `drop` (expected `inspect` or `invalidate`)",
        );
    }

    #[test]
    fn parse_loop_collect_unknown_contract_is_stable() {
        let args = vec![
            "--json".to_owned(),
            "--wat".to_owned(),
            "target".to_owned(),
            "--after-break".to_owned(),
        ];
        let mut parser = BuiltinArgParser::new(&args);
        let mut json = false;
        let mut target = None::<String>;
        let unknown = parser
            .parse_loop_collect_unknown(|parser, arg| {
                if parser.consume_json_flag(arg, &mut json) {
                    return Ok(ParseLoopAction::Handled);
                }
                if arg.starts_with('-') {
                    return Ok(ParseLoopAction::Unknown);
                }
                target = Some(arg.to_owned());
                Ok(ParseLoopAction::Break)
            })
            .expect("parse loop should succeed");

        assert!(json);
        assert_eq!(target.as_deref(), Some("target"));
        assert_eq!(unknown, vec!["--wat".to_owned()]);
        assert_eq!(parser.remaining(), &["--after-break".to_owned()]);
    }

    #[test]
    fn positional_task_invocation_contract_is_stable() {
        let args = vec!["target".to_owned(), "--flag".to_owned(), "value".to_owned()];
        let mut parser = BuiltinArgParser::new(&args);
        let name = parser.next().expect("first positional arg");
        let invocation = parser.positional_task_invocation(name);
        assert_eq!(invocation.name, "target");
        assert_eq!(
            invocation.args,
            vec!["--flag".to_owned(), "value".to_owned()]
        );
    }

    #[test]
    fn unknown_if_flag_or_contract_is_stable() {
        let parser = BuiltinArgParser::new(&[]);

        let unknown = parser
            .unknown_if_flag_or("--wat", |_| Ok(ParseLoopAction::Handled))
            .expect("flag path should succeed");
        assert!(matches!(unknown, ParseLoopAction::Unknown));

        let handled = parser
            .unknown_if_flag_or("target", |_| Ok(ParseLoopAction::Handled))
            .expect("positional path should succeed");
        assert!(matches!(handled, ParseLoopAction::Handled));
    }

    fn assert_task_invocation(error: RunnerError, expected: &str) {
        match error {
            RunnerError::TaskInvocation(message) => assert_eq!(message, expected),
            other => panic!("expected RunnerError::TaskInvocation, received: {other}"),
        }
    }
}
