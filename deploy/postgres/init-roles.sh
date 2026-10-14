#!/bin/sh
# Пользователи и базы PostgreSQL (ТЗ, раздел 13): приложение и Rauthy работают под разными
# пользователями без прав суперпользователя. Суперпользователь ($POSTGRES_USER) — только для обслуживания.
#
# Запускается автоматически при создании нового тома (docker-entrypoint-initdb.d) и вручную для уже
# существующей базы: `make db-roles` — скрипт идемпотентен.
set -eu

APP_DB=${APP_DB:-family_budget}
APP_ROLE=${APP_DB_USER:-fb_app}
APP_PASSWORD=${APP_DB_PASSWORD:?APP_DB_PASSWORD не задан}
RAUTHY_ROLE=${RAUTHY_DB_USER:-rauthy}
RAUTHY_PASSWORD=${RAUTHY_DB_PASSWORD:?RAUTHY_DB_PASSWORD не задан}
PSQL="psql -q -v ON_ERROR_STOP=1 --username ${POSTGRES_USER:-postgres} --no-password"

$PSQL --dbname postgres \
  -v app_role="$APP_ROLE" -v app_password="$APP_PASSWORD" \
  -v rauthy_role="$RAUTHY_ROLE" -v rauthy_password="$RAUTHY_PASSWORD" <<'SQL'
SELECT format('CREATE ROLE %I LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE', :'app_role')
 WHERE NOT EXISTS (SELECT FROM pg_roles WHERE rolname = :'app_role') \gexec
SELECT format('CREATE ROLE %I LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE', :'rauthy_role')
 WHERE NOT EXISTS (SELECT FROM pg_roles WHERE rolname = :'rauthy_role') \gexec
SELECT format('ALTER ROLE %I PASSWORD %L', :'app_role', :'app_password') \gexec
SELECT format('ALTER ROLE %I PASSWORD %L', :'rauthy_role', :'rauthy_password') \gexec
SELECT format('CREATE DATABASE %I OWNER %I', 'rauthy', :'rauthy_role')
 WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = 'rauthy') \gexec
SQL

# База приложения принадлежит пользователю приложения (миграции создают и меняют таблицы).
$PSQL --dbname postgres -v db="$APP_DB" -v app_role="$APP_ROLE" <<'SQL'
SELECT format('CREATE DATABASE %I OWNER %I', :'db', :'app_role')
 WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = :'db') \gexec
SELECT format('ALTER DATABASE %I OWNER TO %I', :'db', :'app_role') \gexec
SQL

# Уже существующие таблицы (база до обновления) — передать пользователю приложения.
$PSQL --dbname "$APP_DB" -v app_role="$APP_ROLE" <<'SQL'
SELECT format('ALTER TABLE %I.%I OWNER TO %I', schemaname, tablename, :'app_role')
  FROM pg_tables WHERE schemaname = 'public' AND tableowner <> :'app_role' \gexec
SELECT format('ALTER SEQUENCE %I.%I OWNER TO %I', sequence_schema, sequence_name, :'app_role')
  FROM information_schema.sequences WHERE sequence_schema = 'public' \gexec
SELECT format('GRANT ALL ON SCHEMA public TO %I', :'app_role') \gexec
SQL
echo "init-roles: пользователи $APP_ROLE и $RAUTHY_ROLE готовы"
