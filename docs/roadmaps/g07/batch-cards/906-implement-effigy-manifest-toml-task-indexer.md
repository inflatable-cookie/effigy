# 906 - Implement Effigy Manifest, TOML, And Task Indexer

Roadmap: [`../006-effigy-manifest-toml-and-task-graph-indexer.md`](../006-effigy-manifest-toml-and-task-graph-indexer.md)
Strict lane: [`../../../specs/085-code-graph-intelligence-strict-lane.md`](../../../specs/085-code-graph-intelligence-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-17

## Purpose

Make Effigy manifests and task topology first-class graph facts.

## Scope

- index `effigy.toml` and include fragments
- extract tasks, systems, workspaces, containers, services, bundles, deploy
  providers, and state stacks
- emit relation edges between tasks, workspaces, containers, services, catalogs,
  scripts, bundles, state layers, and deploy packages
- represent secret declarations without reading values

## Guardrails

- use existing manifest composition APIs where possible
- no ad hoc parsing when typed APIs exist
- no vault reads
- no mutation/formatting behavior

## Acceptance

- graph can explain task/container/bundle ownership
- facts point to manifest source paths
- composed ownership is visible enough for agents to avoid stale assumptions

## Next Task

Execute `907`.
