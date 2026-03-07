# Track 10: Environment and Secret Management

Status: Draft
Track: Environment and Secret Management
Owner:
Last updated: 2026-03-07
Primary Effigy tags: `SECURITY`, `UX`, `CONFIG`

## 1) Problem statement

How should environment variables and secrets be managed? What balances:
- Security (secrets not exposed)
- Convenience (easy for developers)
- Flexibility (different environments)
- Automation (CI/CD support)

## 2) Why this track matters to Effigy

Effigy handles environment variables. Research validates:
- .env file loading patterns
- Secret management approaches
- Environment variable precedence
- Security best practices

## 3) Cross-tool comparison

| Tool | Approach | Security | Convenience | Best For |
|------|----------|----------|-------------|----------|
| dotenv | .env files | Low | High | Development |
| direnv | Directory-specific | Medium | High | Project switching |
| 1Password CLI | Secret injection | High | Medium | Production |
| Effigy (current) | TOML env section | Medium | High | Task-specific |

### Environment Management Spectrum

**Simple (dotenv)**
```bash
# .env
API_KEY=secret123
```
- Pros: Simple, widely supported
- Cons: Secrets in files

**Directory-specific (direnv)**
```bash
# .envrc
export API_KEY="secret123"
```
- Pros: Automatic loading
- Cons: Requires shell hook

**Secure (1Password CLI)**
```bash
op://vault/item/field
```
- Pros: No secrets in files
- Cons: Requires service, complexity

**Task-specific (Effigy)**
```toml
[env]
API_KEY = "${LOCAL_API_KEY}"
```
- Pros: Scoped to task
- Cons: Configuration needed

## 4) Repeated patterns

### Universal environment needs

1. **Loading from files**
   - .env files
   - Multiple environments (.env.local, .env.production)
   - Variable expansion

2. **Precedence**
   - Process env > file > default
   - Task-specific overrides
   - Catalog-level settings

3. **Secret handling**
   - Don't commit secrets
   - Runtime injection
   - Audit trail

4. **Development vs. production**
   - Different sources
   - Different security requirements
   - Different tooling

### Tool-specific innovations

**direnv: Automatic loading**
- Load on directory enter
- Unload on exit
- Shell hook integration

**1Password CLI: Secret references**
- Path-based references
- Runtime resolution
- No secrets in code

**dotenv: Simplicity**
- Single file
- Key=value format
- Universal support

## 5) Frontier research signals

- **Secret scanning**: Automatically detect secrets in code
- **Short-lived credentials**: Dynamic secrets
- **Hardware security keys**: YubiKey, etc.
- **Cloud secret managers**: AWS Secrets Manager, Azure Key Vault

## 6) Effigy implications

### Recommended direction

**Current approach + enhancements:**

1. **Keep TOML env section** (current)
   ```toml
   [env]
   DATABASE_URL = "postgres://localhost/dev"
   
   [tasks.prod.env]
   DATABASE_URL = "postgres://prod/db"
   ```

2. **Add .env file loading**
   ```toml
   [env]
   env_file = ".env"
   # Or multiple
   env_file = [".env", ".env.local"]
   ```

3. **Add secret provider integration**
   ```toml
   [env]
   API_KEY = { secret = "1password://Production/API/key" }
   # Or generic
   API_KEY = { secret = "${API_KEY_SECRET}" }
   ```

4. **Clear precedence rules**
   ```
   1. Task-specific env
   2. Catalog-level env
   3. Process environment
   4. .env file
   5. Default values
   ```

### Risks to avoid

1. **Secrets in git**: Never commit secrets
2. **Over-complexity**: Keep simple cases simple
3. **Vendor lock-in**: Support multiple providers

### Evidence or prototype needed

- [ ] .env file loading implementation
- [ ] Secret provider interface
- [ ] Security review

## 7) Implementation suggestions

### .env file loading

```toml
[catalog]
alias = "api"

[env]
# Load from file
env_file = ".env"

# Or inline (overrides file)
DEBUG = "true"
```

### Secret references

```toml
[env]
# Generic secret reference
API_KEY = { secret = "op://Production/API/key" }

# Or environment variable reference
API_KEY = { env = "PROD_API_KEY" }

# Or default with override
API_KEY = { default = "dev-key", env = "API_KEY" }
```

### Precedence

```rust
pub fn resolve_env(
    key: &str,
    task_env: Option<&Value>,
    catalog_env: Option<&Value>,
    process_env: &Env,
    env_files: &[EnvFile],
) -> Option<String> {
    // 1. Task-specific
    if let Some(v) = task_env.and_then(|e| e.get(key)) {
        return Some(v.clone());
    }
    // 2. Catalog-level
    if let Some(v) = catalog_env.and_then(|e| e.get(key)) {
        return Some(v.clone());
    }
    // 3. Process environment
    if let Some(v) = process_env.get(key) {
        return Some(v.clone());
    }
    // 4. .env file
    for file in env_files {
        if let Some(v) = file.get(key) {
            return Some(v.clone());
        }
    }
    None
}
```

## 8) Comparison: Approaches

| Approach | Pros | Cons | Effigy |
|----------|------|------|--------|
| .env files | Simple | Insecure | ✅ Support |
| direnv | Automatic | Shell hook | ⚠️ Optional |
| 1Password | Secure | Complex | ✅ Support |
| **Effigy** | **Integrated** | **Learning** | **✅ Keep** |

## 9) Source inventory

| Source | Type | Confidence | Notes |
|--------|------|------------|-------|
| direnv dossier | high | Directory-specific env |
| 1Password CLI dossier | high | Secret injection |
| dotenv documentation | high | File format |

## 10) Decision state

- [ ] `promote to concept work` — Document env strategy
- [ ] `continue research` — Current approach good
- [ ] `prototype first` — Test secret provider

**Current leaning**: Keep current TOML env section, add .env loading and secret provider interface.

## Next Task

1. Draft Translation Memo 010: Environment Strategy
2. Complete Phase 2 research

