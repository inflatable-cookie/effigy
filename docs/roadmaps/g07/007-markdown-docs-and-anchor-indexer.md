# g07.007 - Markdown Docs And Anchor Indexer

Status: Complete
Depends on: `g07.004`

## Goal

Index docs as navigable graph nodes so agents can find contracts, guides,
roadmaps, specs, and logs without broad Markdown scans.

## Scope

- index Markdown files under docs and skills
- extract headings and anchors
- extract internal links
- extract fenced code block metadata
- extract referenced local paths where unambiguous
- link docs to code paths when a local file path is mentioned
- distinguish active docs from historical roadmap/log material where path
  conventions make that clear

## Graph Concepts

- doc file
- heading
- anchor
- local link
- code fence
- local path reference
- guide/contract/spec/roadmap/log classification

## Non-Goals

- no prose summarization
- no LLM-generated abstract
- no full Markdown rendering
- no broken-link checker replacement
- no rewriting old historical references

## Tests

- guide fixture with headings, anchors, and local links
- contract fixture with code fences
- roadmap/log fixture classification
- local path reference extraction
- generated anchor stability

## Acceptance Criteria

- agents can query docs by heading/anchor without scanning all Markdown
- graph can answer "which docs mention this file?"
- active vs historical documentation is visible in graph metadata
- snippets stay bounded and provenance-backed

## Next Task

Execute `g07.008`.
