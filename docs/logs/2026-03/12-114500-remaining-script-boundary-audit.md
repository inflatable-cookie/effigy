# Remaining Script Boundary Audit

Date: 2026-03-12
Owner: Platform

## Summary

After the docs-policy cleanup, the remaining top-level `scripts/` surface is
small and mostly justified by real external boundaries.

Current keep set:

- `scripts/check-distribution-first-publish.sh`
- `scripts/check-release-gates.sh`
- `scripts/check-release-install-from-tag.sh`
- `scripts/check-release-smoke.sh`
- `scripts/effigy-dev`
- `scripts/install-local-bin-links.sh`
- `scripts/prepare-release.sh`

Removed in this closeout:

- `src/bin/effigy-release-qa.rs`

That helper bin added an extra compatibility layer on top of
`scripts/check-release-gates.sh`. The repo now leads with
`effigy release gates --repo .`, and the cargo alias can call the same
built-in release-gates command directly.

## Vision Target Delta

- Primary tags touched: `OPERATE`, `MAINT`, `RELEASE`
- Moved from `ambiguous leftover script surface with one extra Rust trampoline`
  to `explicit remaining compatibility boundaries plus direct cargo aliasing`
- Remains open: when maintainers want to retire the three release compatibility
  wrappers after enough real built-in release evidence accumulates

## Keep / Retire Decision

### Keep

`scripts/check-distribution-first-publish.sh`

- rationale: still owns real side effects across tag install, crates.io
  install, optional Homebrew install/upgrade, and artifact log capture
- not just a validator alias

`scripts/check-release-gates.sh`

- rationale: explicit compatibility entrypoint documented in release protocol
  and wrappers guide
- still useful where an external caller expects a script path
- retirement target: yes, once guide `049` retirement criteria are met

`scripts/check-release-install-from-tag.sh`

- rationale: thin but stable compatibility entrypoint for tag-install
  validation
- acceptable because the external contract is a script path, not because the
  logic belongs in shell
- retirement target: yes, on the same criteria as `check-release-gates.sh`

`scripts/check-release-smoke.sh`

- rationale: binary smoke harness around an already-built release artifact
- this is an external-binary probe, not just command aliasing
- retirement target: no current retirement target; this is a durable external
  boundary unless release-artifact probing moves somewhere else

`scripts/effigy-dev`

- rationale: first-class contributor-facing dev channel entrypoint
- distinct operator value beyond simple shell convenience
- retirement target: no current retirement target; this is an intentional dev
  channel entrypoint

`scripts/install-local-bin-links.sh`

- rationale: local-machine PATH/bootstrap glue
- exactly the kind of task where shell is still appropriate
- retirement target: no current retirement target; local symlink/bootstrap work
  is an appropriate shell use case

`scripts/prepare-release.sh`

- rationale: explicit backup path retained by release policy and docs
- no longer primary, but still intentionally kept as a compatibility/backstop
  channel
- retirement target: yes, once guide `049` retirement criteria are met

## Retirement Criteria

The release compatibility wrappers should be retired only when all of the
following are true:

- at least two consecutive real Effigy releases completed through the built-in
  `effigy release ...` path without wrapper fallback
- hosted release workflows and tag-install validation stayed green across those
  releases
- no active CI, docs, or downstream operator contract still points to the
  wrapper path as the primary release entrypoint
- maintainers explicitly confirm that the wrappers are no longer needed as a
  backstop channel

### Retired

`src/bin/effigy-release-qa.rs`

- rationale: redundant trampoline over the wrapper, not a real boundary
- cargo alias now calls `effigy release gates --repo .` directly

## Conclusion

The remaining top-level script surface is now small enough that additional
cleanup should be driven by release/distribution policy decisions, not by a
general anti-shell goal.
