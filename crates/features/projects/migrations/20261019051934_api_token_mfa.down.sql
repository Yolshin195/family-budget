-- projects: api_token_mfa (down)
ALTER TABLE fb_api_tokens DROP COLUMN IF EXISTS mfa;
