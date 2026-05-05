# Runtime Container Cleanup Inventory

Date: 2026-05-05

## Change

Completed card `387`.

Ranked cleanup targets after `g03.030`, `g03.031`, and `g03.032`.

## Decision

Start with simple runner cwd/root callers. The wrappers are already
context-backed, but the old local call shape still appears in many command
entry modules.

## Next Task

Implement card `388`.
