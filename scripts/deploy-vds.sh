#!/usr/bin/env bash
# Развёртывание на VDS по docs/deploy-vds.md: готовый образ из ghcr.io, Docker, Caddy, Rauthy. Ubuntu 24.04 / Debian 12.
#
#   sudo ./deploy-vds.sh install vds.conf   чистый сервер → приложение по HTTPS (шаги 3–13). Повторный запуск безопасен:
#                                          готовое пропускается, .env не перезаписывается. Образец: deploy/vds.conf.example
#   ./deploy-vds.sh update [ТЕГ]            копия БД → git pull → новый образ → перезапуск (шаг 14.1)
#   ./deploy-vds.sh rollback [ТЕГ]          откат на образ, работавший до последнего update, или на ТЕГ (шаг 14.2)
#   ./deploy-vds.sh check                   проверки шага 12
#
# update, rollback и check запускаются от пользователя deploy (или через sudo) и берут настройки из .env.
# Вручную остаётся то, что требует человека: пароль и passkey администратора Rauthy (шаг 11.1),
# хранение .env и приватного ключа age вне сервера.
set -euo pipefail

if [ -t 1 ]; then B=$'\033[1m' G=$'\033[32m' Y=$'\033[33m' R=$'\033[31m' N=$'\033[0m'; else B='' G='' Y='' R='' N=''; fi
WARNINGS=()
step() { printf '\n%s==> %s%s\n' "$B" "$*" "$N"; }
info() { printf '    %s\n' "$*"; }
ok()   { printf '    %s✓%s %s\n' "$G" "$N" "$*"; }
warn() { printf '%s[!]%s %s\n' "$Y" "$N" "$*" >&2; WARNINGS+=("$*"); }
die()  { printf '%s[x]%s %s\n' "$R" "$N" "$*" >&2; exit 1; }
usage() { sed -n '2,/^set -euo/p' "$0" | sed '$d; s/^# \{0,1\}//'; }

# ---------------------------------------------------------------- общее

# Команды docker, git и make — от владельца приложения: у него группа docker и вход в ghcr.io (~/.docker).
as_app() {
  if [ "$(id -un)" = "$APP_USER" ]; then "$@"
  elif [ "$(id -u)" -eq 0 ]; then sudo -u "$APP_USER" -H -- "$@"
  else die "запускайте от пользователя $APP_USER или через sudo"; fi
}
compose() { as_app docker compose "$@"; }

# Значение в одинарных кавычках: так его одинаково читают docker compose и sh (make backup).
sq() {
  case $2 in *"'"* | *$'\n'*) die "$1: одинарная кавычка или перевод строки в значении не поддерживаются" ;; esac
  printf "'%s'" "$2"
}
env_get() { sed -n "s/^$1=//p" "$APP_DIR/.env" | tail -n 1 | sed "s/^'\(.*\)'\$/\1/; s/^\"\(.*\)\"\$/\1/"; }
# Заменить или добавить KEY=VALUE в .env; владелец и права файла сохраняются.
env_set() {
  local tmp; tmp=$(mktemp)
  K=$1 V=$2 awk 'index($0, ENVIRON["K"] "=") == 1 { if (!done) print ENVIRON["K"] "=" ENVIRON["V"]; done = 1; next }
                 { print } END { if (!done) print ENVIRON["K"] "=" ENVIRON["V"] }' "$APP_DIR/.env" > "$tmp"
  cat "$tmp" > "$APP_DIR/.env"
  rm -f "$tmp"
}

# wait_for СЕКУНД ЧТО команда… — повторять команду, пока не получится.
wait_for() {
  local deadline=$((SECONDS + $1)) what=$2; shift 2
  until "$@" >/dev/null 2>&1; do
    [ "$SECONDS" -lt "$deadline" ] || { warn "не дождался: $what"; return 1; }
    sleep 3
  done
}
http_ok() { [ "$(curl -fsS --max-time 5 "$1" 2>/dev/null)" = ok ]; }
local_app_url() {
  local p; p=$(env_get APP_PORT); p=${p:-8080}
  case $p in *:*) ;; *) p=127.0.0.1:$p ;; esac
  echo "http://$p"
}
wait_app() {
  if wait_for 120 "ответ приложения на /health" http_ok "$(local_app_url)/health"; then ok "приложение отвечает на /health"; return; fi
  compose logs --tail 40 app >&2 || true
  die "приложение не отвечает на /health — логи выше, причины — docs/deploy-vds.md, шаг 16"
}
valid_tag() { [[ $1 =~ ^[A-Za-z0-9_][A-Za-z0-9_.-]{0,127}$ ]] || die "некорректный тег образа: $1"; }

# Тег sha-<коммит> образа, на котором сейчас работает app (метка org.opencontainers.image.revision из CI).
running_tag() {
  local id rev
  id=$(compose ps -q app 2>/dev/null) && [ -n "$id" ] || return 1
  rev=$(as_app docker inspect -f '{{index .Config.Labels "org.opencontainers.image.revision"}}' "$id") || return 1
  [[ $rev =~ ^[0-9a-f]{7,}$ ]] || return 1
  echo "sha-${rev:0:7}"
}

# Для update, rollback и check: всё берётся из уже развёрнутого приложения.
app_context() {
  APP_DIR=${APP_DIR:-/srv/family-budget}
  [ -f "$APP_DIR/.env" ] && [ -f "$APP_DIR/compose.yaml" ] || die "в $APP_DIR нет развёрнутого приложения (сначала install)"
  cd "$APP_DIR"
  APP_USER=$(stat -c %U "$APP_DIR")
}

# ---------------------------------------------------------------- install

load_conf() {
  if [ $# -gt 0 ]; then
    [ -r "$1" ] || die "нет файла настроек $1 (образец: deploy/vds.conf.example)"
    # shellcheck source=/dev/null
    . "$(realpath "$1")"
  fi
  DEPLOY_USER=${DEPLOY_USER:-deploy}
  APP_DIR=${APP_DIR:-/srv/family-budget}
  REPO_URL=${REPO_URL:-https://github.com/Yolshin195/family-budget.git}
  REPO_BRANCH=${REPO_BRANCH:-main}
  APP_IMAGE_TAG=${APP_IMAGE_TAG:-latest}
  GUEST_DEMO=${GUEST_DEMO:-1}
  SMTP_URL=${SMTP_URL:-} SMTP_PORT=${SMTP_PORT:-587} SMTP_TLS_MODE=${SMTP_TLS_MODE:-tls}
  SMTP_USERNAME=${SMTP_USERNAME:-} SMTP_PASSWORD=${SMTP_PASSWORD:-}
  SWAP_SIZE=${SWAP_SIZE:-2G}
  SSH_HARDEN=${SSH_HARDEN:-1}

  local v
  for v in DOMAIN AUTH_DOMAIN ADMIN_EMAIL; do
    [ -n "${!v:-}" ] || die "не задан $v (vds.conf, образец — deploy/vds.conf.example)"
  done
  for v in DOMAIN AUTH_DOMAIN; do
    [[ ${!v} =~ ^[A-Za-z0-9]([A-Za-z0-9.-]*[A-Za-z0-9])?$ ]] || die "$v=${!v}: нужно имя домена без схемы и пути"
    [[ ${!v} != *example.com ]] || die "$v=${!v} — значение из образца, впишите свой домен"
  done
  [ "$DOMAIN" != "$AUTH_DOMAIN" ] || die "DOMAIN и AUTH_DOMAIN должны различаться"
  [[ $ADMIN_EMAIL =~ ^[^@[:space:]\'\"]+@[^@[:space:]\'\"]+$ ]] || die "ADMIN_EMAIL=$ADMIN_EMAIL: некорректный email"
  [[ $SMTP_PORT =~ ^[0-9]+$ ]] || die "SMTP_PORT должен быть числом"
  valid_tag "$APP_IMAGE_TAG"
  [ -z "${GHCR_TOKEN:-}" ] || [ -n "${GHCR_USER:-}" ] || die "GHCR_TOKEN задан без GHCR_USER (логин GitHub)"
  [ -z "${BACKUP_AGE_RECIPIENT:-}" ] || [[ $BACKUP_AGE_RECIPIENT =~ ^age1[0-9a-z]+$ ]] \
    || die "BACKUP_AGE_RECIPIENT — публичный ключ age (age1…), не приватный"
  [ "$DEPLOY_USER" != root ] || die "DEPLOY_USER не может быть root"
  SMTP_FROM=${SMTP_FROM:-Семейный бюджет <no-reply@$DOMAIN>}
  APP_USER=$DEPLOY_USER
  [ -n "$SMTP_URL" ] || warn "SMTP не настроен: без писем Rauthy войти сможет только администратор (заполните SMTP_* в .env)"
}

install_system() {
  step "Система: обновления, часовой пояс, утилиты (шаги 3.1, 3.5, 3.7)"
  OS_ID=$(. /etc/os-release && echo "$ID")
  OS_CODENAME=$(. /etc/os-release && echo "${UBUNTU_CODENAME:-$VERSION_CODENAME}")
  case $OS_ID in ubuntu | debian) ;; *) die "поддерживаются Ubuntu и Debian, а здесь $OS_ID" ;; esac
  export DEBIAN_FRONTEND=noninteractive NEEDRESTART_MODE=a
  local apt=(apt-get -yq -o Dpkg::Options::=--force-confdef -o Dpkg::Options::=--force-confold)
  apt-get update -q
  "${apt[@]}" full-upgrade
  "${apt[@]}" install sudo git make age jq curl ca-certificates gnupg openssl ufw cron unattended-upgrades
  [ -z "${TIMEZONE:-}" ] || timedatectl set-timezone "$TIMEZONE"
  # То же, что ответ «Yes» в dpkg-reconfigure -plow unattended-upgrades.
  printf 'APT::Periodic::Update-Package-Lists "1";\nAPT::Periodic::Unattended-Upgrade "1";\n' > /etc/apt/apt.conf.d/20auto-upgrades
  ok "$(. /etc/os-release && echo "$PRETTY_NAME"), часовой пояс $(timedatectl show -p Timezone --value)"
}

install_swap() {
  local mem_mb; mem_mb=$(awk '/^MemTotal:/ { print int($2 / 1024) }' /proc/meminfo)
  [ "$SWAP_SIZE" != 0 ] && [ "$mem_mb" -le 2560 ] && [ -z "$(swapon --noheadings --show)" ] || return 0
  step "Swap $SWAP_SIZE: RAM всего $mem_mb МБ (шаг 3.6)"
  [ -e /swapfile ] || fallocate -l "$SWAP_SIZE" /swapfile
  chmod 600 /swapfile
  mkswap /swapfile >/dev/null
  swapon /swapfile
  grep -q '^/swapfile ' /etc/fstab || echo '/swapfile none swap sw 0 0' >> /etc/fstab
  ok "$(free -h | awk '/^Swap:/ { print $2 }') swap"
}

install_user() {
  step "Пользователь $APP_USER с sudo и SSH-ключом (шаг 3.2)"
  if id "$APP_USER" >/dev/null 2>&1; then
    info "уже есть"
  else
    useradd --create-home --shell /bin/bash --user-group "$APP_USER"
    if [ -n "${DEPLOY_PASSWORD:-}" ]; then
      printf '%s:%s\n' "$APP_USER" "$DEPLOY_PASSWORD" | chpasswd
    elif [ -t 0 ]; then
      info "Задайте пароль $APP_USER — он нужен для sudo:"
      until passwd "$APP_USER"; do :; done
    else
      GENERATED_PASSWORD=$(openssl rand -base64 18)
      printf '%s:%s\n' "$APP_USER" "$GENERATED_PASSWORD" | chpasswd
    fi
  fi
  usermod -aG sudo "$APP_USER"

  # Ключи того, кто запустил скрипт (через sudo — его, иначе root), плюс SSH_PUBKEY из настроек.
  local home keys src key
  home=$(getent passwd "$APP_USER" | cut -d: -f6)
  keys=$home/.ssh/authorized_keys
  src=/root/.ssh/authorized_keys
  if [ -n "${SUDO_USER:-}" ] && [ "$SUDO_USER" != root ]; then
    src=$(getent passwd "$SUDO_USER" | cut -d: -f6)/.ssh/authorized_keys
  fi
  install -d -m 700 -o "$APP_USER" -g "$APP_USER" "$home/.ssh"
  [ -f "$keys" ] || install -m 600 -o "$APP_USER" -g "$APP_USER" /dev/null "$keys"
  {
    if [ "$src" != "$keys" ] && [ -r "$src" ]; then cat "$src"; fi
    if [ -n "${SSH_PUBKEY:-}" ]; then printf '%s\n' "$SSH_PUBKEY"; fi
  } | while IFS= read -r key; do
    if [ -n "$key" ] && ! grep -qxF -- "$key" "$keys"; then printf '%s\n' "$key" >> "$keys"; fi
  done
  if [ -s "$keys" ]; then ok "SSH-ключи: $keys"; else warn "у $APP_USER нет SSH-ключа: задайте SSH_PUBKEY и запустите install ещё раз"; fi
}

install_firewall() {
  step "Файрвол: SSH, 80, 443 (шаг 3.4)"
  local ports port
  install -d -m 755 /run/sshd # sshd -T/-t без неё не работают, если ssh запущен через ssh.socket
  ports=$(sshd -T 2>/dev/null | awk '$1 == "port" { print $2 }')
  for port in ${ports:-22}; do ufw allow "$port/tcp" >/dev/null; done
  ufw allow 80/tcp >/dev/null
  ufw allow 443/tcp >/dev/null
  ufw allow 443/udp >/dev/null # HTTP/3
  ufw --force enable >/dev/null
  ok "ufw включён: SSH ${ports:-22}, 80, 443"
}

install_docker() {
  step "Docker (шаг 4)"
  if docker compose version >/dev/null 2>&1; then
    info "уже установлен"
  else
    install -m 0755 -d /etc/apt/keyrings
    curl -fsSL "https://download.docker.com/linux/$OS_ID/gpg" -o /etc/apt/keyrings/docker.asc
    chmod a+r /etc/apt/keyrings/docker.asc
    echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/$OS_ID $OS_CODENAME stable" \
      > /etc/apt/sources.list.d/docker.list
    apt-get update -q
    apt-get -yq install docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
  fi
  if [ ! -f /etc/docker/daemon.json ]; then
    mkdir -p /etc/docker
    cat > /etc/docker/daemon.json <<'EOF'
{
  "log-driver": "json-file",
  "log-opts": { "max-size": "20m", "max-file": "5" }
}
EOF
    systemctl restart docker
  fi
  systemctl enable --now docker containerd >/dev/null 2>&1
  usermod -aG docker "$APP_USER"
  local bashrc v
  bashrc=$(getent passwd "$APP_USER" | cut -d: -f6)/.bashrc
  grep -qx 'export ENGINE=docker' "$bashrc" 2>/dev/null || echo 'export ENGINE=docker' >> "$bashrc"
  v=$(docker compose version --short | sed 's/^v//')
  [ "$(printf '%s\n' 2.24 "$v" | sort -V | head -n 1)" = 2.24 ] || die "нужен docker compose 2.24 или новее, а установлен $v"
  ok "docker compose $v"
}

install_caddy() {
  step "Caddy (шаг 5)"
  if command -v caddy >/dev/null; then info "уже установлен"; return; fi
  apt-get -yq install debian-keyring debian-archive-keyring apt-transport-https
  curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' \
    | gpg --batch --yes --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
  curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' > /etc/apt/sources.list.d/caddy-stable.list
  chmod o+r /usr/share/keyrings/caddy-stable-archive-keyring.gpg /etc/apt/sources.list.d/caddy-stable.list
  apt-get update -q
  apt-get -yq install caddy
  ok "$(caddy version | cut -d' ' -f1)"
}

install_repo() {
  step "Код приложения в $APP_DIR (шаг 6)"
  if [ -d "$APP_DIR/.git" ]; then
    info "уже склонирован"
  else
    [ ! -e "$APP_DIR" ] || [ -z "$(ls -A "$APP_DIR")" ] || die "$APP_DIR не пуст и это не git-репозиторий"
    install -d -o "$APP_USER" -g "$APP_USER" "$APP_DIR"
    as_app git clone --quiet --branch "$REPO_BRANCH" "$REPO_URL" "$APP_DIR" \
      || die "не удалось клонировать $REPO_URL. Приватный репозиторий: склонируйте его по шагу 6 инструкции и запустите install ещё раз"
  fi
  cd "$APP_DIR"
  # Серверные файлы — мимо git, чтобы git pull не мешал (шаг 8.3).
  local f
  for f in compose.override.yaml deploy/rauthy/bootstrap-prod/ backups/ .previous-image-tag; do
    grep -qxF "$f" .git/info/exclude 2>/dev/null || echo "$f" >> .git/info/exclude
  done
  ok "$(as_app git log -1 --format='%h %s')"
}

install_env() {
  step "Файл .env с секретами (шаг 7)"
  if [ -f .env ]; then info "уже есть — не перезаписываю (правка: nano $APP_DIR/.env)"; return; fi
  local enc_id smtp_url smtp_from smtp_user smtp_pass
  enc_id=$(openssl rand -hex 4)
  smtp_url=$(sq SMTP_URL "$SMTP_URL")
  smtp_from=$(sq SMTP_FROM "$SMTP_FROM")
  smtp_user=$(sq SMTP_USERNAME "$SMTP_USERNAME")
  smtp_pass=$(sq SMTP_PASSWORD "$SMTP_PASSWORD")
  install -m 600 -o "$APP_USER" -g "$APP_USER" /dev/null .env
  cat > .env <<EOF
# Создан scripts/deploy-vds.sh $(date '+%Y-%m-%d %H:%M'). Справка по переменным — .env.example и docs/deploy-vds.md.
# ---------------------------------------------------------------- приложение
APP_ENV=prod
APP_BASE_URL=https://${DOMAIN}
SESSION_SECRET=$(openssl rand -base64 48 | tr -d '\n')
# Только localhost: снаружи приложение доступно через Caddy.
APP_PORT=127.0.0.1:8080
# Образ из GitHub Container Registry (см. compose.override.yaml): latest, версия или sha-<коммит>.
APP_IMAGE_TAG=${APP_IMAGE_TAG}
# В продакшене демо-данные запрещены (приложение не стартует с SEED_DEMO=1).
SEED_DEMO=0
# Кнопка «Посмотреть демо» на лендинге (гость — только просмотр демо-семьи). 0 — убрать.
GUEST_DEMO=${GUEST_DEMO}
CONTACT_EMAIL=${ADMIN_EMAIL}
# Приложение за Caddy: IP клиента — из X-Forwarded-For.
TRUST_PROXY=1
RATE_LIMIT_PER_MIN=600

# ---------------------------------------------------------------- PostgreSQL
# Только hex: пароли подставляются в DATABASE_URL, спецсимволы его сломают.
DB_PASSWORD=$(openssl rand -hex 24)
APP_DB_PASSWORD=$(openssl rand -hex 24)
RAUTHY_DB_PASSWORD=$(openssl rand -hex 24)

# ---------------------------------------------------------------- вход через Rauthy (OIDC)
OIDC_ISSUER=https://${AUTH_DOMAIN}/auth/v1/
OIDC_CLIENT_ID=family-budget
# Заполняет deploy-vds.sh после первого старта Rauthy (то же, что make rauthy-secrets).
OIDC_CLIENT_SECRET=
OIDC_INTERNAL_URL=http://rauthy:8443
# Необязательно (шаг 11.4). Формат <имя>\$<секрет> — только в одинарных кавычках.
RAUTHY_ADMIN_API_KEY=

# ---------------------------------------------------------------- Rauthy
RAUTHY_PORT=8443
RAUTHY_PUB_URL=${AUTH_DOMAIN}
RAUTHY_ADMIN_EMAIL=${ADMIN_EMAIL}
RAUTHY_SECRET_RAFT=$(openssl rand -hex 24)
RAUTHY_SECRET_API=$(openssl rand -hex 24)
RAUTHY_ENC_KEYS=${enc_id}/$(openssl rand -base64 32)
RAUTHY_ENC_KEY_ACTIVE=${enc_id}
RAUTHY_PROXY_MODE=true
# Сети Docker: Caddy приходит через шлюз сети compose, приложение — из этой же сети.
RAUTHY_TRUSTED_PROXIES=172.16.0.0/12
RAUTHY_COOKIE_MODE=host
RAUTHY_RP_ID=${AUTH_DOMAIN}
RAUTHY_RP_ORIGIN=https://${AUTH_DOMAIN}:443

# Письма Rauthy: регистрация, сброс пароля, код 2FA.
SMTP_URL=${smtp_url}
SMTP_PORT=${SMTP_PORT}
SMTP_TLS_MODE=${SMTP_TLS_MODE}
SMTP_FROM=${smtp_from}
SMTP_USERNAME=${smtp_user}
SMTP_PASSWORD=${smtp_pass}

# ---------------------------------------------------------------- резервные копии
BACKUP_DIR=./backups
# Публичный ключ age (age1…), шаг 13.
BACKUP_AGE_RECIPIENT=
BACKUP_KEEP_DAYS=30
EOF
  ok "секреты сгенерированы — сохраните копию .env в менеджер паролей"
}

install_compose_override() {
  step "Продакшен-настройки compose и клиент Rauthy (шаг 8)"
  if [ -f compose.override.yaml ]; then
    info "compose.override.yaml уже есть"
  else
    cat > compose.override.yaml <<'EOF'
# Продакшен на VDS: поверх compose.yaml, подхватывается docker compose автоматически.
services:
  app:
    # Готовый образ из GitHub Container Registry вместо сборки на сервере.
    build: !reset null
    image: ghcr.io/yolshin195/family-budget:${APP_IMAGE_TAG:-latest}
    # environment:
    #   TZ: Europe/Moscow        # часовой пояс в контейнере (в образе по умолчанию Asia/Bangkok)

  rauthy:
    volumes:
      # Клиент family-budget с адресами продакшена, без демо-пользователей.
      - ./deploy/rauthy/bootstrap-prod:/app/bootstrap:ro
    # environment:
    #   OPEN_USER_REG: "false"     # закрыть свободную регистрацию: пользователей заводит администратор
EOF
    chown "$APP_USER:" compose.override.yaml
    ok "compose.override.yaml"
  fi

  # Rauthy читает клиента отсюда только при первом старте с пустой базой.
  local dir=deploy/rauthy/bootstrap-prod
  if [ -f "$dir/clients.json" ]; then
    info "$dir уже есть"
  else
    mkdir -p "$dir"
    cat > "$dir/clients.json" <<EOF
[
  {
    "id": "family-budget",
    "name": "Семейный бюджет",
    "secret": "generate",
    "redirect_uris": ["https://${DOMAIN}/auth/callback"],
    "post_logout_redirect_uris": ["https://${DOMAIN}/"],
    "enabled": true,
    "flows_enabled": ["authorization_code", "refresh_token"],
    "access_token_alg": "EdDSA",
    "id_token_alg": "EdDSA",
    "auth_code_lifetime": 60,
    "access_token_lifetime": 1800,
    "scopes": ["openid", "profile", "email"],
    "default_scopes": ["openid", "profile", "email"],
    "challenges": ["S256"],
    "force_mfa": false
  }
]
EOF
    echo '[]' > "$dir/users.json"
    chown -R "$APP_USER:" "$dir"
    chmod -R a+rX "$dir" # Rauthy в контейнере работает не от deploy
    ok "$dir: redirect_uri https://$DOMAIN/auth/callback"
  fi
}

check_dns() {
  step "DNS (шаг 2)"
  local host resolved ip mine found
  mine=" $(hostname -I 2>/dev/null) "
  for host in "$DOMAIN" "$AUTH_DOMAIN"; do
    resolved=$(getent ahostsv4 "$host" | awk '{ print $1 }' | sort -u | xargs) || true
    if [ -z "$resolved" ]; then
      warn "$host не резолвится: пока нет A-записи на этот сервер, Caddy не получит сертификат"
      continue
    fi
    found=
    for ip in $resolved; do
      if [[ $mine == *" $ip "* ]]; then found=1; fi
    done
    if [ -n "$found" ]; then ok "$host → $resolved"
    else warn "$host → $resolved, а адреса сервера:${mine% } (за NAT это нормально, иначе исправьте A-запись)"; fi
  done
}

install_caddyfile() {
  step "Конфигурация Caddy (шаг 9)"
  local new out
  new=$(mktemp)
  cat > "$new" <<EOF
{
	email ${ADMIN_EMAIL}
}

${DOMAIN} {
	reverse_proxy 127.0.0.1:8080
}

${AUTH_DOMAIN} {
	reverse_proxy 127.0.0.1:8443
}
EOF
  if cmp -s "$new" /etc/caddy/Caddyfile; then
    info "не изменилась"
    rm -f "$new"
  else
    [ ! -f /etc/caddy/Caddyfile ] || cp /etc/caddy/Caddyfile "/etc/caddy/Caddyfile.bak.$(date +%Y%m%d%H%M%S)"
    install -m 644 "$new" /etc/caddy/Caddyfile
    rm -f "$new"
    out=$(caddy validate --config /etc/caddy/Caddyfile --adapter caddyfile 2>&1) || { echo "$out" >&2; die "Caddyfile не прошёл проверку"; }
    ok "/etc/caddy/Caddyfile: $DOMAIN → :8080, $AUTH_DOMAIN → :8443"
  fi
  systemctl enable caddy >/dev/null 2>&1
  systemctl reload-or-restart caddy
}

# Секрет клиента family-budget из вывода `rauthy bootstrap get --format env` (то же, что make rauthy-secrets):
# строка ИМЯ=ЗНАЧЕНИЕ, где в имени есть secret и, по возможности, family-budget.
client_secret_from_rauthy() {
  compose exec -T rauthy /app/rauthy bootstrap get --format env 2>/dev/null | awk -v q="'" '
    { sub(/^export[ \t]+/, ""); eq = index($0, "="); if (!eq) next
      key = tolower(substr($0, 1, eq - 1)); val = substr($0, eq + 1); gsub("^[\"" q "]|[\"" q "]$", "", val)
      if (key !~ /secret/ || val == "") next
      gsub(/[^a-z]/, "", key)
      if (key ~ /familybudget/) best = val; else { other = val; n++ } }
    END { if (best != "") print best; else if (n == 1) print other }'
}

# Шаг 16: подсеть сети compose должна входить в RAUTHY_TRUSTED_PROXIES (по умолчанию 172.16.0.0/12).
fix_trusted_proxies() {
  local subnet
  subnet=$(as_app docker network inspect family-budget_default -f '{{(index .IPAM.Config 0).Subnet}}' 2>/dev/null) || return 0
  case $subnet in '' | 172.1[6-9].* | 172.2[0-9].* | 172.3[01].*) return 0 ;; esac
  [ "$(env_get RAUTHY_TRUSTED_PROXIES)" = 172.16.0.0/12 ] || return 0 # уже настроено вручную
  info "сеть compose — $subnet, вне 172.16.0.0/12: записываю её в RAUTHY_TRUSTED_PROXIES"
  env_set RAUTHY_TRUSTED_PROXIES "$subnet"
  compose up -d --wait --wait-timeout 180 rauthy
}

start_stack() {
  step "Образы и первый запуск (шаг 10)"
  compose config --quiet
  if [ -n "${GHCR_TOKEN:-}" ]; then
    printf '%s' "$GHCR_TOKEN" | as_app docker login ghcr.io -u "$GHCR_USER" --password-stdin >/dev/null
    ok "вход в ghcr.io как $GHCR_USER"
  fi
  if [ -z "$(compose ps -a -q app 2>/dev/null)" ]; then
    compose pull --quiet || die "docker compose pull не удался. Ответ denied/unauthorized от ghcr.io — пакет приватный:
    задайте GHCR_USER и GHCR_TOKEN (classic-токен GitHub с правом read:packages) и запустите install ещё раз"
    ok "образы скачаны"
  else
    info "приложение уже развёрнуто — образы не обновляю (обновление: $APP_DIR/scripts/deploy-vds.sh update)"
  fi

  # Приложение — позже: без секрета клиента оно не пройдёт проверку продакшен-настроек.
  compose up -d --wait --wait-timeout 180 db rauthy || die "db или rauthy не запустились: docker compose logs db rauthy"
  ok "PostgreSQL и Rauthy запущены"
  fix_trusted_proxies

  if [ -z "$(env_get OIDC_CLIENT_SECRET)" ]; then
    info "жду, пока Rauthy создаст клиента family-budget…"
    local secret='' deadline=$((SECONDS + 180))
    until secret=$(client_secret_from_rauthy) && [ -n "$secret" ]; do
      [ "$SECONDS" -lt "$deadline" ] || die "не удалось получить секрет клиента из Rauthy. Выполните в $APP_DIR
    make rauthy-secrets, впишите секрет family-budget в .env (OIDC_CLIENT_SECRET=…) и запустите install ещё раз"
      sleep 5
    done
    secret=$(sq OIDC_CLIENT_SECRET "$secret")
    env_set OIDC_CLIENT_SECRET "$secret"
    ok "секрет клиента family-budget записан в .env"
  fi

  compose up -d app
  wait_app
}

install_backups() {
  step "Резервные копии (шаг 13)"
  [ -z "${BACKUP_AGE_RECIPIENT:-}" ] || env_set BACKUP_AGE_RECIPIENT "$BACKUP_AGE_RECIPIENT"
  if [ -z "$(env_get BACKUP_AGE_RECIPIENT)" ]; then
    warn "резервные копии не настроены: создайте ключ на своём компьютере (age-keygen -o fb-backup.key),
    впишите публичный ключ в BACKUP_AGE_RECIPIENT в vds.conf и запустите install ещё раз"
    return
  fi
  install -d -o "$APP_USER" -g "$APP_USER" backups
  local line="15 3 * * * cd $APP_DIR && ENGINE=docker make backup >> backups/backup.log 2>&1"
  if crontab -u "$APP_USER" -l 2>/dev/null | grep -F 'make backup' >/dev/null; then
    info "cron уже настроен"
  else
    { crontab -u "$APP_USER" -l 2>/dev/null || true; echo "$line"; } | crontab -u "$APP_USER" -
    ok "cron: каждый день в 03:15"
  fi
  as_app env ENGINE=docker make --no-print-directory backup
}

harden_ssh() {
  step "SSH: только ключи, без входа root (шаг 3.3)"
  if [ "$SSH_HARDEN" != 1 ]; then info "пропущено (SSH_HARDEN=0)"; return; fi
  local keys
  keys=$(getent passwd "$APP_USER" | cut -d: -f6)/.ssh/authorized_keys
  if ! grep -qE '(ssh-|ecdsa-|sk-)[a-z0-9@.-]+ AAAA' "$keys" 2>/dev/null; then
    warn "у $APP_USER нет SSH-ключа — вход по паролю и под root оставлен включённым, иначе доступ к серверу пропадёт"
    return
  fi
  cat > /etc/ssh/sshd_config.d/00-hardening.conf <<'EOF'
PermitRootLogin no
PasswordAuthentication no
KbdInteractiveAuthentication no
EOF
  sshd -t || { rm -f /etc/ssh/sshd_config.d/00-hardening.conf; die "sshd -t: ошибка в конфигурации SSH"; }
  systemctl restart ssh 2>/dev/null || systemctl restart sshd
  if [ "$(sshd -T | awk '$1 == "passwordauthentication" { print $2 }')" = no ]; then
    SSH_HARDENED=1
    ok "вход только по ключу, root отключён — текущая сессия не прерывается"
  else
    warn "sshd не подхватил /etc/ssh/sshd_config.d/00-hardening.conf — проверьте Include в /etc/ssh/sshd_config"
  fi
}

install_summary() {
  local ip; ip=$(hostname -I 2>/dev/null | awk '{ print $1 }')
  step "Готово: https://$DOMAIN"
  cat <<EOF
    Осталось вручную:
    1. Пароль администратора Rauthy — в логах первого старта:
         cd $APP_DIR && docker compose logs rauthy 2>&1 | grep -i -B2 -A2 password
       Войдите в https://$AUTH_DOMAIN/auth/v1/admin как $ADMIN_EMAIL, смените пароль и добавьте passkey (шаг 11.1).
    2. Сохраните копию .env в менеджер паролей: sudo cat $APP_DIR/.env
    3. В НОВОМ терминале проверьте вход: ssh $APP_USER@${ip:-<IP сервера>}${SSH_HARDENED:+ — под root войти больше нельзя}.
    4. В браузере: регистрация → письмо → вход → 2FA (шаг 12).
    5. Копии — не только на этом сервере (шаг 13.4); внешний мониторинг https://$DOMAIN/health.
    6. Файл настроек с паролями больше не нужен: shred -u <vds.conf>

    Обновление: $APP_DIR/scripts/deploy-vds.sh update   Откат: … rollback   Проверка: … check
EOF
  if [ -n "${GENERATED_PASSWORD:-}" ]; then
    printf '\n    %sПароль sudo для %s: %s%s — сохраните его и смените командой passwd.\n' "$B" "$APP_USER" "$GENERATED_PASSWORD" "$N"
  fi
  [ ! -f /var/run/reboot-required ] || printf '\n    Обновилось ядро: перезагрузите сервер (sudo reboot) — всё поднимется само.\n'
  if [ ${#WARNINGS[@]} -gt 0 ]; then
    printf '\n%sПредупреждения:%s\n' "$Y" "$N"
    printf '  - %s\n' "${WARNINGS[@]}"
  fi
}

cmd_install() {
  [ "$(id -u)" -eq 0 ] || die "install настраивает систему — запустите через sudo: sudo $0 install vds.conf"
  load_conf "$@"
  cd /
  install_system
  install_swap
  install_user
  install_firewall
  install_docker
  install_caddy
  install_repo
  install_env
  install_compose_override
  check_dns
  install_caddyfile
  start_stack
  install_backups
  harden_ssh
  step "Ждём сертификаты Let's Encrypt"
  if wait_for 120 "https://$DOMAIN/health" http_ok "https://$DOMAIN/health"; then
    cmd_check || warn "не все проверки прошли — см. выше и docs/deploy-vds.md, шаг 16"
  else
    warn "https://$DOMAIN пока недоступен: sudo journalctl -u caddy -n 50 (DNS, порты 80/443 у хостера); потом $APP_DIR/scripts/deploy-vds.sh check"
  fi
  install_summary
}

# ---------------------------------------------------------------- check, update, rollback

check_item() {
  local what=$1; shift
  if "$@" >/dev/null 2>&1; then ok "$what"; else printf '    %s✗%s %s\n' "$R" "$N" "$what"; CHECK_FAILS=$((CHECK_FAILS + 1)); fi
}
issuer_matches() {
  local want; want=$(env_get OIDC_ISSUER)
  [ "$(curl -fsS --max-time 10 "${want%/}/.well-known/openid-configuration" | jq -r .issuer)" = "$want" ]
}
has_hsts() { curl -fsSI --max-time 10 "$1" | grep -i '^strict-transport-security:' >/dev/null; }
all_running() { [ "$(compose ps --status running --services | sort | xargs)" = "app db rauthy" ]; }
ports_local() {
  ! ss -Htln | awk '{ print $4 }' | grep -E ':(8080|8443|5432)$' | grep -vE '^(127\.0\.0\.1|\[::1\]):' >/dev/null
}

cmd_check() {
  app_context
  local base; base=$(env_get APP_BASE_URL)
  CHECK_FAILS=0
  step "Проверка (шаг 12)"
  check_item "контейнеры app, db, rauthy запущены" all_running
  check_item "issuer Rauthy совпадает с OIDC_ISSUER ($(env_get OIDC_ISSUER))" issuer_matches
  check_item "$base/health отвечает ok" http_ok "$base/health"
  check_item "HTTPS с заголовком Strict-Transport-Security" has_hsts "$base/"
  check_item "порты 8080, 8443, 5432 — только на 127.0.0.1" ports_local
  check_item "резервные копии настроены (BACKUP_AGE_RECIPIENT)" test -n "$(env_get BACKUP_AGE_RECIPIENT)"
  [ "$CHECK_FAILS" -eq 0 ]
}

cmd_update() {
  app_context
  local tag=${1:-} prev=''
  [ -z "$tag" ] || valid_tag "$tag"
  step "Обновление (шаг 14.1)"
  if prev=$(running_tag); then
    printf '%s\n' "$prev" | as_app tee .previous-image-tag >/dev/null
    info "сейчас работает $prev"
  fi
  if [ -n "$(env_get BACKUP_AGE_RECIPIENT)" ]; then
    as_app env ENGINE=docker make --no-print-directory backup
  elif [ "${SKIP_BACKUP:-0}" = 1 ]; then
    warn "обновление без резервной копии (SKIP_BACKUP=1)"
  else
    die "резервные копии не настроены (BACKUP_AGE_RECIPIENT в .env, шаг 13). Обновить без копии: SKIP_BACKUP=1 $0 update"
  fi
  as_app git pull --ff-only --quiet
  ok "код: $(as_app git log -1 --format='%h %s')"
  [ -z "$tag" ] || env_set APP_IMAGE_TAG "$tag"
  info "образ: ghcr.io/yolshin195/family-budget:$(env_get APP_IMAGE_TAG)"
  compose pull --quiet
  compose up -d
  wait_app
  as_app docker image prune -f >/dev/null
  ok "работает $(running_tag || env_get APP_IMAGE_TAG)"
  [ -z "$prev" ] || info "откат на $prev: $0 rollback"
}

cmd_rollback() {
  app_context
  local tag=${1:-}
  [ -n "$tag" ] || tag=$(cat .previous-image-tag 2>/dev/null) || true
  [ -n "$tag" ] || die "укажите тег: $0 rollback sha-<коммит> (теги — GitHub → Packages → family-budget)"
  valid_tag "$tag"
  step "Откат приложения на $tag (шаг 14.2)"
  env_set APP_IMAGE_TAG "$tag"
  compose pull --quiet app
  compose up -d app
  wait_app
  info "APP_IMAGE_TAG=$tag зафиксирован в .env; вернуться на свежие версии: $0 update latest"
  info "Миграции БД идут только вперёд: если старый образ не работает с новой схемой — восстановите копию (шаг 15)."
}

main() {
  local cmd=${1:-help}
  [ $# -eq 0 ] || shift
  case $cmd in
    install) cmd_install "$@" ;;
    update) cmd_update "$@" ;;
    rollback) cmd_rollback "$@" ;;
    check) cmd_check ;;
    help | -h | --help) usage ;;
    *) usage >&2; exit 2 ;;
  esac
}

# Весь скрипт читается до запуска: update делает git pull, который может заменить этот файл.
main "$@"
exit
