# K5 Pilot Expectation Needs Rephrasing

Status: open — unscheduled remainder
Created: 2026-09-05
Owner: chatterbox
Source: PR `91` evidence
(`docs/logs/2026-09/05-113123-docs-context-latency-and-freshness-1113.md`),
Chatterbox ruling 2026-09-05
Promoted: the K4 exact-identifier retrieval defect became
[`g09.007`](../roadmaps/g09/007-docs-context-exact-identifier-retrieval.md)
/ strict spec `121` / card `1114` on 2026-09-05 (operator confirmed)
Contract: [`041`](../contracts/041-documentation-graph-profile-contract.md)
Feeds: [`g09.006`](../roadmaps/g09/006-cross-repository-source-routing.md)
replay set

## Issue

The pilot expected guide `051` for "Does release execute publish a GitHub
Release?", but guide `051` never contains the phrase; it says pushing a tag
alone does not publish and that the release workflow publishes artifacts.
Answering K5 needs inference across sections, which contract `041` forbids
(`docs context` returns evidence, not an answer). K5 is a valid question but
not a valid exact-section retrieval oracle as written. Guide `051` ranked 24
of 32 for the phrase at PR `91` head.

## Next Task

Settle with the Northstar Chatterbox before `g09.006` freezes its replay
set: rephrase K5 into an evidence-shaped query using the guide's own words
about publication, or split it into a tool-behaviour question and a
consumer-obligation question. Do not carry the phrase as written into any
recall claim.
