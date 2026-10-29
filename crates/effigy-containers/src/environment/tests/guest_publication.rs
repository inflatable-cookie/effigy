//! Additive publication is an obligation of the same gated activation.
use super::*;
use effigy_kernel::run::{Attachments, Fence, GuestPublicationReceipt, GUEST_PUBLICATION_KIND};

fn receipt(fence: &Fence, generation: u64) -> GuestPublicationReceipt {
    GuestPublicationReceipt {
        environment: fence.clone(),
        container_id: "a".repeat(64),
        vm_boot_id: "boot-a".into(),
        artifact_digest: format!("sha256:{}", "b".repeat(64)),
        context_generation: generation,
    }
}

fn assert_intent(ctx: &EnvContext<'_>, id: RunId, fence: &Fence) {
    let reader = Store::read(&ctx.observe(), ctx.root);
    let effects = effigy_kernel::run::effects(reader.verified().unwrap(), id);
    let effects = effects.verified().unwrap();
    let effect = effects
        .iter()
        .find(|e| e.spec.kind == GUEST_PUBLICATION_KIND)
        .unwrap();
    assert!(effect.spec.required);
    assert_eq!(&effect.spec.target, fence);
    assert_eq!(effect.state, effigy_kernel::run::EffectState::Intended);
}

#[test]
fn guest_publication_preserves_live_holders_but_never_authorizes_a_transition() {
    let home = home();
    let root = root(home.path());
    let ctx = EnvContext::new(&root, CancelToken::default()).unwrap();
    let stack = env("publication");
    activate(&ctx, &stack, &[]).unwrap();
    let first = EnvRecord::read(&ctx.observe(), &root, &stack.resource)
        .verified()
        .unwrap()
        .generation;
    let mut old = holder(&ctx, "old", 31);
    let old_fence = Fence {
        resource: stack.resource.clone(),
        generation: first,
    };
    Attachments::attach(&mut old, &old_fence, AttachMode::Borrow, Wait::FailFast).unwrap();
    let selected = old
        .effects()
        .unwrap()
        .into_iter()
        .find(|effect| effect.spec.kind == "kernel:attachment")
        .unwrap()
        .spec
        .target;
    let new = holder(&ctx, "new", 32);
    let mut published_run = None;
    activate_with(&ctx, &stack, &[(new.id(), AttachMode::Borrow)], |run| {
        let id = run.id();
        published_run = Some(id);
        run.publish_guest_with(|fence| {
            assert_intent(&ctx, id, fence);
            // The new holder is not attached until the receipt completes.
            assert_eq!(run_holder_count(&ctx, &stack), 1);
            Ok(receipt(fence, 1))
        })
    })
    .unwrap();
    assert_eq!(
        old.effects()
            .unwrap()
            .into_iter()
            .find(|effect| effect.spec.kind == "kernel:attachment")
            .unwrap()
            .spec
            .target,
        selected
    );
    assert_eq!(run_holder_count(&ctx, &stack), 2);
    let reader = Store::read(&ctx.observe(), &root);
    let effects = effigy_kernel::run::effects(reader.verified().unwrap(), published_run.unwrap());
    let effects = effects.verified().unwrap();
    let publication = effects
        .iter()
        .find(|e| e.spec.kind == GUEST_PUBLICATION_KIND)
        .unwrap();
    assert_eq!(
        publication.receipt.as_ref().unwrap()["container_id"],
        "a".repeat(64)
    );
    let mut stop = EnvRun::begin(&ctx, &stack, Operation::Down, Wait::FailFast).unwrap();
    assert!(stop
        .stop_with(&selection(), PROJECT, None, || panic!(
            "live holders forbid stop"
        ))
        .is_err());
    stop.finish(false).unwrap();
    for mut run in [old, new] {
        run.finish(WorkloadOutcome::Succeeded).unwrap();
        run.settle(&settlement(&ctx), &mut EnvSettlement).unwrap();
    }
}

fn run_holder_count(ctx: &EnvContext<'_>, stack: &EnvRef) -> usize {
    match EnvRecord::read(&ctx.observe(), ctx.root, &stack.resource) {
        Observed::Verified(record, _) => record.holders.verified().unwrap().len(),
        other => panic!("environment record {other:?}"),
    }
}

#[test]
fn guest_publication_uncertainty_and_interruption_never_activate_or_replay() {
    let home = home();
    let root = root(home.path());
    let ctx = EnvContext::new(&root, CancelToken::default()).unwrap();
    let stack = env("uncertain-publication");
    let calls = std::cell::Cell::new(0);
    let result = activate_with(&ctx, &stack, &[], |run| {
        run.publish_guest_with(|_| {
            calls.set(calls.get() + 1);
            Err(BackendError::Uncertain(
                "pointer switch receipt lost".into(),
            ))
        })
    });
    assert!(result.is_err());
    assert_ne!(
        EnvRecord::read(&ctx.observe(), &root, &stack.resource)
            .verified()
            .unwrap()
            .state,
        EnvState::Active
    );
    let crashed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut run = EnvRun::begin(&ctx, &stack, Operation::Up, Wait::FailFast).unwrap();
        run.publish_guest_with(|_| {
            calls.set(calls.get() + 1);
            panic!("interrupt after additive bytes, before receipt")
        })
        .unwrap();
    }));
    assert!(crashed.is_err());
    assert_ne!(
        EnvRecord::read(&ctx.observe(), &root, &stack.resource)
            .verified()
            .unwrap()
            .state,
        EnvState::Active
    );
    activate_with(&ctx, &stack, &[], |run| {
        run.publish_guest_with(|fence| {
            assert_eq!(
                calls.get(),
                2,
                "recovery must not replay either publication"
            );
            calls.set(3);
            Ok(receipt(fence, 3))
        })
    })
    .unwrap();
    assert_eq!(calls.get(), 3);
    assert_eq!(
        EnvRecord::read(&ctx.observe(), &root, &stack.resource)
            .verified()
            .unwrap()
            .state,
        EnvState::Active
    );
}

#[test]
fn guest_publication_serializes_two_prepares_with_down_and_replacement() {
    use std::sync::{mpsc, Arc, Mutex};
    let home = home();
    let root = root(home.path());
    let stack = env("serial-publication");
    let events = Arc::new(Mutex::new(Vec::new()));
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (attempt_tx, attempt_rx) = mpsc::channel();
    std::thread::scope(|scope| {
        let first_events = events.clone();
        let root = &root;
        let stack = &stack;
        let first = scope.spawn(move || {
            let ctx = EnvContext::new(root, CancelToken::default()).unwrap();
            activate_with(&ctx, stack, &[], |run| {
                let id = run.id();
                run.publish_guest_with(|fence| {
                    assert_intent(&ctx, id, fence);
                    first_events.lock().unwrap().push("prepare-1");
                    entered_tx.send(()).unwrap();
                    release_rx
                        .recv_timeout(std::time::Duration::from_secs(10))
                        .unwrap();
                    first_events.lock().unwrap().push("receipt-1");
                    Ok(receipt(fence, 1))
                })
            })
            .unwrap();
        });
        entered_rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        let next_events = events.clone();
        let second = scope.spawn(move || {
            let ctx = EnvContext::new(root, CancelToken::default()).unwrap();
            attempt_tx.send(()).unwrap();
            activate_with(&ctx, stack, &[], |run| {
                run.publish_guest_with(|fence| {
                    next_events.lock().unwrap().push("prepare-2");
                    Ok(receipt(fence, 2))
                })
            })
            .unwrap();
        });
        attempt_rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        let ctx = EnvContext::new(root, CancelToken::default()).unwrap();
        // Both a stop and a replacement use this same owner gate.
        for op in [Operation::Down, Operation::Up] {
            assert!(matches!(
                EnvRun::begin(&ctx, stack, op, Wait::FailFast),
                Err(EnvError::Busy(_))
            ));
        }
        assert_eq!(*events.lock().unwrap(), ["prepare-1"]);
        release_tx.send(()).unwrap();
        first.join().unwrap();
        second.join().unwrap();
    });
    assert_eq!(
        *events.lock().unwrap(),
        ["prepare-1", "receipt-1", "prepare-2"]
    );
    let ctx = EnvContext::new(&root, CancelToken::default()).unwrap();
    down(
        &ctx,
        &stack,
        Operation::Down,
        &selection(),
        PROJECT,
        None,
        || {
            events.lock().unwrap().push("down");
            Ok(())
        },
    )
    .unwrap();
    activate_with(&ctx, &stack, &[], |run| {
        run.publish_guest_with(|fence| {
            let mut replaced = receipt(fence, 3);
            replaced.container_id = "c".repeat(64);
            replaced.vm_boot_id = "boot-b".into();
            events.lock().unwrap().push("replacement");
            Ok(replaced)
        })
    })
    .unwrap();
    assert_eq!(
        *events.lock().unwrap(),
        ["prepare-1", "receipt-1", "prepare-2", "down", "replacement"]
    );
}
