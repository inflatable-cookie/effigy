use std::cell::RefCell;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(in crate::runner) enum LeaseRefreshPolicy {
    #[default]
    RefreshOnActivation,
    SkipRefresh,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(in crate::runner) enum PublicWorkspaceCleanupOverride {
    #[default]
    Default,
    ForceStopOnExit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(in crate::runner) struct RuntimeSessionContext {
    pub(in crate::runner) lease_refresh_policy: LeaseRefreshPolicy,
    pub(in crate::runner) public_workspace_cleanup: PublicWorkspaceCleanupOverride,
}

thread_local! {
    static RUNTIME_SESSION_CONTEXT_STACK: RefCell<Vec<RuntimeSessionContext>> = const {
        RefCell::new(Vec::new())
    };
}

pub(in crate::runner) fn current_runtime_session_context() -> RuntimeSessionContext {
    RUNTIME_SESSION_CONTEXT_STACK.with(|stack| stack.borrow().last().copied().unwrap_or_default())
}

pub(in crate::runner) fn with_runtime_session_context<T>(
    context: RuntimeSessionContext,
    f: impl FnOnce() -> T,
) -> T {
    struct ContextGuard;

    impl Drop for ContextGuard {
        fn drop(&mut self) {
            RUNTIME_SESSION_CONTEXT_STACK.with(|stack| {
                stack.borrow_mut().pop();
            });
        }
    }

    RUNTIME_SESSION_CONTEXT_STACK.with(|stack| {
        stack.borrow_mut().push(context);
    });
    let _guard = ContextGuard;
    f()
}

#[cfg(test)]
mod tests {
    use super::{
        current_runtime_session_context, with_runtime_session_context, LeaseRefreshPolicy,
        PublicWorkspaceCleanupOverride, RuntimeSessionContext,
    };

    #[test]
    fn runtime_session_context_defaults_when_unset() {
        assert_eq!(
            current_runtime_session_context(),
            RuntimeSessionContext::default()
        );
    }

    #[test]
    fn runtime_session_context_scopes_and_restores() {
        let outer = RuntimeSessionContext {
            lease_refresh_policy: LeaseRefreshPolicy::SkipRefresh,
            public_workspace_cleanup: PublicWorkspaceCleanupOverride::Default,
        };
        let inner = RuntimeSessionContext {
            lease_refresh_policy: LeaseRefreshPolicy::RefreshOnActivation,
            public_workspace_cleanup: PublicWorkspaceCleanupOverride::ForceStopOnExit,
        };

        with_runtime_session_context(outer, || {
            assert_eq!(current_runtime_session_context(), outer);
            with_runtime_session_context(inner, || {
                assert_eq!(current_runtime_session_context(), inner);
            });
            assert_eq!(current_runtime_session_context(), outer);
        });

        assert_eq!(
            current_runtime_session_context(),
            RuntimeSessionContext::default()
        );
    }
}
