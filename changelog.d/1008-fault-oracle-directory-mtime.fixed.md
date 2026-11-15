The kernel fault oracle now ignores directory modification times while continuing to compare entries, file metadata, and file contents, so query-only WAL reads do not appear as filesystem writes.
