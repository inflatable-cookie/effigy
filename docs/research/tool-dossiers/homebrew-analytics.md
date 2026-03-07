# Homebrew (Analytics)

Status: Draft
Tool name: Homebrew
Category: Package manager (telemetry/analytics)
Owner:
Last updated: 2026-03-07
Scope: Homebrew analytics, opt-out telemetry, privacy-respecting metrics

## 1) Why this tool matters

Homebrew has a well-designed, transparent analytics system. It's notable for:
- Opt-out (not opt-in) telemetry
- Public dashboards showing aggregate data
- Clear documentation of what's collected
- Easy to disable

For Effigy, Homebrew represents:
- Privacy-respecting telemetry design
- Transparency and user control patterns
- Public data sharing as benefit
- Implementation of opt-out consent

## 2) Product and era context

### Timeline

- **2013**: Homebrew analytics introduced
- **2016**: Google Analytics migration
- **2021**: InfluxDB/InfluxCloud migration (privacy improvement)
- **2023**: Continued refinement

### Design Philosophy

From Homebrew documentation:

> "Anonymous aggregate user behaviour analytics"
> "You can (and should) review the code to see exactly what is sent"
> "All the code is open source, so you can see exactly what is collected"

### Target Audience

- macOS/Linux developers
- Package maintainers
- Open source contributors

### Ecosystem

- **Formulae**: 4000+ packages
- **Analytics**: Public dashboards
- **Bottles**: Binary packages
- **Casks**: GUI applications

## 3) Defining architectural bets

### Opt-out by default

Analytics enabled by default but easily disabled:

```bash
# Check status
brew analytics
# Analytics are enabled.

# Disable
brew analytics off

# Or environment variable
export HOMEBREW_NO_ANALYTICS=1
```

Controversial choice but:
- High participation (95%+ opt-in rate)
- Transparent about collection
- Easy to disable

### Anonymous aggregation

What is collected:
- HTTP request (formula/cask name, version)
- User agent (Homebrew version, OS version, CPU)
- Error events (if formula fails to install)

What is NOT collected:
- User IDs or identifying information
- IP addresses (discarded immediately)
- Specific computer details
- Command contents (except formula names)

### Public dashboards

Analytics data is public:
- https://formulae.brew.sh/analytics
- Shows: install counts, version popularity, OS distribution
- Helps maintainers prioritize support

Benefits:
- Transparency
- Community benefit
- Maintainer insights

### InfluxDB instead of Google Analytics

2021 migration:
- Self-hosted or InfluxCloud
- Better privacy controls
- Data ownership
- Reduced third-party exposure

## 4) Standout strengths

- **Transparency**: Code is open, dashboards public
- **Easy opt-out**: One command or env var
- **Anonymous**: No identifying information
- **Useful data**: Helps maintainers and users
- **Public benefit**: Community can see trends
- **Privacy-conscious**: Migration away from Google Analytics

## 5) Chronic weaknesses and recurring costs

### Opt-out controversy

Some users prefer opt-in:
- Privacy advocates criticize opt-out
- Legal requirements vary (GDPR)
- Trust dependency

### Data retention

Managing historical data:
- Storage costs
- Data aging policies
- Query performance

### Third-party dependencies

InfluxCloud or self-hosted:
- Operational overhead
- Cost (even if anonymized)
- Reliability

## 6) Between-release corrections

### Early Homebrew (2013-2016)
- Basic Google Analytics
- Less transparency

### Modern Homebrew (2017-2024)
- Better documentation
- InfluxDB migration
- More controls
- Public dashboards

The pattern: More transparency, better privacy, same utility.

## 7) Effigy-relevant lessons

### Adopt carefully

- **Transparency**: Show exactly what's collected
- **Easy opt-out**: One command or env var
- **Anonymous data**: No identifying information
- **Public benefit**: Share aggregate data
- **Self-hosted**: Control data ownership

### Reject early

- **Identifiable data**: Never collect PII
- **Command contents**: Don't capture what users run
- **Difficult opt-out**: Must be easy to disable

### Prototype before deciding

- Effigy telemetry implementation
- Dashboard design
- Opt-out mechanism

## 8: Effigy Telemetry Design

### Option 1: Opt-out (Homebrew-style)

```toml
# effigy.toml
[telemetry]
enabled = true  # Default true
```

```bash
effigy telemetry off  # Disable
export EFFIGY_NO_TELEMETRY=1  # Or env var
```

### Option 2: Opt-in (strict privacy)

```bash
effigy telemetry on  # Explicit enable
```

Lower participation but higher trust.

### Option 3: Tiered consent

```bash
effigy telemetry level minimal   # Basic (default)
effigy telemetry level detailed  # More metrics
effigy telemetry off             # None
```

User chooses level of sharing.

## 9: Source inventory

| Source | Type | Version/Era | Confidence | Notes |
|--------|------|-------------|------------|-------|
| [Homebrew analytics docs](https://docs.brew.sh/Analytics) | official docs | current | high | Primary reference |
| [Homebrew analytics code](https://github.com/Homebrew/brew/blob/master/Library/Homebrew/utils/analytics.rb) | source | current | high | Implementation |
| [Formulae analytics](https://formulae.brew.sh/analytics) | dashboard | current | high | Public data |
| Homebrew blog | blog | 2021 | medium | InfluxDB migration |

## 10: Open questions

- Opt-in vs. opt-out: what's right for Effigy?
- What metrics are genuinely useful?
- How to balance transparency with data utility?

## Next Task

Compare against VS Code and other tools in Track 15 synthesis.

