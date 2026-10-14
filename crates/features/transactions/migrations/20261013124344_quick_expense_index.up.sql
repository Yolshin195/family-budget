-- Фича: transactions. Быстрый расход (ТЗ, раздел 10): последний счёт пользователя и его частые
-- категории за 30 дней считаются из транзакций — индекс под фильтр «проект + автор + время».
CREATE INDEX fb_transactions_quick_idx ON fb_transactions (project_id, created_by, created_at DESC);
