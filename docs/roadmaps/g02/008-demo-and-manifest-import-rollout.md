# 008 - Demo And Manifest Import Rollout

Generation: `g02`

Status: Complete
Owner: Platform
Created: 2026-04-15
Depends on: 002, 003, 029

## Problem

Effigy's demo surface and manifest composition/import system are proven in the
product, but their adoption across consumer repos is incomplete and uneven.

## Goal

Roll out the demo system and manifest import/composition model across the
intended repo cohort without forcing demos into repos that do not yet have a
meaningful proof surface.

## Scope

- complete manifest include/import adoption where the split-manifest model is
  now the right default
- adopt demos where a repo has a real operator proof loop worth preserving
- leave repos without a real demo need out of the rollout instead of faking it

## Closeout

This rollout did not become a live `g02` execution lane.

The underlying product surfaces shipped, but the cross-repo adoption program
was intentionally not run as part of `g02`. That is now a sequencing decision,
not unfinished work inside the closed generation.

## Exit Condition

This milestone is complete on the `g02` boundary because the generation is
closed and any future adoption push must be re-sequenced deliberately in the
live queue.

## Next Task

Leave this roadmap closed.

If demo or manifest-import adoption becomes active again, rehome it into the
live generation or backlog instead of pretending `g02` is still open.
