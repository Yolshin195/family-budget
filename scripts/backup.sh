#!/bin/sh
# Резервные копии обеих баз (family_budget и rauthy): pg_dump в контейнере db → шифрование age →
# BACKUP_DIR. Копии старше BACKUP_KEEP_DAYS (30) удаляются. Ежедневный запуск — cron или systemd timer:
#   15 3 * * *  cd /srv/family-budget && make backup >> backups/backup.log 2>&1
set -eu
cd "$(dirname "$0")/.."
[ -f .env ] && set -a && . ./.env && set +a

ENGINE=${ENGINE:-podman}
DIR=${BACKUP_DIR:-./backups}
KEEP=${BACKUP_KEEP_DAYS:-30}
RECIPIENT=${BACKUP_AGE_RECIPIENT:?BACKUP_AGE_RECIPIENT не задан: без шифрования копии не создаются}
command -v age >/dev/null || { echo "нужна утилита age (https://age-encryption.org)" >&2; exit 1; }

mkdir -p "$DIR"
STAMP=$(date -u +%Y%m%dT%H%M%SZ)
for DB in family_budget rauthy; do
  OUT="$DIR/$DB-$STAMP.dump.age"
  $ENGINE compose exec -T db pg_dump -U budget -Fc "$DB" | age -r "$RECIPIENT" > "$OUT.tmp"
  mv "$OUT.tmp" "$OUT"
  echo "копия: $OUT ($(wc -c < "$OUT") байт)"
done
find "$DIR" -name '*.dump.age' -mtime +"$KEEP" -print -delete
