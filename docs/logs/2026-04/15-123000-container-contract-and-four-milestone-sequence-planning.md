# Container Contract And Four-Milestone Sequence Planning

Date: 2026-04-15
Roadmap: `g02.006`
Spec: `docs/specs/006-colima-container-environment-strict-lane.md`

## Summary

Opened the next active product lane around a Colima-based container
environment surface and recorded the four major milestone sequence the project
now needs to work through.

## Shaped Next Active Lane

Opened `g02.006` as the active lane for:

- `effigy container <name> ...` command grammar
- manifest registry of named container environments
- default container alias support
- attached owner-session lifecycle
- task integration without making `effigy dev` globally special
- Colima-first v1 scope for machine-blocking web-development environments

The planning direction explicitly prefers:

- a named container registry instead of a built-in `dev` abstraction
- attached sessions owned by one Effigy task/session process
- shutdown-on-owner-exit by default
- task-level delegation such as `effigy dev` -> `effigy container default up`
  only when the repo chooses it

## Four-Milestone Sequence

The sequence is now explicit:

1. `g02.006` Colima container environment contract and implementation proof
2. `g02.007` distribution release and consumer rollout
3. `g02.008` demo and manifest-import rollout
4. `g02.009` vault-backed varlock rollout

Reason for this order:

- the container framework is the active machine blocker
- the distribution release still needs closing, but the user wants the
  container system specified in detail before locking in
- the demos/import and varlock rollouts remain important, but they are less
  urgent than the machine-blocking container lane

## Outcome

The repo now has:

- one active strict lane for the container-environment contract
- one ready planning card for settling the v1 container contract
- one explicit queued sequence for the other three major milestones so they do
  not get lost while the container work is active

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `ROUTE`, `MAINT`
- Moved: the post-distribution planning posture shifted from a paused lane with
  several user-priority follow-ons into one explicit active container lane plus
  a recorded four-milestone sequence
- Remaining open: the exact host/service integration boundary for the v1
  container surface and the later rollout work for distribution, demos/imports,
  and varlock

## Next Task

Execute `docs/roadmaps/g02/batch-cards/106-decide-colima-container-v1-contract.md`
to settle the v1 `effigy container` contract before implementation starts.
