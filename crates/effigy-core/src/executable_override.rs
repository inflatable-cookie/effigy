use std::cell::RefCell;

thread_local! {
    static EXECUTABLE_OVERRIDE: RefCell<Option<String>> = const { RefCell::new(None) };
}

pub fn current() -> Option<String> {
    EXECUTABLE_OVERRIDE.with(|value| value.borrow().clone())
}

pub fn set_scoped(value: Option<String>) -> ScopedExecutableOverride {
    let previous = EXECUTABLE_OVERRIDE.with(|slot| slot.replace(value));
    ScopedExecutableOverride { previous }
}

pub struct ScopedExecutableOverride {
    previous: Option<String>,
}

impl Drop for ScopedExecutableOverride {
    fn drop(&mut self) {
        EXECUTABLE_OVERRIDE.with(|slot| {
            slot.replace(self.previous.take());
        });
    }
}
