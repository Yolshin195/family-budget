#!/usr/bin/env bash
# Резервные копии обеих баз (family_budget и rauthy): pg_dump в контейнере db → шифрование age → BACKUP_DIR.
# Ежедневный запуск — cron или systemd timer. Ещё копию делает scripts/deploy-vds.sh update перед обновлением
# (автообновление вызывает его, только когда в ghcr.io вышла новая сборка, а не при каждой проверке):
#   15 3 * * *  cd /srv/family-budget && make backup >> backups/backup.log 2>&1
#
# Чтобы копии не заняли диск, каждой базы хранится не больше трёх, сколько бы раз в день ни запускалась копия:
#   - самая свежая;
#   - самая свежая из сделанных BACKUP_OLD_DAYS (7) и больше дней назад — при ежедневных копиях ей 7–12 дней;
#   - самая старая из более поздних (1–6 дней) — дорастёт до 7 дней и сменит предыдущую.
# Файлы с другими именами (например, переименованная вручную копия) не удаляются.
set -euo pipefail
shopt -s nullglob
cd "$(dirname "$0")/.."
[ -f .env ] && set -a && . ./.env && set +a

ENGINE=${ENGINE:-podman}
DIR=${BACKUP_DIR:-./backups}
# Не больше 15: политика конфиденциальности обещает, что удалённые данные уходят из копий за 30 дней.
OLD=${BACKUP_OLD_DAYS:-7}
[[ $OLD =~ ^([1-9]|1[0-5])$ ]] || { echo "BACKUP_OLD_DAYS — число дней от 1 до 15" >&2; exit 1; }
RECIPIENT=${BACKUP_AGE_RECIPIENT:?BACKUP_AGE_RECIPIENT не задан: без шифрования копии не создаются}
command -v age >/dev/null || { echo "нужна утилита age (https://age-encryption.org)" >&2; exit 1; }

mkdir -p "$DIR"
# Недописанные копии прерванных запусков.
find "$DIR" -name '*.dump.age.tmp' -mtime +0 -print -delete
TMP=''
trap '[ -z "$TMP" ] || rm -f "$TMP"' EXIT
STAMP=$(date -u +%Y%m%dT%H%M%SZ)
for DB in family_budget rauthy; do
  OUT="$DIR/$DB-$STAMP.dump.age"
  TMP=$OUT.tmp
  # pipefail: при ошибке pg_dump копия не появится и не вытеснит хорошие.
  $ENGINE compose exec -T db pg_dump -U budget -Fc "$DB" | age -r "$RECIPIENT" > "$TMP"
  mv "$TMP" "$OUT"
  TMP=''
  echo "копия: $OUT ($(wc -c < "$OUT") байт)"
done

# Возраст — по дате UTC в имени, а не по времени файла: оно меняется при копировании.
CUTOFF=$(date -u -d "$OLD days ago" +%Y%m%d 2>/dev/null || date -u -v-"$OLD"d +%Y%m%d)
STAMP_GLOB='[0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9]T[0-9][0-9][0-9][0-9][0-9][0-9]Z'
for DB in family_budget rauthy; do
  newest='' old='' young=''
  # Имена сортируются по времени: от старых к новым.
  for f in "$DIR/$DB-"$STAMP_GLOB.dump.age; do
    newest=$f
    day=${f#"$DIR/$DB-"}
    if [ "${day%%T*}" -le "$CUTOFF" ]; then old=$f; elif [ -z "$young" ]; then young=$f; fi
  done
  for f in "$DIR/$DB-"$STAMP_GLOB.dump.age; do
    case $f in "$newest" | "$old" | "$young") ;; *) rm -f -- "$f" && echo "удалена: $f" ;; esac
  done
done
echo "в $DIR: $(du -sh "$DIR" | cut -f1)"
