# 025 - Varlock @env-spec Integration

Generation: `g01`

Status: Complete
Owner: Platform
Created: 2026-03-10
Depends on: 015

## Vision Alignment

This roadmap implements native environment variable schema validation and
secret resolution in Effigy. Projects declare their environment contracts in
`.env.schema` files using the @env-spec DSL, and Effigy resolves, validates,
and securely injects variables at task execution time.

Completion note:
- The original checklist included an "unsafe memory inspection after drop"
  proof target for zeroization. Effigy now ships practical owned-buffer
  zeroization verification before deallocation, but the stricter post-drop
  inspection style is intentionally deferred because reading freed memory would
  rely on undefined behavior and would not be a trustworthy regression test.

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

Implement a Rust parser for the @env-spec DSL.

Location: `src/env_schema/parser.rs` (with AST types in `src/env_schema/ast.rs`)

The parser must handle:
- Comment annotations: `# @type=port @default=3000 @required @sensitive`
- Value expressions: literals, `exec('command')`, `env('VAR')`, `${VAR}`
  template interpolation
- Type annotations: `port`, `url`, `enum(a,b,c)`, `string` with constraints
- Flags: `@required`, `@sensitive`, `@optional`

Tasks:
- [x] Define AST types for schema representation (`EnvSchema`, `EnvEntry`,
  `EnvAnnotation`, `EnvValue`, `EnvType`)
- [x] Implement parser for comment-line annotations (`@key=value` pairs)
- [x] Implement parser for value expressions (literals, `exec()`, `env()`,
  templates)
- [x] Implement parser for full `.env.schema` files (entries with annotations)
- [x] Add structured error types with line numbers for parse failures
- [x] Test against @env-spec RFC examples from varlock discussions

## 2) Resolution Engine

Implement resolution of environment values from multiple sources.

Location: `src/env_schema/resolver.rs`

Resolution priority (highest to lowest):
1. Explicit environment (process env)
2. Local `.env` file overrides
3. Schema defaults and computed values
4. `exec()` command output
5. `env()` references to other variables
6. Template interpolation (`${VAR}`)

Tasks:
- [x] Implement `ResolvedEnv` container holding all resolved values
- [x] Implement `exec()` resolution with configurable timeout (default 30s)
- [x] Implement `env()` resolution with environment variable lookup
- [x] Implement template interpolation (`${VAR}` expansion)
- [x] Implement circular dependency detection (A references B references A)
- [x] Implement resolution ordering (topological sort of dependencies)
- [x] Implement caching of resolved values within a resolution pass
- [x] Add `.env` file loading with override semantics

## 3) Security Types

Implement secure string handling for sensitive environment values.

Location: `src/env_schema/secret.rs`

Tasks:
- [x] Implement `SecretString` type wrapping `zeroize::Zeroizing<String>`
- [x] Implement `EnvValue` enum: `Plain(String)` vs `Secret(SecretString)`
- [x] Override `Display` to show `[REDACTED]` for secrets
- [x] Override `Debug` to show `SecretString(****)` for secrets
- [x] Verify zeroization occurs on drop (practical owned-buffer zeroization is
  covered; strict post-drop unsafe memory inspection is intentionally deferred
  as out of scope for stable regression tests)
- [x] Ensure secrets are never logged by Effigy's output system

## 4) Type Validation

Implement validators for the @env-spec type system.

Location: `src/env_schema/validator.rs`

Supported types:
- `port` - integer 1-65535
- `url` - valid URL format
- `enum(a,b,c)` - value must be one of the listed options
- `string` - with optional min/max length constraints
- Pattern matching via regex (if `@pattern` annotation present)

Tasks:
- [x] Implement `Validator` trait with `validate(&self, value: &str) -> Result`
- [x] Implement `PortValidator` (1-65535 range check)
- [x] Implement `UrlValidator` (URL format validation)
- [x] Implement `EnumValidator` (membership check)
- [x] Implement `StringValidator` (length constraints)
- [x] Implement `PatternValidator` (regex matching)
- [x] Produce structured validation errors with variable name and expected type

## 5) Module Integration

Wire the parser, resolver, and validator into a cohesive public API.

Location: `src/env_schema.rs`

```rust
// Public API sketch
pub fn load_env_schema(path: &Path) -> Result<EnvSchema>;
pub async fn resolve_env(schema: &EnvSchema, env: &Environment) -> Result<ResolvedEnv>;
pub fn validate_env(schema: &EnvSchema, resolved: &ResolvedEnv) -> Vec<ValidationError>;
```

Tasks:
- [x] Create `src/env/mod.rs` with public API re-exports
- [x] Implement `.env.schema` auto-detection in project root
- [x] Implement schema loading with parse error reporting
- [x] Implement full resolve-then-validate pipeline
- [x] Add `ResolvedEnv` method to export as `HashMap<String, EnvValue>`

## 6) Runtime Integration

Connect environment resolution to Effigy's task execution system.

Location: `src/runtime.rs` (modifications)

Tasks:
- [x] Load `.env.schema` during runtime initialization (when present)
- [x] Resolve environment variables before task execution
- [x] Pass resolved variables to child process environment
- [x] Make resolved values available internally for conditional logic
- [x] Add `--env-schema` flag to override schema path
- [x] Report resolution and validation errors before task execution starts

## 7) Configuration

Add `[env_schema]` section to `effigy.toml` for controlling integration behavior.

Location: `src/runner/manifest/config_sections.rs`

```toml
[env_schema]
# Enable/disable env-spec integration (default: true when .env.schema exists)
enabled = true
# Override schema file path (default: .env.schema in project root)
schema = ".env.schema"
# Exec timeout in seconds (default: 30)
exec_timeout = 30
```

Tasks:
- [x] Define `EnvConfig` struct with serde deserialization
- [x] Add `[env]` section support to effigy.toml parsing
- [x] Implement defaults (enabled when schema exists, 30s timeout)
- [x] Validate configuration on load

## 8) Tests

Comprehensive test coverage for all modules.

Location: `tests/env_*.rs`

Tasks:
- [x] Parser unit tests: annotations, value expressions, full schemas
- [x] Resolver unit tests covering `exec()` behavior and timeouts
- [x] Resolver tests for circular dependency detection
- [x] Secret handling tests: zeroization, redaction in Display/Debug
- [x] Validator tests: ports, URLs, enums, strings, patterns
- [x] Integration tests: full schema load → resolve → validate pipeline
- [x] Integration tests: resolved env passed to task execution
- [x] Edge case tests: empty schemas, missing files, UTF-8 content

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
