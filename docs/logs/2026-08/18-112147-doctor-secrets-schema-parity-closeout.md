# Doctor Secrets Schema Parity Closeout

Status: complete
Created: 2026-08-18
Roadmap: g08.033
Batch: doctor-secrets-schema-parity

## Summary

Doctor now accepts the secret manifest surface already owned by contract 032
and the canonical parser. The corrected local CLI is installed. Bovine's
health gate moved from a false schema error to `ok:19, warn:0, err:0` after its
machine-local dependency topology was normalized.

## Changes

- admitted root `secrets` in doctor's supplemental top-level allowlist
- admitted task `secrets` and validated the existing `required` mode
- added positive contract-shape and negative enum regression coverage
- left nested secret schema ownership in the strict canonical parser
- installed `effigy v0.11.0+local.17c109f.dirty`
- removed Longhorn's machine-local Poodle links, then rematerialized its
  registry dependency tree without changing tracked files
- kept the logs index single-domain by rendering its governance-template
  reference as a repo-relative path instead of an indexed log link

## Consumer Reproduction And Proof

- baseline: installed doctor rejected Bovine's supported root and task secret
  keys; Longhorn also exposed nested Poodle links into Bovine
- corrected source binary: the secret-key error disappeared; only the nested
  dependency-link warning remained
- corrected installed binary plus normalized links: Bovine doctor reported
  `ok:19, warn:0, err:0`
- Bovine's direct Poodle link remained healthy
- Longhorn `package.json` and `bun.lock` hashes were unchanged before and after
  unlink plus `bun install`; its Git worktree remained clean

## Vision Target Delta

- Primary tags: `CONTRACT`, `MAINT`, `OPERATE`
- Movement: doctor contradicted the supported contract-032 manifest surface ->
  source and installed doctor agree with the canonical parser, with consumer
  proof
- Remaining gap: None

## Validation Performed

- command: `cargo test -p effigy-doctor validate_manifest_schema_`
  - result: pass, 19 tests
- command: `cargo fmt --all -- --check`
  - result: pass
- command: `cargo clippy --all-targets -- -D warnings`
  - result: pass
- command: `effigy qa`
  - result: pass, including 3,263 nextest tests
- command: source and installed `effigy doctor` against Bovine
  - result: secret schema error removed; final installed proof clean
- command: `effigy --json deps status` in Bovine
  - result: one healthy direct Poodle link, zero warnings or errors

## Risks

- Doctor still has a supplemental allowlist beside the canonical parser. This
  fix keeps the duplication narrow; a broader schema consolidation was outside
  the lane.

## Next Task

Run the second governance review by 2026-09-17. Await operator intent for the
next Horizon theme.
