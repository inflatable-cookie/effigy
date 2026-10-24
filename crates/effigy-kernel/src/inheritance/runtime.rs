//! Runtime class constraints are checked again at durable effect boundaries.
use super::*;
use crate::id::RunId;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RunClass {
    Foreground,
    CheckoutDurable,
    MachineDurable,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeParent {
    purpose: String,
    pub run: RunId,
    pub class: RunClass,
    pub hook: Option<ProcessIdentity>,
    #[serde(default)]
    pub closure: Option<ClosureIdentity>,
}
impl RuntimeParent {
    pub fn new(run: RunId, class: RunClass, hook: Option<ProcessIdentity>) -> Self {
        Self {
            purpose: "effigy.runtime-parent.v1".into(),
            run,
            class,
            hook,
            closure: None,
        }
    }
    /// Binds the custody backup's anonymous cleanup pipe into this grant.
    pub fn with_closure(mut self, closure: Option<ClosureIdentity>) -> Self {
        self.closure = closure;
        self
    }
    /// Text selects the inherited descriptor; only a live native capability
    /// supplies the grant. Invalid or unknown grants are errors, never absence.
    /// A closed descriptor carries no grant at all and reads as absence.
    pub fn inherited(cx: &OperationContext) -> io::Result<Option<Self>> {
        let Some(marker) = std::env::var_os(ENV) else {
            return Ok(None);
        };
        // Once the descriptor is known lost, the slot may since hold an
        // unrelated file this process opened; never probe it again.
        if CAPABILITY_LOST.load(std::sync::atomic::Ordering::Acquire) {
            return Ok(None);
        }
        let marker = marker
            .to_str()
            .ok_or_else(|| refused("invalid runtime parent selector"))?;
        let capability = match InheritedCapability::capture(cx, marker) {
            Ok(capability) => capability,
            // The selector survived but its descriptor didn't: an intermediate
            // process (Bun, Node, Python's subprocess) closed inherited
            // descriptors. No grant arrived, so there is nothing to verify or
            // forge; refusing would block every nested Effigy behind such a
            // tool. The invocation runs as an independent one, as if the
            // variable were unset.
            Err(error) if error.raw_os_error() == Some(libc::EBADF) => {
                if !CAPABILITY_LOST.swap(true, std::sync::atomic::Ordering::AcqRel) {
                    eprintln!(
                        "[warn] {ENV} is set but its descriptor was closed by an intermediate process; running as an independent invocation"
                    );
                }
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        let grant: Self = capability.payload(cx)?;
        if grant.purpose != "effigy.runtime-parent.v1" {
            return Err(refused("unsupported runtime parent grant"));
        }
        grant.verify_hook()?;
        Ok(Some(grant))
    }
    fn verify_hook(&self) -> io::Result<()> {
        if let Some(hook) = &self.hook {
            if !matches!(ident::process_identity(hook.pid), Observed::Verified(ref current, _) if current == hook)
                || !matches!(
                    ident::process_state(hook.pid),
                    Observed::Verified(
                        ident::ProcessState::Active | ident::ProcessState::Stopped,
                        _
                    )
                )
            {
                return Err(refused("admission hook is no longer verified live"));
            }
        }
        Ok(())
    }
}
/// Set when this process found its inherited capability descriptor closed.
static CAPABILITY_LOST: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

thread_local! {
    static SCOPES: RefCell<Vec<Weak<RuntimeParent>>> = const { RefCell::new(Vec::new()) };
}
/// Thread-local lexical authority. Weak entries allow out-of-order guard
/// drops without removing a different live scope or lifting its restriction.
#[derive(Debug)]
pub struct RunScope {
    _grant: Rc<RuntimeParent>,
}
impl RunScope {
    pub fn enter(grant: RuntimeParent) -> io::Result<Self> {
        SCOPES.with(|scopes| {
            let mut scopes = scopes.borrow_mut();
            scopes.retain(|entry| entry.strong_count() != 0);
            if scopes.len() >= 128 {
                return Err(refused("runtime scope nesting exceeds 128"));
            }
            let grant = Rc::new(grant);
            scopes.push(Rc::downgrade(&grant));
            Ok(Self { _grant: grant })
        })
    }
}
/// An already-admitted durable owner keeps its cleanup authority when its
/// parent lease ends. A live foreground lexical scope still forbids journal
/// writes through handles opened before that scope was entered.
pub(crate) fn require_journal_write() -> io::Result<()> {
    SCOPES.with(|scopes| {
        if scopes
            .borrow()
            .iter()
            .filter_map(Weak::upgrade)
            .any(|grant| grant.class == RunClass::Foreground)
        {
            return Err(refused(
                "late durable effect refused: current tree is foreground",
            ));
        }
        Ok(())
    })
}

/// A foreground tree cannot open a writable journal, initialize durable
/// layout or start an import, including through a late-bound built-in.
pub fn require_durable(cx: &OperationContext) -> io::Result<()> {
    cx.check("durable runtime authority")
        .map_err(|error| io::Error::other(format!("{error:?}")))?;
    let check = |grant: &RuntimeParent| {
        grant.verify_hook()?;
        if grant.class == RunClass::Foreground {
            return Err(refused(
                "late durable effect refused: parent tree is foreground",
            ));
        }
        Ok(())
    };
    SCOPES.with(|scopes| {
        for grant in scopes.borrow().iter().filter_map(Weak::upgrade) {
            check(&grant)?;
        }
        Ok::<(), io::Error>(())
    })?;
    if let Some(grant) = RuntimeParent::inherited(cx)? {
        check(&grant)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_selector_whose_descriptor_was_closed_reads_as_no_parent() {
        // nextest runs each test in its own process, so the variable and the
        // reserved slot are this test's alone. If the slot happens to be open
        // the closed-descriptor case can't be modelled here.
        // SAFETY: F_GETFD only queries the descriptor table.
        if unsafe { libc::fcntl(CHILD_FD, libc::F_GETFD) } >= 0 {
            return;
        }
        std::env::set_var(ENV, format!("memory-v1:{CHILD_FD}:{}", "0".repeat(64)));
        let cx = super::super::tests::context(crate::ctx::Placement::Host);
        let parent = RuntimeParent::inherited(cx.operation());
        std::env::remove_var(ENV);
        assert!(matches!(parent, Ok(None)), "{parent:?}");
    }
    #[test]
    fn scopes_cannot_lift_a_live_foreground_restriction_by_drop_order() {
        let cx = super::super::tests::context(crate::ctx::Placement::Host);
        let run = RunId::new(1, [3; 10]).unwrap();
        let outer =
            RunScope::enter(RuntimeParent::new(run, RunClass::CheckoutDurable, None)).unwrap();
        let inner = RunScope::enter(RuntimeParent::new(run, RunClass::Foreground, None)).unwrap();
        drop(outer);
        assert!(require_durable(cx.operation()).is_err());
        let child =
            RunScope::enter(RuntimeParent::new(run, RunClass::MachineDurable, None)).unwrap();
        assert!(require_durable(cx.operation()).is_err());
        drop(inner);
        assert!(require_durable(cx.operation()).is_ok());
        drop(child);
    }
    #[test]
    fn foreground_scope_cannot_write_a_store_opened_before_it() {
        let fixture = tempfile::tempdir().unwrap();
        let cx = super::super::tests::context(crate::ctx::Placement::Host);
        let root = crate::root::StateRoot::effects(&cx, fixture.path()).unwrap();
        let mut store = crate::store::Store::open(&cx, &root).unwrap();
        let _scope = RunScope::enter(RuntimeParent::new(
            RunId::new(1, [5; 10]).unwrap(),
            RunClass::Foreground,
            None,
        ))
        .unwrap();
        let mut called = false;
        let result: Result<(), crate::store::StoreError> =
            store.transaction_within(&cx, crate::store::BusyWait::Context, |_| {
                called = true;
                Ok(())
            });
        assert!(result.is_err());
        assert!(!called);
    }
    #[test]
    fn foreground_scope_refuses_factories_and_store_before_durable_layout() {
        let fixture = tempfile::tempdir().unwrap();
        let cx = super::super::tests::context(crate::ctx::Placement::Host);
        let run = RunId::new(1, [4; 10]).unwrap();
        let _scope = RunScope::enter(RuntimeParent::new(run, RunClass::Foreground, None)).unwrap();
        let location = crate::migration::CheckoutLocation {
            root: fixture.path().join("missing-checkout"),
            git_directory: None,
        };
        assert!(crate::root::StateRoot::checkout_effects(
            &cx,
            &fixture.path().join("missing-home"),
            &location
        )
        .is_err());
        assert!(
            crate::root::StateRoot::machine_effects(&cx, &fixture.path().join("missing-home"))
                .is_err()
        );
        assert!(crate::root::StateRoot::runtime_machine_effects(
            &cx,
            &fixture.path().join("missing-home")
        )
        .is_err());
        assert_eq!(std::fs::read_dir(fixture.path()).unwrap().count(), 0);
        let root = crate::root::StateRoot::effects(&cx, fixture.path()).unwrap();
        assert!(crate::store::Store::open(&cx, &root).is_err());
        assert!(!root.path().join("kernel.db").exists());
    }
}
