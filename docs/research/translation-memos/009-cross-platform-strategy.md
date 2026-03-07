# Translation Memo 009: Cross-Platform Strategy

Status: Draft
Memo: 009
Owner: Research
Last updated: 2026-03-07
Related track: Track 09 — Cross-Platform Portability

## 1) Effigy problem statement

Effigy targets macOS, Linux, and Windows. Research validates:
- Current approach (native shell) is appropriate
- Need platform conditionals for cross-platform tasks
- Path handling needs attention
- Windows CI testing essential

## 2) External evidence summary

From comparative analysis of Just, Deno, and Task:

**Just**:
- Shell abstraction (sh everywhere)
- Consistent but less native feel
- Excellent Windows support

**Deno**:
- Single binary, Rust-based
- Cross-platform by design
- No external dependencies

**Task**:
- Go-based, cross-compiled
- Built-in shell handling
- Native on all platforms

**Patterns**:
- Single binary distribution helps
- Native shell gives flexibility
- Abstraction adds complexity
- Test on all platforms

## 3) Recommendation

**Keep native shell approach with enhancements:**

### Current (keep)

Users write commands in their platform's native shell:
```toml
[tasks.build]
run = "cargo build"  # Works on user's shell
```

### Add platform conditionals

```toml
# Option 1: Inline conditionals
[tasks.build]
run = [
    { if = "windows", run = "build.bat" },
    { if = "unix", run = "./build.sh" }
]

# Option 2: Platform-specific sections
[tasks.build]
run = "./build.sh"

[tasks.build.windows]
run = "build.bat"
```

### Add path normalization

```toml
[tasks.build]
# Effigy normalizes paths
run = "compile {project}/src/main.rs -o {project}/out"
```

### Not recommended

- Shell abstraction (Just approach): Too complex
- Ignoring Windows: Must support all platforms
- Assuming /bin/sh everywhere: Windows users expect PowerShell

## 4) Tradeoffs Effigy accepts

| Tradeoff | Cost | Mitigation |
|----------|------|------------|
| Native shell | Platform-specific scripts | Document examples |
| No abstraction | Users must know shell | Provide templates |
| Testing burden | Must test on 3 platforms | CI coverage |

## 5) What must be true before adoption

Already true:
- [x] Rust provides cross-platform APIs
- [x] Single binary distribution
- [ ] Platform conditionals implemented
- [ ] Windows CI testing

## 6) Required prototype or validation work

**Phase 1: Platform conditionals**
- [ ] Design syntax (inline vs. sections)
- [ ] Implement parsing
- [ ] Test on all platforms

**Phase 2: Path handling**
- [ ] Path normalization
- [ ] Home directory expansion
- [ ] Cross-platform path separators

**Phase 3: Testing**
- [ ] Windows CI
- [ ] Linux CI
- [ ] macOS CI

## 7) Promotion target

- [x] `concept contract work` — Document portability approach
- [ ] `roadmap execution planning` — Implementation roadmap
- [ ] `watch only` — Not applicable
- [ ] `reject` — Not applicable

## 8) Sources

| Source | Confidence | Notes |
|--------|------------|-------|
| Just dossier | high | Shell abstraction patterns |
| Deno dossier | high | Single binary, Rust |
| Track 09 synthesis | high | Native shell validated |

## 9) Implementation plan

### Platform conditionals

```rust
#[derive(Serialize, Deserialize)]
struct RunStep {
    #[serde(default)]
    if_platform: Option<Platform>,
    run: String,
}

pub fn should_run(step: &RunStep) -> bool {
    match &step.if_platform {
        None => true,
        Some(p) => p == &Platform::current(),
    }
}
```

### Path normalization

```rust
pub fn expand_path(path: &str, project_root: &Path) -> PathBuf {
    let path = if path.starts_with("~/") {
        home_dir().join(&path[2..])
    } else if path.starts_with("{project}") {
        project_root.join(&path[10..])  // Skip "{project}/"
    } else {
        PathBuf::from(path)
    };
    
    dunce::simplified(&path).to_path_buf()
}
```

### Windows testing

```yaml
# Example CI matrix
strategy:
  matrix:
    os: [ubuntu-latest, macos-latest, windows-latest]
```

## Next Task

1. Create concept document: `docs/concepts/cross-platform.md`
2. Begin Track 10: Environment Management
