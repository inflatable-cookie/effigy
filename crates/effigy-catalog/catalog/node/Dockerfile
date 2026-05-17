ARG NODE_VERSION=22
FROM node:${NODE_VERSION}-bookworm

ARG WORKSPACE_UID=1000
ARG WORKSPACE_GID=1000
ARG WORKING_DIR=/app

# ── System tools ──────────────────────────────────────────────────────
# Tools expected in an interactive workspace shell and by coding agents.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        bash \
        ca-certificates \
        curl \
        fd-find \
        git \
        jq \
        less \
        openssh-client \
        procps \
        ripgrep \
        socat \
        unzip \
        wget \
    && rm -rf /var/lib/apt/lists/*

RUN ln -sf /usr/bin/fdfind /usr/local/bin/fd

# ── Non-root workspace user ───────────────────────────────────────────
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

ENV HOME=/home/dev

# ── Git safe.directory ────────────────────────────────────────────────
RUN git config --system --add safe.directory '*'

# ── SSH agent socket bridge ───────────────────────────────────────────
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

WORKDIR ${WORKING_DIR}

CMD ["sleep", "infinity"]
