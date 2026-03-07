# VS Code (Telemetry)

Status: Draft
Tool name: VS Code
Category: Code editor (telemetry architecture)
Owner:
Last updated: 2026-03-07
Scope: VS Code telemetry, crash reporter, extension telemetry

## 1) Why this tool matters

VS Code has a comprehensive, well-documented telemetry system. It's notable for:
- Multiple telemetry channels (errors, usage, performance)
- First-party and third-party extension separation
- Detailed documentation of collected data
- Granular user controls

For Effigy, VS Code represents:
- Multi-channel telemetry architecture
- Extension ecosystem telemetry
- Performance and error tracking
- Privacy controls and transparency

## 2) Product and era context

### Timeline

- **2015**: VS Code initial release
- **2016**: Telemetry system introduced
- **2018**: GDPR compliance updates
- **2020**: Extension telemetry guidelines
- **2022**: Crash reporter improvements
- **2024**: Continued refinement

### Design Philosophy

From VS Code documentation:

> "VS Code collects telemetry to improve the product"
> "You have complete control over your data"
> "Extensions may collect their own telemetry"

### Target Audience

- Developers using VS Code
- Extension authors
- Enterprise administrators

### Ecosystem

- **Core telemetry**: Usage, errors, performance
- **Extension telemetry**: Per-extension opt-in/out
- **Crash reporter**: Separate system
- **Experiments**: A/B testing framework

## 3) Defining architectural bets

### Multi-channel telemetry

VS Code separates telemetry types:

| Channel | Purpose | Opt-out |
|---------|---------|---------|
| Usage data | Feature usage, commands | Yes |
| Error telemetry | Crash reports, exceptions | Yes |
| Performance | Timing, responsiveness | Yes |
| Experiments | A/B testing | Yes |

Each can be controlled independently:

```json
// settings.json
{
  "telemetry.telemetryLevel": "off",  // All off
  "telemetry.telemetryLevel": "error", // Errors only
  "telemetry.telemetryLevel": "all"    // Everything (default)
}
```

### First-party vs. third-party

Clear separation:

- **VS Code core**: Microsoft's telemetry
- **Extensions**: Each extension's own telemetry
- **UI**: Different UI indicators for each

Extension authors must:
- Declare telemetry in package.json
- Follow extension guidelines
- Respect user settings

### Crash reporter separation

Crash reporter is separate from telemetry:
```json
{
  "telemetry.crashReporter.reporterId": "..."
}
```

Can be disabled independently.

### GDPR compliance

EU-specific handling:
- IP address processing in EU
- Data retention limits
- User rights (export, deletion)
- Clear privacy statements

## 4) Standout strengths

- **Granular controls**: Separate switches for each type
- **Clear documentation**: Detailed lists of collected events
- **Extension separation**: First vs. third party distinguished
- **Enterprise controls**: Admin policies for organizations
- **Crash reporting**: Separate, valuable channel
- **Transparency**: Public documentation of practices

## 5) Chronic weaknesses and recurring costs

### Extension ecosystem complexity

Extensions do their own telemetry:
- Inconsistent practices
- Variable quality
- User confusion

### Trust issues

Microsoft ownership:
- Some users distrust Microsoft
- Opt-out default criticized
- Enterprise concerns

### Data volume

Rich telemetry generates data:
- Storage costs
- Processing overhead
- Privacy surface area

## 6) Between-release corrections

### Early VS Code (2015-2017)
- Basic telemetry
- Less documentation
- Fewer controls

### Modern VS Code (2018-2024)
- GDPR compliance
- Granular controls
- Better documentation
- Extension guidelines

The pattern: More controls, better transparency, regulatory compliance.

## 7) Effigy-relevant lessons

### Adopt carefully

- **Multiple channels**: Different telemetry types
- **Granular controls**: User can choose what to share
- **Clear documentation**: Explain exactly what's collected
- **Extension separation**: Distinguish core vs. plugin telemetry

### Reject early

- **Mandatory telemetry**: Always allow opt-out
- **Opaque collection**: Must document everything
- **Bundled consent**: Separate different data types

### Prototype before deciding

- Effigy telemetry channels
- User control UI
- Documentation approach

## 8: Effigy Telemetry Architecture

### Option 1: Single channel

```bash
effigy telemetry off  # All telemetry
```

Simple but coarse.

### Option 2: Multi-channel (VS Code-style)

```toml
# effigy.toml
[telemetry]
usage = false      # Feature usage
errors = true      # Error reports
performance = true # Timing data
```

Granular control.

### Option 3: Extension-specific

```toml
[telemetry]
core = true

[telemetry.extensions.cache-s3]
enabled = true

[telemetry.extensions.my-plugin]
enabled = false
```

Per-extension control.

## 9: Source inventory

| Source | Type | Version/Era | Confidence | Notes |
|--------|------|-------------|------------|-------|
| [VS Code telemetry docs](https://code.visualstudio.com/docs/getstarted/telemetry) | official docs | current | high | Primary reference |
| [Extension telemetry](https://code.visualstudio.com/api/extension-guides/telemetry) | docs | current | high | Extension guidelines |
| [GDPR compliance](https://code.visualstudio.com/docs/getstarted/telemetry#_gdpr-and-vs-code) | docs | current | high | EU compliance |
| VS Code source | source | latest | high | Implementation |

## 10: Open questions

- How many telemetry channels does Effigy need?
- Should extensions be required to respect core telemetry settings?
- What's the right default (opt-in vs. opt-out)?

## Next Task

Compare against Homebrew and other tools in Track 15 synthesis.

