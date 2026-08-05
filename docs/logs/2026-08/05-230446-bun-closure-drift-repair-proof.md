# Bun Closure Drift And Repair Proof

Status: complete
Created: 2026-08-05
Roadmap: g08.023
Batch: 1063-prove-bun-closure-drift-and-repair

## Summary

- proved full-closure save-less linking with Bun `1.3.14` in a disposable
  consumer and isolated HOME
- linked direct `@effigy-proof/protocol` and transitive `@effigy-proof/core`
  packages without changing any package manifest or lockfile byte
- reproduced a partial closure through a real `bun install`, detected it in
  status and doctor, and repaired it through managed re-link
- proved duplicate Svelte peer evidence preserves both exact physical paths in
  text and JSON
- unlinked both packages, released both unshared Effigy-owned registrations,
  and restored the registry-style dependency tree through normal install

## Changes

- managed Bun re-link now accepts a partial consumer closure when the exact
  repo/library key already exists in Effigy desired state
- an unmanaged partial closure remains a planning error, so Effigy does not
  claim an ambiguous manually linked package set
- the focused apply regression now removes one package from an established
  two-package closure and proves one re-link restores both without duplicating
  desired state or registration references

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `MAINT`
- Movement: fixture-only Bun round trips -> real install drift, repair, peer,
  and cleanup proof across the complete local package closure
- Remaining gap: operator and agent guidance plus suite closeout in card `1064`

## Validation Performed

- command: `bun install` followed by `effigy --json deps link bun ../library`
  - result: direct and transitive packages linked from the local library;
    verification passed and all five immutable-file observations were unchanged
- command: `bun install` while linked
  - result: Bun replaced the direct protocol symlink and retained the
    transitive core symlink, producing a deterministic one-of-two partial
    closure without manifest or lock churn
- command: `effigy --json deps status bun` and `effigy --json doctor`
  - result: status reported `bun-link-partial-closure` as a conflict; doctor
    reported the same evidence and re-link remediation as an error
- command: `effigy --json deps link bun ../library`
  - result: managed re-link ran one explicit `bun link ... --no-save`, restored
    both packages, returned healthy status, and preserved exact hashes
- command: text and JSON status/doctor with duplicate Svelte resolution
  - result: both forms reported consumer
    `/private/tmp/effigy-bun-proof.xTpPhh/consumer/node_modules/svelte` and local
    `/private/tmp/effigy-bun-proof.xTpPhh/library-peer/svelte`
- command: `effigy --json deps unlink bun ../library`
  - result: both consumer links and both unshared owned registrations were
    removed; verification passed; desired state became empty
- command: final `bun install` and `bun pm ls --all`
  - result: protocol and transitive core resolved from the registry-style
    fixture again; `package.json` hash remained
    `ba71a72bd79e3f8427e92cb799ec591632aa2ca4c330e34cba8d0b456f99a3f7` and
    `bun.lock` remained
    `925ee5a97b3b54a24fd61a91894cb6c7b450ce3a6fab13cb003fcb23d91d98bd`
- command: `cargo test -p effigy-deps`
  - result: 68 unit, 2 real Bun, and 3 real Cargo integration tests passed

## Risks

- Bun install drift can be partial rather than complete; the desired-state
  ownership gate is required so repair stays safe
- published portfolio TS acceptance remains pending the first published TS
  library; this proof uses equivalent file-backed registry packages

## Next Task

- Execute ready card `1064`: publish operator/agent guidance and close the
  dependency suite.
