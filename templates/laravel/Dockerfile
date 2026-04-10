FROM composer:2 AS vendor
WORKDIR /app
RUN composer create-project laravel/laravel . --prefer-dist --no-interaction "^12.0"
COPY overrides/routes/web.php /app/routes/web.php
COPY overrides/resources/views/welcome.blade.php /app/resources/views/welcome.blade.php
RUN mkdir -p /app/database && touch /app/database/database.sqlite

FROM php:8.4-cli-alpine
WORKDIR /app
RUN apk add --no-cache sqlite-dev \
    && docker-php-ext-install pdo pdo_sqlite
COPY --from=vendor /app /app
ENV APP_ENV=production
ENV APP_DEBUG=false
ENV APP_URL=http://localhost
ENV DB_CONNECTION=sqlite
ENV DB_DATABASE=/app/database/database.sqlite
EXPOSE 8000
CMD ["sh", "-c", "php artisan key:generate --force >/dev/null 2>&1 || true && php artisan serve --host=0.0.0.0 --port=${PORT:-8000}"]
