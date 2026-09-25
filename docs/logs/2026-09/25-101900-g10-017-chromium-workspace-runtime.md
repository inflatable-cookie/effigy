# g10.017 Opt-in Chromium workspace runtime

Status: implementation complete; awaiting independent review, current-base CI, and merge
Created: 2026-09-25
Roadmap: g10.017
Baseline: `1f8341283477c896649bfabbacf0bca88c87426c` (planning commit on `main`)

## Resolution

`workspace-rust-bun` now accepts `browser_runtime` (`none` default, `chromium`
opt-in). Compose passes `BROWSER_RUNTIME`. The Dockerfile installs Debian
Bookworm Chromium shared libraries and a basic font set only in the `chromium`
branch, then deletes apt metadata. Unknown values fail the image build with
`unsupported BROWSER_RUNTIME='…'; expected none or chromium`. The image does
not contain Playwright, Node, `npx`, or a browser binary.

## Package list

Declared install set (Playwright 1.55.1 debian-12 chromium deps plus a basic
font set):

- fonts: `fonts-dejavu-core`, `fonts-liberation`, `fonts-noto-color-emoji`,
  `fonts-unifont`
- libs: `libasound2`, `libatk-bridge2.0-0`, `libatk1.0-0`, `libatspi2.0-0`,
  `libcairo2`, `libcups2`, `libdbus-1-3`, `libdrm2`, `libgbm1`, `libglib2.0-0`,
  `libnspr4`, `libnss3`, `libpango-1.0-0`, `libx11-6`, `libxcb1`,
  `libxcomposite1`, `libxdamage1`, `libxext6`, `libxfixes3`, `libxkbcommon0`,
  `libxrandr2`

On linux-arm64 `rust:1.91-bookworm`, apt reported 27 newly installed packages
and 5 glib upgrades (`Need to get 20.9 MB`, `46.5 MB of additional disk
space`). `fonts-dejavu-core` was already present on the base image. Image
sizes: `effigy-g10-017:none` 1.852GB, `effigy-g10-017:chromium` 1.927GB.

## Compose args

- Default assembly emits `BROWSER_RUNTIME: "none"`.
- Explicit `browser_runtime = "chromium"` emits `BROWSER_RUNTIME: "chromium"`.
- A typo such as `"chrome"` is forwarded to the build; the Dockerfile named
  error fires. Recorded: `unsupported BROWSER_RUNTIME='chrome'; expected none
  or chromium` (build exit 1, no image).

## linux-arm64 smoke

Host: macOS aarch64, Colima aarch64, containerd, `nerdctl`. First default-off
build failed pulling `rust:1.91-bookworm` (`lookup registry-1.docker.io on
192.168.5.3:53 … i/o timeout`). Retry succeeded after host and VM DNS recovered.

Default-off (`BROWSER_RUNTIME=none`), `--user 1000:1000`, `--entrypoint bash`:

- `uid=1000(dev) gid=1000(dev)` on `aarch64`
- `cargo 1.91.1`, `bun 1.3.14`
- `libnss3` and `libatk-bridge2.0-0` absent; `fonts-liberation` not installed

Opt-in Chromium image, same user, Playwright installed into `dev`'s cache
(not baked into the image):

- `bun add @playwright/test@1.55.1`
- `bunx playwright install chromium` → Chromium 140.0.7339.186, Playwright
  build v1193, path
  `/home/dev/.cache/ms-playwright/chromium-1193/chrome-linux/chrome`
- `ldd` on that binary: no `not found` lines
- `bun smoke.mjs` launched headless Chromium as `dev`, rendered local HTML
  `effigy g10.017 chromium smoke`, reported `browserVersion: 140.0.7339.186`,
  closed with `SMOKE_OK`

The catalog entrypoint writes `/var/log/effigy-ssh-bridge.log` as root, so a
plain `--user 1000:1000` run without `--entrypoint` fails before the command.
Recorded in `PAPERCUTS.md`; the smoke bypassed the entrypoint.

## Validation

- `cargo test -p effigy-catalog --test integration --locked workspace_rust_bun`:
  6 passed (default `none`, explicit `chromium`, unknown value forwarded,
  Dockerfile named-error and no Playwright/Node/npx bake-in).
- Remaining repository validation is recorded below this implementation
  checkpoint as it completes.

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`.
- Movement: shared `workspace-rust-bun` image can opt into Chromium OS
  libraries without a per-project Dockerfile; default path stays toolchain-only.
- Remaining gap: independent exact-head review, current-base CI, merge, then
  the separate Underlay bundle input and version gate.

## Next Task

Independent review and current-base CI on the implementation PR. Bundle wiring
stays a downstream repository task after merge.
