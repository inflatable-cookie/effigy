# 029 — Documentation QA

Use `effigy qa:docs` after a coherent documentation edit. It checks links, JSON examples, required knowledge files, agent defaults, workflow paths, and changelog structure. Run `effigy qa` before a PR.

CHANGELOG validation runs through `effigy changelog validate CHANGELOG.md`, the same parser `effigy release status` uses. Release headings must separate the version and date with ASCII ` - ` (U+002D); a Unicode em dash fails both surfaces with the same message, so the changelog grammar cannot drift between docs QA and release readiness.

## Review

- Update the [owning knowledge file](../knowledge/README.md) when product truth changes.
- Keep user guides in `docs/guides/` and link new pages from [the guides index](README.md).
- Check that command examples match current help and that JSON examples use current schemas.
- Turn links to removed process records into plain references to Git history.
- Record unresolved leads with Queue `lead.add`; use lanes, lane documents and `plan.set` for prioritized intent.

`effigy docs check links` with no paths checks `README.md` and Markdown under `docs/` at the repository root. That default scope stays repository-root relative.

An explicit path argument resolves against the invoking task's cwd. In a nested catalog whose cwd is `docs/`, `docs check links README.md knowledge/README.md` checks `docs/README.md` and `docs/knowledge/README.md`, and the report prints the resolved set (`resolved paths: docs/README.md, docs/knowledge/README.md`). A cwd outside the checkout, such as `effigy --repo <ROOT>` run from an unrelated directory, keeps resolving explicit arguments from the repository root.

For one release, an existing root-relative spelling from a nested cwd keeps passing: when the cwd-relative path is missing and the repository-root path exists, the check uses the root path and warns with the cwd-proof `{repo}` spelling. `{repo}` and `{cwd}` also expand inside `{ task = "docs check ..." }` references. `{repo}` is the selected catalog root, which is the task's cwd, so `{ task = "docs check links {repo}/README.md" }` works under both path rules.

Recursive checks skip `target/` and `node_modules/` at directory entry; an explicit file path is still checked. A missing or unreadable owned Markdown file fails. `json-examples`, `paths`, and `workflow-paths` can run separately when diagnosing one failure, and `effigy changelog validate` can be rerun alone for a changelog edit. [JSON contracts](../knowledge/contracts/README.md) have their own `effigy qa:json` check. The CI docs job runs the same `qa:docs` selector. Traversal rules: [docs-check source traversal](../knowledge/architecture/030-docs-check-source-traversal.md).
