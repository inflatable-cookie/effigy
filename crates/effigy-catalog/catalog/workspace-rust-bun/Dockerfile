ARG RUST_VERSION=1.91
FROM rust:${RUST_VERSION}-bookworm

ARG WORKSPACE_UID=1000
ARG WORKSPACE_GID=1000
ARG WORKSPACE_MOUNT=/workspace-root
ARG BUN_VERSION=""

# ── System tools ──────────────────────────────────────────────────────
# Tools every Rust/Bun workspace tends to need: build toolchain, TLS,
# git, and procps for interactive shell work.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        ca-certificates \
        curl \
        git \
        libssl-dev \
        openssh-client \
        pkg-config \
        procps \
        socat \
    && rm -rf /var/lib/apt/lists/*

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
ENV BUN_INSTALL=/usr/local/bun
RUN if [ -n "${BUN_VERSION}" ]; then \
        curl -fsSL https://bun.sh/install | bash -s "bun-v${BUN_VERSION}"; \
    else \
        curl -fsSL https://bun.sh/install | bash; \
    fi \
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
