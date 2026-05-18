# 907 - Implement Markdown Docs And Anchor Indexer

Roadmap: [`../007-markdown-docs-and-anchor-indexer.md`](../007-markdown-docs-and-anchor-indexer.md)
Strict lane: [`../../../specs/085-code-graph-intelligence-strict-lane.md`](../../../specs/085-code-graph-intelligence-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-17

## Purpose

Index documentation as queryable graph nodes.

## Scope

- index Markdown files under docs and skills
- extract headings, anchors, links, code fence metadata, and local path
  references
- classify guide/contract/spec/roadmap/log files by path
- link docs to code paths where references are unambiguous

## Guardrails

- no prose summarization
- no broken-link checker replacement
- no rewriting historical docs
- no unbounded snippets

## Acceptance

- agents can query docs by heading/anchor
- graph can answer which docs mention a file
- active vs historical docs are visible in metadata

## Next Task

Execute `908`.
