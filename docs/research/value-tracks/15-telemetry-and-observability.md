# Track 15: Telemetry and Observability

Status: Draft
Value track: Telemetry and Observability (Homebrew, VS Code)
Created: 2026-03-07
Tools covered: Homebrew analytics, VS Code telemetry

## 1) Synthesis

### Common Patterns

| Pattern | Homebrew | VS Code | Description |
|---------|----------|---------|-------------|
| Default | Opt-out | Opt-out | Enabled by default |
| Data types | Usage only | Usage + errors + perf | Multiple channels |
| Anonymity | Anonymous | Potentially identifiable | Privacy level |
| Public data | Yes (dashboards) | No | Data sharing |
| Controls | On/off | Granular | Control level |
| Transparency | High | High | Documentation |
| Third-party | N/A | Separate | Extension handling |

### Key Insights

**The opt-in vs. opt-out debate:**

| Approach | Participation | User Trust | Implementation |
|----------|--------------|------------|----------------|
| Opt-in | Low (~10%) | High | GDPR-friendly |
| Opt-out | High (~95%) | Medium | Common practice |
| Tiered | Medium | Medium | User choice |

Homebrew and VS Code both use opt-out with easy disable.

**What data is actually useful:**

| Data Type | Use Case | Privacy Risk |
|-----------|----------|--------------|
| Command usage counts | Prioritize features | Low |
| Error rates | Fix bugs | Medium (stack traces) |
| Performance timing | Optimize | Low |
| Feature adoption | Deprecation decisions | Low |
| Full command contents | Debugging | High (don't collect) |

**Key principle**: Collect what's useful, not what's available.

**Transparency builds trust:**

Both tools succeed because:
- Open source implementation
- Detailed documentation
- Easy opt-out
- Clear data descriptions

### What Works

**Homebrew patterns:**
- Opt-out with easy disable
- Anonymous aggregation
- Public dashboards
- InfluxDB (self-controlled)
- Formula name only (not full commands)

**VS Code patterns:**
- Multiple telemetry channels
- Granular user controls
- Extension separation
- Enterprise policies
- Detailed documentation

**Common success factors:**
- Transparency (code and docs)
- Easy opt-out
- Anonymous data
- Clear value proposition

### What Doesn't

**Anti-patterns:**
- Collecting command contents (privacy risk)
- Identifiable data (trust risk)
- Hidden collection (trust destruction)
- Difficult opt-out (user frustration)

**Pain points:**
- Opt-out default (controversial)
- Extension ecosystem inconsistency
- Data retention complexity
- Third-party dependencies

## 2) Cross-Tool Capabilities Matrix

| Capability | Homebrew | VS Code | Effigy Should |
|------------|----------|---------|---------------|
| **Default** | Opt-out | Opt-out | Opt-out (or ask on first run) |
| **Controls** | On/off | Granular | Simple on/off |
| **Channels** | Single | Multiple | Single (start) |
| **Anonymity** | Anonymous | Debatable | Anonymous |
| **Public data** | Yes | No | Consider |
| **Extensions** | N/A | Separate | Same as core |
| **Transparency** | High | High | High |
| **Third-party** | Self-hosted | Microsoft | Self-hosted |

## 3) Telemetry patterns

### Pattern 1: Minimal viable telemetry

Collect only:
- Feature usage (counters)
- Error counts (no stack traces initially)
- Version information

```json
{
  "event": "task_executed",
  "task_type": "build",
  "duration_ms": 1234,
  "cache_hit": true,
  "version": "1.2.3"
}
```

### Pattern 2: Error reporting

Separate error telemetry:
```json
{
  "type": "error",
  "error_code": "E001",
  "task": "build",
  "version": "1.2.3",
  "os": "linux"
}
```

No stack traces initially (privacy), just error codes.

### Pattern 3: Public dashboards

Share aggregate data:
- Task popularity
- Cache hit rates
- OS distribution

Benefits:
- Community transparency
- Maintainer insights
- User trust

## 4) Privacy Comparison

| Approach | Privacy Level | Trust Level | Data Utility |
|----------|--------------|-------------|--------------|
| Anonymous + opt-out | High | Medium | High |
| Anonymous + opt-in | High | High | Low |
| Identifiable + opt-out | Low | Low | Medium |
| No telemetry | Maximum | High | None |

Effigy target: Anonymous + opt-out (or ask on first run).

## 5) Gaps and Opportunities

### Gaps in current tools

1. **Opt-out controversy**: Privacy advocates prefer opt-in
2. **Extension inconsistency**: VS Code extensions vary in practices
3. **Data ownership**: Third-party platforms (Google Analytics)
4. **First-run experience**: No tool asks on first run

### Opportunities for Effigy

1. **First-run prompt**: Ask user on first execution
2. **Anonymous by design**: No identifying data possible
3. **Self-hosted**: Own the telemetry infrastructure
4. **Public dashboards**: Share aggregate insights
5. **Simple controls**: One setting, not many

## 6: Recommendations for Effigy

### Core Principle

> Telemetry should be transparent, anonymous, and easy to control. Ask on first run rather than surprising users later.

### Specific Recommendations

**1. First-Run Prompt**

```
Welcome to Effigy v1.0.0!

Help improve Effigy by sharing anonymous usage statistics?
[Yes] [No] [Learn more]

What we collect:
• Which commands are used (counts only)
• Error codes when things go wrong
• Performance timing (how long tasks take)
• Effigy version and OS type

What we DON'T collect:
• Your code or commands
• File contents or paths
• Personal information
• IP addresses

You can change this anytime with: effigy telemetry off
```

**2. Anonymous Data Only**

```rust
// Data to collect
struct TelemetryEvent {
    event_type: String,      // "task_executed", "error"
    task_type: Option<String>, // "build", "test" (generic types)
    duration_ms: Option<u64>,
    error_code: Option<String>, // "E001", not full error
    cache_hit: Option<bool>,
    effigy_version: String,
    os: String,              // "linux", "macos", "windows"
    arch: String,            // "x86_64", "aarch64"
}

// NEVER collect:
// - Command arguments
// - File paths
// - User IDs
// - IP addresses (strip immediately)
```

**3. Self-Hosted Infrastructure**

```rust
// Send to Effigy-controlled endpoint
const TELEMETRY_ENDPOINT: &str = "https://telemetry.effigy.dev/v1";

// Open source the telemetry server
// Community can verify what's stored
```

**4. Simple Controls**

```bash
effigy telemetry              # Show status
effigy telemetry on           # Enable
effigy telemetry off          # Disable

# Or environment variable
export EFFIGY_TELEMETRY=0    # Disable
```

**5. Public Dashboard**

Share aggregate data:
- Most used tasks (anonymized)
- Cache hit rates by task type
- Error frequency by code
- OS/architecture distribution

URL: https://telemetry.effigy.dev/public

**6. Transparency**

- Open source telemetry client code
- Document every event type
- Public dashboard shows what we see
- Regular transparency reports

## 7: Implementation Phases

### Phase 1: Foundation (MVP)

- First-run prompt
- Basic usage counts
- Simple on/off control
- Anonymous data only

### Phase 2: Error Reporting

- Error code collection
- No stack traces (privacy)
- Helps identify bugs

### Phase 3: Performance

- Task timing data
- Cache hit/miss rates
- Performance optimization

### Phase 4: Public Dashboard

- Aggregate data visualization
- Community insights
- Transparency demonstration

## 8: Open Questions

- Should we support enterprise telemetry policies?
- How long to retain data?
- Should extensions be allowed to send telemetry?
- What's the self-hosting cost model?

## 9: Summary Table

| Aspect | Recommendation |
|--------|----------------|
| Default | Ask on first run |
| Controls | Simple on/off |
| Anonymity | No identifying data |
| Data types | Usage, errors (codes), performance |
| Infrastructure | Self-hosted |
| Transparency | Public dashboard, open code |
| Extensions | Follow core setting |

## 10: Next Steps

1. Design telemetry data schema
2. Implement first-run prompt
3. Build telemetry client
4. Deploy self-hosted collector
5. Create public dashboard
6. Document privacy practices
7. Complete Phase 3 research program

---

**This completes Track 15 and the entire Phase 3 research program.**
