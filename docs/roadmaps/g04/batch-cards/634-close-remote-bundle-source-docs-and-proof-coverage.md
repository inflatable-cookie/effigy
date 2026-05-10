# 634 - Close Remote Bundle Source Docs And Proof Coverage

Lane: [`065-remote-bundle-sources-git-and-oci-delivery-strict-lane.md`](../065-remote-bundle-sources-git-and-oci-delivery-strict-lane.md)

Status: Complete
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

## Result

- visible bundle docs now reflect typed path sources and bare `bundle inspect`
- focused remote-bundle proof coverage is green across parser, manifest, and
  runner seams
- `g04.022` is complete

## Next Task

Select the next queued `g04` roadmap and open its strict lane.
