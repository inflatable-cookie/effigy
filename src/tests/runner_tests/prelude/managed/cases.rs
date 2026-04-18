use super::super::runtime::Path;

pub(in crate::runner::tests) enum ManagedInvocation {
    Dev,
    DevWithRepo,
    TaskWithRepo(&'static str),
}

pub(in crate::runner::tests) struct ManagedOutputCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) invocation: ManagedInvocation,
    pub(in crate::runner::tests) args: &'static [&'static str],
    pub(in crate::runner::tests) expected: &'static [&'static str],
    pub(in crate::runner::tests) expected_absent: &'static [&'static str],
    pub(in crate::runner::tests) setup: fn(&Path),
}

pub(in crate::runner::tests) struct ManagedOutputDerivedCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) invocation: ManagedInvocation,
    pub(in crate::runner::tests) args: &'static [&'static str],
    pub(in crate::runner::tests) expected: &'static [&'static str],
    pub(in crate::runner::tests) expected_absent: &'static [&'static str],
    pub(in crate::runner::tests) expected_derived: fn(&Path) -> Vec<String>,
    pub(in crate::runner::tests) setup: fn(&Path),
}

pub(in crate::runner::tests) struct ManagedInvalidDefinitionCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) manifest: &'static str,
    pub(in crate::runner::tests) expected_task: &'static str,
    pub(in crate::runner::tests) expected_process: &'static str,
    pub(in crate::runner::tests) expected_detail_substring: Option<&'static str>,
}

pub(in crate::runner::tests) struct ManagedStreamBuiltinTestCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) suite: &'static str,
    pub(in crate::runner::tests) task_ref: &'static str,
}

pub(in crate::runner::tests) struct ManagedTaskRefInvalidCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) manifest: &'static str,
    pub(in crate::runner::tests) expected_reference: &'static str,
    pub(in crate::runner::tests) expected_detail: &'static str,
}

pub(in crate::runner::tests) struct ManagedProfileNotFoundCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) invocation: ManagedInvocation,
    pub(in crate::runner::tests) args: &'static [&'static str],
    pub(in crate::runner::tests) setup: fn(&Path),
    pub(in crate::runner::tests) expected_task: &'static str,
    pub(in crate::runner::tests) expected_profile: &'static str,
    pub(in crate::runner::tests) expected_available: &'static [&'static str],
}

pub(in crate::runner::tests) struct ManagedNonZeroExitCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) invocation: ManagedInvocation,
    pub(in crate::runner::tests) args: &'static [&'static str],
    pub(in crate::runner::tests) setup: fn(&Path),
    pub(in crate::runner::tests) expected_task: &'static str,
    pub(in crate::runner::tests) expected_profile: &'static str,
    pub(in crate::runner::tests) expected_processes: &'static [(&'static str, &'static str)],
}

pub(in crate::runner::tests) struct ManagedUnlockInvocationErrorCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) args: &'static [&'static str],
    pub(in crate::runner::tests) expected: &'static [&'static str],
}

pub(in crate::runner::tests) struct ManagedUnlockSuccessCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) args: &'static [&'static str],
    pub(in crate::runner::tests) lock_files: &'static [(&'static str, &'static str)],
    pub(in crate::runner::tests) removed_lock_files: &'static [&'static str],
    pub(in crate::runner::tests) expected: &'static [&'static str],
}
