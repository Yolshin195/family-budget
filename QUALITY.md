# Качество кода

Журнал агента качества: каждый запуск — одно небольшое улучшение отдельным PR (ветка `quality/*`, метка `quality`).

## Метрики

| Дата (UTC) | clippy pedantic | Покрытие | unwrap/expect вне тестов | Тестов |
|---|---|---|---|---|
| 2026-10-27 | 844 | н/д | 152 | 323, все зелёные (было 320) |

Как считается:

- clippy pedantic — уникальные предупреждения (сообщение + место) `cargo clippy --workspace --all-targets -- -W clippy::pedantic`;
- покрытие — `cargo llvm-cov --workspace --summary-only`; «н/д» — в среде запуска не скачался компонент `llvm-tools`;
- unwrap/expect — `.unwrap()` и `.expect(` в `crates/**/*.rs` без `tests.rs`, `tests/` и кода после `#[cfg(test)]`;
- тесты — сумма «passed» по `cargo test --workspace`.

## Бэклог

1. Тесты критичной логики, ещё не закрытые напрямую:
   - `planning::domain::Plan::split` — проверяется только через сервис (`planning/src/tests.rs`), нет доменного теста
     на `end_on` раньше `last` и на ошибку «нет дат после»;
   - `planning::domain::ToBase` / `kernel::rates::rate_on` — курс с обратной стороны (база → валюта), если такой
     случай поддерживается.
2. unwrap/expect вне тестов: 151 из 152 — `lock().unwrap()` в памяти-адаптерах `crates/features/*/src/adapters/memory.rs`
   (используются в тестах и демо, отравление мьютекса — уже паника). Кандидат: общий помощник в `kernel::testing`
   вместо повторения `lock().unwrap()`. Единственный в рабочем коде — `kernel/src/crypto.rs:51`, обоснован
   (`HMAC принимает ключ любой длины`).
3. Касты `i64`↔`usize` без проверки (`cast_possible_truncation`, `cast_sign_loss`, 21+21) — в т. ч.
   `crates/features/mcp/src/service.rs` (стр. ~615, ~1670): лимиты/страницы из запросов MCP. Проверить, что
   отрицательные и огромные значения отсекаются до каста.
4. clippy pedantic, крупные группы: `missing_errors_doc` (244), `must_use_candidate` (170), `doc_markdown` (105),
   `match_same_arms` (26), `format_push_string` (19), `needless_pass_by_value` (18), `unused_self` (16).
   Большинство — шум для приложения; полезные — `match_same_arms`, `needless_pass_by_value`, `unused_self`.

## Сделано

- 2026-10-27 — тесты `Plan::dates`: быстрый пропуск ранних шагов сверяется с полным перебором (1200 сочетаний
  правила, интервала, даты начала с 31-м и 29.02, окна, `end_on`, `effective_from`), `effective_from` и
  `has_dates_before`, предел `MAX_OCCURRENCES`. [#60](https://github.com/Yolshin195/family-budget/pull/60).

## Отклонено

Пока нет.
