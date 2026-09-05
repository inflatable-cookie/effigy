# Portfolio routing fixture

Frozen input for the cross-repository cases in
`scripts/benchmark-docs-context.rhai` (roadmap `g09.006`, spec `122`).

The benchmark copies this tree to a scratch directory, turns each intended
checkout into a real git repository, and mutates the copy to reach the states a
committed tree cannot hold (a dirty file, a duplicate single-valued field). The
committed bytes here stay clean, so the fixture is readable as evidence.

| child | intended status | why |
| --- | --- | --- |
| `repos/shared-atlas` | `ok` | git checkout, `[docs_policy.graph]` profile, `share = true` |
| `repos/baseline-notes` | `ok` | git checkout, no profile (baseline Markdown), `share = true` |
| `repos/private-vault` | `not-shared` | git checkout with a manifest that never opts in |
| `repos/loose-notes` | `invalid` | a directory, not a checkout |
| `repos/worktrees/decoy-checkout` | never considered | `worktrees` is a skipped container name |
| `repos/.hidden-annex` | never considered | hidden directory |
| `absent-directory` | `missing` | named by `portfolio.toml`, does not exist |

`shared-atlas` and `baseline-notes` both carry the term `tolerance ledger`, so
one query reaches both and proves the two blocks stay separate instead of
merging into one ranked list.
