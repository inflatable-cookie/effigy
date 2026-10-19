//! Scoped hooks for engine proofs using the kernel as a dependency. Available
//! only with `fault-injection`; no environment variable enables a hook.
use std::cell::RefCell;
use std::io;

type Hook = Box<dyn FnMut(&'static str) -> io::Result<()>>;
thread_local! { static HOOK: RefCell<Option<Hook>> = const { RefCell::new(None) }; }

/// Calls `run` with a thread-local fault hook, restoring the prior hook even
/// when it unwinds. The hook may record, fail or kill at a named crossing.
pub fn with_hook<T>(
    hook: impl FnMut(&'static str) -> io::Result<()> + 'static,
    run: impl FnOnce() -> T,
) -> T {
    struct Restore(Option<Hook>);
    impl Drop for Restore {
        fn drop(&mut self) {
            HOOK.with(|active| {
                active.replace(self.0.take());
            });
        }
    }
    let _restore = Restore(HOOK.with(|active| active.replace(Some(Box::new(hook)))));
    run()
}
#[doc(hidden)]
pub fn point(name: &'static str) -> io::Result<()> {
    HOOK.with(|active| match active.borrow_mut().as_mut() {
        Some(hook) => hook(name),
        None => Ok(()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_hooks_are_unarmed_by_default_and_scoped_across_unwind() {
        assert!(point("ReplaceWrite").is_ok());
        let result = std::panic::catch_unwind(|| {
            with_hook(
                |_| Err(io::Error::from_raw_os_error(libc::ENOSPC)),
                || {
                    assert_eq!(
                        point("ReplaceWrite").expect_err("injected").raw_os_error(),
                        Some(libc::ENOSPC)
                    );
                    with_hook(|_| Ok(()), || assert!(point("ReplaceWrite").is_ok()));
                    assert!(point("ReplaceWrite").is_err());
                    panic!("exercise restoration");
                },
            );
        });
        assert!(result.is_err());
        assert!(point("ReplaceWrite").is_ok());
        std::thread::spawn(|| assert!(point("ReplaceWrite").is_ok()))
            .join()
            .expect("other thread");
    }
}
