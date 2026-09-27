# 029 — Documentation QA

Use `effigy qa:docs` after a coherent documentation edit. It checks links, JSON examples, required knowledge files, agent defaults, and workflow paths. Run `effigy qa` before a PR.

## Review

- Update the [owning knowledge file](../knowledge/README.md) when product truth changes.
- Keep user guides in `docs/guides/` and link new pages from [the guides index](README.md).
- Check that command examples match current help and that JSON examples use current schemas.
- Turn links to removed process records into plain references to Git history.
- Record unresolved leads with Queue `lead.add`; use lanes, lane documents and `plan.set` for prioritized intent.

`effigy docs check links`, `json-examples`, `paths`, and `workflow-paths` can run separately when diagnosing one failure. [JSON contracts](../knowledge/contracts/README.md) have their own `effigy qa:json` check. The CI docs job runs the same `qa:docs` selector.
