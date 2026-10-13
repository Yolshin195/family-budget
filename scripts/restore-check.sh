#!/bin/sh
# Ежемесячная проверка восстановления: последняя копия family_budget → временная база restore_check,
# проверочные запросы, удаление временной базы. Нужен приватный ключ age: BACKUP_AGE_KEY_FILE.
set -eu
cd "$(dirname "$0")/.."
[ -f .env ] && set -a && . ./.env && set +a

ENGINE=${ENGINE:-podman}
DIR=${BACKUP_DIR:-./backups}
KEY=${BACKUP_AGE_KEY_FILE:?BACKUP_AGE_KEY_FILE — путь к приватному ключу age}
LAST=$(ls -1t "$DIR"/family_budget-*.dump.age 2>/dev/null | head -1)
[ -n "$LAST" ] || { echo "копий нет в $DIR" >&2; exit 1; }

PSQL="$ENGINE compose exec -T db psql -U budget -v ON_ERROR_STOP=1"
$PSQL -d postgres -c "DROP DATABASE IF EXISTS restore_check" -c "CREATE DATABASE restore_check"
age -d -i "$KEY" "$LAST" | $ENGINE compose exec -T db pg_restore -U budget -d restore_check --no-owner
$PSQL -d restore_check -At -c "SELECT 'пользователей: ' || count(*) FROM fb_users" -c "SELECT 'операций: ' || count(*) FROM fb_transactions"
$PSQL -d postgres -c "DROP DATABASE restore_check"
echo "восстановление из $LAST проверено"
