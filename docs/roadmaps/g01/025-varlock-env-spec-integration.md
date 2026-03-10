# 025 - Varlock @env-spec Integration

Generation: `g01`

Status: Planned
Owner: Platform
Created: 2026-03-10
Depends on: 015

## Vision Alignment

This roadmap implements native environment variable schema validation and
secret resolution in Effigy. Projects declare their environment contracts in
`.env.schema` files using the @env-spec DSL, and Effigy resolves, validates,
and securely injects variables at task execution time.

## Primary Tags

- `OPERATE`
- `MAINT`

## Target Envelope

- Effigy natively parses `.env.schema` files using the @env-spec grammar.
- Environment variables are resolved from defaults, `.env` overrides, and
  external providers (via `exec()`).
- Secrets are handled securely in memory with zeroization on drop.
- Type validation catches configuration errors before task execution.
- Integration is opt-in and does not change behavior for projects without
  `.env.schema`.

## Vision Target Delta

- Moved from `no environment schema awareness` toward `declarative environment
  contracts with secure secret resolution and type validation`.

## 1) @env-spec Parser

Implement a Rust parser for the @env-spec DSL using `nom` parser combinators.

Location: `src/env/parser.rs` (with AST types in `src/env/ast.rs`)

The parser must handle:
- Comment annotations: `# @type=port @default=3000 @required @sensitive`
- Value expressions: literals, `exec('command')`, `env('VAR')`, `${VAR}`
  template interpolation
- Type annotations: `port`, `url`, `enum(a,b,c)`, `string` with constraints
- Flags: `@required`, `@sensitive`, `@optional`

Tasks:
- [ ] Define AST types for schema representation (`EnvSchema`, `EnvEntry`,
  `EnvAnnotation`, `EnvValue`, `EnvType`)
- [ ] Implement nom parser for comment-line annotations (`@key=value` pairs)
- [ ] Implement nom parser for value expressions (literals, `exec()`, `env()`,
  templates)
- [ ] Implement parser for full `.env.schema` files (entries with annotations)
- [ ] Add structured error types with line numbers for parse failures
- [ ] Test against @env-spec RFC examples from varlock discussions

## 2) Resolution Engine

Implement async resolution of environment values from multiple sources.

Location: `src/env/resolver.rs`

Resolution priority (highest to lowest):
1. Explicit environment (process env)
2. Local `.env` file overrides
3. Schema defaults and computed values
4. `exec()` command output
5. `env()` references to other variables
6. Template interpolation (`${VAR}`)

Tasks:
- [ ] Implement `ResolvedEnv` container holding all resolved values
- [ ] Implement `exec()` resolution with configurable timeout (default 30s)
- [ ] Implement `env()` resolution with environment variable lookup
- [ ] Implement template interpolation (`${VAR}` expansion)
- [ ] Implement circular dependency detection (A references B references A)
- [ ] Implement resolution ordering (topological sort of dependencies)
- [ ] Implement caching of resolved values within a resolution pass
- [ ] Add `.env` file loading with override semantics

## 3) Security Types

Implement secure string handling for sensitive environment values.

Location: `src/env/secret.rs`

Tasks:
- [ ] Implement `SecretString` type wrapping `zeroize::Zeroizing<String>`
- [ ] Implement `EnvValue` enum: `Plain(String)` vs `Secret(SecretString)`
- [ ] Override `Display` to show `[REDACTED]` for secrets
- [ ] Override `Debug` to show `SecretString(****)` for secrets
- [ ] Verify zeroization occurs on drop (test with unsafe memory inspection)
- [ ] Ensure secrets are never logged by Effigy's output system

## 4) Type Validation

Implement validators for the @env-spec type system.

Location: `src/env/validator.rs`

Supported types:
- `port` - integer 1-65535
- `url` - valid URL format
- `enum(a,b,c)` - value must be one of the listed options
- `string` - with optional min/max length constraints
- Pattern matching via regex (if `@pattern` annotation present)

Tasks:
- [ ] Implement `Validator` trait with `validate(&self, value: &str) -> Result`
- [ ] Implement `PortValidator` (1-65535 range check)
- [ ] Implement `UrlValidator` (URL format validation)
- [ ] Implement `EnumValidator` (membership check)
- [ ] Implement `StringValidator` (length constraints)
- [ ] Implement `PatternValidator` (regex matching)
- [ ] Produce structured validation errors with variable name and expected type

## 5) Module Integration

Wire the parser, resolver, and validator into a cohesive public API.

Location: `src/env/mod.rs`

```rust
// Public API sketch
pub fn load_env_schema(path: &Path) -> Result<EnvSchema>;
pub async fn resolve_env(schema: &EnvSchema, env: &Environment) -> Result<ResolvedEnv>;
pub fn validate_env(schema: &EnvSchema, resolved: &ResolvedEnv) -> Vec<ValidationError>;
```

Tasks:
- [ ] Create `src/env/mod.rs` with public API re-exports
- [ ] Implement `.env.schema` auto-detection in project root
- [ ] Implement schema loading with parse error reporting
- [ ] Implement full resolve-then-validate pipeline
- [ ] Add `ResolvedEnv` method to export as `HashMap<String, EnvValue>`

## 6) Runtime Integration

Connect environment resolution to Effigy's task execution system.

Location: `src/runtime.rs` (modifications)

Tasks:
- [ ] Load `.env.schema` during runtime initialization (when present)
- [ ] Resolve environment variables before task execution
- [ ] Pass resolved variables to child process environment
- [ ] Make resolved values available internally for conditional logic
- [ ] Add `--env-schema` flag to override schema path
- [ ] Report resolution and validation errors before task execution starts

## 7) Configuration

Add `[env]` section to `effigy.toml` for controlling integration behavior.

Location: `src/config/env.rs`

```toml
[env]
# Enable/disable env-spec integration (default: true when .env.schema exists)
enabled = true
# Override schema file path (default: .env.schema in project root)
schema = ".env.schema"
# Exec timeout in seconds (default: 30)
exec-timeout = 30
```

Tasks:
- [ ] Define `EnvConfig` struct with serde deserialization
- [ ] Add `[env]` section support to effigy.toml parsing
- [ ] Implement defaults (enabled when schema exists, 30s timeout)
- [ ] Validate configuration on load

## 8) Tests

Comprehensive test coverage for all modules.

Location: `tests/env_*.rs`

Tasks:
- [ ] Parser unit tests: annotations, value expressions, full schemas
- [ ] Resolver unit tests with mocked `exec()` commands
- [ ] Resolver tests for circular dependency detection
- [ ] Secret handling tests: zeroization, redaction in Display/Debug
- [ ] Validator tests: ports, URLs, enums, strings, patterns
- [ ] Integration tests: full schema load → resolve → validate pipeline
- [ ] Integration tests: resolved env passed to task execution
- [ ] Edge case tests: empty schemas, missing files, UTF-8 content

## Completion Criteria

This roadmap is complete when:
1. Parser handles all @env-spec constructs documented in the research.
2. `exec()` resolution works with real external providers (e.g., `op read`).
3. Template interpolation and `env()` references resolve correctly.
4. Secrets are zeroized on drop and redacted in all output.
5. Type validation catches invalid values with clear error messages.
6. `.env.schema` is auto-detected and resolved during `effigy run`.
7. Local `.env` file overrides schema defaults.
8. All tests pass: `cargo test`.

## Dependencies

- `nom` - parser combinators
- `tokio` - async exec() resolution
- `zeroize` - memory clearing for secrets
- `thiserror` - structured error types

## Watch-outs

- **Circular dependencies**: Template `${A}` referencing `${B}` referencing
  `${A}` must be detected and reported, not cause infinite loops.
- **Exec timeouts**: Hanging commands (e.g., broken 1Password CLI) must not
  block Effigy indefinitely. Default 30s timeout with configuration override.
- **Secret exposure**: Debug logs, error messages, and JSON output must never
  print secret values. Audit all Display/Debug impls.
- **Unicode**: `.env.schema` files may contain UTF-8. Handle encoding carefully
  in the parser.

## Reference Documents

- Handoff: `docs/handoffs/varlock-integration-implementation.md`
- Research: `docs/research/value-tracks/16-secure-secrets-management.md`
- @env-spec RFC: https://github.com/dmno-dev/varlock/discussions/17
