-- Время операции (docs/group-1.md, шаг 1, раздел 5.3): момент в UTC; пусто — время неизвестно. Лента внутри дня
-- идёт по нему, а у операций без времени — по времени внесения (created_at).
ALTER TABLE fb_transactions ADD COLUMN occurred_at TIMESTAMPTZ NULL;

-- Время старых операций — только того же дня, что и операция. День считается в поясе Asia/Bangkok: в нём работает
-- сервер (TZ в Dockerfile), и это пояс существующих пользователей (решение 5.3).
-- 1. Операции из SMS банка и подтверждённые им — время самого раннего SMS: оно точнее времени внесения.
UPDATE fb_transactions t
SET occurred_at = date_trunc('second', r.received_at)
FROM (SELECT project_id, transaction_id, MIN(received_at) AS received_at
      FROM fb_external_refs
      WHERE received_at IS NOT NULL
      GROUP BY project_id, transaction_id) r
WHERE r.project_id = t.project_id
  AND r.transaction_id = t.id
  AND (r.received_at AT TIME ZONE 'Asia/Bangkok')::date = t.occurred_on;

-- 2. Остальные (кроме начальных остатков) — время внесения, если операцию внесли в тот же день.
UPDATE fb_transactions
SET occurred_at = date_trunc('second', created_at)
WHERE occurred_at IS NULL
  AND tx_kind <> 'OPENING'
  AND (created_at AT TIME ZONE 'Asia/Bangkok')::date = occurred_on;
