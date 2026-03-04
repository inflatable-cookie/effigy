use std::str::FromStr;

use crate::runner::RunnerError;

use super::BuiltinArgParser;

impl BuiltinArgParser<'_> {
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
            .map_err(|_| RunnerError::task_invocation(invalid_message(value)))
    }

    pub(in super::super) fn usize_flag_value<F>(
        &mut self,
        missing_message: &str,
        invalid_message: F,
    ) -> Result<usize, RunnerError>
    where
        F: FnOnce(&str) -> String,
    {
        self.parsed_flag_value(missing_message, invalid_message)
    }

    pub(in super::super) fn u64_flag_value<F>(
        &mut self,
        missing_message: &str,
        invalid_message: F,
    ) -> Result<u64, RunnerError>
    where
        F: FnOnce(&str) -> String,
    {
        self.parsed_flag_value(missing_message, invalid_message)
    }

    pub(in super::super) fn mapped_flag_value<T, M, I>(
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
        map(value).ok_or_else(|| RunnerError::task_invocation(invalid_message(value)))
    }

    pub(in super::super) fn bool_literal_flag_value<F>(
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

    pub(in super::super) fn context_bool_literal_flag_value(
        &mut self,
        context: &str,
        flag: &str,
    ) -> Result<bool, RunnerError> {
        self.bool_literal_flag_value(
            &Self::context_argument_requires_bool_literal_value_message(context, flag),
            |value| Self::context_argument_invalid_bool_literal_value_message(context, flag, value),
        )
    }

    pub(in super::super) fn quoted_choice_flag_value<T, M>(
        &mut self,
        flag: &str,
        quoted_choices: &str,
        map: M,
    ) -> Result<T, RunnerError>
    where
        M: FnOnce(&str) -> Option<T>,
    {
        self.mapped_flag_value(
            &format!("`{flag}` requires a value ({quoted_choices})"),
            map,
            |value| format!("invalid `{flag}` value `{value}` (expected {quoted_choices})"),
        )
    }

    pub(in super::super) fn positive_u64_flag_value(
        &mut self,
        flag: &str,
        missing_message: &str,
    ) -> Result<u64, RunnerError> {
        let parsed = self.u64_flag_value(missing_message, |value| {
            format!("invalid `{flag}` value `{value}` (expected a positive integer)")
        })?;
        if parsed == 0 {
            return Err(RunnerError::task_invocation(format!(
                "`{flag}` must be greater than zero"
            )));
        }
        Ok(parsed)
    }

    pub(in super::super) fn positive_usize_flag_value(
        &mut self,
        flag: &str,
        missing_message: &str,
    ) -> Result<usize, RunnerError> {
        let parsed = self.usize_flag_value(missing_message, |value| {
            format!("invalid `{flag}` value `{value}` (expected an integer >= 1)")
        })?;
        if parsed == 0 {
            return Err(RunnerError::task_invocation(format!(
                "`{flag}` must be greater than zero"
            )));
        }
        Ok(parsed)
    }
}
