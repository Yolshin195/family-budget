# Семейная бухгалтерия. `make` — собрать и запустить всё в Podman (app + PostgreSQL).
# Движок можно сменить: make ENGINE=docker

ENGINE  ?= podman
# podman-compose, если установлен, иначе встроенный `podman compose`.
COMPOSE ?= $(shell command -v $(ENGINE)-compose >/dev/null 2>&1 && echo $(ENGINE)-compose || echo $(ENGINE) compose)
APP_PORT ?= 8080
URL      := http://localhost:$(APP_PORT)

.DEFAULT_GOAL := up
.PHONY: up down restart logs ps build rebuild clean db dev test css help

up: ## Собрать образ и запустить app + db в фоне
	$(COMPOSE) up -d --build
	@echo ""
	@echo "Готово: $(URL)   (aleksey@demo.local / demo123, Swagger: $(URL)/docs)"
	@echo "Логи: make logs   Остановить: make down"

down: ## Остановить контейнеры (данные БД сохраняются)
	$(COMPOSE) down

restart: ## Перезапустить приложение без пересборки
	$(COMPOSE) restart app

logs: ## Логи приложения
	$(COMPOSE) logs -f app

ps: ## Состояние контейнеров
	$(COMPOSE) ps

build: ## Только собрать образ
	$(COMPOSE) build app

rebuild: ## Пересобрать образ с нуля и перезапустить
	$(COMPOSE) build --no-cache app
	$(COMPOSE) up -d

clean: ## Остановить и удалить данные БД (демо-данные создадутся заново)
	$(COMPOSE) down -v

db: ## Только PostgreSQL — для локальной разработки (make dev)
	$(COMPOSE) up -d db

dev: db ## cargo run локально против PostgreSQL в контейнере
	DATABASE_URL=postgres://budget:budget@localhost:5432/family_budget cargo run -p family_budget

test: ## Тесты (БД не нужна)
	cargo test --workspace

css: ## Пересобрать Tailwind CSS после правки шаблонов
	npm install --silent && npm run css

help: ## Список команд
	@grep -E '^[a-z-]+:.*## ' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*## "} {printf "  make %-8s %s\n", $$1, $$2}'
