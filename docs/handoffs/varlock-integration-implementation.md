---
title: Varlock Integration Implementation Handoff
status: active
owner: nucleus
updated: 2026-03-07
tags: [handoff, varlock, secrets, env-spec]
---

## Objective

Implement a Rust-based @env-spec parser and resolver that integrates Varlock-style environment variable management into Effigy, enabling schema validation, external secret resolution, and secure in-process secret handling.

## Scope

- Implement @env-spec parser in Rust ( Pest or nom )
- Build resolution engine for `exec()`, `env()`, templates
- Create `SecretString` type with zeroization
- Integrate with Effigy's task execution system
- Support `.env.schema` detection and resolution
- Add validation for types (port, url, enum, string constraints)

**Explicitly out of scope:**
- Varlock CLI binary (we're implementing our own parser)
- Encryption at rest (Varlock doesn't do this yet)
- Git-crypt or age integration (separate feature)
- GUI or web interface

## Inputs

- Research docs: `~/Dev/projects/effigy/docs/research/tool-dossiers/varlock.md`
- Track 16 synthesis: `~/Dev/projects/effigy/docs/research/value-tracks/16-secure-secrets-management.md`
- Translation memo 016c: `~/Dev/projects/effigy/docs/research/translation-memos/016c-varlock-integration.md`
- @env-spec RFC: https://github.com/dmno-dev/varlock/discussions/17

## Constraints

- Must preserve repository conventions in `AGENTS.md`
- Keep edits minimal and diff-friendly
- Use existing Effigy patterns for config loading
- Secrets must be zeroized on drop
- Exec calls must have timeouts
- Don't widen scope beyond what's listed

## Deliverables

### 1. Parser Module
`~/Dev/projects/effigy/src/env/parser.rs`
- @env-spec grammar implementation
- AST types for schema representation
- Error types for parse failures

### 2. Resolution Engine
`~/Dev/projects/effigy/src/env/resolver.rs`
- Async resolution of `exec()` commands
- Template interpolation (`${VAR}`)
- Environment variable fallback
- Caching of resolved values

### 3. Security Types
`~/Dev/projects/effigy/src/env/secret.rs`
- `SecretString` with `zeroize::Zeroize`
- `EnvValue` enum (Plain vs Secret)
- Redaction for Display/Debug

### 4. Validation
`~/Dev/projects/effigy/src/env/validator.rs`
- Type validators (port, url, enum, string constraints)
- Pattern matching with regex
- Custom validation errors

### 5. Integration
`~/Dev/projects/effigy/src/env/mod.rs`
- Public API for Effigy runtime
- `ResolvedEnv` container
- Detection of `.env.schema`

### 6. Runtime Integration
`~/Dev/projects/effigy/src/runtime.rs` (modifications)
- Load env during runtime initialization
- Pass env vars to task execution
- Internal access for conditional logic

### 7. Configuration
`~/Dev/projects/effigy/src/config/env.rs` (new)
- `[env]` section in effigy.toml
- Enable/disable varlock integration
- Schema path override

### 8. Tests
`~/Dev/projects/effigy/tests/env_*.rs`
- Parser tests
- Resolver tests (with mocked exec)
- Secret handling tests
- Integration tests

## Acceptance Criteria

- [ ] Parser handles all @env-spec constructs from research docs
- [ ] `exec('op read "op://vault/item/field"')` resolves correctly
- [ ] `${PORT}` template interpolation works
- [ ] Secrets are zeroized when dropped (verified with test)
- [ ] Validation catches invalid port numbers, malformed URLs
- [ ] Sensitive values are redacted in Debug output
- [ ] Timeout on exec() commands (configurable, default 30s)
- [ ] `.env.schema` auto-detected in project root
- [ ] Local `.env` file overrides schema defaults
- [ ] Effigy can access env vars internally for conditional logic
- [ ] All tests pass: `cargo test`

## Notes

### Context
This is Track 16 research culmination. We evaluated Varlock, SOPS, git-crypt, age, and Doppler. Decision: implement our own @env-spec parser rather than wrapping Varlock CLI because:
1. Varlock is JS/Node-based, we'd need to spawn processes
2. We need in-process access to secrets (not just pass-through)
3. Full control over security (zeroization, timeouts)
4. No external binary dependency

### Essence
The key insight is that @env-spec is a well-designed DSL that Varlock created. We implement the parser/resolver ourselves in Rust, getting all the benefits without the Node.js dependency. The `.env.schema` file lives alongside code (not in `effigy.toml`), which is actually the right design.

### User Workflow
```bash
# 1. Create .env.schema
cat > .env.schema << 'EOF'
# @type=port @default=3000
PORT=3000

# @type=url @required
DATABASE_URL=

# @sensitive @type=string
STRIPE_KEY=exec('op read "op://Prod/Stripe/key"')
EOF

# 2. Run effigy
effigy run dev
# [effigy] Loading .env.schema
# [effigy] ✓ 3 variables resolved
# [dev] Server started on port 3000
```

### Architecture Decisions
- **Parser**: Use `nom` for parser combinators (more idiomatic Rust than Pest)
- **Async**: Tokio for async exec() resolution
- **Security**: `zeroize` crate for memory clearing
- **Error handling**: ThisError for structured errors

### Watch-outs
- **Circular dependencies**: Template `${A}` referencing `${B}` referencing `${A}`
- **Exec timeouts**: Hanging commands should not block forever
- **Secret exposure**: Debug logs must never print secrets
- **Unicode**: env-spec files may have UTF-8, handle carefully

### What to try next
1. Create the grammar module with nom - start with simple KEY=value parsing
2. Define AST types in `src/env/ast.rs`
3. Build a test case with exec() mocking

## Completion Protocol

1. Update `updated:` metadata in this file when complete
2. Create log entry in `~/Dev/projects/effigy/logs/2026-03-07-varlock-integration.md`
3. Summarize outcomes and any unresolved risks:
   - Performance of exec() resolution (cache strategy)
   - Security audit of SecretString implementation
   - Compatibility with future @env-spec versions
