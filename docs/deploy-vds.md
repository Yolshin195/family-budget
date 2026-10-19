# Развёртывание в продакшене на одном VDS

Пошаговая инструкция: чистый VDS → «Семейная бухгалтерия» по HTTPS на своём домене, вход через Rauthy,
ежедневные зашифрованные резервные копии, обновление и откат.

Всё работает на одной машине:

```
Интернет ──443/80──► Caddy (на хосте, сертификаты Let's Encrypt автоматически)
                      ├─ budget.example.com ─► 127.0.0.1:8080 ─► контейнер app     (приложение)
                      └─ auth.example.com   ─► 127.0.0.1:8443 ─► контейнер rauthy  (вход, регистрация, 2FA)
                                                                  контейнер db      (PostgreSQL 16, 127.0.0.1:5432)
```

Снаружи открыты только порты 22 (SSH), 80 и 443. Приложение, Rauthy и PostgreSQL слушают только `127.0.0.1`.

Везде ниже `budget.example.com` — домен приложения, `auth.example.com` — домен Rauthy, `you@example.com` — ваш email.
Подставляйте свои значения.

## Содержание

- [Автоматическая установка скриптом](#автоматическая-установка-скриптом)
1. [Что понадобится](#1-что-понадобится)
2. [DNS](#2-dns)
3. [Подготовка сервера](#3-подготовка-сервера)
4. [Docker](#4-docker)
5. [Caddy — HTTPS и обратный прокси](#5-caddy--https-и-обратный-прокси)
6. [Код приложения](#6-код-приложения)
7. [Файл `.env` с секретами](#7-файл-env-с-секретами)
8. [Продакшен-настройки compose и клиент Rauthy](#8-продакшен-настройки-compose-и-клиент-rauthy)
9. [Конфигурация Caddy](#9-конфигурация-caddy)
10. [Первый запуск](#10-первый-запуск)
11. [Настройка Rauthy](#11-настройка-rauthy)
12. [Проверка](#12-проверка)
13. [Резервные копии](#13-резервные-копии)
14. [Обновление и откат](#14-обновление-и-откат)
15. [Восстановление из копии](#15-восстановление-из-копии)
16. [Если что-то не работает](#16-если-что-то-не-работает)
17. [Итоговый чек-лист](#17-итоговый-чек-лист)

---

## Автоматическая установка скриптом

Шаги 3–13 этой инструкции выполняет `scripts/deploy-vds.sh`. Образ приложения скрипт не собирает, а берёт готовый из
`ghcr.io/yolshin195/family-budget`. Перед запуском нужны шаги [1](#1-что-понадобится) и [2](#2-dns): сервер, домены и
DNS-записи, SMTP-сервис. У root (или у пользователя, от которого вы запускаете `sudo`) должен быть ваш SSH-ключ
(`ssh-copy-id root@<IP сервера>`). Скрипт перенесёт ключ пользователю `deploy`.

```bash
# на сервере (репозиторий публичный; иначе скопируйте оба файла через scp)
curl -fsSLO https://raw.githubusercontent.com/Yolshin195/family-budget/main/scripts/deploy-vds.sh
curl -fsSL https://raw.githubusercontent.com/Yolshin195/family-budget/main/deploy/vds.conf.example -o vds.conf
chmod 600 vds.conf && nano vds.conf      # домены, email, SMTP, публичный ключ age
sudo bash deploy-vds.sh install vds.conf
```

Ключ резервных копий создайте заранее **на своём компьютере**: `age-keygen -o fb-backup.key`. Строку `age1…` из него
впишите в `BACKUP_AGE_RECIPIENT`, а сам файл ключа сохраните в менеджер паролей.

Что делает `install`:

- обновляет систему, настраивает часовой пояс, автообновления безопасности, swap (при RAM до 2,5 ГБ) и ufw (SSH, 80, 443);
- создаёт пользователя `deploy` с sudo и вашим SSH-ключом и спрашивает для него пароль;
- ставит Docker (с ротацией логов) и Caddy, клонирует репозиторий в `/srv/family-budget`;
- генерирует `.env` со всеми секретами, `compose.override.yaml` (образ из GHCR) и клиента Rauthy с адресами продакшена;
- пишет `/etc/caddy/Caddyfile`, предупреждает, если DNS ещё не указывает на сервер;
- запускает PostgreSQL и Rauthy, сам забирает секрет клиента (`make rauthy-secrets`) в `.env` и запускает приложение;
- настраивает ежедневный `make backup` в cron и делает первую копию (если задан `BACKUP_AGE_RECIPIENT`);
- включает автообновление: таймер systemd раз в 5 минут ищет новую сборку в ghcr.io и обновляет приложение, при сбое
  откатывает и пишет в Telegram или на почту (шаг 14.3; только при настроенных копиях, `AUTO_UPDATE=0` — не включать);
- последним шагом отключает вход по паролю и под root, но только если у `deploy` есть SSH-ключ;
- проверяет результат (шаг 12) и печатает, что осталось сделать вручную.

Скрипт можно запускать повторно, например после правки `vds.conf` или исправления DNS: готовое он пропускает, а `.env`
и уже скачанные образы не трогает. После первого запуска вход под root закрыт, поэтому повторно запускайте под `deploy`:
`sudo bash /srv/family-budget/scripts/deploy-vds.sh install vds.conf`.

Вручную остаётся:

- сменить пароль администратора Rauthy и добавить passkey (шаг 11.1);
- сохранить копию `.env` в менеджер паролей;
- проверить регистрацию, письма и 2FA в браузере (шаг 12);
- настроить копирование резервных копий за пределы сервера (шаг 13.4);
- удалить `vds.conf`: `shred -u vds.conf`.

Дальнейшая работа — под пользователем `deploy`:

```bash
/srv/family-budget/scripts/deploy-vds.sh update          # копия БД → git pull → свежий образ → перезапуск (шаг 14.1)
/srv/family-budget/scripts/deploy-vds.sh update 0.3.0    # то же с фиксацией версии образа в APP_IMAGE_TAG
/srv/family-budget/scripts/deploy-vds.sh rollback        # вернуть образ, работавший до последнего update (шаг 14.2)
/srv/family-budget/scripts/deploy-vds.sh rollback sha-1a2b3c4
/srv/family-budget/scripts/deploy-vds.sh check           # проверки шага 12
/srv/family-budget/scripts/deploy-vds.sh auto-update status   # автообновление: таймер и последние запуски (шаг 14.3)
/srv/family-budget/scripts/deploy-vds.sh auto-update test     # пробное уведомление в Telegram / на почту
```

Ниже — те же шаги вручную. По ним удобно разбираться, что именно делает скрипт, и чинить нестандартные случаи.

## 1. Что понадобится

- **VDS** с Ubuntu 24.04 LTS (для Debian 12 команды те же, отличия отмечены).
  - готовый образ из GitHub Container Registry (рекомендуется): 1–2 vCPU, **2 ГБ RAM**, 20 ГБ диска;
  - сборка образа на самом сервере: от 4 ГБ RAM (или swap), +10 ГБ диска, сборка 20–40 минут.
  - Архитектура amd64 или arm64 — образ собирается для обеих.
- **Домен** с доступом к DNS-записям: два имени — для приложения и для Rauthy. Хватит и одного имени, в том числе
  бесплатного технического домена хостера вроде `<имя>.fvds.ru`: Rauthy тогда работает на нём же по пути `/auth/v1`
  (шаг 2).
- **SMTP-сервис**: Rauthy отправляет письма регистрации, сброса пароля и 6-значные коды 2FA. Без SMTP никто, кроме
  администратора, войти не сможет. Бесплатно для семьи хватит личного ящика на Яндексе или Mail.ru с паролем
  приложения (ниже); для больших объёмов — Mailgun, Postmark, Brevo, Unisender Go и т. п.
- **Telegram** (необязательно): бот, в который автообновление пишет об обновлениях и сбоях (шаг 14.3).
- **SSH-ключ** на вашем компьютере (`ssh-keygen -t ed25519`, если ключа ещё нет).
- **Менеджер паролей** (или другое надёжное место вне сервера) для `.env` и ключа резервных копий.

## 2. DNS

У регистратора домена или в DNS-хостинге создайте записи, указывающие на IP сервера:

| Тип | Имя | Значение |
| --- | --- | --- |
| A | `budget` | `<IPv4 сервера>` |
| A | `auth` | `<IPv4 сервера>` |
| AAAA | `budget`, `auth` | `<IPv6 сервера>` — только если у сервера есть IPv6 и он работает |

Проверка (записи обновляются от минут до нескольких часов):

```bash
dig +short budget.example.com
dig +short auth.example.com
```

Обе команды должны вывести IP вашего сервера. Пока этого нет, Caddy не сможет получить сертификаты (шаг 9).

**Один домен.** Если второго имени нет (например, только технический домен хостера `elshin195.fvds.ru`), Rauthy
работает на том же домене: все его адреса начинаются с `/auth/v1`, а у приложения таких нет (у него `/auth/login`,
`/auth/callback`). Скрипт включает этот режим сам, если в `vds.conf` не задан `AUTH_DOMAIN`. Вручную — в шагах 7 и 9
везде вместо `auth.example.com` пишите домен приложения, а Caddyfile возьмите из варианта для одного домена.
Цена: приложение и Rauthy делят один origin, и уязвимость в одном из них браузер не отделит от другого. Для семейного
бюджета это приемлемо, но, когда появится свой домен, лучше перейти на отдельный `auth.`.

**Бесплатный SMTP — ящик Яндекса.** В Яндекс ID → Безопасность → Пароли приложений создайте пароль для почты;
в настройках Почты → Почтовые программы разрешите доступ по IMAP с паролями приложений. В `vds.conf`:
`SMTP_URL=smtp.yandex.ru`, `SMTP_PORT=587`, `SMTP_USERNAME=<ящик>@yandex.ru`, `SMTP_PASSWORD='<пароль приложения>'`.
Яндекс отправляет письма только от адреса ящика, поэтому скрипт сам ставит его отправителем. У Mail.ru то же самое:
`smtp.mail.ru`, `465`, пароль для внешнего приложения.

## 3. Подготовка сервера

### 3.1. Первый вход и обновление

```bash
ssh root@<IP сервера>
apt update && apt full-upgrade -y
timedatectl set-timezone Europe/Moscow     # ваш часовой пояс — для расписания cron и времени в логах
```

### 3.2. Пользователь для работы

Работать под root не нужно — заведите пользователя `deploy` с sudo и перенесите ему ваш SSH-ключ:

```bash
adduser deploy                      # задайте пароль: он нужен для sudo
usermod -aG sudo deploy
rsync --archive --chown=deploy:deploy ~/.ssh /home/deploy
```

**Не закрывая текущую сессию**, откройте второй терминал и проверьте вход: `ssh deploy@<IP сервера>`, затем `sudo whoami`
(должно вывести `root`). Дальше всё — под пользователем `deploy`.

### 3.3. SSH: только ключи, без root

```bash
sudo tee /etc/ssh/sshd_config.d/00-hardening.conf >/dev/null <<'EOF'
PermitRootLogin no
PasswordAuthentication no
KbdInteractiveAuthentication no
EOF
sudo sshd -t && sudo systemctl restart ssh
```

Файл назван `00-…` намеренно: sshd берёт первое найденное значение, а в облачных образах бывает
`50-cloud-init.conf` с `PasswordAuthentication yes`.

Снова проверьте в новом терминале, что `ssh deploy@<IP>` работает, и только потом закрывайте старые сессии.

### 3.4. Файрвол

```bash
sudo ufw allow OpenSSH
sudo ufw allow 80/tcp
sudo ufw allow 443/tcp
sudo ufw allow 443/udp      # HTTP/3 в Caddy, необязательно
sudo ufw enable
sudo ufw status
```

> Docker публикует порты в обход правил ufw. Поэтому все порты контейнеров в этой инструкции привязаны к
> `127.0.0.1` — и именно поэтому в `.env` (шаг 7) стоит `APP_PORT=127.0.0.1:8080`, а не просто `8080`.

### 3.5. Автоматические обновления безопасности

```bash
sudo apt install -y unattended-upgrades
sudo dpkg-reconfigure -plow unattended-upgrades     # ответьте «Yes»
```

### 3.6. Swap (если RAM 2 ГБ и меньше)

```bash
sudo fallocate -l 2G /swapfile
sudo chmod 600 /swapfile
sudo mkswap /swapfile
sudo swapon /swapfile
echo '/swapfile none swap sw 0 0' | sudo tee -a /etc/fstab
free -h
```

### 3.7. Утилиты

```bash
sudo apt install -y git make age jq curl ca-certificates gnupg
```

`age` — шифрование резервных копий, `make` — команды проекта, `jq` — проверки на шаге 12.

## 4. Docker

Проект по умолчанию запускается в Podman, но `Makefile` поддерживает Docker (`ENGINE=docker`). Для сервера
рекомендуется Docker: демон сам поднимает контейнеры после перезагрузки (`restart: unless-stopped`), а продакшен-файл
compose из шага 8 использует возможности `docker compose` (`!reset`).

### 4.1. Установка из официального репозитория Docker

```bash
sudo install -m 0755 -d /etc/apt/keyrings
sudo curl -fsSL https://download.docker.com/linux/ubuntu/gpg -o /etc/apt/keyrings/docker.asc
sudo chmod a+r /etc/apt/keyrings/docker.asc
echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] \
https://download.docker.com/linux/ubuntu $(. /etc/os-release && echo "${UBUNTU_CODENAME:-$VERSION_CODENAME}") stable" \
  | sudo tee /etc/apt/sources.list.d/docker.list >/dev/null
sudo apt update
sudo apt install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
```

На Debian замените оба `linux/ubuntu` на `linux/debian`.

### 4.2. Ротация логов контейнеров

Иначе логи со временем займут весь диск:

```bash
sudo tee /etc/docker/daemon.json >/dev/null <<'EOF'
{
  "log-driver": "json-file",
  "log-opts": { "max-size": "20m", "max-file": "5" }
}
EOF
sudo systemctl restart docker
sudo systemctl enable docker containerd
```

### 4.3. Доступ к Docker без sudo и `ENGINE=docker` по умолчанию

```bash
sudo usermod -aG docker deploy
echo 'export ENGINE=docker' >> ~/.bashrc
exit                                    # перелогиньтесь, чтобы группа применилась
```

```bash
ssh deploy@<IP сервера>
docker compose version                  # нужна версия 2.24 или новее
docker run --rm hello-world
```

> Группа `docker` даёт права, равные root. Не добавляйте в неё посторонних пользователей.

## 5. Caddy — HTTPS и обратный прокси

Caddy сам получает и продлевает сертификаты Let's Encrypt. Установка из официального репозитория:

```bash
sudo apt install -y debian-keyring debian-archive-keyring apt-transport-https
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' \
  | sudo gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' \
  | sudo tee /etc/apt/sources.list.d/caddy-stable.list >/dev/null
sudo chmod o+r /usr/share/keyrings/caddy-stable-archive-keyring.gpg /etc/apt/sources.list.d/caddy-stable.list
sudo apt update
sudo apt install -y caddy
```

Сам конфиг — на шаге 9, когда будут готовы домены и `.env`.

## 6. Код приложения

Из репозитория на сервере нужны `compose.yaml`, `Makefile`, `deploy/` и `scripts/`, поэтому клонируем его целиком.

```bash
sudo mkdir -p /srv/family-budget
sudo chown deploy:deploy /srv/family-budget
git clone https://github.com/Yolshin195/family-budget.git /srv/family-budget
cd /srv/family-budget
```

Если репозиторий приватный, клонируйте по SSH с deploy key: на сервере `ssh-keygen -t ed25519 -f ~/.ssh/fb_deploy -N ''`,
содержимое `~/.ssh/fb_deploy.pub` добавьте в GitHub → репозиторий → Settings → Deploy keys (только чтение), затем

```bash
GIT_SSH_COMMAND='ssh -i ~/.ssh/fb_deploy' git clone git@github.com:Yolshin195/family-budget.git /srv/family-budget
git -C /srv/family-budget config core.sshCommand 'ssh -i ~/.ssh/fb_deploy'
```

## 7. Файл `.env` с секретами

Все настройки и секреты лежат в `/srv/family-budget/.env`. В git он не попадает (`.gitignore`).

Ниже — команда, которая сразу генерирует все случайные секреты. **Сначала поправьте три переменные в начале.**
Выполняйте шаги 7–9 в одной сессии терминала. Если переподключились, задайте `DOMAIN`, `AUTH_DOMAIN` и `ADMIN_EMAIL` заново.

```bash
cd /srv/family-budget

DOMAIN=budget.example.com        # ← домен приложения
AUTH_DOMAIN=auth.example.com     # ← домен Rauthy
ADMIN_EMAIL=you@example.com      # ← ваш email: администратор Rauthy

ENC_ID=$(openssl rand -hex 4)
install -m 600 /dev/null .env           # файл сразу доступен только владельцу
cat > .env <<EOF
# ---------------------------------------------------------------- приложение
APP_ENV=prod
APP_BASE_URL=https://${DOMAIN}
SESSION_SECRET=$(openssl rand -base64 48 | tr -d '\n')
# Только localhost: снаружи приложение доступно через Caddy.
APP_PORT=127.0.0.1:8080
# Образ из GitHub Container Registry (см. compose.override.yaml): latest, версия или sha-<коммит>.
APP_IMAGE_TAG=latest
# В продакшене демо-данные запрещены (приложение не стартует с SEED_DEMO=1).
SEED_DEMO=0
# Кнопка «Посмотреть демо» на лендинге (гость — только просмотр демо-семьи). 0 — убрать.
GUEST_DEMO=1
CONTACT_EMAIL=${ADMIN_EMAIL}
# Приложение за Caddy: IP клиента — из X-Forwarded-For.
TRUST_PROXY=1
RATE_LIMIT_PER_MIN=600
PUBLIC_RATE_LIMIT_PER_MIN=120

# ---------------------------------------------------------------- PostgreSQL
# Только hex: пароли подставляются в DATABASE_URL, спецсимволы его сломают.
DB_PASSWORD=$(openssl rand -hex 24)
APP_DB_PASSWORD=$(openssl rand -hex 24)
RAUTHY_DB_PASSWORD=$(openssl rand -hex 24)

# ---------------------------------------------------------------- вход через Rauthy (OIDC)
OIDC_ISSUER=https://${AUTH_DOMAIN}/auth/v1/
OIDC_CLIENT_ID=family-budget
# Заполняется на шаге 10: make rauthy-secrets
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
RAUTHY_ENC_KEYS=${ENC_ID}/$(openssl rand -base64 32)
RAUTHY_ENC_KEY_ACTIVE=${ENC_ID}
RAUTHY_PROXY_MODE=true
# Сети Docker: Caddy приходит через шлюз сети compose, приложение — из этой же сети.
RAUTHY_TRUSTED_PROXIES=172.16.0.0/12
RAUTHY_COOKIE_MODE=host
RAUTHY_RP_ID=${AUTH_DOMAIN}
RAUTHY_RP_ORIGIN=https://${AUTH_DOMAIN}:443

# Письма Rauthy: регистрация, сброс пароля, код 2FA. ЗАПОЛНИТЕ.
SMTP_URL=smtp.example.com
SMTP_PORT=587
SMTP_TLS_MODE=tls
SMTP_FROM="Семейный бюджет <no-reply@${DOMAIN}>"
SMTP_USERNAME=
SMTP_PASSWORD=

# ---------------------------------------------------------------- резервные копии
BACKUP_DIR=./backups
# Заполняется на шаге 13: публичный ключ age (age1…).
BACKUP_AGE_RECIPIENT=
BACKUP_OLD_DAYS=7
EOF
```

Откройте файл и заполните SMTP (адрес сервера, логин, пароль, адрес отправителя — он должен быть разрешён
вашим SMTP-сервисом):

```bash
nano .env
```

Правила для значений в `.env`:

- значение с пробелами — в двойных кавычках (как `SMTP_FROM`), иначе `make backup` не сможет прочитать файл;
- значение со знаком `$` — в одинарных кавычках, иначе docker compose попытается подставить переменную;
- пароли БД (`DB_PASSWORD`, `APP_DB_PASSWORD`, `RAUTHY_DB_PASSWORD`) применяются **только при первом создании базы**.
  Если поменяете `APP_DB_PASSWORD` или `RAUTHY_DB_PASSWORD` позже, выполните `make db-roles`. Пароль суперпользователя
  после этого меняется только вручную (`ALTER USER budget PASSWORD …`).

**Сразу сохраните копию `.env` в менеджер паролей.** Без `RAUTHY_ENC_KEYS` резервную копию базы Rauthy не расшифровать,
а без остальных секретов придётся заново настраивать вход.

## 8. Продакшен-настройки compose и клиент Rauthy

`compose.yaml` в репозитории рассчитан на локальный запуск: образ приложения собирается из исходников, а клиент Rauthy
и демо-пользователи берутся из `deploy/rauthy/bootstrap` с адресами `localhost`. На сервере это переопределяет файл
`compose.override.yaml`: `docker compose` подхватывает его автоматически, поэтому все команды `make` работают без изменений.

### 8.1. `compose.override.yaml`

```bash
cd /srv/family-budget
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
```

Хотите собирать образ на сервере, а не брать из реестра? Удалите строки `build: !reset null` и `image: …`. Тогда `make`
соберёт образ из исходников, и понадобится RAM из шага 1.

### 8.2. Клиент Rauthy для продакшена

Rauthy создаёт клиента `family-budget` из этих файлов **только при первом старте с пустой базой**:

```bash
mkdir -p deploy/rauthy/bootstrap-prod
cat > deploy/rauthy/bootstrap-prod/clients.json <<EOF
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
echo '[]' > deploy/rauthy/bootstrap-prod/users.json
chmod -R a+rX deploy/rauthy/bootstrap-prod       # Rauthy в контейнере работает не от вашего пользователя
cat deploy/rauthy/bootstrap-prod/clients.json     # проверьте домен
```

### 8.3. Скрыть серверные файлы от git

Чтобы `git status` оставался чистым и `git pull` не мешал:

```bash
printf '%s\n' compose.override.yaml deploy/rauthy/bootstrap-prod/ backups/ >> .git/info/exclude
git status            # «nothing to commit, working tree clean»
```

## 9. Конфигурация Caddy

```bash
sudo tee /etc/caddy/Caddyfile >/dev/null <<EOF
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
sudo caddy validate --config /etc/caddy/Caddyfile
sudo systemctl reload caddy
sudo journalctl -u caddy -n 50 --no-pager
```

Один домен (шаг 2) — вместо двух блоков один:

```
${DOMAIN} {
	handle /auth/v1* {
		reverse_proxy 127.0.0.1:8443
	}
	handle {
		reverse_proxy 127.0.0.1:8080
	}
}
```

В логе должно появиться `certificate obtained successfully` для обоих доменов. Сжатие, HSTS, CSP и остальные заголовки
безопасности ставит само приложение. Caddy перезаписывает `X-Forwarded-For` реальным IP клиента, поэтому
`TRUST_PROXY=1` безопасен. Пока контейнеры не запущены, Caddy будет отвечать `502` — это нормально.

## 10. Первый запуск

### 10.1. Образы

```bash
cd /srv/family-budget
docker compose config --quiet && echo "compose OK"
docker compose pull
```

Если `pull` для `ghcr.io/yolshin195/family-budget` отвечает `denied` / `unauthorized`, пакет приватный. Войдите в реестр
с classic-токеном GitHub (Settings → Developer settings → Personal access tokens, право `read:packages`):

```bash
echo '<токен>' | docker login ghcr.io -u <ваш-логин-github> --password-stdin
docker compose pull
```

### 10.2. PostgreSQL и Rauthy

Приложение пока не запускаем: без секрета клиента оно не пройдёт проверку продакшен-настроек.

```bash
docker compose up -d db rauthy
docker compose ps                       # db — healthy, rauthy — running
docker compose logs -f rauthy           # дождитесь, пока Rauthy запустится; Ctrl+C — выйти из логов
```

При первом старте с пустым томом PostgreSQL выполняет `deploy/postgres/init-roles.sh`. Скрипт создаёт базы
`family_budget` и `rauthy` и пользователей `fb_app` и `rauthy` без прав суперпользователя. Затем Rauthy создаёт
администратора `RAUTHY_ADMIN_EMAIL` и клиента `family-budget`.

### 10.3. Секрет клиента → `.env`

```bash
make rauthy-secrets
```

Найдите в выводе сгенерированный секрет клиента `family-budget` и впишите его в `.env`:

```bash
nano .env          # OIDC_CLIENT_SECRET=<секрет>
```

### 10.4. Приложение

```bash
docker compose up -d app
docker compose logs -f app
```

В логах должно быть `Включённые фичи: [...]`, без ошибок. Миграции базы применяются автоматически при старте.
Если приложение пишет `настройки продакшена: …` и перезапускается, значит, в `.env` не хватает перечисленного — см. шаг 16.

## 11. Настройка Rauthy

### 11.1. Вход администратора

Пароль администратора Rauthy печатается в логах при первом старте:

```bash
docker compose logs rauthy 2>&1 | grep -i -B2 -A2 password
```

Откройте `https://auth.example.com/auth/v1/admin`, войдите как `RAUTHY_ADMIN_EMAIL` и сразу:

1. смените пароль;
2. зарегистрируйте **passkey** (ключ безопасности или биометрия) — админка Rauthy должна быть защищена вторым фактором.

Для повседневной работы с бюджетом лучше завести отдельную обычную учётку через «Создать бюджет» на сайте, а не
пользоваться администратором.

### 11.2. Проверьте клиента

Admin → Clients → `family-budget`:

- Redirect URIs — `https://budget.example.com/auth/callback`;
- Post Logout Redirect URIs — `https://budget.example.com/`.

Если Rauthy уже стартовал со старым bootstrap и там `localhost`, исправьте адреса здесь вручную.

### 11.3. Вход через GitHub и Facebook (необязательно)

Admin → Providers → добавить провайдера. В приложении на стороне GitHub (Settings → Developer settings → OAuth Apps)
или Facebook укажите callback `https://auth.example.com/auth/v1/providers/callback`. Client ID и Secret провайдера
введите в Rauthy.

### 11.4. Удаление пользователя в Rauthy при удалении аккаунта (необязательно)

Admin → API Keys → создать ключ с правом `Users: delete`. В `.env` впишите **в одинарных кавычках**:

```
RAUTHY_ADMIN_API_KEY='<имя ключа>$<секрет>'
```

Затем `docker compose up -d app`. Без ключа пользователь удаляет учётку в кабинете Rauthy сам.

## 12. Проверка

```bash
# Issuer Rauthy — должен совпадать с OIDC_ISSUER из .env символ в символ (со слешем в конце)
curl -s https://auth.example.com/auth/v1/.well-known/openid-configuration | jq -r .issuer
grep OIDC_ISSUER .env

# Приложение живо
curl -s https://budget.example.com/health          # ok

# HTTPS и заголовки безопасности
curl -sI https://budget.example.com/ | grep -iE 'HTTP/|strict-transport|content-security'

# Контейнеры
docker compose ps

# Снаружи открыты только 22, 80, 443; 8080, 8443 и 5432 — только на 127.0.0.1
sudo ss -tlnp | grep -E ':(22|80|443|8080|8443|5432)\b'
```

Проверка в браузере:

1. `https://budget.example.com` — лендинг, замок в адресной строке.
2. «Посмотреть демо» (если `GUEST_DEMO=1`) — демо-семья в режиме просмотра.
3. «Создать бюджет» → регистрация в Rauthy → письмо пришло → вход → онбординг (имя, валюта, язык).
4. Профиль → Безопасность → включите 2FA, выйдите и войдите снова — код приходит по почте.

Перезагрузка: `sudo reboot`, через минуту `docker compose ps` и `curl https://budget.example.com/health`. Docker и Caddy
стартуют сами, контейнеры поднимаются по `restart: unless-stopped`.

## 13. Резервные копии

`make backup` делает `pg_dump` баз `family_budget` и `rauthy`, шифрует их ключом [age](https://age-encryption.org) и
кладёт в `backups/`. Копию делает и `update` перед каждым обновлением, поэтому, чтобы копии не заняли диск, каждой
базы хранится не больше трёх: самая свежая, самая старая из последних 6 дней (1–6 дней назад) и самая свежая из
сделанных неделю и больше назад (7–12 дней). Старшая копия подменяется средней, когда та дорастёт до недели, —
через неделю после первого запуска копия не моложе недели есть всегда. Порог — `BACKUP_OLD_DAYS` в `.env` (7, от 1
до 15). Файлы с другими именами не удаляются: чтобы сохранить копию надолго, переименуйте её
(`family_budget-перед-переездом.dump.age`).

### 13.1. Ключ шифрования

```bash
age-keygen -o ~/family-budget-backup.key
# Public key: age1…   ← это значение
grep 'public key' ~/family-budget-backup.key
nano /srv/family-budget/.env       # BACKUP_AGE_RECIPIENT=age1…
```

**Приватный ключ `~/family-budget-backup.key` сохраните вне сервера** (менеджер паролей) и удалите с сервера:

```bash
cat ~/family-budget-backup.key     # скопируйте содержимое в менеджер паролей
shred -u ~/family-budget-backup.key
```

На сервере остаётся только публичный ключ. Даже если сервер взломают, старые копии не расшифровать.

### 13.2. Первая копия вручную

```bash
cd /srv/family-budget
make backup
ls -lh backups/
```

### 13.3. Ежедневно по расписанию

```bash
mkdir -p /srv/family-budget/backups
crontab -e
```

Добавьте строку (каждый день в 03:15 по времени сервера):

```
15 3 * * * cd /srv/family-budget && ENGINE=docker make backup >> backups/backup.log 2>&1
```

### 13.4. Копии — не только на этом сервере

Копия на том же диске пропадёт вместе с сервером. Раз в сутки забирайте `backups/` на другую машину или в облачное
хранилище. Например, с домашнего компьютера или второго сервера:

```bash
rsync -av --ignore-existing deploy@<IP сервера>:/srv/family-budget/backups/ ~/family-budget-backups/
```

или настройте на сервере `rclone` с S3-совместимым хранилищем (`rclone copy backups/ remote:family-budget-backups`
отдельной строкой в cron после бэкапа). Файлы уже зашифрованы, хранилищу можно не доверять.

### 13.5. Проверка восстановления — раз в месяц

```bash
cd /srv/family-budget
install -m 600 /dev/null ~/fb.key && nano ~/fb.key    # вставьте приватный ключ из менеджера паролей
BACKUP_AGE_KEY_FILE=$HOME/fb.key make restore-check
shred -u ~/fb.key
```

Скрипт восстановит последнюю копию во временную базу, покажет число пользователей и операций и удалит временную базу.

## 14. Обновление и откат

Пуш в `main`, меняющий код или сборку, публикует образ `ghcr.io/yolshin195/family-budget` с тегами `latest`, версией
из `Cargo.toml` и `sha-<коммит>`. Теги ставятся только при зелёных тестах. Правки только документации и скриптов
образ не пересобирают, поэтому тег `sha-` есть не у каждого коммита.

### 14.1. Обновить приложение

```bash
cd /srv/family-budget
make backup                         # всегда перед обновлением
git pull                            # compose.yaml, скрипты, версия Rauthy
docker compose pull
docker compose up -d
docker compose logs -f app          # миграции и старт
docker image prune -f               # удалить старые образы
```

Всё это одной командой: `scripts/deploy-vds.sh update`. Скрипт ещё и запоминает работавший образ для `rollback`.

Для предсказуемости вместо `latest` можно зафиксировать версию: в `.env` укажите `APP_IMAGE_TAG=0.2.0` или
`APP_IMAGE_TAG=sha-1a2b3c4`. Список тегов — на странице пакета в GitHub (Packages → family-budget).

Перед обновлением Rauthy (его версия зафиксирована в `compose.yaml`, Renovate присылает PR) прочитайте
[release notes Rauthy](https://github.com/sebadob/rauthy/releases): между версиями бывают изменения настроек.

### 14.2. Откатить приложение

```bash
nano .env                           # APP_IMAGE_TAG=sha-<коммит предыдущей сборки> (Packages → family-budget)
docker compose up -d app
```

Или `scripts/deploy-vds.sh rollback`: без аргумента он вернёт образ, работавший до последнего `update`.

Миграции базы выполняются только вперёд. Если новая версия уже изменила схему, старый образ может с ней не работать.
Тогда откат — это старый образ плюс восстановление базы из копии, сделанной перед обновлением (шаг 15).

### 14.3. Автоматическое обновление

Как Flux и Argo CD в Kubernetes, только проще: сервер сам раз в 5 минут смотрит в ghcr.io, есть ли новая сборка.
У GitHub нет ни ключей, ни доступа к серверу, а наружу не открыто ничего нового. Таймер ставит `install`; на уже
развёрнутом сервере — `sudo /srv/family-budget/scripts/deploy-vds.sh auto-update on`.

Каждые 5 минут таймер `family-budget-update.timer` запускает `deploy-vds.sh auto-update`:

1. `docker compose pull app` — скачать образ с тегом из `APP_IMAGE_TAG`. Если он совпадает с запущенным, на этом всё:
   ни копии БД, ни перезапуска.
2. Новая сборка → `update` целиком: копия БД, `git pull`, перезапуск, ожидание `/health`. Уведомление «обновлено».
3. Не поднялась → `rollback` на работавший образ и уведомление с концом журнала. `rollback` закрепляет
   `APP_IMAGE_TAG=sha-…`, поэтому автообновление встаёт на паузу. Исправили в `main` и сборка прошла —
   `deploy-vds.sh update latest`. Сборку, которая не встала, таймер второй раз не ставит.

Новая сборка появляется только после зелёных тестов на `main` (шаг 14), поэтому за `latest` можно следить смело.
Вместе с образом `git pull` приносит и остальное из `main`, в том числе обновление Rauthy из PR Renovate: такие PR
мержите, прочитав release notes Rauthy. Вручную `update` и `rollback` работают как раньше: с таймером они не
пересекаются (общая блокировка).

**Уведомления.** Без них о сбое ночью вы узнаете от семьи. Заполните в `/srv/family-budget/.env` (или в `vds.conf`
перед `install`) Telegram, почту или оба канала:

- **Telegram.** Напишите [@BotFather](https://t.me/BotFather) `/newbot` и получите токен `TELEGRAM_BOT_TOKEN`.
  Отправьте своему боту любое сообщение и откройте `https://api.telegram.org/bot<токен>/getUpdates`: число из
  `"chat":{"id":…}` — это `TELEGRAM_CHAT_ID`.
- **Почта.** Уходит через те же `SMTP_*`, что у Rauthy, на `NOTIFY_EMAIL` (по умолчанию — `CONTACT_EMAIL`).

Проверка: `deploy-vds.sh auto-update test`.

```bash
/srv/family-budget/scripts/deploy-vds.sh auto-update status    # следующая проверка, работающая версия, журнал
/srv/family-budget/scripts/deploy-vds.sh auto-update           # проверить прямо сейчас
sudo journalctl -u family-budget-update -n 50                  # полный журнал
sudo /srv/family-budget/scripts/deploy-vds.sh auto-update off  # выключить; on — включить снова
```

`deploy-vds.sh check` тоже показывает, была ли последняя проверка без ошибок.

## 15. Восстановление из копии

На том же сервере или на новом. Для нового сервера пройдите шаги 3–9 с **тем же `.env`** из менеджера паролей:
без прежних `RAUTHY_ENC_KEYS` данные Rauthy не расшифровать.

```bash
cd /srv/family-budget
docker compose stop app rauthy
docker compose up -d db
install -m 600 /dev/null ~/fb.key && nano ~/fb.key    # приватный ключ age

# выберите копии на нужную дату
ls -1t backups/*.dump.age | head
FB=backups/family_budget-<дата>.dump.age
RA=backups/rauthy-<дата>.dump.age

age -d -i ~/fb.key "$FB" | docker compose exec -T db pg_restore -U budget -d family_budget --clean --if-exists
age -d -i ~/fb.key "$RA" | docker compose exec -T db pg_restore -U budget -d rauthy --clean --if-exists
shred -u ~/fb.key

docker compose up -d
docker compose logs -f app
```

На новом сервере сначала скопируйте файлы копий в `backups/` и запустите только `db` (`docker compose up -d db`),
не запуская Rauthy: иначе он успеет создать пустую базу со своим bootstrap.

## 16. Если что-то не работает

| Симптом | Что проверить |
| --- | --- |
| `app` постоянно перезапускается, в логе `настройки продакшена: …` | В сообщении перечислено, чего не хватает: `SESSION_SECRET` (≥ 32 символов), `APP_BASE_URL` и `OIDC_ISSUER` только с `https://`, `OIDC_CLIENT_SECRET`. После правки `.env` — `docker compose up -d app` |
| `SEED_DEMO=1 запрещён в продакшене` | В `.env` должно быть `SEED_DEMO=0` |
| `БД недоступна (попытка …)` | `docker compose ps db`, `docker compose logs db`. Если `.env` менялся после первого старта, пароли в базе старые: `make db-roles` |
| Caddy не получает сертификат | `dig +short <домен>` указывает на сервер; порты 80 и 443 открыты (`sudo ufw status`, файрвол в панели хостера); `sudo journalctl -u caddy -f` |
| `502 Bad Gateway` | Контейнер не запущен или упал: `docker compose ps`, `docker compose logs app` / `rauthy` |
| Rauthy отвечает ошибкой или блокирует запросы | Подсеть сети compose должна входить в `RAUTHY_TRUSTED_PROXIES`: `docker network inspect family-budget_default -f '{{(index .IPAM.Config 0).Subnet}}'`. Если там не `172.x`, впишите эту подсеть и выполните `docker compose up -d rauthy` |
| После входа ошибка `redirect_uri` | Admin Rauthy → Clients → `family-budget` → Redirect URIs = `https://<домен>/auth/callback` |
| Ошибка проверки issuer при входе | `OIDC_ISSUER` должен совпадать с `issuer` из discovery (шаг 12) символ в символ, включая `https` и слеш в конце |
| Не приходят письма | Настройки `SMTP_*`, `docker compose logs rauthy`. Многие хостеры блокируют порт 25 — используйте 587 (`SMTP_TLS_MODE=tls`) или 465. Добавьте SPF/DKIM для домена отправителя, иначе письма уйдут в спам |
| Passkey в админке Rauthy не работает | `RAUTHY_RP_ID` = домен Rauthy без схемы, `RAUTHY_RP_ORIGIN` = `https://<домен Rauthy>:443`. Меняйте `RP_ID` только до регистрации ключей |
| `make backup`: синтаксическая ошибка | Значение с пробелами в `.env` без кавычек (обычно `SMTP_FROM`) |
| Диск заполнен | `docker system df`, `docker image prune -f`, размер `backups/`, `journalctl --vacuum-size=200M` |

Полезные команды (все — из `/srv/family-budget`):

```bash
make ps                 # состояние контейнеров
make logs               # логи приложения
docker compose logs --tail 100 rauthy
docker compose restart app
docker compose exec db psql -U budget -d family_budget    # консоль БД (суперпользователь)
```

**Не запускайте на сервере `make clean`** — эта команда удаляет тома с базой данных и Rauthy.

## 17. Итоговый чек-лист

- [ ] DNS `budget.` и `auth.` указывают на сервер, сертификаты выпущены
- [ ] SSH только по ключу, root-вход выключен, ufw: 22, 80, 443
- [ ] `ss -tlnp`: 8080, 8443, 5432 слушают только `127.0.0.1`
- [ ] `.env`: `APP_ENV=prod`, `SEED_DEMO=0`, `TRUST_PROXY=1`, права `600`, копия — в менеджере паролей
- [ ] Rauthy: пароль администратора сменён, passkey добавлен, redirect URI клиента — продакшен-домен
- [ ] SMTP работает: регистрация и код 2FA приходят на почту
- [ ] `make backup` в cron, приватный ключ age вне сервера, копии уходят с сервера, `make restore-check` проходит
- [ ] Автообновление включено, `deploy-vds.sh auto-update test` доходит до Telegram или почты
- [ ] После `sudo reboot` всё поднимается само
- [ ] Внешний мониторинг `https://budget.example.com/health` (UptimeRobot, Uptime Kuma и т. п.)
