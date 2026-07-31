ARG RUST_VERSION=1.91
FROM rust:${RUST_VERSION}-bookworm

ARG WORKSPACE_UID=1000
ARG WORKSPACE_GID=1000
ARG WORKSPACE_MOUNT=/workspace-root
# Mandatory: the bun release to install (e.g. 1.3.14). No default on
# purpose — the build fails fast when it is unset instead of silently
# pulling whatever `latest` happens to be.
ARG BUN_VERSION

# ── System tools ──────────────────────────────────────────────────────
# Tools every Rust/Bun workspace tends to need: build toolchain, TLS,
# git, and procps for interactive shell work.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        ca-certificates \
        curl \
        fd-find \
        git \
        jq \
        less \
        libssl-dev \
        openssh-client \
        pkg-config \
        procps \
        ripgrep \
        socat \
        unzip \
        wget \
    && rm -rf /var/lib/apt/lists/*

RUN ln -sf /usr/bin/fdfind /usr/local/bin/fd

# ── Non-root workspace user ───────────────────────────────────────────
# Always create (or reuse) a user named `dev`, aligned with the host
# UID/GID so bind-mounted workspace files round-trip cleanly. The name
# is fixed so `systems.<name>.user = "dev"` overrides stay stable across
# hosts with different uid/gid defaults.
RUN set -eux; \
    group_name=dev; \
    if getent group "${WORKSPACE_GID}" >/dev/null; then \
        group_name="$(getent group "${WORKSPACE_GID}" | cut -d: -f1)"; \
    else \
        groupadd --gid "${WORKSPACE_GID}" "${group_name}"; \
    fi; \
    useradd --uid "${WORKSPACE_UID}" --gid "${WORKSPACE_GID}" --create-home --shell /bin/bash dev; \
    if [ "${group_name}" != "dev" ]; then \
        usermod --gid "${group_name}" dev; \
    fi

# ── Bun ───────────────────────────────────────────────────────────────
# Pinned install: BUN_VERSION selects the exact GitHub release
# (bun-v${BUN_VERSION}) and the archive is verified against the release's
# published SHA256 checksums before anything is executed. The upstream
# installer script is deliberately not piped into bash — no unversioned,
# unverified code path.
ENV BUN_INSTALL=/usr/local/bun
RUN set -eux; \
    if [ -z "${BUN_VERSION}" ]; then \
        echo "BUN_VERSION build arg is required (e.g. BUN_VERSION=1.3.14); refusing to install an unpinned bun" >&2; \
        exit 1; \
    fi; \
    case "$(uname -m)" in \
        x86_64) bun_arch="x64" ;; \
        aarch64) bun_arch="aarch64" ;; \
        *) echo "unsupported architecture for bun: $(uname -m)" >&2; exit 1 ;; \
    esac; \
    release_base="https://github.com/oven-sh/bun/releases/download/bun-v${BUN_VERSION}"; \
    curl -fsSL "${release_base}/bun-linux-${bun_arch}.zip" -o "/tmp/bun-linux-${bun_arch}.zip"; \
    curl -fsSL "${release_base}/SHASUMS256.txt" -o /tmp/bun-SHASUMS256.txt; \
    grep "bun-linux-${bun_arch}\.zip\$" /tmp/bun-SHASUMS256.txt | (cd /tmp && sha256sum --check --strict -); \
    unzip -q "/tmp/bun-linux-${bun_arch}.zip" -d /tmp/bun-unpack; \
    mkdir -p "${BUN_INSTALL}/bin"; \
    install -m 0755 "/tmp/bun-unpack/bun-linux-${bun_arch}/bun" "${BUN_INSTALL}/bin/bun"; \
    ln -sf bun "${BUN_INSTALL}/bin/bunx"; \
    rm -rf "/tmp/bun-linux-${bun_arch}.zip" /tmp/bun-SHASUMS256.txt /tmp/bun-unpack \
    && ln -sf /usr/local/bun/bin/bun /usr/local/bin/bun \
    && ln -sf /usr/local/bun/bin/bunx /usr/local/bin/bunx

# ── Cargo on PATH for non-login shells ────────────────────────────────
RUN ln -sf /usr/local/cargo/bin/cargo /usr/local/bin/cargo \
    && ln -sf /usr/local/cargo/bin/rustc /usr/local/bin/rustc \
    && ln -sf /usr/local/cargo/bin/rustup /usr/local/bin/rustup

# ── Ownership alignment ───────────────────────────────────────────────
RUN mkdir -p /home/dev/.cargo "${WORKSPACE_MOUNT}" \
    && chown -R dev:"${WORKSPACE_GID}" /home/dev /usr/local/cargo /usr/local/bun "${WORKSPACE_MOUNT}"

ENV HOME=/home/dev
ENV CARGO_HOME=/usr/local/cargo

# ── Git safe.directory ────────────────────────────────────────────────
# Workspace-root and library-mount paths are bind-mounted from the host,
# owned by the developer's host UID. Tooling that runs as root inside the
# container (`effigy prep`, package managers, etc.) would otherwise hit
# git's "dubious ownership" guard. Trust is implicit in a dev container —
# every mounted tree is the developer's own working copy — so allow any
# path system-wide.
RUN git config --system --add safe.directory '*'

# ── SSH agent socket bridge ───────────────────────────────────────────
# Colima creates the forwarded host SSH agent socket inside its VM owned
# by root; bind-mounting it preserves those perms, so the non-root `dev`
# workspace user can't talk to it directly. We bridge it via socat to a
# dev-owned socket at `/tmp/effigy-ssh-auth.sock`, then point
# `SSH_AUTH_SOCK` at the bridge. Bridging avoids any Colima-side
# immutability that would defeat a plain chmod on the forwarded socket.
RUN set -eux; \
    { \
        echo '#!/bin/bash'; \
        echo 'set -e'; \
        echo 'host_sock=/run/host-services/ssh-auth.sock'; \
        echo 'bridge_sock=/tmp/effigy-ssh-auth.sock'; \
        echo 'log=/var/log/effigy-ssh-bridge.log'; \
        echo 'mkcert_ca=/usr/local/share/ca-certificates/effigy-mkcert.crt'; \
        echo ': > "$log" || true'; \
        echo 'echo "[effigy-entrypoint] $(date -u +%FT%TZ) start" >> "$log" 2>&1 || true'; \
        echo 'dev_uid=$(id -u dev 2>/dev/null || echo 1000)'; \
        echo 'dev_gid=$(id -g dev 2>/dev/null || echo 1000)'; \
        echo 'echo "[effigy-entrypoint] dev uid=$dev_uid gid=$dev_gid" >> "$log"'; \
        echo 'if [ -f "$mkcert_ca" ]; then'; \
        echo '    if update-ca-certificates >>"$log" 2>&1; then'; \
        echo '        echo "[effigy-entrypoint] mkcert root CA installed into system trust store" >> "$log"'; \
        echo '    else'; \
        echo '        echo "[effigy-entrypoint] WARNING update-ca-certificates failed (see lines above)" >> "$log"'; \
        echo '    fi'; \
        echo 'fi'; \
        echo 'if [ -S "$host_sock" ]; then'; \
        echo '    echo "[effigy-entrypoint] host_sock present at $host_sock" >> "$log"'; \
        echo '    rm -f "$bridge_sock"'; \
        echo '    socat UNIX-LISTEN:"$bridge_sock",fork,reuseaddr,unlink-early,user=$dev_uid,group=$dev_gid,mode=0600 UNIX-CONNECT:"$host_sock" >>"$log" 2>&1 &'; \
        echo '    echo "[effigy-entrypoint] socat pid=$!" >> "$log"'; \
        echo '    for _ in 1 2 3 4 5 6 7 8 9 10; do'; \
        echo '        [ -S "$bridge_sock" ] && break'; \
        echo '        sleep 0.1'; \
        echo '    done'; \
        echo '    if [ -S "$bridge_sock" ]; then'; \
        echo '        echo "[effigy-entrypoint] bridge_sock created at $bridge_sock" >> "$log"'; \
        echo '    else'; \
        echo '        echo "[effigy-entrypoint] WARNING bridge_sock never appeared" >> "$log"'; \
        echo '    fi'; \
        echo 'else'; \
        echo '    echo "[effigy-entrypoint] WARNING host_sock missing at $host_sock - SSH agent forwarding will not work" >> "$log"'; \
        echo 'fi'; \
        echo 'exec "$@"'; \
    } > /usr/local/bin/effigy-entrypoint; \
    chmod 0755 /usr/local/bin/effigy-entrypoint

ENTRYPOINT ["/usr/local/bin/effigy-entrypoint"]

WORKDIR ${WORKSPACE_MOUNT}

CMD ["sleep", "infinity"]
