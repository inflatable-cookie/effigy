use super::{ensure_no_unknown_builtin_args, ensure_no_unknown_builtin_args_with_prefix};
use crate::runner::error::RunnerError;
use effigy_cli::TaskInvocation;

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

    pub(in super::super) fn parse_loop_require_no_unknown_with_prefix<F>(
        &mut self,
        task_name: &str,
        prefix: &str,
        on_arg: F,
    ) -> Result<(), RunnerError>
    where
        F: FnMut(&mut Self, &str) -> Result<ParseLoopAction, RunnerError>,
    {
        let unknown = self.parse_loop_collect_unknown(on_arg)?;
        ensure_no_unknown_builtin_args_with_prefix(task_name, prefix, &unknown)
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
mod tests;
