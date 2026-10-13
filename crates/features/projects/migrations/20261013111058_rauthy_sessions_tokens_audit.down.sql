-- Откат входа через Rauthy. Пароли не восстанавливаются: после отката пользователям нужно задать их заново.
DROP TABLE IF EXISTS fb_audit_log;
DROP TABLE IF EXISTS fb_api_tokens;
DROP TABLE IF EXISTS fb_sessions;
DROP INDEX IF EXISTS fb_projects_created_by_idx;
ALTER TABLE fb_projects DROP COLUMN IF EXISTS require_2fa;
DROP INDEX IF EXISTS fb_users_email_idx;
CREATE UNIQUE INDEX IF NOT EXISTS fb_users_email_uniq ON fb_users (lower(email));
ALTER TABLE fb_users DROP COLUMN IF EXISTS theme;
ALTER TABLE fb_users DROP COLUMN IF EXISTS avatar_url;
ALTER TABLE fb_users ADD COLUMN IF NOT EXISTS password_hash TEXT NOT NULL DEFAULT '';
DROP INDEX IF EXISTS fb_users_rauthy_sub_uniq;
ALTER TABLE fb_users DROP COLUMN IF EXISTS rauthy_sub;
