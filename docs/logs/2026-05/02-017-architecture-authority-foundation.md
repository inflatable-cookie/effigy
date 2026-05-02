# 02-017 Architecture Authority Foundation

Date: 2026-05-02
Roadmap: `g03.017`
Batch: `356`

## What changed

- rewrote [docs/architecture/010-package-map.md](../../architecture/010-package-map.md)
  into a current authority map instead of a stale flat-module inventory
- updated [docs/architecture/000-overview.md](../../architecture/000-overview.md)
  so it points readers at the live ownership surfaces
- demoted
  [docs/architecture/020-container-infrastructure-design.md](../../architecture/020-container-infrastructure-design.md)
  from live authority to background design reference

## Why it mattered

The runtime/container hardening lanes changed the code shape enough that the
 old package map had become misleading.

The main risk was stale authority:

- one old doc still described a flatter runner than the code now has
- session context, workspace ownership, typed container assembly, and typed
  error families were no longer visible in the architecture map
- the long-form container design doc still looked more authoritative than it
  really is for current module ownership

## Result

The runtime/container core now has one current architecture authority surface
 that matches the live code seams closely enough for the final proof lane to
 use it as real reference material.
