# Signal Cargo Portfolio Proof

Status: complete
Created: 2026-08-05
Roadmap: g08.023
Batch: 1062-prove-signal-links-across-flat-and-nested-consumers

## Summary

- proved Signal `v0.1.0` linking against disposable committed-HEAD clones of
  Soundcheck and Loophole with the current local Effigy binary
- linked 16 Signal crates in flat Soundcheck and 65 workspace/crate
  resolutions across Loophole's `aura/src-tauri`, `pulse`, and `spark`
- proved a disposable `signal-plugin-inventory` source edit rebuilt the linked
  crate and propagated through Soundcheck's consumer chain
- proved dry-run, status, and doctor are lock-neutral; unlink restored exact
  tagged resolution and every tracked lockfile byte

## Exact Closure

- Soundcheck — 16 crates: `signal-dsp`, `signal-dsp-resample`,
  `signal-dsp-spectral`, `signal-dsp-stretch`, `signal-hardware`,
  `signal-hardware-cpal`, `signal-ipc`, `signal-plugin`, `signal-plugin-au`,
  `signal-plugin-bridge`, `signal-plugin-clap`, `signal-plugin-inventory`,
  `signal-plugin-lv2`, `signal-plugin-vst3`, `signal-primitives`,
  `signal-render-plane`
- Loophole `aura/src-tauri` — 23 crates: the shared 21-crate Pulse/Spark
  closure plus `signal-hardware-coremidi` and `signal-plugin-bridge`
- Loophole `pulse` and `spark` — 21 crates each: `signal-analysis`,
  `signal-analysis-character`, `signal-analysis-loudness`, `signal-dsp`,
  `signal-dsp-resample`, `signal-dsp-spectral`, `signal-dsp-stretch`,
  `signal-graph`, `signal-hardware`, `signal-hardware-cpal`,
  `signal-host-local`, `signal-ipc`, `signal-plugin`, `signal-plugin-au`,
  `signal-plugin-clap`, `signal-plugin-inventory`, `signal-plugin-lv2`,
  `signal-plugin-vst3`, `signal-primitives`, `signal-render-plane`,
  `signal-runtime`

## Defects Corrected

- Cargo discovery no longer treats archived `reference/` trees or orphaned
  descendant manifests as live workspaces
- planning and status use locked metadata and can skip a stale unrelated
  workspace that declares none of the target-library crates
- link verification, status, and unlink inspect only persisted consumer roots,
  preventing a repo-root patch from rewriting unrelated nested locks
- owned `[[patch.unused]]` entries classify as active-link state rather than
  unrelated drift

## Validation Performed

- command: `effigy --json deps link cargo ../signal --dry-run`
  - result: Soundcheck 16 and Loophole 65 resolutions; all pre-link lock hashes
    unchanged; observed wall time about 4 seconds for Loophole
- command: `effigy --json deps link cargo ../signal`
  - result: 16/16 Soundcheck and 65/65 Loophole metadata/tree checks passed;
    Loophole changed only its three planned locks; about 11 seconds
- command: `cargo check -p soundcheck-core`, edit Signal, repeat with `-vv`
  - result: baseline 9.3 seconds; edit rebuild 0.5 seconds and reported dirty
    `signal-plugin-inventory`, `soundcheck-library-scan`,
    `soundcheck-library-jobs`, and `soundcheck-core`
- command: `effigy --json deps status cargo` and `effigy doctor`
  - result: both reported active-link do-not-commit errors for every affected
    lock; all lock hashes remained unchanged
- command: `effigy --json deps unlink cargo ../signal`
  - result: Soundcheck 16/16 and Loophole 65/65 tagged-source checks passed;
    affected locks reported `active-links -> clean`
- command: targeted `cargo tree` after unlink
  - result: Soundcheck, Aura, Pulse, and Spark resolved Signal
    `v0.1.0` at `e52721a9`
- command: `cargo test -p effigy-deps`
  - result: 73 unit/integration/doc tests passed during the correction batch
- command: `effigy qa:ci:fast`
  - result: 1,625 tests, released-surface checks, and all 25 JSON contracts
    passed after the locked-metadata contract fixture gained a baseline lock
- command: `effigy qa:docs` and `git diff --check`
  - result: passed

## Live Worktree Evidence

- Signal and Loophole retained their exact starting HEAD, status hash, and
  tracked-diff hash
- Soundcheck retained its exact starting HEAD and tracked-diff hash; its live
  status gained one concurrent untracked source file while the proof ran
- every mutating proof command ran below `/tmp/effigy-signal-proof*`; no proof
  command targeted a live portfolio worktree

## Timings

- Loophole: dry-run 4.3s, link 11.3s, status 1.5s, doctor about 27s, unlink
  about 15s
- Soundcheck: dry-run plus link 5.4s, baseline check 9.3s, edit rebuild 0.5s,
  doctor about 16s

## Risks

- active Cargo links deliberately remain doctor/status errors because tracked
  locks contain do-not-commit path resolution until unlink
- an unrelated workspace with a stale lock is skipped only when locked metadata
  fails specifically on lock update and its local manifests declare no target
  crate; target-declaring workspaces still fail loudly

## Next Task

- Execute ready card `1063` and prove Bun closure drift and idempotent repair.
