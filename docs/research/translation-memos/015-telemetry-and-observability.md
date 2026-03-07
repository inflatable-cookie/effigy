# Translation Memo 015: Telemetry and Observability

**Status:** Draft  
**Track:** 15 - Telemetry and Observability  
**Tools:** Homebrew analytics, VS Code telemetry  
**Date:** 2026-03-07  
**Related:** All prior translation memos (implementation context)

## Executive Summary

This memo translates Track 15 research findings into concrete implementation guidance for Effigy's telemetry and observability strategy. The key insight: **Telemetry should be transparent, anonymous, and easy to control. Ask on first run rather than surprising users with opt-out defaults.**

This completes the Phase 3 research program covering Scale & Integration.

## Research Summary

### Homebrew Analytics
- **Strengths**: Anonymous, opt-out, public dashboards, easy disable, InfluxDB (self-controlled)
- **Weaknesses**: Opt-out controversial with privacy advocates
- **Pattern**: Formula usage counts, public benefit through dashboards

### VS Code Telemetry
- **Strengths**: Granular controls, detailed documentation, multi-channel, extension separation
- **Weaknesses**: Extension ecosystem inconsistency, Microsoft ownership concerns
- **Pattern**: Usage + errors + performance as separate channels

### Common Pattern
Both succeed through:
1. Transparency (open code, documentation)
2. Easy opt-out
3. Anonymous data
4. Clear value proposition

## Core Principles

### 1. Ask on First Run

Don't surprise users with opt-out defaults. Ask explicitly:

```
Help improve Effigy by sharing anonymous usage statistics?
[Yes] [No] [Learn more]
```

Benefits:
- Respects user agency
- Higher trust than opt-out
- Clear consent

### 2. Anonymous by Design

Design telemetry so identifying data is impossible:

✅ Collect:
- Command type (`build`, `test`) not full command
- Error code (`E001`) not stack trace
- Duration, cache hits, version, OS

❌ Never collect:
- File paths or contents
- Command arguments
- User IDs or identifying info
- IP addresses (strip immediately)

### 3. Self-Hosted Infrastructure

Own the data pipeline:

```
Effigy Client → telemetry.effigy.dev → Self-hosted InfluxDB
     ↑                                        ↓
   Open source                       Public dashboard
```

Benefits:
- No third-party dependencies
- Community can verify
- Data ownership

### 4. Public Benefit

Share aggregate data:
- https://telemetry.effigy.dev/public
- Task popularity
- Cache effectiveness
- Error patterns

Transparency builds trust.

## Proposed Implementation

### Phase 1: First-Run Prompt

**Implementation:**

```rust
// On first run (no config exists)
if !config.exists() {
    let consent = prompt_telemetry_consent();
    config.telemetry_enabled = consent;
    config.save()?;
}
```

**Prompt design:**

```
╔══════════════════════════════════════════════════════════╗
║              Welcome to Effigy v1.0.0!                   ║
╠══════════════════════════════════════════════════════════╣
║                                                          ║
║  Help improve Effigy by sharing anonymous usage          ║
║  statistics? This data helps prioritize features and     ║
║  fix bugs.                                               ║
║                                                          ║
║  What we collect:                          [Learn more]  ║
║  • Command types used (counts only)                      ║
║  • Error codes when things fail                          ║
║  • Performance timing                                    ║
║  • Effigy version and OS type                            ║
║                                                          ║
║  What we DON'T collect:                                  ║
║  • Your code or file contents                            ║
║  • Command arguments or paths                            ║
║  • Personal information                                  ║
║  • IP addresses                                          ║
║                                                          ║
║  You can change this anytime: effigy telemetry off       ║
║                                                          ║
║     [ Yes, share anonymously ]  [ No, keep private ]     ║
║                                                          ║
╚══════════════════════════════════════════════════════════╝
```

**Non-interactive environments (CI):**
```bash
export EFFIGY_TELEMETRY=0  # Auto-disable in CI
# Or detect CI=true automatically
```

### Phase 2: Telemetry Client

**Data schema:**

```rust
#[derive(Serialize)]
struct TelemetryEvent {
    // Event metadata
    event_type: &'static str,     // "task_start", "task_end", "error"
    event_id: Uuid,               // For correlating start/end
    timestamp: DateTime<Utc>,
    
    // Context (anonymous)
    session_id: Uuid,             // Random, ephemeral
    effigy_version: &'static str, // "1.0.0"
    os: &'static str,             // "linux", "macos", "windows"
    arch: &'static str,           // "x86_64", "aarch64"
    
    // Event-specific (no identifying data)
    task_type: Option<&'static str>, // "build", "test", "lint"
    error_code: Option<&'static str>, // "E001", "E002"
    duration_ms: Option<u64>,
    cache_hit: Option<bool>,
    cache_tier: Option<&'static str>, // "local", "remote"
}
```

**Sending logic:**

```rust
pub struct Telemetry {
    enabled: bool,
    buffer: Vec<TelemetryEvent>,
    client: reqwest::Client,
}

impl Telemetry {
    pub fn record(&mut self, event: TelemetryEvent) {
        if !self.enabled { return; }
        self.buffer.push(event);
        
        // Flush periodically or on size threshold
        if self.buffer.len() >= 10 {
            self.flush();
        }
    }
    
    fn flush(&mut self) {
        if self.buffer.is_empty() { return; }
        
        // Send asynchronously, don't block
        let events = std::mem::take(&mut self.buffer);
        tokio::spawn(async move {
            let _ = send_telemetry(events).await;
        });
    }
}
```

**Respects user settings:**
```rust
impl Drop for Telemetry {
    fn drop(&mut self) {
        // Final flush on shutdown
        self.flush();
    }
}
```

### Phase 3: Controls

**CLI commands:**

```bash
# Check status
effigy telemetry
# Telemetry: enabled
# Events sent (this session): 42
# Last sent: 2026-03-07 12:34:56

# Enable/disable
effigy telemetry on
effigy telemetry off

# Show what's collected
effigy telemetry --explain
```

**Environment variables:**

```bash
export EFFIGY_TELEMETRY=1    # Enable
export EFFIGY_TELEMETRY=0    # Disable
export EFFIGY_TELEMETRY_URL  # Override endpoint (enterprise)
```

**Configuration:**

```toml
# effigy.toml
[telemetry]
enabled = true
url = "https://telemetry.effigy.dev/v1"  # Override
```

### Phase 4: Public Dashboard

**URL:** https://telemetry.effigy.dev/public

**Visualizations:**

1. **Task Popularity** (last 30 days)
   ```
   build  ████████████████████████████████████  45%
   test   ████████████████████████              30%
   lint   ████████████                            15%
   watch  ████                                       5%
   other  ██                                         3%
   ```

2. **Cache Effectiveness**
   ```
   Hit Rate: 78%
   Local:    65%
   Remote:   35%
   ```

3. **Error Frequency**
   ```
   E001 (Task not found):     12%
   E002 (Command failed):      8%
   E003 (Cache unavailable):   3%
   ```

4. **Platform Distribution**
   ```
   macOS (arm64):    40%
   Linux (x86_64):   35%
   macOS (x86_64):   15%
   Windows:          10%
   ```

**API for programmatic access:**
```bash
curl https://telemetry.effigy.dev/api/v1/stats
```

### Phase 5: Self-Hosted Infrastructure

**Architecture:**

```
┌──────────────┐      ┌─────────────┐      ┌─────────────┐
│ Effigy Client│──────→│ API Gateway │──────→│ InfluxDB    │
│ (thousands)  │      │ (Rust/Axum) │      │ (Time-series)│
└──────────────┘      └─────────────┘      └─────────────┘
                                                    │
                              ┌─────────────────────┘
                              ▼
                    ┌──────────────────┐
                    │ Grafana/Public   │
                    │ Dashboard        │
                    └──────────────────┘
```

**Open source:**
- Telemetry server code: `github.com/effigy/telemetry-server`
- Community can audit and self-host

## Data Retention Policy

| Data Type | Retention | Rationale |
|-----------|-----------|-----------|
| Raw events | 90 days | Debugging, aggregation |
| Aggregated stats | 2 years | Long-term trends |
| Public dashboard | Indefinite | Transparency |

Automatic purging of old raw events.

## Privacy Checklist

- [ ] No command arguments collected
- [ ] No file paths collected
- [ ] No user IDs or identifying info
- [ ] IP addresses stripped immediately
- [ ] Session IDs are random UUIDs
- [ ] Data encrypted in transit (TLS)
- [ ] Data encrypted at rest
- [ ] Public dashboard shows only aggregates
- [ ] Easy opt-out (one command)
- [ ] Open source client

## Enterprise Considerations

**Self-hosted telemetry:**
```bash
# Enterprise runs their own collector
export EFFIGY_TELEMETRY_URL="https://telemetry.company.internal"
```

**Policy compliance:**
```toml
# Corporate policy mandates opt-out
[telemetry]
enabled = false  # Enforced by IT
```

## Comparison: Effigy vs. Prior Art

| Aspect | Homebrew | VS Code | Effigy (Proposed) |
|--------|----------|---------|-------------------|
| Default | Opt-out | Opt-out | **Ask first run** |
| Controls | On/off | Granular | **Simple on/off** |
| Anonymity | Yes | Debatable | **Strict** |
| Public data | Yes | No | **Yes** |
| Self-hosted | Yes | No | **Yes** |
| Transparency | High | High | **High** |

## Success Criteria

- First-run prompt implemented
- All telemetry data is anonymous
- Easy opt-out (one command)
- Public dashboard live
- Code is open source
- Privacy policy published
- No identifying data possible by design

## Open Questions

1. What's the cost model for self-hosted telemetry?
2. Should we support telemetry plugins (e.g., Datadog integration)?
3. How to handle enterprise proxy configurations?
4. Should we publish transparency reports?

## Related Concepts

- Concept: Anonymous Telemetry
- Concept: First-Run Consent
- Concept: Public Observability
- Concept: Self-Hosted Infrastructure
- Roadmap: Phase 3, Track 15

## Phase 3 Research Program Complete

This memo completes the 15-track research program:

| Phase | Tracks | Status |
|-------|--------|--------|
| Phase 1: Core Execution | 5 tracks (01-05) | ✅ Complete |
| Phase 2: Developer Experience | 5 tracks (06-10) | ✅ Complete |
| Phase 3: Scale & Integration | 5 tracks (11-15) | ✅ Complete |

**Total Deliverables:**
- 31 tool dossiers
- 15 value track syntheses
- 15 translation memos

**Next:** Implementation phase informed by research.

