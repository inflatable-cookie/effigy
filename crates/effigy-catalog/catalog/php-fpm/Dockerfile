ARG PHP_VERSION=8.3
FROM php:${PHP_VERSION}-fpm
ARG WORKSPACE_UID=1000
ARG WORKSPACE_GID=1000
ARG COMPOSER_HOME=/home/dev/.config/composer
ARG COMPOSER_GLOBAL_PACKAGES=""
ARG NODE_GLOBAL_PACKAGES=""

# ── install-php-extensions ────────────────────────────────────────────
# Community standard tool that handles system-dependency resolution,
# pecl vs core detection, and cross-platform builds for PHP extensions.
# Copy it from the published installer image instead of fetching a GitHub
# release URL at build time, which makes fresh builds brittle when GitHub
# serves transient 5xx responses.
COPY --from=mlocati/php-extension-installer:latest /usr/bin/install-php-extensions /usr/local/bin/

# ── System tools ──────────────────────────────────────────────────────
# Tools that are useful for development regardless of which extensions
# are installed.
RUN apt-get update && apt-get install -y --no-install-recommends \
        # Version control and archive tools
        git unzip curl wget \
        # SSH client for git push over ssh and remote interactive use
        # (pairs with the host gitconfig / known_hosts / agent-socket
        # mounts driven by the workspace catalog params)
        openssh-client \
        # Bridge the root-owned host SSH agent socket to a dev-accessible
        # path on container start (see effigy-entrypoint below)
        socat \
        # Build tools (needed by some composer packages)
        make \
        # Database CLI clients for debugging
        default-mysql-client \
        # Network debugging
        iputils-ping dnsutils \
        # Process management
        procps \
    && rm -rf /var/lib/apt/lists/*

# Create a standard non-root workspace user for interactive shells and
# workspace-seeded task handoff.
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

# ── PHP extensions ────────────────────────────────────────────────────
# install-php-extensions handles all system dependency resolution,
# pecl vs core extension detection, GD configuration (freetype, jpeg,
# webp), and cleanup. Pass the EXTENSIONS build arg as a space-separated
# list of extension names.
#
# Common extensions and what they provide:
#   pdo_mysql    - MySQL/MariaDB via PDO
#   pdo_pgsql    - PostgreSQL via PDO
#   gd           - Image processing (freetype, jpeg, webp auto-configured)
#   intl         - Internationalization (ICU)
#   zip          - ZIP archive handling
#   exif         - Image metadata
#   bcmath       - Arbitrary precision math
#   opcache      - Bytecode caching (recommended for all projects)
#   redis        - Redis client extension
#   memcached    - Memcached client extension
#   imagick      - ImageMagick bindings
#   xdebug       - Debugging and profiling
#   apcu         - User-land object cache
#   soap         - SOAP client/server
#   imap         - IMAP mail handling
#   ldap         - LDAP directory access
#   mongodb      - MongoDB driver
#
# See https://github.com/mlocati/docker-php-extension-installer#supported-php-extensions
# for the full list of supported extensions.

ARG EXTENSIONS=""
RUN if [ -n "$EXTENSIONS" ]; then \
        install-php-extensions $EXTENSIONS; \
    fi

# ── PHP configuration ─────────────────────────────────────────────────
# Development-friendly defaults. These can be overridden by mounting a
# custom php.ini or .ini fragment into /usr/local/etc/php/conf.d/.
RUN { \
        echo 'memory_limit = 512M'; \
        echo 'upload_max_filesize = 256M'; \
        echo 'post_max_size = 256M'; \
        echo 'max_execution_time = 300'; \
        echo 'max_input_time = 300'; \
        echo 'date.timezone = UTC'; \
        echo 'display_errors = On'; \
        echo 'display_startup_errors = On'; \
        echo 'error_reporting = E_ALL'; \
        echo 'log_errors = On'; \
        echo 'error_log = /dev/stderr'; \
        echo 'short_open_tag = Off'; \
        echo 'realpath_cache_size = 4096K'; \
        echo 'realpath_cache_ttl = 600'; \
        echo 'opcache.enable = 1'; \
        echo 'opcache.enable_cli = 0'; \
        echo 'opcache.memory_consumption = 256'; \
        echo 'opcache.interned_strings_buffer = 16'; \
        echo 'opcache.max_accelerated_files = 20000'; \
        echo 'opcache.validate_timestamps = 1'; \
        echo 'opcache.revalidate_freq = 1'; \
    } > /usr/local/etc/php/conf.d/effigy-dev.ini

# ── PHP-FPM tuning ───────────────────────────────────────────────────
# Run workers as the workspace user so HTTP requests can write to the same
# bind-mounted paths as the interactive shell.
RUN set -eux; \
    workspace_group="$(id -gn dev)"; \
    sed -i \
        -e 's/^user = .*/user = dev/' \
        -e "s/^group = .*/group = ${workspace_group}/" \
        /usr/local/etc/php-fpm.d/www.conf

# Use a static pool with enough workers for local development.
RUN { \
        echo '[www]'; \
        echo 'pm = dynamic'; \
        echo 'pm.max_children = 20'; \
        echo 'pm.start_servers = 4'; \
        echo 'pm.min_spare_servers = 2'; \
        echo 'pm.max_spare_servers = 6'; \
        echo 'pm.max_requests = 500'; \
        echo 'request_terminate_timeout = 300'; \
        echo 'catch_workers_output = yes'; \
        echo 'decorate_workers_output = no'; \
    } > /usr/local/etc/php-fpm.d/effigy-dev.conf

# ── Git safe.directory ────────────────────────────────────────────────
# Workspace-root and library-mount paths are bind-mounted from the host,
# owned by the developer's host UID. Tooling that runs as root inside the
# container (`effigy prep`, composer-invoked git, etc.) would otherwise
# hit git's "dubious ownership" guard. Trust is implicit in a dev
# container — every mounted tree is the developer's own working copy — so
# allow any path system-wide.
RUN git config --system --add safe.directory '*'

# ── Composer ──────────────────────────────────────────────────────────
COPY --from=composer:latest /usr/bin/composer /usr/bin/composer

# Allow Composer to run as root (common in dev containers).
ENV COMPOSER_ALLOW_SUPERUSER=1
ENV COMPOSER_HOME=${COMPOSER_HOME}
ENV COMPOSER_CACHE_DIR=/home/dev/.cache/composer
ENV PNPM_HOME=/home/dev/.local/share/pnpm
ENV PATH=${COMPOSER_HOME}/vendor/bin:${PNPM_HOME}:${PATH}

RUN set -eux; \
    workspace_group="$(id -gn dev)"; \
    mkdir -p "${COMPOSER_HOME}" "${COMPOSER_CACHE_DIR}"; \
    chown -R dev:"${workspace_group}" "${COMPOSER_HOME}" "${COMPOSER_CACHE_DIR}"

RUN set -eux; \
    packages="$(printf '%s\n' "${COMPOSER_GLOBAL_PACKAGES}" | xargs)"; \
    if [ -n "${packages}" ]; then \
        composer global config --no-plugins allow-plugins true; \
        composer global require --no-interaction --prefer-dist ${packages}; \
    fi; \
    workspace_group="$(id -gn dev)"; \
    chown -R dev:"${workspace_group}" "${COMPOSER_HOME}" "${COMPOSER_CACHE_DIR}"

# ── Node.js (optional, for frontend asset compilation) ────────────────
# Many PHP projects need Node for asset builds (webpack, vite, mix).
# Install via nodesource for a current LTS version.
ARG NODE_VERSION=""
RUN if [ -n "$NODE_VERSION" ]; then \
        curl -fsSL https://deb.nodesource.com/setup_${NODE_VERSION}.x | bash - \
        && apt-get install -y --no-install-recommends nodejs \
        && rm -rf /var/lib/apt/lists/* \
        && npm install -g npm@latest \
        && corepack enable \
        && corepack prepare pnpm@latest --activate \
        && if [ -n "$NODE_GLOBAL_PACKAGES" ]; then npm install -g $NODE_GLOBAL_PACKAGES; fi; \
    fi

# ── SSH agent socket bridge ───────────────────────────────────────────
# Colima creates the forwarded host SSH agent socket inside its VM owned
# by root; bind-mounting it preserves those perms, so the non-root `dev`
# workspace user can't talk to it directly. We bridge it via socat to a
# dev-owned socket at `/tmp/effigy-ssh-auth.sock`, then point
# `SSH_AUTH_SOCK` at the bridge. Bridging avoids any Colima-side
# immutability that would defeat a plain chmod on the forwarded socket.
# The wrapper preserves the base image's `docker-php-entrypoint` so PHP
# signal/init handling is unchanged.
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

ENTRYPOINT ["/usr/local/bin/effigy-entrypoint", "docker-php-entrypoint"]
CMD ["php-fpm"]

# ── Entrypoint ────────────────────────────────────────────────────────
WORKDIR /var/www/html
