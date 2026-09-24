# Hickory Dependabot retarget after g10.014

Created: 2026-09-24
Roadmap: g10.016

## Summary

PR #117 merged `hickory-proto` and `hickory-server` at 0.26.2 as planned in `g10.014`. Before source-PR disposition finished, Dependabot retargeted #97 and #100 to 0.26.3. #97 was mistakenly closed with a #117 supersession comment. Its branch had been deleted, so both GraphQL reopen and REST state update failed. A correction comment records that 0.26.3 remains outstanding. #100 remains open. `g10.016` owns both targets after `g10.015`.

## Evidence

- [PR #117](https://github.com/inflatable-cookie/effigy/pull/117) merged at `dd3d702455d24cc2c86e027fd4d7ed7b503b4635` with the 0.26.2 targets.
- [PR #97](https://github.com/inflatable-cookie/effigy/pull/97) now targets `hickory-proto 0.26.3`; [correction comment](https://github.com/inflatable-cookie/effigy/pull/97#issuecomment-5818679884) preserves the disposition error.
- [PR #100](https://github.com/inflatable-cookie/effigy/pull/100) now targets `hickory-server 0.26.3` and remains open.
- GitHub REST reopen returned HTTP 422: `state cannot be changed. The dependabot/cargo/hickory-proto-0.26.2 branch has been deleted.`

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`.
- Movement: newly retargeted bot work has a serial owner and exact acceptance rather than a false supersession.
- Remaining gap: review and merge the 0.26.3 replacement after `g10.015`, then link the actual merge in both source PR threads.

## Next Task

Complete `g10.015`, then dispatch `g10.016` through Queue.
