use super::super::RunnerError;
use std::str::FromStr;

pub(super) struct BuiltinArgParser<'a> {
    args: &'a [String],
    index: usize,
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

    pub(super) fn next_value(&mut self, missing_message: &str) -> Result<&'a str, RunnerError> {
        let value = self
            .args
            .get(self.index)
            .ok_or_else(|| RunnerError::TaskInvocation(missing_message.to_owned()))?;
        self.index += 1;
        Ok(value.as_str())
    }

    pub(super) fn consume_flag(&self, arg: &str, flag: &str, value: &mut bool) -> bool {
        if arg != flag {
            return false;
        }
        *value = true;
        true
    }

    pub(super) fn consume_json_flag(&self, arg: &str, value: &mut bool) -> bool {
        self.consume_flag(arg, "--json", value)
    }

    pub(super) fn consume_help_flag(&self, arg: &str, value: &mut bool) -> bool {
        if arg == "--help" || arg == "-h" {
            *value = true;
            return true;
        }
        false
    }

    pub(super) fn string_flag_value(
        &mut self,
        missing_message: &str,
    ) -> Result<String, RunnerError> {
        Ok(self.next_value(missing_message)?.to_owned())
    }

    fn parsed_flag_value<T, F>(
        &mut self,
        missing_message: &str,
        invalid_message: F,
    ) -> Result<T, RunnerError>
    where
        T: FromStr,
        F: FnOnce(&str) -> String,
    {
        let value = self.next_value(missing_message)?;
        value
            .parse::<T>()
            .map_err(|_| RunnerError::TaskInvocation(invalid_message(value)))
    }

    pub(super) fn usize_flag_value<F>(
        &mut self,
        missing_message: &str,
        invalid_message: F,
    ) -> Result<usize, RunnerError>
    where
        F: FnOnce(&str) -> String,
    {
        self.parsed_flag_value(missing_message, invalid_message)
    }

    pub(super) fn u64_flag_value<F>(
        &mut self,
        missing_message: &str,
        invalid_message: F,
    ) -> Result<u64, RunnerError>
    where
        F: FnOnce(&str) -> String,
    {
        self.parsed_flag_value(missing_message, invalid_message)
    }

    pub(super) fn mapped_flag_value<T, M, I>(
        &mut self,
        missing_message: &str,
        map: M,
        invalid_message: I,
    ) -> Result<T, RunnerError>
    where
        M: FnOnce(&str) -> Option<T>,
        I: FnOnce(&str) -> String,
    {
        let value = self.next_value(missing_message)?;
        map(value).ok_or_else(|| RunnerError::TaskInvocation(invalid_message(value)))
    }

    pub(super) fn bool_literal_flag_value<F>(
        &mut self,
        missing_message: &str,
        invalid_message: F,
    ) -> Result<bool, RunnerError>
    where
        F: FnOnce(&str) -> String,
    {
        self.mapped_flag_value(
            missing_message,
            |value| match value {
                "true" => Some(true),
                "false" => Some(false),
                _ => None,
            },
            invalid_message,
        )
    }

    pub(super) fn positive_u64_flag_value(
        &mut self,
        flag: &str,
        missing_message: &str,
    ) -> Result<u64, RunnerError> {
        let parsed = self.u64_flag_value(missing_message, |value| {
            format!("invalid `{flag}` value `{value}` (expected a positive integer)")
        })?;
        if parsed == 0 {
            return Err(RunnerError::TaskInvocation(format!(
                "`{flag}` must be greater than zero"
            )));
        }
        Ok(parsed)
    }

    pub(super) fn positive_usize_flag_value(
        &mut self,
        flag: &str,
        missing_message: &str,
    ) -> Result<usize, RunnerError> {
        let parsed = self.usize_flag_value(missing_message, |value| {
            format!("invalid `{flag}` value `{value}` (expected an integer >= 1)")
        })?;
        if parsed == 0 {
            return Err(RunnerError::TaskInvocation(format!(
                "`{flag}` must be greater than zero"
            )));
        }
        Ok(parsed)
    }

    pub(super) fn remaining(&self) -> &'a [String] {
        &self.args[self.index..]
    }
}
