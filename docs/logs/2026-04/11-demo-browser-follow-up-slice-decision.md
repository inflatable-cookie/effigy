# Demo Browser Follow Up Slice Decision

Date: 2026-04-11
Roadmap: `g02.003`
Batch: `03.15`

## Summary

Chose artifact-opening affordances as the next bounded browser slice after the
shipped list/detail foundation.

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `ROUTE`
- Moved from `browser foundation shipped but next operator affordance still
  ambiguous` to `browser follow-up sequence fixed around artifact-first
  inspection rather than premature log streaming`
- Remaining open:
  - implement artifact-opening affordances in the browser
  - revisit live log visibility after artifact inspection is usable
  - keep broader runtime cancellation and desktop-client questions out of the
    current lane

## Why Artifact First

- `browser-proof-report` already produces a human-checkable HTML artifact and
  supporting text snapshots, but the browser only lists those paths today.
- `lifecycle-window` also produces operator-meaningful artifact files such as
  `status.txt`, `heartbeat.txt`, and `events.log`, which makes artifact access
  immediately useful even before any log-streaming work exists.
- live log visibility would push the lane toward tailing, streaming, and
  terminal-shape questions that are materially broader than the current browser
  foundation.
- artifact-opening stays grounded in runner-owned state that already exists in
  `demo inspect`, so it is a tighter follow-up than building another live data
  channel.

## Deferred Concern

Live log visibility remains valuable, especially for long-running demos, but it
should follow artifact affordances instead of becoming the next slice by
default.

## Validation

- `git diff --check`
- `effigy qa:docs`

## Next Task

Implement `022-implement-demo-browser-artifact-affordances.md` as the next
bounded browser batch.
