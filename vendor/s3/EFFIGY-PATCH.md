# Effigy security patch

This directory contains the crates.io source for `s3 0.1.36`, distributed
under the included MIT licence.

Effigy's only source change is the `quick-xml` dependency constraint: it is
raised from `0.40.1` to `0.41.0` to address RUSTSEC-2026-0194 and
RUSTSEC-2026-0195. Remove this directory and the root `[patch.crates-io]`
entry once upstream `s3` publishes a compatible release.
