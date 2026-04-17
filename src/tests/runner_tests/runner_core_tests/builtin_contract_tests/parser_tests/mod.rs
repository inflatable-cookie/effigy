use crate::runner::tests::prelude::TaskInvocation;

fn parser_task() -> TaskInvocation {
    TaskInvocation {
        name: "builtin-parse".to_owned(),
        args: Vec::new(),
    }
}

mod completion;
mod config;
mod unlock;
mod watch;
