# BuildBuddy

Status: Draft
Tool name: BuildBuddy
Category: remote build service (managed RBE, build insights)
Owner:
Last updated: 2026-03-07
Scope: BuildBuddy Cloud, self-hosted deployment, build analytics

## 1) Why this tool matters

BuildBuddy is a managed remote build execution service. It provides:
- Managed Bazel Remote Execution
- Build analytics and insights
- Distributed caching
- Build UI/dashboard

For Effigy, BuildBuddy represents:
- Managed remote build services
- Build analytics patterns
- Commercial RBE offerings
- Build observability

## 2) Product and era context

### Timeline

- **2019**: BuildBuddy founded
- **2020**: BuildBuddy Cloud launched
- **2021-2023**: Feature expansion, enterprise growth
- **2024**: Continued scaling, more integrations

### Design Philosophy

From BuildBuddy documentation:

> "Faster builds, better insights"
> "Zero-configuration remote builds"
> "Visibility into build performance"

### Target Audience

- Teams using Bazel
- Organizations wanting managed RBE
- Teams needing build analytics
- Companies prioritizing CI speed

### Offerings

- **BuildBuddy Cloud**: Managed SaaS
- **BuildBuddy Enterprise**: Self-hosted
- **BuildBuddy OSS**: Open-source core

## 3) Defining architectural bets

### Managed service model

BuildBuddy operates as SaaS:
```bash
# .bazelrc
build --remote_executor=grpcs://remote.buildbuddy.io
build --remote_cache=grpcs://remote.buildbuddy.io
```

Users don't manage infrastructure.

### Build analytics

BuildBuddy captures build data:
- Timing information
- Cache hit rates
- Action breakdown
- Failure patterns

Dashboard shows:
- Build trends
- Cache effectiveness
- Performance bottlenecks

### Hybrid approach

BuildBuddy supports:
- Cloud execution (their workers)
- Self-hosted workers (your infrastructure)
- Cache-only mode (no remote execution)

Flexible deployment options.

### Bazel-native

BuildBuddy designed for Bazel:
- Implements RBE API
- Bazel-specific optimizations
- Bazel query integration

Tight integration with Bazel ecosystem.

## 4) Standout strengths

- **Managed service**: No infrastructure to maintain
- **Quick setup**: Minutes to configure
- **Build UI**: Visual build insights
- **Analytics**: Performance tracking
- **Scalability**: Handles large builds
- **Support**: Commercial support available

## 5) Chronic weaknesses and recurring costs

### Bazel lock-in

BuildBuddy is Bazel-focused:
- RBE protocol is Bazel's
- Optimized for Bazel workflows
- Less value for other build tools

### Cost

BuildBuddy pricing:
- Free tier: Limited usage
- Team: ~$50/user/month
- Enterprise: Custom pricing

Can be expensive at scale.

### Vendor dependency

Using managed service means:
- Network dependency
- Vendor lock-in
- Data in third-party system

Migration would require work.

### Limited customization

Managed service constraints:
- Can't modify worker environment
- Limited to provided features
- Custom integrations may be hard

## 6) Between-release corrections

### Early BuildBuddy (2019-2020)
- Basic RBE implementation
- Simple UI

### Modern BuildBuddy (2021-2024)
- Advanced analytics
- Enterprise features
- Better performance
- More integrations

The pattern: Maturing from basic RBE to comprehensive build platform.

## 7) Effigy-relevant lessons

### Adopt carefully

- **Managed services**: Value of not managing infrastructure
- **Build analytics**: Visibility into performance
- **Cache analytics**: Understanding cache effectiveness
- **Build UI**: Visual build progress

### Reject early

- **Bazel-specific approach**: Effigy should be generic
- **Vendor lock-in**: Support multiple backends
- **High cost**: Should have affordable options

### Prototype before deciding

- Build analytics dashboard
- Cache hit rate tracking
- Remote cache backend support

## 8) Comparison: Self-hosted vs. Managed

| Aspect | Self-hosted (Buildbarn) | Managed (BuildBuddy) |
|--------|------------------------|---------------------|
| Control | Full | Limited |
| Cost | Infrastructure + maintenance | Subscription |
| Setup | Complex | Simple |
| Support | Community | Commercial |
| Customization | Unlimited | Limited |

**For Effigy**: Support generic backends, let users choose.

## 9) Effigy Integration Ideas

### Remote cache backend

```toml
[cache.remote]
enabled = true
backend = "http"
endpoint = "https://cache.buildbuddy.io"
api_key = "..."
```

### Build analytics

```toml
[analytics]
enabled = true
backend = "buildbuddy"
endpoint = "https://app.buildbuddy.io"
```

### Generic interface

```toml
[cache.remote]
enabled = true
backend = "s3"  # s3, gcs, azure, http, buildbuddy
```

## 10: Source inventory

| Source | Type | Version/Era | Confidence | Notes |
|--------|------|-------------|------------|-------|
| [BuildBuddy docs](https://www.buildbuddy.io/docs/introduction) | official docs | current | high | Primary reference |
| [BuildBuddy blog](https://www.buildbuddy.io/blog/) | blog | ongoing | high | Updates |
| GitHub issues/discussions | community | ongoing | medium | Usage patterns |

## 11: Open questions

- What's the typical cost savings vs. self-hosted?
- How do users handle vendor lock-in concerns?
- What's the migration path off BuildBuddy?

## Next Task

Compare against Bazel RBE and other tools in Track 11 synthesis on remote execution.

