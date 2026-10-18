//! Runtime mechanisms shared by Effigy engines. No engine policy lives here.
//!
//! Gates, the store and `atomic_replace` open the host state root. Nothing
//! here installs signal handlers, mints checkout tokens, spawns processes or
//! performs network probes.
#![deny(missing_debug_implementations, unsafe_op_in_unsafe_fn)]

pub mod ctx;
pub mod custody;
pub mod fs;
pub mod gate;
pub mod id;
pub mod ident;
pub mod observe;
pub mod outcome;
pub mod probe;
pub mod record;
pub mod retention;
pub mod root;
pub mod run;
pub mod signal;
pub mod store;
pub mod supervise;

#[cfg(all(test, feature = "fault-injection"))]
mod fault;
#[cfg(all(test, feature = "fault-injection"))]
mod fault_tests;

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
        result
    }};
}
