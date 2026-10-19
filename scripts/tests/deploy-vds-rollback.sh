#!/usr/bin/env bash
# Сценарии сломанного обновления для scripts/deploy-vds.sh (issue #43) — без сервера и Docker.
# docker, curl и make подменены фейками из этого файла (состояние — в $STATE), git настоящий: «сервер» — клон
# временного репозитория, новая версия — коммит в нём.
#
#   bash scripts/tests/deploy-vds-rollback.sh
#
# FAKE_PG=1 — команды psql идут в настоящий PostgreSQL (PG_HOST, PG_PORT, PG_USER; нужны роль fb_app и база
# family_budget): так проверяется SQL снимка и восстановления, а не только порядок шагов.
# CODE читают условия check через eval.
# shellcheck disable=SC2034
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
SCRIPT=${DEPLOY_SCRIPT:-$ROOT/scripts/deploy-vds.sh}
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
FAILS=0
REPO=ghcr.io/yolshin195/family-budget

# ---------------------------------------------------------------- фейки

mkdir -p "$WORK/bin"
# docker: контейнер app (запущен или нет, какой образ), локальные теги, реестр, базы — файлы в $STATE.
cat > "$WORK/bin/docker" <<'FAKE'
#!/usr/bin/env bash
set -euo pipefail
S=$STATE
echo "docker $*" >> "$S/calls"
get() { cat "$S/$1" 2>/dev/null || true; }
tag_of_env() { sed -n 's/^APP_IMAGE_TAG=//p' "$APP_DIR/.env" | tail -n 1; }
# Образ по тегу: latest — новая сборка (если её скачали), sha-… — та, чей тег это.
image_for() {
  case $1 in
    latest) if [ -f "$S/pulled_new" ]; then echo new; else echo old; fi ;;
    sha-aaaaaaa) echo old ;;
    sha-bbbbbbb) echo new ;;
    *) echo missing ;;
  esac
}
psql_cmd() {
  local -a sql=()
  while [ $# -gt 0 ]; do
    case $1 in -c) sql+=("$2"); shift 2 ;; *) shift ;; esac
  done
  for q in "${sql[@]}"; do
    echo "$q" >> "$S/sql"
    if [ "${FAKE_PG:-0}" = 1 ]; then
      PGOPTIONS='-c client_min_messages=warning' psql -h "$PG_HOST" -p "$PG_PORT" -U "$PG_USER" -d postgres -v ON_ERROR_STOP=1 -qAt -c "$q"
      continue
    fi
    case $q in
      "SELECT 1 FROM pg_database WHERE datname = '"*)
        db=${q#*datname = \'}; db=${db%\'}
        if grep -qxF "$db" "$S/dbs"; then echo 1; fi ;;
      "SELECT pg_get_userbyid"*) echo fb_app ;;
      "SELECT pg_terminate_backend"*) ;;
      "DROP DATABASE IF EXISTS "*)
        db=${q#DROP DATABASE IF EXISTS }; db=${db%% *}
        grep -vxF "$db" "$S/dbs" > "$S/dbs.tmp" || true; mv "$S/dbs.tmp" "$S/dbs" ;;
      "CREATE DATABASE "*)
        [ "$(get db_full)" != 1 ] || { echo "ERROR: could not write to file: No space left on device" >&2; exit 1; }
        db=${q#CREATE DATABASE }; db=${db%% *}; echo "$db" >> "$S/dbs" ;;
      "ALTER DATABASE "*" RENAME TO "*)
        from=${q#ALTER DATABASE }; from=${from%% *}; to=${q##* }
        sed -i "s/^$from\$/$to/" "$S/dbs" ;;
      *) echo "фейковый psql не знает запрос: $q" >&2; exit 1 ;;
    esac
  done
}
if [ "$1" = compose ]; then
  shift
  case "$1 ${2:-}" in
    "ps -q") [ "$(get app_running)" = 1 ] && echo cid || true ;;
    "stop app") echo 0 > "$S/app_running" ;;
    "up -d")
      img=$(image_for "$(tag_of_env)")
      [ "$img" != missing ] || { echo "образ не найден" >&2; exit 1; }
      echo "$img" > "$S/running"; echo 1 > "$S/app_running"; echo 0 > "$S/restarts" ;;
    "pull --quiet")
      [ "$(get registry)" != down ] || { echo "ghcr.io недоступен" >&2; exit 1; }
      [ "$(tag_of_env)" != latest ] || touch "$S/pulled_new"
      grep -qxF "$(tag_of_env)" "$S/tags" || echo "$(tag_of_env)" >> "$S/tags"
      # Реестр «падает» сразу после того, как update скачал новую сборку.
      [ "${REGISTRY_DIES_AFTER_PULL:-0}" != 1 ] || echo down > "$S/registry" ;;
    "config --format") printf '{"services":{"app":{"image":"%s:%s"}}}\n' "$REPO" "$(tag_of_env)" ;;
    "logs --tail") echo "(журнал app)" ;;
    "exec -T")
      shift 2
      while [ "$1" != db ]; do shift; done # -e PGOPTIONS=…
      shift; [ "$1" = psql ] && shift; psql_cmd "$@" ;;
    *) echo "фейковый docker compose не знает: $*" >&2; exit 1 ;;
  esac
  exit 0
fi
case $1 in
  inspect)
    fmt=$3
    case $fmt in
      *revision*) [ "$(get running)" = new ] && echo bbbbbbb2222 || echo aaaaaaa1111 ;;
      '{{.Image}}') echo "sha256:$(get running)" ;;
      '{{.RestartCount}}')
        n=$(get restarts); n=${n:-0}
        # Сборка, которая падает через несколько секунд: каждый взгляд на контейнер — ещё один перезапуск.
        if [ "$(get running)" = new ] && [ "${NEW_CRASH:-0}" = 1 ]; then echo $((n + 1)) > "$S/restarts"; fi
        echo "$n" ;;
    esac ;;
  images) cat "$S/tags" ;;
  rmi) t=${2##*:}; grep -vxF "$t" "$S/tags" > "$S/tags.tmp" || true; mv "$S/tags.tmp" "$S/tags" ;;
  tag) t=${3##*:}; grep -qxF "$t" "$S/tags" || echo "$t" >> "$S/tags" ;;
  image)
    case $2 in
      prune) ;;
      inspect)
        ref=${!#}; t=${ref##*:}
        if [ "$3" = -f ]; then echo "sha256:$(image_for "$t")"; else grep -qxF "$t" "$S/tags"; fi ;;
    esac ;;
  *) echo "фейковый docker не знает: $*" >&2; exit 1 ;;
esac
FAKE

# curl: ответы приложения по тому, какой образ запущен. Новая сборка ведёт себя по NEW_HEALTH / NEW_READY / NEW_HOME.
cat > "$WORK/bin/curl" <<'FAKE'
#!/usr/bin/env bash
url=${!#}; code_only=0
for a in "$@"; do [ "$a" = '%{http_code}' ] && code_only=1; done
S=$STATE
running=$(cat "$S/running" 2>/dev/null); up=$(cat "$S/app_running" 2>/dev/null)
code=000
if [ "$up" = 1 ]; then
  case $url in
    */health) if [ "$running" = new ]; then code=${NEW_HEALTH:-200}; else code=200; fi ;;
    */ready) if [ "$running" = new ]; then code=${NEW_READY:-200}; else code=404; fi ;; # у старой сборки /ready нет
    */) if [ "$running" = new ]; then code=${NEW_HOME:-200}; else code=200; fi ;;
  esac
fi
if [ $code_only = 1 ]; then printf '%s' "$code"; exit 0; fi
[ "$code" = 200 ] || exit 22
echo ok
FAKE
printf '#!/bin/sh\necho "make $*" >> "$STATE/calls"\n' > "$WORK/bin/make"
printf '#!/bin/sh\nexit 0\n' > "$WORK/bin/sleep"
chmod +x "$WORK/bin"/*

# ---------------------------------------------------------------- стенд

# Свежий «сервер»: клон репозитория на старом коммите, в origin — новый; работает старый образ.
setup() {
  local n=$1
  export STATE=$WORK/$n/state APP_DIR=$WORK/$n/app
  mkdir -p "$STATE"
  git init -q -b main "$WORK/$n/origin"
  git -C "$WORK/$n/origin" -c user.email=t@t -c user.name=t commit -q --allow-empty -m "старая версия"
  git clone -q "$WORK/$n/origin" "$APP_DIR"
  git -C "$WORK/$n/origin" -c user.email=t@t -c user.name=t commit -q --allow-empty -m "новая версия"
  touch "$APP_DIR/compose.yaml"
  printf 'APP_IMAGE_TAG=latest\nAPP_PORT=8080\nBACKUP_AGE_RECIPIENT=age1test\n' > "$APP_DIR/.env"
  echo old > "$STATE/running"; echo 1 > "$STATE/app_running"; echo 0 > "$STATE/restarts"
  echo latest > "$STATE/tags"
  : > "$STATE/calls"; : > "$STATE/sql"
  if [ "${FAKE_PG:-0}" = 1 ]; then
    pg -c "DROP DATABASE IF EXISTS family_budget_before_update" -c "DROP DATABASE IF EXISTS family_budget_failed" \
      -c "DROP DATABASE IF EXISTS family_budget WITH (FORCE)" -c "CREATE DATABASE family_budget OWNER fb_app"
    pg -d family_budget -c "CREATE TABLE marker (v text)" -c "INSERT INTO marker VALUES ('до обновления')" -c "ALTER TABLE marker OWNER TO fb_app"
  else
    printf 'postgres\nfamily_budget\nrauthy\n' > "$STATE/dbs"
  fi
}
pg() { PGOPTIONS='-c client_min_messages=warning' psql -h "$PG_HOST" -p "$PG_PORT" -U "$PG_USER" -d postgres -v ON_ERROR_STOP=1 -qAt "$@" >/dev/null; }
# Новая версия «испортила» базу: в настоящем PostgreSQL — запись, которой не было до обновления.
break_db() { [ "${FAKE_PG:-0}" != 1 ] || pg -d family_budget -c "INSERT INTO marker VALUES ('после обновления')"; }

# Код выхода — в stdout, вывод скрипта — в $OUT.
OUT=$WORK/out
status_of() {
  local code=0
  PATH="$WORK/bin:$PATH" UPDATE_STABLE_SECONDS=1 UPDATE_HEALTH_TIMEOUT=1 UPDATE_READY_TIMEOUT=1 REPO=$REPO \
    bash "$SCRIPT" "$@" > "$OUT" 2>&1 || code=$?
  echo "$code"
}

CASE=''
check() {
  if eval "$2"; then return; fi
  FAILS=$((FAILS + 1))
  printf 'FAIL [%s] %s\n  условие: %s\n  вывод:\n%s\n' "$CASE" "$1" "$2" "$(tail -n 25 "$OUT" | sed 's/^/    /')"
}
env_tag() { sed -n 's/^APP_IMAGE_TAG=//p' "$APP_DIR/.env" | tail -n 1; }
head_msg() { git -C "$APP_DIR" log -1 --format=%s; }
sql_has() { grep -qF "$1" "$STATE/sql"; }
db_rows() {
  if [ "${FAKE_PG:-0}" = 1 ]; then psql -h "$PG_HOST" -p "$PG_PORT" -U "$PG_USER" -d family_budget -qAt -c "SELECT string_agg(v, ',' ORDER BY v) FROM marker"
  else echo "до обновления"; fi
}
db_owner() {
  if [ "${FAKE_PG:-0}" = 1 ]; then psql -h "$PG_HOST" -p "$PG_PORT" -U "$PG_USER" -d postgres -qAt -c "SELECT pg_get_userbyid(datdba) FROM pg_database WHERE datname = 'family_budget'"
  else echo fb_app; fi
}
expect_rolled_back() {
  check "код выхода 3 (откатился сам)" '[ "$CODE" = 3 ]'
  check "закреплён прежний образ" '[ "$(env_tag)" = sha-aaaaaaa ]'
  check "работает прежний образ" '[ "$(cat "$STATE/running")" = old ] && [ "$(cat "$STATE/app_running")" = 1 ]'
  check "код вернулся на прежний коммит" '[ "$(head_msg)" = "старая версия" ]'
  check "база восстановлена из снимка" 'sql_has "CREATE DATABASE family_budget TEMPLATE family_budget_before_update OWNER fb_app"'
  check "сломанная база сохранена" 'sql_has "ALTER DATABASE family_budget RENAME TO family_budget_failed"'
  check "данные — на момент обновления" '[ "$(db_rows)" = "до обновления" ]'
  check "владелец базы — пользователь приложения" '[ "$(db_owner)" = fb_app ]'
}

# ---------------------------------------------------------------- сценарии

CASE="удачное обновление"
setup ok
CODE=$(status_of update)
check "код выхода 0" '[ "$CODE" = 0 ]'
check "сначала копия make backup" 'grep -q "make --no-print-directory backup" "$STATE/calls"'
check "снимок базы перед обновлением" 'sql_has "CREATE DATABASE family_budget_before_update TEMPLATE family_budget"'
check "прежний образ сохранён на диске" 'grep -qx sha-aaaaaaa "$STATE/tags"'
check "работает новая сборка" '[ "$(cat "$STATE/running")" = new ]'
check "код обновлён" '[ "$(head_msg)" = "новая версия" ]'
check "тег не закреплён" '[ "$(env_tag)" = latest ]'

CASE="ручной rollback --with-db после удачного обновления"
break_db
CODE=$(status_of rollback --with-db)
check "код выхода 0" '[ "$CODE" = 0 ]'
check "работает прежний образ" '[ "$(cat "$STATE/running")" = old ]'
check "код вернулся" '[ "$(head_msg)" = "старая версия" ]'
check "база — на момент обновления" '[ "$(db_rows)" = "до обновления" ]'

CASE="новая сборка не отвечает на /health"
setup health
CODE=$(NEW_HEALTH=500 status_of update)
expect_rolled_back

CASE="новая сборка не видит базу (/ready)"
setup ready
CODE=$(NEW_READY=503 status_of update)
expect_rolled_back

CASE="главная страница отвечает 500"
setup home
CODE=$(NEW_HOME=500 status_of update)
expect_rolled_back

CASE="новая сборка падает в первую минуту"
setup crash
CODE=$(NEW_CRASH=1 status_of update)
expect_rolled_back

CASE="ghcr.io недоступен во время отката"
setup offline
# Реестр «падает» сразу после скачивания новой сборки: откат берёт прежний образ с диска.
CODE=$(NEW_HEALTH=500 REGISTRY_DIES_AFTER_PULL=1 status_of update)
expect_rolled_back
check "откат не скачивал образ" '! grep -q "compose pull --quiet app" "$STATE/calls"'

CASE="git pull не прошёл"
setup gitfail
rm -rf "$WORK/gitfail/origin"
CODE=$(status_of update)
check "код выхода 3" '[ "$CODE" = 3 ]'
check "работает прежний образ" '[ "$(cat "$STATE/running")" = old ] && [ "$(cat "$STATE/app_running")" = 1 ]'
check "код не менялся" '[ "$(head_msg)" = "старая версия" ]'

CASE="ghcr.io недоступен ещё до обновления"
setup nopull
echo down > "$STATE/registry"
CODE=$(status_of update)
check "код выхода 3" '[ "$CODE" = 3 ]'
check "работает прежний образ с диска" '[ "$(cat "$STATE/running")" = old ] && [ "$(cat "$STATE/app_running")" = 1 ]'

CASE="снимок базы не снялся (нет места)"
setup diskfull
echo 1 > "$STATE/db_full"
if [ "${FAKE_PG:-0}" = 1 ]; then check "только для фейковой базы" 'true'; else
CODE=$(status_of update)
check "код выхода 1, обновление не начато" '[ "$CODE" = 1 ]'
check "приложение снова запущено" '[ "$(cat "$STATE/app_running")" = 1 ] && [ "$(cat "$STATE/running")" = old ]'
check "код не менялся" '[ "$(head_msg)" = "старая версия" ]'
fi

CASE="NO_AUTO_ROLLBACK=1"
setup manual
CODE=$(NEW_HEALTH=500 NO_AUTO_ROLLBACK=1 status_of update)
check "код выхода 1" '[ "$CODE" = 1 ]'
check "тег не тронут" '[ "$(env_tag)" = latest ]'
check "подсказка про rollback --with-db" 'grep -q "rollback --with-db" "$OUT"'

CASE="auto-update: сборка не прошла проверку"
setup auto
CODE=$(NEW_HEALTH=500 status_of auto-update)
check "ненулевой код выхода" '[ "$CODE" != 0 ]'
check "update сам откатился" 'grep -q "update сам вернул sha-aaaaaaa" "$OUT"'
check "сборка запомнена как неудачная" '[ -s "$APP_DIR/.auto-update-failed" ]'
check "работает прежний образ" '[ "$(cat "$STATE/running")" = old ] && [ "$(cat "$STATE/app_running")" = 1 ]'
CODE=$(status_of auto-update)
check "та же сборка второй раз не ставится" '[ "$CODE" = 0 ] && grep -q "работает последняя сборка" "$OUT"'

if [ "$FAILS" -eq 0 ]; then echo "deploy-vds: сценарии отката — все проверки прошли"; else echo "deploy-vds: провалено проверок: $FAILS"; exit 1; fi
