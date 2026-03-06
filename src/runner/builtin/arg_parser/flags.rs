use crate::runner::error::RunnerError;

use super::BuiltinArgParser;

impl<'a> BuiltinArgParser<'a> {
    pub(in super::super) fn consume_flag(&self, arg: &str, flag: &str, value: &mut bool) -> bool {
        if arg != flag {
            return false;
        }
        *value = true;
        true
    }

    pub(in super::super) fn consume_json_flag(&self, arg: &str, value: &mut bool) -> bool {
        self.consume_flag(arg, "--json", value)
    }

    pub(in super::super) fn consume_any_bool_flag(
        &self,
        arg: &str,
        flags: &mut [(&str, &mut bool)],
    ) -> bool {
        for (flag, value) in flags.iter_mut() {
            if arg == *flag {
                **value = true;
                return true;
            }
        }
        false
    }

    pub(in super::super) fn string_flag_value(
        &mut self,
        missing_message: &str,
    ) -> Result<String, RunnerError> {
        Ok(self.next_value(missing_message)?.to_owned())
    }

    pub(in super::super) fn context_string_flag_value(
        &mut self,
        context: &str,
        flag: &str,
    ) -> Result<String, RunnerError> {
        self.string_flag_value(&Self::context_argument_requires_value_message(
            context, flag,
        ))
    }

    pub(in super::super) fn flag_string_value(
        &mut self,
        flag: &str,
        expected: &str,
    ) -> Result<String, RunnerError> {
        self.string_flag_value(&Self::flag_requires_value_message(flag, expected))
    }

    pub(in super::super) fn required_subcommand(
        &mut self,
        builtin: &str,
        expected: &str,
    ) -> Result<&'a str, RunnerError> {
        self.next().ok_or_else(|| {
            RunnerError::task_invocation(Self::builtin_requires_subcommand_message(
                builtin, expected,
            ))
        })
    }
}
