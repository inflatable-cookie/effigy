ARG PHP_VERSION=8.3
FROM php:${PHP_VERSION}-fpm

ARG EXTENSIONS=""

# Install common system dependencies for PHP extensions.
RUN apt-get update && apt-get install -y --no-install-recommends \
        libpng-dev libjpeg-dev libfreetype6-dev libwebp-dev \
        libzip-dev libicu-dev libmemcached-dev zlib1g-dev \
        libonig-dev libxml2-dev libcurl4-openssl-dev \
        unzip git curl \
    && rm -rf /var/lib/apt/lists/*

# Configure and install extensions.
# GD needs explicit configuration for freetype/jpeg/webp support.
RUN if echo "$EXTENSIONS" | grep -qw "gd"; then \
        docker-php-ext-configure gd \
            --with-freetype --with-jpeg --with-webp; \
    fi

# Install each requested extension.
RUN if [ -n "$EXTENSIONS" ]; then \
        for ext in $EXTENSIONS; do \
            case "$ext" in \
                redis) \
                    pecl install redis && docker-php-ext-enable redis ;; \
                memcached) \
                    pecl install memcached && docker-php-ext-enable memcached ;; \
                xdebug) \
                    pecl install xdebug && docker-php-ext-enable xdebug ;; \
                imagick) \
                    apt-get update && apt-get install -y libmagickwand-dev \
                    && pecl install imagick && docker-php-ext-enable imagick \
                    && rm -rf /var/lib/apt/lists/* ;; \
                *) \
                    docker-php-ext-install "$ext" ;; \
            esac; \
        done; \
    fi

# Install Composer.
COPY --from=composer:latest /usr/bin/composer /usr/bin/composer

WORKDIR /var/www/html
