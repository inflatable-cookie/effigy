//! The one built-in-owned prose and asset inventory `effigy init` emits.
//!
//! The embedded contract block is text, so it cannot be type-checked; the
//! tests below ground its claims in the repository instead: every upstream
//! document it cites must exist at the path it names, and the block must stay
//! inside its own markers.

pub(super) const AGENTS_BLOCK_START: &str = "<!-- BEGIN EFFIGY AGENT CONTRACT -->";
pub(super) const AGENTS_BLOCK_END: &str = "<!-- END EFFIGY AGENT CONTRACT -->";

pub(super) const AGENTS_BLOCK: &str = r#"<!-- BEGIN EFFIGY AGENT CONTRACT -->
## Effigy Agent Contract

Use Effigy as the default command surface for supported project work.

Route by job, not by startup ritual:
- use `rg` and the documentation index (`docs/README.md`) for code and docs understanding
- use `effigy tasks` for selector inventory
- use `effigy doctor` for routing ambiguity or repo health
- use `effigy test --plan` when test execution shape matters

Prefer `effigy <task>`, `effigy test`, and the matching built-in surface over
raw package-manager or shell commands when Effigy covers the path. Use
`effigy --json <command>` whenever another agent or tool will consume output.

Effigy guidance is maintained in the installed shared Agent Skill. Read the
installed `effigy/SKILL.md` from one of these user skill roots when using
Effigy-specific agent guidance: `~/.agents/skills`, `~/.codex/skills`,
`~/.claude/skills`, or `~/.cursor/skills`. Resolve symlinks first; aliases to
the same canonical skill directory are one installation. If distinct roots
contain the skill, report the ambiguity and choose one source explicitly.

This repo's `.agents/skills/effigy` copy is optional project-local content,
not the maintained guidance source. Preserve it if it exists; plain init does
not create or refresh it. The named `skill.codex_project` init action is an
explicit snapshot opt-in and may replace files at maintained paths. Named-skill lookup (`effigy skill run <SKILL>/<task>`) still gives an
invocation project's local skill source precedence, as defined by contract 042.

If no installed Effigy Agent Skill is present, say so and suggest
`npx skills add inflatable-cookie/effigy -g`; init does not download or install
skills. A filesystem check cannot prove what an already-running agent loaded;
use a fresh agent context to verify discovery.

Agent Skill guidance and the `effigy` executable are separate channels. A
current skill does not prove the binary on `PATH` is current; check the
resolved executable independently with `command -v effigy` and
`effigy --version`. Heavy selectors route through the Queue/Nucleus scheduler;
Effigy has no local admission capability query.

Do not add a current-directory repo override while already inside the target
repo. Do not edit
`.github/workflows/` or run release mutations unless the user explicitly asks.

Upstream Effigy reference docs (in the Effigy repository, not paths in this
repo):
- [Effigy agent adoption](https://github.com/inflatable-cookie/effigy/blob/main/docs/guides/047-agent-and-cross-repo-adoption.md)
- [Installed skill task sources](https://github.com/inflatable-cookie/effigy/blob/main/docs/knowledge/contracts/042-external-skill-task-runner-contract.md)
- [Heavy validation admission](https://github.com/inflatable-cookie/effigy/blob/main/docs/guides/080-host-wide-validation-admission.md)
- [JSON contracts](https://github.com/inflatable-cookie/effigy/blob/main/docs/guides/017-json-output-contracts.md)
<!-- END EFFIGY AGENT CONTRACT -->
"#;

pub(super) const GITIGNORE_BLOCK_START: &str = "# BEGIN EFFIGY LOCAL STATE";
pub(super) const GITIGNORE_BLOCK_END: &str = "# END EFFIGY LOCAL STATE";

pub(super) const GITIGNORE_BLOCK: &str = r#"# BEGIN EFFIGY LOCAL STATE
.effigy/
# END EFFIGY LOCAL STATE
"#;

/// Every file the named `skill.codex_project` snapshot writes, in the order it
/// writes them. The managed registry test keeps this list equal to the public
/// skill tree it vendors.
pub(super) const SKILL_FILES: &[(&str, &str)] = &[
    (
        "SKILL.md",
        include_str!("../../../../../skills/effigy/SKILL.md"),
    ),
    (
        "references/agent-operating-loop.md",
        include_str!("../../../../../skills/effigy/references/agent-operating-loop.md"),
    ),
    (
        "references/built-in-surfaces.md",
        include_str!("../../../../../skills/effigy/references/built-in-surfaces.md"),
    ),
    (
        "references/config-shapes.md",
        include_str!("../../../../../skills/effigy/references/config-shapes.md"),
    ),
    (
        "references/first-five-commands.md",
        include_str!("../../../../../skills/effigy/references/first-five-commands.md"),
    ),
    (
        "references/footguns.md",
        include_str!("../../../../../skills/effigy/references/footguns.md"),
    ),
    (
        "references/json-envelope.md",
        include_str!("../../../../../skills/effigy/references/json-envelope.md"),
    ),
    (
        "references/release-protocol.md",
        include_str!("../../../../../skills/effigy/references/release-protocol.md"),
    ),
    (
        "references/selector-routing.md",
        include_str!("../../../../../skills/effigy/references/selector-routing.md"),
    ),
    (
        "references/workflow-shortcuts.md",
        include_str!("../../../../../skills/effigy/references/workflow-shortcuts.md"),
    ),
];

pub(super) const INTERNAL_SKILL_METADATA_BLOCK: &str = "metadata:\n  internal: true\n";

pub(super) const INSTALLED_SKILL_ROOTS: &[&str] = &[
    ".agents/skills",
    ".codex/skills",
    ".claude/skills",
    ".cursor/skills",
];

#[cfg(test)]
mod tests {
    use super::{AGENTS_BLOCK, AGENTS_BLOCK_END, AGENTS_BLOCK_START, GITIGNORE_BLOCK};

    /// Every upstream document the contract block cites must exist in this
    /// repository. The block is embedded text, so a renamed guide would
    /// otherwise leave the contract pointing at nothing.
    #[test]
    fn every_document_the_contract_cites_exists_in_this_repository() {
        let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let prefix = "https://github.com/inflatable-cookie/effigy/blob/main/";
        let mut cited = 0;
        for line in AGENTS_BLOCK.lines() {
            let Some(start) = line.find(prefix) else {
                continue;
            };
            let rest = &line[start + prefix.len()..];
            let path = rest.split(')').next().expect("a link target ends");
            assert!(
                repo_root.join(path).is_file(),
                "the agent contract cites `{path}`, which is not a file in this repository"
            );
            cited += 1;
        }
        assert!(
            cited >= 4,
            "the contract's upstream references were not read"
        );
    }

    #[test]
    fn the_contract_block_stays_inside_its_markers() {
        assert!(AGENTS_BLOCK.starts_with(AGENTS_BLOCK_START));
        assert!(AGENTS_BLOCK.trim_end().ends_with(AGENTS_BLOCK_END));
        assert_eq!(
            AGENTS_BLOCK.matches(AGENTS_BLOCK_START).count(),
            1,
            "one managed block per file"
        );
        assert_eq!(AGENTS_BLOCK.matches(AGENTS_BLOCK_END).count(), 1);
        assert!(AGENTS_BLOCK.contains("## Effigy Agent Contract"));
        assert!(GITIGNORE_BLOCK.lines().next().is_some());
    }
}
