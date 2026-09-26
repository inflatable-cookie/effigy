# Temporary Exceptions

A temporary deviation from routing, JSON compatibility, diagnostic,
maintainability, or release constraints must be explicit, owned, bounded, and
reversible. Urgency alone does not change a public contract.

## Required Decision

An exception decision records:

1. affected commands, repositories, and contract or guardrail;
2. concrete reason and the consequence of waiting;
3. user and operator risk during the exception window;
4. mitigation and validation that still run;
5. accountable owner and explicit expiry;
6. exit steps and the condition that ends the exception.

Queue or the PR review carries the live approval, status, and follow-up. The
owning knowledge file records any product rule that changes. This repository
does not keep a parallel exception log or lifecycle register.

## Approval and Limits

A contract or release exception needs explicit maintainer approval on the
specific change. An exception normally lasts no longer than one release cycle;
a renewal needs evidence that mitigation worked and a revised exit decision.
Expiry without renewal means the deviation is no longer authorized.

No exception can silently change a versioned JSON interface, erase operator
remediation, expose secrets, or bypass mandatory release gates. If a gate or
contract itself must change, make that product decision explicitly, update the
owning contract and validation together, and obtain the required human release
authority. The [release procedure](../contracts/release.md) still controls
release mutations.

A closed exception leaves its outcome in Queue and Git history. It should not
remain as an active rule in knowledge after the normal contract is restored.
