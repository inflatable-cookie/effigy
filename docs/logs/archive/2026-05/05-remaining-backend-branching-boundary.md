# Remaining Backend Branching Boundary

Date: 2026-05-05

## Summary

Completed card `395`.

## Decision

Move Colima start runtime selection behind `ContainerManager`.

## Rationale

Runner command code no longer owns direct backend branching. The remaining
production backend checks are compatibility-layer code. Colima start command
assembly is the smallest remaining case that still reaches through the legacy
compose backend enum for backend detection.

## Next

Implement card `396`.
