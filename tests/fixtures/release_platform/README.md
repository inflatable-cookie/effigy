# Release platform fixtures (contract 054 §4)

One directory per `[release]` shape the release platform resolves. Each is a
whole project root: copy it into a temporary workspace, `git init`, and run
`effigy release --plan` or `effigy release status` against it.

| Directory | Shape | Contract |
| --- | --- | --- |
| `shorthand/` | Single identity, today's keys | 054 §4.1 |
| `named-identities/` | `core` and `react`, `follows`, artifacts, `tag` and `task` targets | 054 §4.2 |
| `digest-target/` | `kind = "digest"` with a deploy task target | 054 §4.3 |
| `longhorn/` | Longhorn's real `[release]` table, copied from `config/release.toml` | Compatibility |
| `poodle/` | Poodle's real `[release]` table, copied from `effigy.toml` | Compatibility |
| `underlay/` | Underlay's real `[release]` table, copied from `effigy.toml` | Compatibility |

The three consumer fixtures copy the consumer's own `[release]` keys, gates and
version sources; only the paths inside the fixture are trimmed to what
resolution reads. They exist to prove the shorthand keeps those eleven
projects working unchanged. No fixture pushes, publishes or tags anything.
