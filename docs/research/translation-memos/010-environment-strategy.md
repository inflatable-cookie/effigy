# Translation Memo 010: Environment Strategy

Status: Draft
Memo: 010
Owner: Research
Last updated: 2026-03-07
Related track: Track 10 — Environment and Secret Management

## 1) Effigy problem statement

Effigy handles environment variables via TOML. Research validates:
- Current approach is good
- Could add .env file loading
- Could add secret provider integration
- Need clear precedence rules

## 2) External evidence summary

From comparative analysis of direnv and 1Password CLI:

**direnv**:
- Directory-specific environment
- Automatic loading/unloading
- Shell hook required
- Good for project switching

**1Password CLI**:
- Secret injection
- No secrets in files
- Service accounts for CI
- Subscription required

**Patterns**:
- Multiple approaches for different needs
- Security vs. convenience tradeoff
- Clear precedence is essential
- Don't commit secrets

## 3) Recommendation

**Keep TOML env section with enhancements:**

### Current (keep)

```toml
[env]
DATABASE_URL = "postgres://localhost/dev"

[tasks.prod.env]
DATABASE_URL = "postgres://prod/db"
```

### Add .env file loading

```toml
[env]
env_file = ".env"  # Load from file
DEBUG = "true"     # Override/add
```

### Add secret references

```toml
[env]
# From secret provider
API_KEY = { secret = "1password://Production/API/key" }

# From environment variable
API_KEY = { env = "PROD_API_KEY" }

# With default
API_KEY = { default = "dev-key", env = "API_KEY" }
```

### Clear precedence

1. Task-specific env
2. Catalog-level env  
3. Process environment
4. .env file
5. Default values

### Not recommended

- direnv dependency: Don't require shell hooks
- 1Password exclusivity: Support multiple providers
- Secrets in TOML: Never commit secrets

## 4) Tradeoffs Effigy accepts

| Tradeoff | Cost | Mitigation |
|----------|------|------------|
| TOML configuration | More verbose than .env | Both supported |
| Secret provider complexity | Setup required | Optional feature |
| Precedence complexity | Must understand order | Clear documentation |

## 5) What must be true before adoption

- [x] TOML env section works
- [ ] .env file loading
- [ ] Secret provider interface
- [ ] Security review

## 6) Required prototype or validation work

**Phase 1: .env file loading**
- [ ] Parse .env format
- [ ] Variable expansion
- [ ] Multiple file support

**Phase 2: Secret providers**
- [ ] Generic interface
- [ ] 1Password integration
- [ ] Environment fallback

**Phase 3: Documentation**
- [ ] Precedence examples
- [ ] Security best practices
- [ ] CI/CD patterns

## 7) Promotion target

- [x] `concept contract work` — Document env strategy
- [ ] `roadmap execution planning` — Implementation roadmap
- [ ] `watch only` — Not applicable
- [ ] `reject` — Not applicable

## 8) Sources

| Source | Confidence | Notes |
|--------|------------|-------|
| direnv dossier | high | Directory-specific env |
| 1Password CLI dossier | high | Secret injection |
| Track 10 synthesis | high | Integrated approach validated |

## 9) Implementation plan

### Phase 1: .env loading

```rust
pub fn load_env_file(path: &Path) -> Result<HashMap<String, String>> {
    let content = fs::read_to_string(path)?;
    parse_dotenv(&content)
}

fn parse_dotenv(content: &str) -> HashMap<String, String> {
    // Handle KEY=VALUE
    // Handle KEY="value with spaces"
    // Handle export KEY=value
    // Ignore comments
}
```

### Phase 2: Secret providers

```rust
pub trait SecretProvider {
    fn resolve(&self, reference: &str) -> Result<String>;
}

pub struct OnePasswordProvider;
pub struct EnvironmentProvider;
```

### Phase 3: Configuration

```toml
[env]
# Load from files
env_file = [".env", ".env.local"]

# Inline values (override files)
DEBUG = "true"

# Secret references
API_KEY = { secret = "op://Production/API/key" }
```

## 10: Phase 2 Completion Summary

Track 10 completes Phase 2 research (Developer Experience).

All 10 Phase 2 recommendations:
- 006: Hybrid shell completions
- 007: Rustc-style error format
- 008: Distributed + change detection
- 009: Native shell + platform conditionals
- 010: TOML env + .env loading + secrets

## Next Task

1. Create Phase 2 completion summary
2. Plan Phase 3 (Scale & Integration) or begin implementation

