//! Ordinary test runs prepare the kernel's custody fixture binary.
//!
//! Proofs in several crates re-exec `kernel-custody-fixture` as their kernel
//! helper (contract 052 §5.7): the release Run's custody and git proof takes it
//! from `EFFIGY_KERNEL_CUSTODY_FIXTURE` when its own selector builds it, and
//! otherwise looks for the built binary. Cargo builds a package's binary
//! targets when one of that package's integration tests is selected, so this
//! target exists only to make a plain `effigy test` run - which selects every
//! integration test - build the fixture.
//!
//! `EFFIGY_KERNEL_CUSTODY_FIXTURE` is deliberately not consulted here: this is
//! the build trigger, not a proof that a selector named the binary.
