use super::*;

pub(crate) enum ManagedInvocation {
    Dev,
    DevWithRepo,
    TaskWithRepo(&'static str),
}

pub(crate) struct ManagedOutputCase {
    pub(crate) workspace: &'static str,
    pub(crate) invocation: ManagedInvocation,
    pub(crate) args: &'static [&'static str],
    pub(crate) expected: &'static [&'static str],
    pub(crate) expected_absent: &'static [&'static str],
    pub(crate) setup: fn(&Path),
}

pub(crate) struct ManagedOutputDerivedCase {
    pub(crate) workspace: &'static str,
    pub(crate) invocation: ManagedInvocation,
    pub(crate) args: &'static [&'static str],
    pub(crate) expected: &'static [&'static str],
    pub(crate) expected_absent: &'static [&'static str],
    pub(crate) expected_derived: fn(&Path) -> Vec<String>,
    pub(crate) setup: fn(&Path),
}

pub(crate) struct ManagedInvalidDefinitionCase {
    pub(crate) workspace: &'static str,
    pub(crate) manifest: &'static str,
    pub(crate) expected_task: &'static str,
    pub(crate) expected_process: &'static str,
    pub(crate) expected_detail_substring: Option<&'static str>,
}

pub(crate) struct ManagedStreamBuiltinTestCase {
    pub(crate) workspace: &'static str,
    pub(crate) suite: &'static str,
    pub(crate) task_ref: &'static str,
}

pub(crate) struct ManagedTaskRefInvalidCase {
    pub(crate) workspace: &'static str,
    pub(crate) manifest: &'static str,
    pub(crate) expected_reference: &'static str,
    pub(crate) expected_detail: &'static str,
}

pub(crate) struct ManagedProfileNotFoundCase {
    pub(crate) workspace: &'static str,
    pub(crate) invocation: ManagedInvocation,
    pub(crate) args: &'static [&'static str],
    pub(crate) setup: fn(&Path),
    pub(crate) expected_task: &'static str,
    pub(crate) expected_profile: &'static str,
    pub(crate) expected_available: &'static [&'static str],
}

pub(crate) struct ManagedNonZeroExitCase {
    pub(crate) workspace: &'static str,
    pub(crate) invocation: ManagedInvocation,
    pub(crate) args: &'static [&'static str],
    pub(crate) setup: fn(&Path),
    pub(crate) expected_task: &'static str,
    pub(crate) expected_profile: &'static str,
    pub(crate) expected_processes: &'static [(&'static str, &'static str)],
}

pub(crate) struct ManagedUnlockInvocationErrorCase {
    pub(crate) workspace: &'static str,
    pub(crate) args: &'static [&'static str],
    pub(crate) expected: &'static [&'static str],
}

pub(crate) struct ManagedUnlockSuccessCase {
    pub(crate) workspace: &'static str,
    pub(crate) args: &'static [&'static str],
    pub(crate) lock_files: &'static [(&'static str, &'static str)],
    pub(crate) removed_lock_files: &'static [&'static str],
    pub(crate) expected: &'static [&'static str],
}
