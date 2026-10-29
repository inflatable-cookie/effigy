//! Runtime mechanisms shared by Effigy engines. No engine policy lives here.
//!
//! Host state, journalled process custody, placement and read-only probes
//! share one authority. A binary installs `signal::SignalRouter` once before
//! any spawn; library callers pass its cancellation token.
#![deny(missing_debug_implementations, unsafe_op_in_unsafe_fn)]

#[cfg(feature = "fault-injection")]
pub mod fault_injection;

mod closure;
pub mod ctx;
pub mod custody;
pub mod fs;
pub mod gate;
pub mod human;
pub mod id;
pub mod ident;
pub mod location;
pub mod objects;
pub mod observe;
pub mod outcome;
pub mod probe;
pub mod record;
pub mod remote;
pub mod retention;
pub mod root;
pub mod run;
pub mod signal;
mod socket_at;
pub mod store;
#[cfg(feature = "fault-injection")]
pub mod strict_fake;
pub mod supervise;
pub mod workspace_channel;
pub mod workspace_session;

#[cfg(all(test, feature = "fault-injection"))]
mod fault;
#[cfg(all(test, feature = "fault-injection"))]
mod fault_tests;
#[cfg(all(test, feature = "fault-injection"))]
mod faults;
#[cfg(all(test, feature = "fault-injection"))]
mod fixture;

/// A named fault point (§12.1): `io::Result<()>`, always `Ok` outside
/// fault-injection test builds.
#[macro_export]
#[doc(hidden)]
macro_rules! fault_point {
    ($point:ident) => {{
        #[cfg(all(test, feature = "fault-injection"))]
        let result = $crate::fault::point($crate::fault::Point::$point);
        #[cfg(not(all(test, feature = "fault-injection")))]
        let result = ::std::io::Result::<()>::Ok(());
        #[cfg(feature = "fault-injection")]
        let result = result.and_then(|()| $crate::fault_injection::point(stringify!($point)));
        result
    }};
}
