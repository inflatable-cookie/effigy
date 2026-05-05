use std::cell::RefCell;

use effigy_context::EffigyRuntimeContext;

thread_local! {
    static ACTIVE_RUNTIME_CONTEXT: RefCell<Option<EffigyRuntimeContext>> = const { RefCell::new(None) };
}

pub(in crate::runner) fn with_runtime_context<T>(
    context: &EffigyRuntimeContext,
    run: impl FnOnce() -> T,
) -> T {
    ACTIVE_RUNTIME_CONTEXT.with(|active| {
        let previous = active.replace(Some(context.clone()));
        let output = run();
        active.replace(previous);
        output
    })
}

pub(in crate::runner) fn active_runtime_context() -> Option<EffigyRuntimeContext> {
    ACTIVE_RUNTIME_CONTEXT.with(|active| active.borrow().clone())
}
