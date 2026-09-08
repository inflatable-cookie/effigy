# 124 Stale Local Install Recovery Strict Lane

Status: Satisfied (2026-09-08; PR `95`, merge `24e842196`)
Owner: Effigy manifest error boundary and local build provenance
Created: 2026-09-08
Roadmap: [`g09.009`](../../roadmaps/g09/009-stale-local-install-recovery.md)
Ready card: [`1117`](../../roadmaps/g09/batch-cards/1117-stale-local-install-recovery.md)
Guide: [`057`](../../guides/057-bootstrap-repo-bringup.md)
Papercut: [`PAPERCUTS.md`](../../../PAPERCUTS.md)

## Outcome

When Effigy's repository-local installed binary is provably older than the
current Effigy checkout and cannot parse that checkout's manifest, the existing
parse failure names both revisions and the source-build command that refreshes
the install. Strict parsing and ordinary consumer errors stay unchanged.

## Fixed Decisions

- Diagnose at the normal manifest error boundary because this failure happens
  before `doctor` and repository tasks can run.
- A stale diagnosis requires all of these facts: the executable is the current
  checkout's `.local-install/bin/effigy`; its recorded `+local.<sha>` identity
  resolves in that checkout; the recorded commit is an ancestor of, and differs
  from, current `HEAD`; and strict manifest parsing failed.
- The hint includes the installed identity, current checkout revision, and
  `cargo run --bin effigy -- bootstrap:local` from the checkout root.
- Preserve the original parse error verbatim. Add one text/JSON error hint; do
  not downgrade the error, retry with a lossy schema, or special-case the new
  manifest key.
- If provenance, ancestry, executable placement, or repository identity cannot
  be proved, render the existing parse failure only. Release binaries and
  consumer repositories must not be called stale from a semver comparison or a
  coincidental unknown field.

## Whole-Lane Review Oracle

Reject the lane if any counterexample survives:

1. The reproduced `.local-install` binary built from a pre-`docs_policy.sources`
   ancestor still reports only `unknown field sources` with no actionable
   refresh command.
2. The diagnosis appears for a current local install, a release/global binary,
   a non-ancestor local build, or a consumer repository.
3. The raw TOML error, non-zero exit, text error shape, or JSON stdout purity is
   lost.
4. `doctor` or `bootstrap:local` must successfully parse the new manifest before
   the hint can be produced.
5. The repair introduces a schema compatibility fallback, automatic rebuild,
   network mutation, release mutation, or workflow change.
6. The papercut, bootstrap guidance, evidence log, and `g09` closeout do not
   agree on the shipped recovery path.

## Validation And Evidence

Card `1117` maps every oracle row to focused manifest/error tests, a real
repository-local stale-install fixture or equivalent process-level proof,
current and false-positive controls, text and JSON assertions, `effigy qa`,
format, clippy, and `git diff --check`. One dated evidence log records the
installed and checkout identities.

## Stop Conditions

Stop and return facts if the hint cannot be made provenance-safe without a new
manifest compatibility layer, if detecting it requires mutating the checkout,
or if the recovery command cannot run from source without the installed binary.

## Next Task

Finish the `g09` roadmap, evidence, and dispatch-handoff closeout, then return
to Chatterbox for the operator-requested Northstar Refresh.
