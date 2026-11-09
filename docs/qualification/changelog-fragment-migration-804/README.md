# Changelog fragment migration (effigy#804)

One-time record of moving the `[Unreleased]` section of `CHANGELOG.md` into
`changelog.d/` fragments. `mapping.tsv` has one row per source entry.

## Result

- 314 `[Unreleased]` entries at `HEAD` became 314 fragments, one per row:
  security 52, added 49, changed 55, fixed 125, removed 33.
- 21 legacy `### Breaking` entries map to `removed`. Each body starts with
  the explicit marker `**Breaking:** `, so the severity stays visible.
  `removed` keeps the same bump as `Breaking` (MINOR under `0.x`, MAJOR later).
- No legacy `### Deprecated` entries existed. Deprecated maps to `changed`
  with a `**Deprecated:** ` marker when one appears.
- Published sections (`## [0.14.1]` and older) are byte-identical to `HEAD`.
- `[Unreleased]` is empty, and `changelog.d/.gitkeep` keeps the adoption marker.
- Merged fragments `796-…fixed.md` and `803-…fixed.md` were normalized to the
  canonical one-entry body. Their text is unchanged: 796 lost its leading `- `,
  803 lost its `### Fixed` header and bullet. Rows 2 and 3 of `mapping.tsv`
  record this.

## Columns

`ordinal` and `source_line` locate the entry in `HEAD`'s `CHANGELOG.md`.
`source_paragraphs` is the count of paragraphs in the source entry. `fragment`
is the file name. `source_sha256` is the SHA-256 of the source entry's text,
with paragraphs joined and whitespace collapsed to single spaces. For a
migrated fragment, the same digest of its body (without the Breaking marker)
must match.

## Names

An entry whose text cites `effigy#N` takes that task number as its name.
Entries without a task reference take `legacy-NNN`, where `NNN` is the entry's
ordinal in the original section. No task number was invented.

## Exceptions

- Ordinal 1846 (`changed`, source line 1846) had three paragraphs. A fragment
  holds one paragraph, so the paragraphs join with a single space. Every word
  is kept; only the paragraph breaks are lost.
- Ordinal 287 (`changed`, `legacy-287-a-host-run-in-a-linked.changed.md`,
  source lines 2474 to 2485) had a nested sub-bullet at source line 2485,
  `  - the whole value, …`. A fragment holds one paragraph and has no nested
  list, so the sub-bullet joins its parent paragraph after `Only an endpoint
  is rewritten -`. Every word and the `- ` marker are kept. The parser counts
  the sub-bullet as a separate entry in the old file, so `effigy changelog
  analyze` on `HEAD`'s changelog reports Changed 56. The migrated set reports
  Changed 56 as well: 55 legacy fragments plus this task's `804` fragment. No
  bump changes.

## Verification

The equivalence check reads `HEAD:CHANGELOG.md` again, recomputes every
entry's text and digest with the same paragraph and nesting rules, and compares
each row against its fragment. It also
confirms the fragment set equals the files on disk, that the published bytes
are identical, and that `[Unreleased]` is empty. Its last run passed with no
problems.
