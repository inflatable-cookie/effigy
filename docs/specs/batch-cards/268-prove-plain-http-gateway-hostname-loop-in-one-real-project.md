# 268 Prove Plain HTTP Gateway Hostname Loop In One Real Project

Status: landed
Updated: 2026-04-18
Roadmap: `g02.014`
Spec: `docs/specs/014-rust-native-gateway-strict-lane.md`

## Objective

Close the remaining credibility gap in `g02.014` by proving that one real
project can come up behind the host-native gateway and answer on its declared
plain HTTP `.test` hostname through Effigy itself.

## Scope

- use one real consumer repo with the integrated container path already proven
  elsewhere in `g02`
- declare or reuse one manifest-owned `[containers.<name>.dns]` domain on the
  real project path
- exercise `effigy gateway up`, `container up`, hostname reachability,
  `gateway status`, and teardown through the real product surface
- prove both route registration and route removal from actual lifecycle
  commands, not only crate tests or direct route-table inspection
- keep widening bounded to product fixes exposed directly by the proof

Recommended proof target:

- `/Users/tom/Dev/projects/underlay-reference`

## Out Of Scope

- TLS and certificate setup
- non-container task-owned gateway routes
- multi-project coordination or dashboard work
- speculative gateway UX cleanup not exposed by the proof

## Acceptance

- one real project can start behind the gateway and answer on
  `http://<domain>.test`
- the proof uses the real gateway daemon plus route registration through
  `effigy container up`
- `effigy gateway status` exposes the live registered route during the run
- `effigy container down` or `reset` removes the route and the hostname stops
  resolving or proxying cleanly afterward
- any product gap exposed by the proof is either fixed in-batch or written down
  as the explicit next card

## Proof Notes

The proof should prefer the real resolver path on macOS.

If `/etc/resolver/test` setup still needs elevated access on this machine, use
that as an operator prerequisite for the proof rather than widening into a new
permission model. The batch still has to prove the real `.test` hostname loop,
not only a manual `Host` header proxy request.

Use unprivileged override ports only when needed to avoid turning the batch
into privileged-port debugging. The thing that must be real is hostname-based
routing through the gateway-owned path.

## Outcome

This batch landed through `/Users/tom/Dev/projects/underlay-reference`.

What the proof established:

- `effigy gateway up` runs cleanly on unprivileged override ports with one
  honest macOS resolver warning when `/etc/resolver/test` still needs sudo
- `effigy container up --repo /Users/tom/Dev/projects/underlay-reference
  --detach --json` now registers a real route for `underlay-mail.test`
- `effigy gateway status --json` shows the live registered route on the shared
  route table
- direct DNS proof against the gateway resolver returns `127.0.0.1` for
  `underlay-mail.test`
- proxy proof against the gateway returns Mailpit HTML with `200 OK` when the
  request uses the registered hostname
- `effigy container down` removes the route and the gateway falls back to its
  `503 No Route` response for that host afterward

What the proof exposed and fixed in-batch:

- gateway registration could only proxy the first declared `host.ports` entry,
  which broke multi-port consumer stacks like `underlay-reference`
- the manifest now supports optional `[containers.<name>.dns].port` so a repo
  can choose the declared host port the gateway should proxy

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

No further execution lives on this card. Plan the remaining TLS closeout batch
next.
