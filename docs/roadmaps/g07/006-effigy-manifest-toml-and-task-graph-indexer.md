# g07.006 - Effigy Manifest, TOML, And Task Graph Indexer

Status: Complete
Depends on: `g07.004`

## Goal

Make Effigy-specific configuration first-class graph data.

Agents working in Effigy repos need task, manifest, container, bundle, and
workspace context as much as language symbols.

## Scope

- index `effigy.toml` and included manifest fragments
- index TOML sections and keys as graph nodes
- extract Effigy concepts:
  - tasks
  - task steps
  - catalogs
  - systems
  - workspaces
  - containers
  - services
  - bundles
  - deploy providers
  - state stacks and layers
  - docs and test policy references
- emit relation edges:
  - task -> command/script
  - task -> container/workspace
  - workspace -> container
  - container -> service
  - service -> catalog
  - bundle -> source
  - state stack -> layer/capture
  - deploy provider -> package source

## Implementation Guidance

Prefer Effigy's manifest parser and composed manifest inspection over ad hoc TOML
string scanning where possible.

Use TOML parsing for generic TOML files, but use composed-manifest metadata for
Effigy-specific ownership and source provenance.

## Non-Goals

- no mutation or formatter behavior
- no replacement for `effigy config`
- no indexing secrets values
- no reading private vault contents

## Tests

- manifest include graph fixtures
- bundle source fixtures
- compact task shape fixtures
- state stack fixtures
- deploy provider fixtures
- JSON graph facts with source path provenance

## Acceptance Criteria

- `effigy graph` can explain task and container ownership
- graph facts point back to manifest source files and paths
- secrets are represented as declarations only, never values
- generated/composed ownership is clear enough for agents to avoid stale assumptions

## Next Task

Execute `g07.007`.
