# 02-018 Final Boundary Decision

Date: 2026-05-02
Roadmap: `g03.018`
Batch: `361`

## Decision

Close `g03.018`.

## Why

The runtime/container hardening lane now has direct executable proof for the
 main brittle seams that drove the whole program:

- bootstrap setup versus bootstrap start handoff posture
- lease-refresh versus no-lease activation behavior
- direct workspace versus seeded workspace cleanup ownership
- reused-runtime gateway readiness parity across standard, deferred, and
  explicit exec activation
- host Composer home integration
- explicit SSH-home mount integration
- external host mount attachment
- shared-service env projection and binding behavior

That is now enough to stop calling the runtime/container core under-proven.

## Acceptance Bar

The runtime/container core is now `v1.0`-grade enough when judged against this
 hardening program's target:

- runtime/session ownership is explicit and typed
- generated container policy no longer depends on repeated YAML surgery for the
  main rewrite-heavy seams
- public workspace lifecycle and provisioning no longer sit in one mixed
  hotspot
- the main runtime/container failure seams have typed error families
- the historical brittle seams have direct executable proof instead of docs
  alone

This does not mean the container system is “finished forever”.

It means the remaining work is no longer a blocking architecture-hardening
 program for `v1.0`.

## Consequence

There is no active strict lane now.

The next honest move is planning: either another `v1.0` program, or deliberate
 cleanup outside the runtime/container hardening chain.
