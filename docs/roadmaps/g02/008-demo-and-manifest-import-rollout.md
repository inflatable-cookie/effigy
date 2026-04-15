# 008 - Demo And Manifest Import Rollout

Generation: `g02`

Status: Planned
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

## Exit Condition

This milestone is complete when the intended consumer cohort has a coherent
manifest composition posture and demo adoption is explicit rather than
inconsistent or folkloric.

## Next Task

Execute this rollout after the higher-priority distribution release and the
machine-blocking container framework are both out of the way.
