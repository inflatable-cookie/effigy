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
        let value = self.args.get(self.index).ok_or_else(|| {
            RunnerError::TaskInvocation(missing_message.to_owned())
        })?;
        self.index += 1;
        Ok(value.as_str())
    }

    pub(super) fn bool_flag(&mut self, value: &mut bool) {
        *value = true;
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
        value.parse::<T>().map_err(|_| {
            RunnerError::TaskInvocation(invalid_message(value))
        })
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

    pub(super) fn remaining(&self) -> &'a [String] {
        &self.args[self.index..]
    }
}
