//! Read-only status fallback for state left by the pre-kernel gateway.

use std::io;

use crate::server::GatewayConfig;

use super::{error, home, Status};

pub(super) fn legacy_status(config: &GatewayConfig) -> io::Result<Status> {
    if crate::identity::read_snapshot(&config.pid_file_path)
        .map_err(error)?
        .is_some()
    {
        return Ok(Status::simple(
            "setup_required",
            Some("an existing gateway needs migration; run `effigy gateway setup`".into()),
        ));
    }
    if crate::setup::read_receipt(home(config)?).is_err() {
        Ok(Status::simple(
            "setup_required",
            Some("run `effigy gateway setup`".into()),
        ))
    } else {
        Ok(Status::simple("stopped", None))
    }
}
