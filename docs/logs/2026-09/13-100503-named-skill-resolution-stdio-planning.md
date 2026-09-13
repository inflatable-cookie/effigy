# Named Skill Resolution and Stdio Planning

Status: complete
Created: 2026-09-13
Roadmap: g10.001
Batch: g10 opening and task promotion

## Summary

- Operator selected an agent-native skill-execution runway.
- Promoted deterministic installed-skill lookup and raw stdio passthrough as
  one bounded ready task.
- Kept `--json` as Effigy's envelope and explicit `--path` behavior unchanged.

## Changes

- Opened g10 with only `g10.001` in the approved frontier.
- Extended contract 042 and architecture 025 with discovery, transport,
  compatibility, failure, and byte-level proof requirements.
- Prepared one Queue handoff for implementation, independent review, merge,
  and canonical closeout.

## Vision Target Delta

- Primary tags: `ROUTE`, `CONTRACT`, `OPERATE`
- Movement: baseline explicit-path/human-or-envelope execution -> current ready
  contract for local skill-name discovery and opt-in raw transport
- Remaining gap: implementation and independent proof in `g10.001`

## Validation Performed

- command: `effigy qa:docs`
  - result: passed
- command: `git diff --check`
  - result: passed

## Risks

- Multiple distinct user-root copies must fail rather than select arbitrary
  code.
- Passthrough must bypass rendering without bypassing preflight.

## Next Task

- Dispatch `g10.001` through Northstar Queue.
