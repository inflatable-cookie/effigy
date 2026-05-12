# 072 - Manifest Section Decomposition Strict Lane

Roadmap: [`g04.036`](../roadmaps/g04/036-manifest-section-decomposition.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Split oversized manifest parsing files into section-owned modules while keeping
the composed manifest format and public Rust API behavior stable.

## Hard Boundaries

- no manifest TOML grammar changes
- no composed manifest behavior changes
- no bundle source behavior changes
- no provider package behavior changes
- no state/deploy/container command behavior changes
- no app-specific config logic
- no `.github/workflows/` edits
- no release execution

## Ownership Boundary

This lane is structural. The goal is to reduce context load in:

- `crates/effigy-manifest/src/bundles.rs`
- `crates/effigy-manifest/src/config_sections.rs`

The first pass should prefer internal module splits and facade re-exports over
public API churn.

## Candidate Splits

Potential internal owners:

- bundle config grammar
- bundle source grammar
- bundle materialization/cache identity
- deploy config section
- state config section
- container config section
- object-store config section
- manifest root/import config
- shared section parse errors

The exact sequence must follow actual dependency seams discovered in `669`.

## Execution Chain

- `668` complete: opened the lane, added strict-lane and contract anchors, and
  selected the first classification card
- `669` complete: mapped manifest ownership and selected bundle source/cache
  as the first split
- `670` complete: split bundle source and cache modules
- `671` complete: split container config section
- `672` complete: split remaining config sections
- `673` complete: closed manifest section decomposition proof

## Exit Condition

This lane is complete when the oversized manifest files are reduced to bounded
owners or facade modules, section behavior is still covered, and downstream
callers do not need to know about the internal split.

## Next Task

Execute `g04.037` for deploy domain boundary hardening.
