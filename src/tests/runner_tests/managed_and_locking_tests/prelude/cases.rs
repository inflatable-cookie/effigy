pub(crate) struct ManagedPlanCase {
    pub(crate) workspace: &'static str,
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
