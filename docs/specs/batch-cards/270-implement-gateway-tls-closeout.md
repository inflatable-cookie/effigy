# 270 Implement Gateway TLS Closeout

Status: landed
Updated: 2026-04-18
Roadmap: `g02.014`
Spec: `docs/specs/014-rust-native-gateway-strict-lane.md`

## Objective

Finish the remaining `g02.014` product gap by turning the crate-level TLS
helpers into one real gateway product path with a bounded HTTPS proof.

## Scope

- add `effigy gateway setup-tls` to CLI help, parsing, runner dispatch, and
  JSON/plain output
- wire mkcert availability and CA-install checks into an honest operator-facing
  setup flow
- generate gateway certificates for TLS-enabled registered routes when
  containers come up
- keep certificate ownership aligned with route ownership for deregistration and
  teardown
- make gateway startup/status output clear about HTTPS readiness vs
  HTTP-only fallback
- prove one real TLS hostname loop on a consumer repo that already uses the
  gateway path

Recommended proof target:

- `/Users/tom/Dev/projects/underlay-reference`

## Out Of Scope

- non-container task-owned gateway routes
- cross-project dashboard or coordination work owned by `g02.016`
- widening into wildcard cert strategy or alternative CA backends
- automating host prerequisites beyond bounded guidance and setup commands

## Acceptance

- `effigy gateway setup-tls` is a real product command with clear failure
  guidance when mkcert is missing or trust install fails
- a container with `[containers.<name>.dns].tls = true` causes the needed cert
  material to exist under the gateway state path before HTTPS is expected to
  serve that route
- route teardown no longer leaves the gateway pretending HTTPS is ready for
  removed domains
- `effigy gateway status` and related operator output make TLS readiness
  explicit enough to debug host setup without reading logs directly
- one real project answers through the HTTPS gateway path for its declared
  hostname, and teardown removes that route cleanly afterward

## Proof Notes

Keep the proof bounded to one real domain on one real repo.

Treat `/etc/resolver/test` and mkcert trust-store prompts as operator-owned
host prerequisites, not product bugs, as long as Effigy reports them honestly.

Use unprivileged override ports when needed. The thing that must be real is the
HTTPS hostname loop through Effigy's gateway-owned routing and cert path, not
privileged-port troubleshooting.

## Outcome

This batch landed through the product path plus one real consumer proof.

What is now real:

- `effigy gateway setup-tls` is a real CLI and runner command with honest
  mkcert failure guidance
- the gateway now enables HTTPS on its product path, reports HTTPS bind state,
  and projects TLS readiness through `gateway up/status`
- TLS-enabled container domains now mint route-owned certs on registration and
  remove them on teardown
- live cert reload is now real, so the gateway does not need a restart when a
  TLS route appears or disappears
- trust-store installation remains operator-owned on this machine, but that no
  longer blocks bounded TLS route registration or HTTPS proof

Real proof established through `/Users/tom/Dev/projects/underlay-reference`:

- `effigy container up --repo ... --detach --json` registered
  `underlay-mail.test -> 127.0.0.1:8025` with `tls = true`
- the gateway minted cert material under `~/.effigy/gateway/certs/`
- `effigy gateway status --json` showed one live TLS route with one ready cert
- `curl -k --resolve underlay-mail.test:18443:127.0.0.1 https://underlay-mail.test:18443/`
  returned Mailpit with `200 OK`
- `effigy container down` removed the route and removed the cert files

Host prerequisites that remain honest operator work:

- `/etc/resolver/test` still needs sudo on this machine
- `mkcert -install` still needs an interactive trust-store path on this
  machine, and `setup-tls` now reports that real failure text directly

## Validation

- `cargo test -p effigy-gateway --lib -- --nocapture`
- `cargo test --lib runner::gateway_command::tests -- --nocapture`
- `cargo test --lib runner::container_command::gateway_registration::tests -- --nocapture`
- `cargo test --lib parse_gateway_setup_tls_supports_json -- --nocapture`
- `cargo test --lib render_gateway_help_shows_lifecycle_examples -- --nocapture`
- `cargo fmt --all`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

No further execution lives on this card. Close the bounded `g02.014` lane and
leave broader cross-project/dashboard follow-through to `g02.016`.
