# 073 - Deploy Domain Boundary Hardening Strict Lane

Roadmap: [`g04.037`](../roadmaps/g04/037-deploy-domain-boundary-hardening.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Reduce deploy runner ownership by separating transaction models, report
persistence, provider-package dispatch, and text rendering boundaries.

## Hard Boundaries

- no deploy command grammar changes
- no JSON schema drift
- no provider-specific live behavior in Effigy core
- no provider resource provisioning
- no provider secret management
- no database rollback promises
- no release command execution from deploy
- no `.github/workflows/` edits
- no release execution

## Ownership Boundary

This lane is structural. It may move pure deploy transaction models, config
parsing, report paths, history scanning, provider package context, and rendering
helpers out of `transaction.rs`.

Runner code remains responsible for side effects:

- command dispatch
- loading composed manifests
- invoking state apply
- invoking provider package scripts
- writing active/latest/history reports
- returning final text or JSON command output

## Crate Decision Rule

Do not add an `effigy-deploy` crate unless a card proves a stable domain API
that can be used without runner concerns. Internal modules are preferred for
this tranche.

## Execution Chain

- `674` complete: opened deploy domain boundary lane
- `675` complete: classified deploy transaction ownership
- `676` complete: extracted deploy report models and history helpers
- `677` complete: isolated provider package dispatch context
- `678` complete: split deploy text rendering from transaction state
- `679` complete: closed deploy boundary proof

## Exit Condition

This lane is complete when deploy transaction runner files are split by durable
ownership, JSON outputs remain stable, provider-package dispatch has a narrow
boundary, and `deploy export` remains separate from live deployment.

## Next Task

Execute `g04.038` for docs-policy, CLI help, and fixture deduplication.
