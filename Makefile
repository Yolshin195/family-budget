# Семейная бухгалтерия. `make` — собрать и запустить всё в Podman (приложение + PostgreSQL + Rauthy).
# Движок можно сменить: make ENGINE=docker

ENGINE  ?= podman
# podman-compose, если установлен, иначе встроенный `podman compose`.
COMPOSE ?= $(shell command -v $(ENGINE)-compose >/dev/null 2>&1 && echo $(ENGINE)-compose || echo $(ENGINE) compose)
APP_PORT ?= 8080
URL      := http://localhost:$(APP_PORT)
# Node для сборки CSS и статики — в контейнере, на машине ставить не нужно.
NODE     := $(ENGINE) run --rm -v "$(CURDIR)":/w -w /w docker.io/library/node:22-alpine

.DEFAULT_GOAL := up
.PHONY: up down restart logs ps build rebuild clean db dev test css assets db-roles rauthy-secrets backup restore-check audit help

up: ## Собрать образ и запустить приложение, PostgreSQL и Rauthy в фоне
	$(COMPOSE) up -d --build
	@echo ""
	@echo "Готово: $(URL)   (вход — через Rauthy: http://localhost:8443; демо — кнопка «Посмотреть демо»)"
	@echo "Первый запуск: make rauthy-secrets → OIDC_CLIENT_SECRET в .env → make restart"
	@echo "Логи: make logs   Остановить: make down"

down: ## Остановить контейнеры (данные БД сохраняются)
	$(COMPOSE) down

restart: ## Перезапустить приложение без пересборки
	$(COMPOSE) up -d app

logs: ## Логи приложения
	$(COMPOSE) logs -f app

ps: ## Состояние контейнеров
	$(COMPOSE) ps

build: ## Только собрать образ
	$(COMPOSE) build app

rebuild: ## Пересобрать образ с нуля и перезапустить
	$(COMPOSE) build --no-cache app
	$(COMPOSE) up -d

clean: ## Остановить и удалить данные БД и Rauthy (демо-данные создадутся заново)
	$(COMPOSE) down -v

db: ## Только PostgreSQL и Rauthy — для локальной разработки (make dev)
	$(COMPOSE) up -d db
	@until $(COMPOSE) exec -T db pg_isready -q -h 127.0.0.1 -U budget -d family_budget 2>/dev/null; do sleep 1; done
	@$(COMPOSE) exec -T db sh /docker-entrypoint-initdb.d/10-roles.sh
	$(COMPOSE) up -d rauthy

db-roles: ## Пользователи БД без прав суперпользователя (для базы, созданной до обновления)
	$(COMPOSE) exec -T db sh /docker-entrypoint-initdb.d/10-roles.sh

rauthy-secrets: ## Сгенерированный секрет клиента family-budget и пароли демо-пользователей Rauthy
	@$(COMPOSE) exec -T rauthy /app/rauthy bootstrap get --format env

dev: db ## cargo run локально против PostgreSQL и Rauthy в контейнерах
	APP_ENV=dev BIND_ADDR=127.0.0.1:$(APP_PORT) DATABASE_URL=postgres://fb_app:$${APP_DB_PASSWORD:-fb_app_dev}@localhost:5432/family_budget cargo run -p family_budget

test: ## Тесты (БД не нужна)
	cargo test --workspace

css: ## Пересобрать Tailwind CSS после правки шаблонов
	$(NODE) sh -c "npm ci --silent && npm run css"

assets: ## Пересобрать всю статику: CSS, htmx, шрифты, иконки, favicon, OG-картинку
	$(NODE) sh -c "npm ci --silent && npm run build"

backup: ## Зашифрованные резервные копии баз family_budget и rauthy (хранятся BACKUP_KEEP_DAYS дней)
	ENGINE=$(ENGINE) ./scripts/backup.sh

restore-check: ## Проверка восстановления последней копии во временную базу
	ENGINE=$(ENGINE) ./scripts/restore-check.sh

audit: ## Проверка зависимостей: cargo audit и cargo deny
	cargo audit
	cargo deny check

help: ## Список команд
	@grep -E '^[a-z-]+:.*## ' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*## "} {printf "  make %-15s %s\n", $$1, $$2}'
