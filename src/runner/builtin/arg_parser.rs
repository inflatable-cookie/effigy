use super::super::RunnerError;

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

    pub(super) fn remaining(&self) -> &'a [String] {
        &self.args[self.index..]
    }
}
