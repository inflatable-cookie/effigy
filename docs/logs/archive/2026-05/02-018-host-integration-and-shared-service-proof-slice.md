# 02-018 Host-Integration And Shared-Service Proof Slice

Date: 2026-05-02
Roadmap: `g03.018`
Batch: `360`

## What changed

- added one integrated `effigy-containers` proof case that exercises:
  - host Composer home opt-in
  - explicit full SSH-home mount
  - external host mount attachment
  - shared MariaDB env projection
- kept the proof bounded to the live under-proven seam instead of widening the
  whole runtime test matrix again

## Why it mattered

After `358`, the runtime/container core was better-proven on lifecycle and
 ownership behavior, but still weaker on host-integration and shared-service
 evidence.

The missing problem was not feature coverage. It was combined proof:

- host integration was still mostly proven by separate one-off assertions
- shared-service env projection was proven separately
- there was no small integrated stack proof exercising those seams together

## Result

The proof lane now has direct executable coverage for the remaining
 host-integration and shared-service seam that was still under-proven after the
 first matrix batch.
