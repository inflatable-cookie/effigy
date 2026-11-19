Four QA routes are gone because a simpler one covers each job. The frozen test
and code rating tables and their drift selectors (`qa:quality:tests`,
`qa:quality:code`, `test:quality:table`) are replaced by one closed review
artifact, `docs/knowledge/quality.md`; scores, locators and body hashes are no
longer refreshed, and the tables stay readable at the commits they were judged
on. The whole-suite census is `nextest list`'s collected IDs: `test:inventory`
and its static Rust parser, with the zero-unattributed attribution gate, are
retired. Later-stage diagnosis runs the stage's own selector, so
`qa:later-stages`, its per-stage evidence protocol and
`test:qa:later-stages-wrapper` are retired. The kernel boundary ratchet now
fails only on new raw authority: a construct total that falls, or an existing
boundary that moves to another file at the same total, passes, while a rise
still fails and names the files that carry it.
