//! Private-fixture start-up knobs. Inert outside a private (test) gateway.
use super::GatewayConfig;
#[cfg(debug_assertions)]
use super::RunnerError;
#[cfg(debug_assertions)]
use std::time::Duration;
use std::{io, process::Command};

#[cfg(debug_assertions)]
const HOLD: &str = "EFFIGY_TEST_GATEWAY_STARTUP_HOLD";

/// A private daemon never inherits its client's start-up barrier.
pub(super) fn configure(command: &mut Command, config: &GatewayConfig) -> io::Result<()> {
    #[cfg(debug_assertions)]
    if config.is_private() {
        command.env_remove(HOLD);
    }
    let _ = (command, config);
    Ok(())
}

/// A bounded fault barrier after launch but before a successful `up` response.
#[cfg(debug_assertions)]
pub(super) fn hold_client(config: &GatewayConfig) -> Result<(), RunnerError> {
    if std::env::var(HOLD).as_deref() != Ok("1") || !config.is_private() {
        return Ok(());
    }
    let Some(root) = config.private_state_root() else {
        return Ok(());
    };
    let record = tempfile::NamedTempFile::new_in(root).map_err(RunnerError::Cwd)?;
    record
        .persist_noclobber(root.join(".fixture-client-held"))
        .map_err(|error| RunnerError::Cwd(error.error))?;
    let operation = effigy_gateway::operator::context(
        Duration::from_secs(30),
        effigy_kernel::ctx::EffectClass::Observe,
    );
    while !root.join(".fixture-client-release").exists() {
        operation
            .check("fixture gateway startup barrier")
            .map_err(|error| {
                RunnerError::task_invocation(format!("fixture startup barrier: {error:?}"))
            })?;
        std::thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}
