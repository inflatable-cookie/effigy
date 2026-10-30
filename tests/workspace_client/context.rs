// Included only in context's existing cfg(test) module. Production visibility
// and the module's real reader/verification paths remain unchanged.
use super::*;

#[derive(Default)]
struct NoReads {
    calls: Vec<String>,
}
impl GuestReader for NoReads {
    fn read_file(
        &mut self,
        path: &str,
    ) -> Result<Option<Vec<u8>>, crate::runner::error::RunnerError> {
        self.calls.push(path.into());
        panic!("unexpected guest read, including credential/state/vault: {path}")
    }
    fn file_sha256(
        &mut self,
        path: &str,
    ) -> Result<Option<String>, crate::runner::error::RunnerError> {
        self.calls.push(path.into());
        panic!("unexpected guest digest read: {path}")
    }
}
impl CheckoutReader for NoReads {
    fn read_checkout_file(
        &mut self,
        path: &str,
    ) -> Result<Option<Vec<u8>>, crate::runner::error::RunnerError> {
        self.calls.push(path.into());
        panic!("unexpected mounted checkout read: {path}")
    }
}

#[test]
fn static_reads_and_stale_context_never_open_guest_state_vault_or_credentials() {
    let context = context(4);
    let mut guest = NoReads::default();
    let mut checkout = NoReads::default();
    let mut reads = GuestLocalReads {
        context: &context,
        guest: &mut guest,
        checkout: &mut checkout,
    };
    assert!(matches!(
        reads.read(&LocalRead::Version, 4).expect("local version"),
        LocalReadOutcome::Version { .. }
    ));
    assert!(matches!(
        reads.read(&LocalRead::Sources, 4).expect("local sources"),
        LocalReadOutcome::Sources(_)
    ));
    for request in [
        LocalRead::Version,
        LocalRead::Tasks,
        LocalRead::Sources,
        LocalRead::CheckoutFile(".effigy/state/kernel.db".into()),
        LocalRead::CheckoutFile(".effigy/secrets/vault".into()),
    ] {
        assert!(matches!(
            reads.read(&request, 3),
            Err(LocalReadError::Refused(ContextRefusal {
                code: ContextRefusalCode::StaleContext,
                ..
            }))
        ));
    }
    assert!(guest.calls.is_empty());
    assert!(checkout.calls.is_empty());
}

macro_rules! fence_case {
    ($name:ident, $edit:expr, $code:ident) => {
        #[test]
        fn $name() {
            let mut live = live(4);
            ($edit)(&mut live);
            let error = context(4)
                .verify(&live)
                .expect_err("fence refuses before reads or effects");
            assert_eq!(error.code, ContextRefusalCode::$code);
            assert_eq!(
                error.names_reprepare(),
                error.code != ContextRefusalCode::StaleContext
            );
        }
    };
}
fence_case!(
    version_lock_refuses_before_guest_io,
    |live: &mut LiveGuestRun| live.binary.version = "0.14.1".into(),
    VersionMismatch
);
fence_case!(
    digest_lock_refuses_before_guest_io,
    |live: &mut LiveGuestRun| live.binary.digest = "sha256:foreign".into(),
    VersionMismatch
);
fence_case!(
    protocol_lock_refuses_before_guest_io,
    |live: &mut LiveGuestRun| live.binary.protocol = 2,
    VersionMismatch
);
fence_case!(
    generation_lock_refuses_before_guest_io,
    |live: &mut LiveGuestRun| live.context_generation = 3,
    StaleContext
);
fence_case!(
    container_lock_refuses_before_guest_io,
    |live: &mut LiveGuestRun| live.container.container_id = "short-prefix".into(),
    IdentityMismatch
);
fence_case!(
    boot_lock_refuses_before_guest_io,
    |live: &mut LiveGuestRun| live.container.vm_boot_id = "foreign-boot".into(),
    IdentityMismatch
);
fence_case!(
    environment_lock_refuses_before_guest_io,
    |live: &mut LiveGuestRun| live.environment_generation = "next".into(),
    IdentityMismatch
);
fence_case!(
    session_lock_refuses_before_guest_io,
    |live: &mut LiveGuestRun| live.session_run_id = Some("foreign-session".into()),
    IdentityMismatch
);

#[test]
fn missing_host_channel_refuses_authority_without_affecting_local_reads() {
    let mut context = context(4);
    context.channel.endpoint = None;
    assert_eq!(
        context.verify(&live(4)).expect_err("no fallback").code,
        ContextRefusalCode::Unavailable
    );
    for class in [GuestOperationClass::Static, GuestOperationClass::Observe] {
        assert!(!class.requires_enrolment());
    }
}
