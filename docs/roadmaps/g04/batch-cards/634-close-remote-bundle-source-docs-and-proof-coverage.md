# 634 - Close Remote Bundle Source Docs And Proof Coverage

Lane: [`065-remote-bundle-sources-git-and-oci-delivery-strict-lane.md`](../065-remote-bundle-sources-git-and-oci-delivery-strict-lane.md)

Status: Ready
Owner: Platform
Created: 2026-05-10

## Goal

Finish the remaining docs and proof closeout for remote bundle sources, then
close `g04.022`.

## Scope

- update the visible bundle guides to match the typed `base` forms
- make the command reference reflect bare `bundle inspect`
- keep the changelog and lane docs aligned with the shipped surface
- run one focused proof round that covers shipped, path, git, and OCI source
  behaviors already landed in this lane

## Acceptance

- no active docs still recommend `[bundle].base_path`
- bundle command docs reflect bare `effigy bundle inspect`
- focused remote-bundle proofs are green
- `g04.022` can be marked complete

## Next Task

Close the remaining docs/proof slice and finish the lane.
