# Distribution Release Lane Activation And Linux Rehearsal Container Plan

Date: 2026-04-15 19:15 Europe/London
Roadmap: `g02.007`

## Summary

Activated the distribution release-closure lane instead of leaving it as a
planned follow-up behind the paused container work.

The first bounded batch is now explicit:

- add one Effigy-owned local Linux build rehearsal container
- use it during pre-release prep to exercise the Linux build and GLIBC floor
  path locally
- keep broader consumer rollout work behind that proof

## Why This Moved First

The shipped container surface now makes it practical to prove more of Effigy's
own release path locally.

That is a better first release-closure step than going straight to release
notes or cross-repo rollout while the Linux build still depends mostly on CI
trust and manual operator orchestration.

## State Change

- `g02.007` is now active
- a strict lane now exists for release closure
- `111` is the active ready card
- the next implementation batch is local Linux rehearsal, not consumer rollout

## Vision Target Delta

- Tags: `RELEASE`, `OPERATE`, `MAINT`
- Moved: `g02.007` from planned release closure into an active lane with one
  explicit local Linux proof batch for pre-release prep.
- Open: implement and validate the Linux rehearsal container, then decide
  whether release closure can proceed directly or needs one more bounded
  release-hardening batch.

## Next Task

Execute
[`111-implement-linux-release-rehearsal-container.md`](../../specs/batch-cards/111-implement-linux-release-rehearsal-container.md).
