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
# https://github.com/mlocati/docker-php-extension-installer
ADD --chmod=0755 https://github.com/mlocati/docker-php-extension-installer/releases/latest/download/install-php-extensions /usr/local/bin/

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

# ── Composer ──────────────────────────────────────────────────────────
COPY --from=composer:latest /usr/bin/composer /usr/bin/composer

# Allow Composer to run as root (common in dev containers).
ENV COMPOSER_ALLOW_SUPERUSER=1
ENV COMPOSER_HOME=${COMPOSER_HOME}
ENV PATH=${COMPOSER_HOME}/vendor/bin:${PATH}

RUN set -eux; \
    workspace_group="$(id -gn dev)"; \
    mkdir -p "${COMPOSER_HOME}"; \
    chown -R dev:"${workspace_group}" "${COMPOSER_HOME}"

RUN set -eux; \
    packages="$(printf '%s\n' "${COMPOSER_GLOBAL_PACKAGES}" | xargs)"; \
    if [ -n "${packages}" ]; then \
        composer global config --no-plugins allow-plugins true; \
        composer global require --no-interaction --prefer-dist ${packages}; \
    fi; \
    workspace_group="$(id -gn dev)"; \
    chown -R dev:"${workspace_group}" "${COMPOSER_HOME}"

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
        && if [ -n "$NODE_GLOBAL_PACKAGES" ]; then npm install -g $NODE_GLOBAL_PACKAGES; fi; \
    fi

# ── Entrypoint ────────────────────────────────────────────────────────
WORKDIR /var/www/html
