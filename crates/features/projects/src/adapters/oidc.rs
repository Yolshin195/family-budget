//! Исходящий адаптер: Rauthy как OpenID Connect провайдер (крейт `openidconnect`).
//!
//! Authorization code + PKCE S256; `state` и `nonce` — случайные, хранятся в подписанной cookie на
//! время входа. ID-токен проверяется по ключам Rauthy (JWKS): подпись, `iss`, `aud`, `nonce`, срок.
//! Метаданные провайдера (discovery + JWKS) кэшируются на час и перечитываются при смене ключей.
use std::time::{Duration, Instant};

use async_trait::async_trait;
use kernel::config::Oidc;
use kernel::error::{AppError, AppResult};
use kernel::i18n::Lang;
use openidconnect::core::{
    CoreAuthenticationFlow, CoreClient, CoreIdToken, CoreIdTokenVerifier, CoreJsonWebKeySet, CoreJsonWebKeyUse,
    CoreJwsSigningAlgorithm, CoreProviderMetadata,
};
use openidconnect::{
    AsyncHttpClient, AuthorizationCode, ClientId, ClientSecret, CsrfToken, HttpRequest, IssuerUrl, JsonWebKey,
    JsonWebKeyAlgorithm, Nonce, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope, TokenResponse, reqwest,
};
use serde::Deserialize;
use tokio::sync::RwLock;

use crate::domain::{AccessToken, VerifiedLogin};
use crate::ports::{AuthRequest, IdentityProvider};

const REFRESH: Duration = Duration::from_secs(3600);

struct Discovered {
    at: Instant,
    metadata: CoreProviderMetadata,
    end_session: Option<String>,
}

pub struct RauthyProvider {
    cfg: Oidc,
    http: reqwest::Client,
    /// Клиент для openidconnect: тот же reqwest, но с подменой публичного адреса на внутренний.
    oidc_http: RewritingHttp,
    cache: RwLock<Option<Discovered>>,
}

/// HTTP-клиент openidconnect: запросы на публичный адрес Rauthy уходят на внутренний (`OIDC_INTERNAL_URL`).
#[derive(Clone)]
struct RewritingHttp {
    inner: reqwest::Client,
    public: String,
    internal: Option<String>,
}

impl RewritingHttp {
    fn map(&self, url: &str) -> String {
        match (&self.internal, url.strip_prefix(&self.public)) {
            (Some(internal), Some(rest)) => format!("{internal}{rest}"),
            _ => url.to_string(),
        }
    }
}

impl<'c> AsyncHttpClient<'c> for RewritingHttp {
    type Error = <reqwest::Client as AsyncHttpClient<'c>>::Error;
    type Future = <reqwest::Client as AsyncHttpClient<'c>>::Future;

    fn call(&'c self, mut request: HttpRequest) -> Self::Future {
        if self.internal.is_some()
            && let Ok(uri) = self.map(&request.uri().to_string()).parse()
        {
            *request.uri_mut() = uri;
        }
        self.inner.call(request)
    }
}

/// Rauthy не ответил или ответил не так: временный сбой (503), подробности — в журнал.
fn unavailable(e: impl std::fmt::Display) -> AppError {
    AppError::Unavailable(anyhow::anyhow!("Rauthy недоступен: {e}"))
}

/// Неверный адрес в настройках OIDC: ошибка конфигурации (500), а не недоступность Rauthy.
fn misconfigured(e: impl std::fmt::Display) -> AppError {
    AppError::Internal(anyhow::anyhow!("OIDC: неверный адрес в настройках: {e}"))
}

impl RauthyProvider {
    /// Адрес для запроса сервера: публичный origin Rauthy → внутренний (`OIDC_INTERNAL_URL`).
    fn internal(&self, url: &str) -> String {
        self.oidc_http.map(url)
    }

    pub fn new(cfg: Oidc) -> Self {
        let http = reqwest::ClientBuilder::new()
            // Ответы OIDC-эндпоинтов не должны уводить запросы на другой адрес (SSRF).
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();
        let oidc_http = RewritingHttp { inner: http.clone(), public: cfg.origin(), internal: cfg.internal_url.clone() };
        Self { cfg, http, oidc_http, cache: RwLock::new(None) }
    }

    /// Discovery + JWKS из кэша; `force` — перечитать (например, Rauthy сменил ключи подписи).
    async fn discovered(&self, force: bool) -> AppResult<(CoreProviderMetadata, Option<String>)> {
        if !force && let Some(d) = self.cache.read().await.as_ref().filter(|d| d.at.elapsed() < REFRESH) {
            return Ok((d.metadata.clone(), d.end_session.clone()));
        }
        let issuer = IssuerUrl::new(self.cfg.issuer.clone()).map_err(misconfigured)?;
        let metadata = CoreProviderMetadata::discover_async(issuer, &self.oidc_http).await.map_err(unavailable)?;
        // end_session_endpoint нет в стандартных метаданных крейта — читаем из того же документа.
        let end_session = self
            .http
            .get(self.internal(&format!("{}/.well-known/openid-configuration", self.cfg.base())))
            .send()
            .await
            .ok()
            .and_then(|r| r.error_for_status().ok());
        let end_session = match end_session {
            Some(r) => r
                .text()
                .await
                .ok()
                .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
                .and_then(|v| v.get("end_session_endpoint").and_then(|e| e.as_str()).map(str::to_string)),
            None => None,
        };
        *self.cache.write().await =
            Some(Discovered { at: Instant::now(), metadata: metadata.clone(), end_session: end_session.clone() });
        Ok((metadata, end_session))
    }

    /// Проверка ID-токена ключами Rauthy (JWKS) и алгоритмами из его метаданных.
    fn verifier(&self, metadata: CoreProviderMetadata) -> AppResult<CoreIdTokenVerifier<'static>> {
        let algs = metadata.id_token_signing_alg_values_supported().clone();
        Ok(CoreIdTokenVerifier::new_confidential_client(
            ClientId::new(self.cfg.client_id.clone()),
            ClientSecret::new(self.cfg.client_secret.clone()),
            metadata.issuer().clone(),
            metadata.jwks().clone(),
        )
        .set_allowed_algs(algs))
    }
}

/// Запас на расхождение часов при проверке сроков токена, секунд.
const LEEWAY: i64 = 60;

#[derive(Deserialize)]
struct JwsHeader {
    alg: CoreJwsSigningAlgorithm,
    kid: Option<String>,
    typ: Option<String>,
}

#[derive(Deserialize)]
struct AccessClaims {
    iss: String,
    sub: Option<String>,
    aud: serde_json::Value,
    exp: i64,
    nbf: Option<i64>,
    /// Rauthy: `Bearer` у access-токена, `Id` — у ID-токена.
    typ: Option<String>,
    azp: Option<String>,
    /// RFC 9068: клиент — в `client_id`.
    client_id: Option<String>,
    /// Токен привязан к ключу клиента (DPoP) — как обычный Bearer его принимать нельзя.
    cnf: Option<serde_json::Value>,
}

/// Проверить JWT access-токен: подпись ключом из JWKS (по `kid`, только асимметричные алгоритмы),
/// `iss`, `exp`/`nbf` (запас 60 с), `aud` содержит ресурс, это именно access-токен (не ID и не
/// refresh), без привязки DPoP. Отдельно от провайдера — чтобы покрыть тестами.
pub fn verify_access_jwt(
    token: &str,
    jwks: &CoreJsonWebKeySet,
    issuer: &str,
    audience: &str,
    now: i64,
) -> Result<AccessToken, String> {
    use base64::Engine;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let b64 = |part: &str| URL_SAFE_NO_PAD.decode(part).map_err(|_| "токен: неверный base64".to_string());
    let parts: Vec<&str> = token.split('.').collect();
    let [header_b64, claims_b64, signature_b64] = parts[..] else { return Err("токен: не JWS".into()) };
    let header: JwsHeader = serde_json::from_slice(&b64(header_b64)?).map_err(|e| format!("токен: заголовок: {e}"))?;
    if matches!(
        header.alg,
        CoreJwsSigningAlgorithm::HmacSha256
            | CoreJwsSigningAlgorithm::HmacSha384
            | CoreJwsSigningAlgorithm::HmacSha512
            | CoreJwsSigningAlgorithm::None
    ) {
        return Err("токен: алгоритм подписи не разрешён".into());
    }
    if header.typ.as_deref().is_some_and(|t| !t.eq_ignore_ascii_case("jwt") && !t.eq_ignore_ascii_case("at+jwt")) {
        return Err("токен: не access-токен (typ)".into());
    }
    let signature = b64(signature_b64)?;
    let message = format!("{header_b64}.{claims_b64}");
    let signed = jwks
        .keys()
        .iter()
        .filter(|k| match (&header.kid, k.key_id()) {
            (Some(kid), Some(id)) => id.as_str() == kid,
            (Some(_), None) => false,
            (None, _) => true,
        })
        .filter(|k| k.key_use().is_none_or(|u| *u == CoreJsonWebKeyUse::Signature))
        .filter(|k| match k.signing_alg() {
            JsonWebKeyAlgorithm::Algorithm(a) => *a == header.alg,
            JsonWebKeyAlgorithm::Unspecified => true,
            _ => false,
        })
        .any(|k| k.verify_signature(&header.alg, message.as_bytes(), &signature).is_ok());
    if !signed {
        return Err("токен: подпись не подходит ни к одному ключу провайдера".into());
    }
    let c: AccessClaims = serde_json::from_slice(&b64(claims_b64)?).map_err(|e| format!("токен: claims: {e}"))?;
    if c.iss != issuer {
        return Err("токен: чужой издатель".into());
    }
    if c.exp + LEEWAY < now || c.nbf.is_some_and(|nbf| nbf - LEEWAY > now) {
        return Err("токен: истёк или ещё не действует".into());
    }
    let audience_ok = match &c.aud {
        serde_json::Value::String(a) => a == audience,
        serde_json::Value::Array(list) => list.iter().any(|a| a.as_str() == Some(audience)),
        _ => false,
    };
    if !audience_ok {
        return Err("токен: выпущен не для этого ресурса (aud)".into());
    }
    if c.typ.as_deref().is_some_and(|t| t != "Bearer") {
        return Err("токен: не access-токен".into());
    }
    if c.cnf.is_some() {
        return Err("токен: привязан к ключу клиента (DPoP)".into());
    }
    let sub = c.sub.filter(|s| !s.is_empty()).ok_or_else(|| "токен: нет sub".to_string())?;
    let client_id = c.azp.or(c.client_id).unwrap_or_default();
    Ok(AccessToken { sub, client_id })
}

/// Проверить ID-токен и достать из него пользователя. Отдельно — чтобы покрыть тестами.
pub fn verify_id_token(
    token: &CoreIdToken,
    verifier: &CoreIdTokenVerifier<'_>,
    nonce: &str,
) -> Result<VerifiedLogin, String> {
    let claims = token.claims(verifier, &Nonce::new(nonce.to_string())).map_err(|e| e.to_string())?;
    let email = claims
        .email()
        .map(|e| e.as_str().to_string())
        .ok_or_else(|| "в ID-токене нет email (scope email)".to_string())?;
    Ok(VerifiedLogin {
        sub: claims.subject().as_str().to_string(),
        email,
        // Rauthy не выдаёт `name` — собираем из given/family name, иначе preferred_username.
        name: claims
            .name()
            .and_then(|n| n.get(None))
            .map(|n| n.as_str().to_string())
            .or_else(|| {
                let given = claims.given_name().and_then(|n| n.get(None)).map(|n| n.as_str().to_string());
                let family = claims.family_name().and_then(|n| n.get(None)).map(|n| n.as_str().to_string());
                let full = [given, family].into_iter().flatten().collect::<Vec<_>>().join(" ");
                (!full.trim().is_empty()).then_some(full)
            })
            .or_else(|| claims.preferred_username().map(|u| u.as_str().to_string())),
        picture: claims.picture().and_then(|p| p.get(None)).map(|p| p.as_str().to_string()),
        amr: claims.auth_method_refs().map(|v| v.iter().map(|a| a.as_str().to_string()).collect()).unwrap_or_default(),
    })
}

#[async_trait]
impl IdentityProvider for RauthyProvider {
    fn enabled(&self) -> bool {
        true
    }

    async fn authorize(&self, lang: Lang) -> AppResult<AuthRequest> {
        let (metadata, _) = self.discovered(false).await?;
        let client = CoreClient::from_provider_metadata(
            metadata,
            ClientId::new(self.cfg.client_id.clone()),
            Some(ClientSecret::new(self.cfg.client_secret.clone())),
        )
        .set_redirect_uri(RedirectUrl::new(self.cfg.redirect_url.clone()).map_err(misconfigured)?);
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
        let (url, state, nonce) = client
            .authorize_url(CoreAuthenticationFlow::AuthorizationCode, CsrfToken::new_random, Nonce::new_random)
            .add_scope(Scope::new("email".into()))
            .add_scope(Scope::new("profile".into()))
            .set_pkce_challenge(challenge)
            .add_extra_param("ui_locales", lang.code())
            .url();
        Ok(AuthRequest {
            url: url.to_string(),
            state: state.secret().clone(),
            nonce: nonce.secret().clone(),
            pkce_verifier: verifier.secret().clone(),
        })
    }

    async fn exchange(&self, code: &str, pkce_verifier: &str, nonce: &str) -> AppResult<VerifiedLogin> {
        let (metadata, _) = self.discovered(false).await?;
        let client = CoreClient::from_provider_metadata(
            metadata.clone(),
            ClientId::new(self.cfg.client_id.clone()),
            Some(ClientSecret::new(self.cfg.client_secret.clone())),
        )
        .set_redirect_uri(RedirectUrl::new(self.cfg.redirect_url.clone()).map_err(misconfigured)?);
        let response = client
            .exchange_code(AuthorizationCode::new(code.to_string()))
            .map_err(unavailable)?
            .set_pkce_verifier(PkceCodeVerifier::new(pkce_verifier.to_string()))
            .request_async(&self.oidc_http)
            .await
            .map_err(|e| AppError::Forbidden(format!("Rauthy не принял код входа: {e}")))?;
        let id_token = response.id_token().ok_or_else(|| AppError::Forbidden("Rauthy не вернул ID-токен".into()))?;
        match verify_id_token(id_token, &self.verifier(metadata)?, nonce) {
            Ok(login) => Ok(login),
            Err(first) => {
                // Возможно, Rauthy сменил ключ подписи: перечитать JWKS и проверить ещё раз.
                let (fresh, _) = self.discovered(true).await?;
                verify_id_token(id_token, &self.verifier(fresh)?, nonce)
                    .map_err(|e| AppError::Forbidden(format!("ID-токен не прошёл проверку: {e} ({first})")))
            }
        }
    }

    async fn logout_url(&self, post_logout: &str) -> Option<String> {
        let (_, end_session) = self.discovered(false).await.ok()?;
        let end = end_session?;
        let sep = if end.contains('?') { '&' } else { '?' };
        Some(format!(
            "{end}{sep}client_id={}&post_logout_redirect_uri={}",
            kernel::web::url_encode(&self.cfg.client_id),
            kernel::web::url_encode(post_logout)
        ))
    }

    async fn delete_user(&self, sub: &str) -> AppResult<bool> {
        let Some(key) = &self.cfg.admin_api_key else { return Ok(false) };
        let url = self.internal(&format!("{}/users/{}", self.cfg.base(), kernel::web::url_encode(sub)));
        let resp = self
            .http
            .delete(url)
            .header("Authorization", format!("API-Key {key}"))
            .send()
            .await
            .map_err(unavailable)?;
        if resp.status().is_success() || resp.status() == reqwest::StatusCode::NOT_FOUND {
            Ok(true)
        } else {
            Err(unavailable(format!("удаление пользователя: HTTP {}", resp.status())))
        }
    }

    async fn issuer(&self) -> Option<String> {
        self.discovered(false).await.ok().map(|(m, _)| m.issuer().as_str().to_string())
    }

    async fn verify_access_token(&self, token: &str, audience: &str) -> AppResult<Option<AccessToken>> {
        let (metadata, _) = self.discovered(false).await?;
        let now = chrono::Utc::now().timestamp();
        match verify_access_jwt(token, metadata.jwks(), metadata.issuer().as_str(), audience, now) {
            Ok(t) => Ok(Some(t)),
            Err(_) if self.stale(STALE_FOR_RETRY).await => {
                // Возможно, Rauthy сменил ключ подписи. Перечитываем не чаще раза в минуту —
                // поток мусорных токенов не должен превращаться в поток запросов к Rauthy.
                let (fresh, _) = self.discovered(true).await?;
                Ok(verify_access_jwt(token, fresh.jwks(), fresh.issuer().as_str(), audience, now).ok())
            }
            Err(_) => Ok(None),
        }
    }
}

/// Перечитать JWKS ради неизвестного токена можно, если кэшу больше минуты.
const STALE_FOR_RETRY: Duration = Duration::from_secs(60);

impl RauthyProvider {
    async fn stale(&self, age: Duration) -> bool {
        self.cache.read().await.as_ref().is_none_or(|d| d.at.elapsed() >= age)
    }
}

/// Rauthy не настроен (разработка без OIDC): вход только через `DEV_LOGIN`.
pub struct DisabledProvider;

#[async_trait]
impl IdentityProvider for DisabledProvider {
    fn enabled(&self) -> bool {
        false
    }
    async fn authorize(&self, _lang: Lang) -> AppResult<AuthRequest> {
        Err(AppError::NotFound("вход через Rauthy не настроен (OIDC_ISSUER)".into()))
    }
    async fn exchange(&self, _code: &str, _pkce: &str, _nonce: &str) -> AppResult<VerifiedLogin> {
        Err(AppError::NotFound("вход через Rauthy не настроен (OIDC_ISSUER)".into()))
    }
    async fn logout_url(&self, _post_logout: &str) -> Option<String> {
        None
    }
    async fn delete_user(&self, _sub: &str) -> AppResult<bool> {
        Ok(false)
    }
    async fn issuer(&self) -> Option<String> {
        None
    }
    async fn verify_access_token(&self, _token: &str, _audience: &str) -> AppResult<Option<AccessToken>> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    //! Проверка ID-токена (раздел 13 ТЗ): неверный nonce, чужой aud, чужой iss, истёкший срок,
    //! чужой ключ подписи → отказ. Ключи в testdata — только для тестов.
    use chrono::{Duration as ChronoDuration, Utc};
    use openidconnect::core::{
        CoreIdTokenClaims, CoreJsonWebKeySet, CoreJwsSigningAlgorithm, CoreRsaPrivateSigningKey,
    };
    use openidconnect::{
        Audience, AuthenticationMethodReference, EmptyAdditionalClaims, EndUserEmail, JsonWebKeyId, PrivateSigningKey,
        StandardClaims, SubjectIdentifier,
    };

    use super::*;

    const KEY: &str = include_str!("testdata/test_rsa.pem");
    const OTHER_KEY: &str = include_str!("testdata/other_rsa.pem");
    const ISSUER: &str = "https://auth.example.test/auth/v1";
    const CLIENT: &str = "family-budget";

    fn key(pem: &str) -> CoreRsaPrivateSigningKey {
        CoreRsaPrivateSigningKey::from_pem(pem, Some(JsonWebKeyId::new("k1".into()))).unwrap()
    }

    fn verifier() -> CoreIdTokenVerifier<'static> {
        CoreIdTokenVerifier::new_confidential_client(
            ClientId::new(CLIENT.into()),
            ClientSecret::new("secret".into()),
            IssuerUrl::new(ISSUER.into()).unwrap(),
            CoreJsonWebKeySet::new(vec![key(KEY).as_verification_key()]),
        )
    }

    fn token(iss: &str, aud: &str, nonce: &str, expires_in: ChronoDuration, signing: &str) -> CoreIdToken {
        let now = Utc::now();
        let claims = CoreIdTokenClaims::new(
            IssuerUrl::new(iss.into()).unwrap(),
            vec![Audience::new(aud.into())],
            now + expires_in,
            now - ChronoDuration::minutes(1),
            StandardClaims::new(SubjectIdentifier::new("rauthy-user-1".into()))
                .set_email(Some(EndUserEmail::new("anna@example.test".into()))),
            EmptyAdditionalClaims {},
        )
        .set_nonce(Some(Nonce::new(nonce.into())))
        .set_auth_method_refs(Some(vec![
            AuthenticationMethodReference::new("pwd".into()),
            AuthenticationMethodReference::new("otp".into()),
        ]));
        CoreIdToken::new(claims, &key(signing), CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha256, None, None).unwrap()
    }

    #[test]
    fn valid_token_gives_user_and_second_factor() {
        let t = token(ISSUER, CLIENT, "n-1", ChronoDuration::minutes(5), KEY);
        let login = verify_id_token(&t, &verifier(), "n-1").unwrap();
        assert_eq!((login.sub.as_str(), login.email.as_str()), ("rauthy-user-1", "anna@example.test"));
        assert!(login.mfa(), "amr содержит otp — код из письма");
    }

    #[test]
    fn wrong_nonce_is_rejected() {
        let t = token(ISSUER, CLIENT, "n-1", ChronoDuration::minutes(5), KEY);
        assert!(verify_id_token(&t, &verifier(), "n-2").is_err());
    }

    #[test]
    fn foreign_audience_is_rejected() {
        let t = token(ISSUER, "other-app", "n-1", ChronoDuration::minutes(5), KEY);
        assert!(verify_id_token(&t, &verifier(), "n-1").is_err());
    }

    #[test]
    fn foreign_issuer_is_rejected() {
        let t = token("https://evil.example.test/auth/v1", CLIENT, "n-1", ChronoDuration::minutes(5), KEY);
        assert!(verify_id_token(&t, &verifier(), "n-1").is_err());
    }

    #[test]
    fn expired_token_is_rejected() {
        let t = token(ISSUER, CLIENT, "n-1", -ChronoDuration::minutes(5), KEY);
        assert!(verify_id_token(&t, &verifier(), "n-1").is_err());
    }

    #[test]
    fn token_signed_by_another_key_is_rejected() {
        let t = token(ISSUER, CLIENT, "n-1", ChronoDuration::minutes(5), OTHER_KEY);
        assert!(verify_id_token(&t, &verifier(), "n-1").is_err());
    }

    // ------------------------------------------------ access-токены (MCP, RFC 8707)

    const RESOURCE: &str = "https://budget.example.test/mcp";

    fn jwks() -> CoreJsonWebKeySet {
        CoreJsonWebKeySet::new(vec![key(KEY).as_verification_key()])
    }

    fn jws(header: serde_json::Value, claims: serde_json::Value, signing: &str) -> String {
        use base64::Engine;
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let msg =
            format!("{}.{}", URL_SAFE_NO_PAD.encode(header.to_string()), URL_SAFE_NO_PAD.encode(claims.to_string()));
        let sig = key(signing).sign(&CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha256, msg.as_bytes()).unwrap();
        format!("{msg}.{}", URL_SAFE_NO_PAD.encode(sig))
    }

    fn header() -> serde_json::Value {
        serde_json::json!({"alg": "RS256", "kid": "k1", "typ": "JWT"})
    }

    fn claims(now: i64) -> serde_json::Value {
        serde_json::json!({"iss": ISSUER, "sub": "rauthy-user-1", "aud": ["claude", RESOURCE], "exp": now + 300, "nbf": now - 10, "iat": now - 10, "typ": "Bearer", "azp": "claude"})
    }

    fn with(now: i64, key: &str, value: serde_json::Value) -> serde_json::Value {
        let mut c = claims(now);
        c[key] = value;
        c
    }

    #[test]
    fn access_token_for_this_resource_is_accepted() {
        let now = Utc::now().timestamp();
        let t = verify_access_jwt(&jws(header(), claims(now), KEY), &jwks(), ISSUER, RESOURCE, now).unwrap();
        assert_eq!(t, AccessToken { sub: "rauthy-user-1".into(), client_id: "claude".into() });
        let single = with(now, "aud", serde_json::json!(RESOURCE));
        assert!(verify_access_jwt(&jws(header(), single, KEY), &jwks(), ISSUER, RESOURCE, now).is_ok(), "aud строкой");
    }

    #[test]
    fn access_token_for_another_resource_is_rejected() {
        let now = Utc::now().timestamp();
        let c = with(now, "aud", serde_json::json!(["claude", "https://other.example.test/mcp"]));
        assert!(verify_access_jwt(&jws(header(), c, KEY), &jwks(), ISSUER, RESOURCE, now).is_err());
    }

    #[test]
    fn access_token_checks_issuer_expiry_and_signature() {
        let now = Utc::now().timestamp();
        let check = |c: serde_json::Value, signing: &str| {
            verify_access_jwt(&jws(header(), c, signing), &jwks(), ISSUER, RESOURCE, now)
        };
        assert!(
            check(with(now, "iss", serde_json::json!("https://evil.example.test/auth/v1")), KEY).is_err(),
            "чужой издатель"
        );
        assert!(check(with(now, "exp", serde_json::json!(now - 120)), KEY).is_err(), "истёк");
        assert!(check(with(now, "nbf", serde_json::json!(now + 600)), KEY).is_err(), "ещё не действует");
        assert!(check(claims(now), OTHER_KEY).is_err(), "чужой ключ");
    }

    #[test]
    fn id_refresh_and_dpop_tokens_are_not_access_tokens() {
        let now = Utc::now().timestamp();
        let check = |c: serde_json::Value| verify_access_jwt(&jws(header(), c, KEY), &jwks(), ISSUER, RESOURCE, now);
        assert!(check(with(now, "typ", serde_json::json!("Id"))).is_err(), "ID-токен");
        assert!(check(with(now, "typ", serde_json::json!("Refresh"))).is_err(), "refresh-токен");
        assert!(check(with(now, "cnf", serde_json::json!({"jkt": "x"}))).is_err(), "DPoP");
    }

    #[test]
    fn symmetric_and_none_algorithms_are_rejected() {
        use base64::Engine;
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let now = Utc::now().timestamp();
        for alg in ["HS256", "none"] {
            let h = serde_json::json!({"alg": alg, "kid": "k1"});
            let unsigned = format!(
                "{}.{}.",
                URL_SAFE_NO_PAD.encode(h.to_string()),
                URL_SAFE_NO_PAD.encode(claims(now).to_string())
            );
            assert!(verify_access_jwt(&unsigned, &jwks(), ISSUER, RESOURCE, now).is_err(), "{alg}");
        }
        let unknown_kid = serde_json::json!({"alg": "RS256", "kid": "other"});
        assert!(verify_access_jwt(&jws(unknown_kid, claims(now), KEY), &jwks(), ISSUER, RESOURCE, now).is_err());
    }
}

#[cfg(test)]
mod rewrite_tests {
    use super::*;

    #[test]
    fn server_requests_go_to_internal_address() {
        let cfg = Oidc {
            issuer: "http://localhost:8443/auth/v1/".into(),
            client_id: "family-budget".into(),
            client_secret: "s".into(),
            redirect_url: "http://localhost:8080/auth/callback".into(),
            register_url: String::new(),
            account_url: String::new(),
            admin_api_key: None,
            internal_url: Some("http://rauthy:8443".into()),
        };
        let p = RauthyProvider::new(cfg);
        assert_eq!(p.internal("http://localhost:8443/auth/v1/oidc/token"), "http://rauthy:8443/auth/v1/oidc/token");
        assert_eq!(p.internal("https://other.example/x"), "https://other.example/x", "чужие адреса не трогаем");
    }

    /// Rauthy остановлен: вход отвечает 503 (временно недоступен), а не 500.
    #[tokio::test]
    async fn stopped_rauthy_is_unavailable() {
        // Свободный порт: заняли и сразу отпустили — соединение будет отклонено.
        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let cfg = Oidc {
            issuer: format!("http://127.0.0.1:{port}/auth/v1"),
            client_id: "family-budget".into(),
            client_secret: "s".into(),
            redirect_url: "http://localhost:8080/auth/callback".into(),
            register_url: String::new(),
            account_url: String::new(),
            admin_api_key: None,
            internal_url: None,
        };
        let err = RauthyProvider::new(cfg).authorize(Lang::Ru).await.unwrap_err();
        assert!(matches!(err, AppError::Unavailable(_)), "{err:?}");
        assert_eq!(err.status(), axum::http::StatusCode::SERVICE_UNAVAILABLE);
    }
}
