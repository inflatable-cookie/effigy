ARG RUST_VERSION=1.88
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
        pkg-config \
        procps \
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

WORKDIR ${WORKSPACE_MOUNT}

CMD ["sleep", "infinity"]
