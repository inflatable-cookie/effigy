//! Private-fixture custody in every test profile. Inert without a suite ledger.
use super::{GatewayConfig, RunnerError};
use effigy_kernel::{custody::ProcessReceipt, observe::Observed};
#[cfg(debug_assertions)]
use std::time::Duration;
use std::{io, path::Path, process::Command};

const DETACHED: &str = "EFFIGY_TEST_GATEWAY_DETACHED";
#[cfg(debug_assertions)]
const HOLD: &str = "EFFIGY_TEST_GATEWAY_STARTUP_HOLD";

pub(super) fn configure(command: &mut Command, config: &GatewayConfig) -> io::Result<()> {
    if config.is_private() {
        if let Some(ledger) = std::env::var_os("EFFIGY_TEST_FIXTURE_LEDGER") {
            let nonce = tempfile::NamedTempFile::new_in(ledger)?;
            command
                .env(DETACHED, "1")
                .env(
                    "EFFIGY_CUSTODY_NONCE",
                    nonce
                        .path()
                        .file_name()
                        .ok_or_else(|| io::Error::other("fixture nonce name"))?,
                )
                .env_remove("EFFIGY_TEST_GATEWAY_STARTUP_HOLD");
        }
    }
    Ok(())
}

/// The daemon publishes its own generation before any gateway state or listener.
/// A failed CLI response or killed client therefore cannot skip registration.
pub(super) fn register(config: &GatewayConfig) -> Result<(), RunnerError> {
    if std::env::var(DETACHED).as_deref() != Ok("1") {
        return Ok(());
    }
    let publish = || -> io::Result<()> {
        let owner = config
            .private_state_root()
            .ok_or_else(|| io::Error::other("fixture daemon requires a private root"))?;
        effigy_gateway::private_state::validate_root(owner).map_err(io::Error::other)?;
        let ledger = std::env::var_os("EFFIGY_TEST_FIXTURE_LEDGER")
            .ok_or_else(|| io::Error::other("fixture ledger missing"))?;
        let ledger = Path::new(&ledger);
        let pid = std::process::id();
        let identity = match effigy_kernel::ident::process_identity(pid) {
            Observed::Verified(identity, _) => identity,
            other => {
                return Err(io::Error::other(format!(
                    "fixture daemon identity: {other:?}"
                )))
            }
        };
        let pgid = u32::try_from(nix::unistd::getpgrp().as_raw()).map_err(io::Error::other)?;
        if pgid != pid {
            return Err(io::Error::other("fixture daemon must lead its group"));
        }
        let nonce = std::env::var("EFFIGY_CUSTODY_NONCE").map_err(io::Error::other)?;
        if nonce.is_empty() {
            return Err(io::Error::other("fixture daemon nonce missing"));
        }
        let receipt = ProcessReceipt {
            identity,
            pgid,
            nonce,
            grace_millis: 250,
        };
        let record = tempfile::NamedTempFile::new_in(ledger)?;
        serde_json::to_writer(
            record.as_file(),
            &serde_json::json!({"owner": std::fs::canonicalize(owner)?, "receipt": receipt}),
        )?;
        let destination = ledger.join("processes").join(
            record
                .path()
                .file_name()
                .ok_or_else(|| io::Error::other("fixture record name"))?,
        );
        record
            .persist_noclobber(destination)
            .map_err(io::Error::other)?;
        Ok(())
    };
    publish()
        .map_err(|error| RunnerError::task_invocation(format!("private fixture custody: {error}")))
}

/// A bounded fault barrier after launch but before a successful `up` response.
#[cfg(debug_assertions)]
pub(super) fn hold_client(config: &GatewayConfig) -> Result<(), RunnerError> {
    if std::env::var(HOLD).as_deref() != Ok("1")
        || std::env::var_os("EFFIGY_TEST_FIXTURE_LEDGER").is_none()
    {
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
