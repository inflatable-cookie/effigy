# 009 Vision Governance Review Template v1

Status: Draft
Owner: Platform Lead
Purpose: provide a one-page structure for weekly/monthly/release vision governance reviews.

## 1. Template Contract

Use the following sections in every governance review artifact:

1. `Scope`: cadence window, repos covered, reviewer(s).
2. `Metrics Snapshot`: top deltas against SLOs (`003`).
3. `Risk Status`: active risks and trend direction (`004`).
4. `Exception Status`: open/expiring/overdue exceptions (`005`).
5. `Decision Log`: tradeoff decisions made in this window (`008`).
6. `Actions`: owner, due date, expected tag impact.

## 2. One-Page Markdown Template

```md
# Vision Governance Review — <YYYY-MM-DD>

Scope
- Cadence: <weekly|monthly|release>
- Repos: <list>
- Reviewers: <list>

Metrics Snapshot
- Tag: <ROUTE|CONTRACT|OPERATE|MAINT|RELEASE>
- Observed: <value/window>
- SLO: <target>
- Delta: <up|flat|down + value>
- Note: <brief context>

Risk Status
- Risk ID: <VR-XX>
- Trend: <improving|stable|worsening>
- Signal: <trigger evidence>
- Action: <keep|escalate|close>

Exception Status
- Exception ID: <VE-YYYY-NN>
- State: <active|expiring|overdue>
- Expiry: <date>
- Owner: <role>
- Action: <close|renew|escalate>

Decision Log
- Decision ID: <D-YYYY-NN>
- Principle: <from 008>
- Summary: <one sentence>
- Reversal condition: <trigger>

Actions
- Owner: <name/role>
- Task: <concrete next action>
- Due: <date>
- Tag Impact: <tag list>
```

## 3. Review Quality Rules

1. Keep the review to one page unless incident detail requires an appendix.
2. Every worsening metric/risk needs an action owner and due date.
3. Every expiring or overdue exception needs explicit disposition.
4. Use consistent IDs so trends can be followed across cycles.

## Next Task

Integrate this template into logs guidance so governance reviews use a single consistent structure.
