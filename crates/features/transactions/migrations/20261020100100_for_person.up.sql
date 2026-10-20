-- Группа 1, шаг 6: для кого позиция чека (справочник people); пусто — «Для всех». Получатель шапки не хранится —
-- он выводится из позиций (docs/group-1.md, 5.6). FK на чужую таблицу нет: ссылку проверяет сервис через порт.
ALTER TABLE fb_transaction_items ADD COLUMN for_person_code TEXT NULL;
CREATE INDEX fb_transaction_items_for ON fb_transaction_items (for_person_code, transaction_id) WHERE for_person_code IS NOT NULL;
