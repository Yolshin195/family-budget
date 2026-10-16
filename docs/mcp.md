# MCP: бюджет через Claude

Сервер MCP (Model Context Protocol) — `https://<домен>/mcp`. Через него Claude (claude.ai, приложение на
телефоне, Claude Desktop, Claude Code) работает с бюджетом **под вашим аккаунтом**: в одном проекте и с теми
правами, которые вы выбрали. Удалять через ассистента ничего нельзя; права меняются только в браузере.

Фича — `crates/features/mcp`, план и решения — [mcp-plan.md](mcp-plan.md).

## Инструменты

| Инструмент | Что делает | Право |
| --- | --- | --- |
| `get_context` | Проект, базовая валюта, счета с остатками, дерево категорий (ru/en/th), частые категории, права | всегда |
| `list_transactions` | Операции за период; фильтры: счёт, вид, категория с подкатегориями, текст | просмотр |
| `get_balances` | Остатки счетов на дату | просмотр |
| `get_budget` | Бюджет недели / месяца / года: лимит, факт, план, прогноз, доступно | просмотр |
| `list_upcoming_payments` | Плановые платежи и их статусы | просмотр |
| `add_expense`, `add_income` | Расход / доход: сумма + категория или позиции (чек) | операции |
| `add_transfer` | Перевод и обмен валют между счетами, комиссия | операции |
| `update_transaction` | Исправить операцию (Claude спрашивает подтверждение) | операции |
| `pay_planned` | Оплатить плановый платёж | операции |
| `skip_planned` | Пропустить плановый платёж | плановые платежи |

`tools/list` показывает только разрешённые инструменты, и каждый вызов ещё раз проверяет права и роль в
проекте (VIEWER — только просмотр). Каждое изменение через ассистента пишется в журнал безопасности
(«Профиль → Безопасность»).

## Вход

| Клиент | Как входит | Проект и права |
| --- | --- | --- |
| Claude Code, MCP Inspector | Личный API-токен `fbt_…` в заголовке `Authorization` | у токена |
| claude.ai, приложение Claude на телефоне и компьютере | OAuth через провайдера входа (сейчас Rauthy): access-токен, выпущенный для ресурса `https://<домен>/mcp` | страница «Ассистенты» (`/assistants`) |

Токен провайдера сам по себе прав не даёт: пока на странице «Ассистенты» не открыт доступ, Claude видит
только подсказку открыть её. Отключение там действует сразу.

### Claude Code

1. «Профиль → Безопасность → API-токены»: проект и права (например, «операции»), срок.
2. `claude mcp add --transport http family-budget https://<домен>/mcp --header "Authorization: Bearer fbt_…"`

### claude.ai (и приложение на телефоне)

**Один раз — клиент в Rauthy** (админка Rauthy → Clients → New client; на стенде `make` клиент `claude`
создаётся из `deploy/rauthy/bootstrap/clients.json`, но два поля ниже всё равно задать вручную):

| Поле | Значение |
| --- | --- |
| Client ID | `claude` (как `MCP_OAUTH_CLIENT_ID`) |
| Confidential | нет (публичный клиент, PKCE S256) |
| Redirect URIs | `https://claude.ai/api/mcp/auth_callback` |
| Flows | `authorization_code`, `refresh_token` |
| Scopes | `openid` (+ `profile`, `email`) |
| **Allowed resources** | `https://<домен>/mcp` |
| **Default audience** | `https://<домен>/mcp` — нужен для входа через GitHub/Facebook: для них Rauthy 0.37 не обрабатывает `resource` |
| Access token lifetime | 3600 |

**На сайте:** «Профиль → Безопасность → Ассистенты (MCP)» → выбрать проект и права → «Сохранить доступ».

**В claude.ai:** «Настроить» (Customize) → «Коннекторы» → «+» → «Добавить свой коннектор»:

- адрес: `https://<домен>/mcp` (ровно так, без слеша в конце);
- «Дополнительные настройки» → свой OAuth-клиент, Client ID `claude`, секрет пустой
  (CIMD и автоматическая регистрация у Rauthy с Claude не работают — см. mcp-plan.md);
- «Подключить» → вход в Rauthy → коннектор доступен и в приложении на телефоне.

Требования к серверу: HTTPS, доступ из интернета по IPv4 (Claude ходит из облака Anthropic).

## Проверка

```bash
# Без токена — 401 и адрес метаданных
curl -si -X POST https://<домен>/mcp | grep -i www-authenticate
# Метаданные ресурса (RFC 9728): издатель токенов — Rauthy
curl -s https://<домен>/.well-known/oauth-protected-resource/mcp
# С личным токеном — список инструментов
curl -s -X POST https://<домен>/mcp -H "Authorization: Bearer fbt_…" -H 'Content-Type: application/json' \
  -H 'Accept: application/json, text/event-stream' -H 'MCP-Protocol-Version: 2025-06-18' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'
```

Или MCP Inspector: `npx @modelcontextprotocol/inspector` → Streamable HTTP, `http://localhost:8080/mcp`,
заголовок `Authorization: Bearer fbt_…` (в dev разрешены `localhost` и `127.0.0.1`).

## Переезд на другой провайдер входа

MCP-часть от провайдера не зависит: она получает «пользователь + OAuth-клиент» через порт `Accounts`.
Если новый провайдер выдаёт JWT access-токены с `aud` = `https://<домен>/mcp` (RFC 8707 / 9068), достаточно
сменить `OIDC_ISSUER` и завести у него клиент `claude` — проверка (`verify_access_jwt` в
`crates/features/projects/src/adapters/oidc.rs`) та же. Иначе (непрозрачные токены) — заменить только этот
метод адаптера на интроспекцию (RFC 7662). Когда провайдер сам покажет экран согласия с выбором проекта,
страница «Ассистенты» станет делегированным согласием, а таблица `fb_mcp_grants` останется источником прав.
