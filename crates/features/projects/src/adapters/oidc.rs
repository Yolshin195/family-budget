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
use openidconnect::core::{CoreAuthenticationFlow, CoreClient, CoreIdToken, CoreIdTokenVerifier, CoreProviderMetadata};
use openidconnect::{
    AsyncHttpClient, AuthorizationCode, ClientId, ClientSecret, CsrfToken, HttpRequest, IssuerUrl, Nonce, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope,
    TokenResponse, reqwest,
};
use tokio::sync::RwLock;

use crate::domain::VerifiedLogin;
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
        if self.internal.is_some() {
            if let Ok(uri) = self.map(&request.uri().to_string()).parse() {
                *request.uri_mut() = uri;
            }
        }
        self.inner.call(request)
    }
}

fn unavailable(e: impl std::fmt::Display) -> AppError {
    AppError::Internal(anyhow::anyhow!("Rauthy недоступен: {e}"))
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
        if !force {
            if let Some(d) = self.cache.read().await.as_ref().filter(|d| d.at.elapsed() < REFRESH) {
                return Ok((d.metadata.clone(), d.end_session.clone()));
            }
        }
        let issuer = IssuerUrl::new(self.cfg.issuer.clone()).map_err(unavailable)?;
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
        *self.cache.write().await = Some(Discovered { at: Instant::now(), metadata: metadata.clone(), end_session: end_session.clone() });
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

/// Проверить ID-токен и достать из него пользователя. Отдельно — чтобы покрыть тестами.
pub fn verify_id_token(token: &CoreIdToken, verifier: &CoreIdTokenVerifier<'_>, nonce: &str) -> Result<VerifiedLogin, String> {
    let claims = token.claims(verifier, &Nonce::new(nonce.to_string())).map_err(|e| e.to_string())?;
    let email = claims.email().map(|e| e.as_str().to_string()).ok_or_else(|| "в ID-токене нет email (scope email)".to_string())?;
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
        let client = CoreClient::from_provider_metadata(metadata, ClientId::new(self.cfg.client_id.clone()), Some(ClientSecret::new(self.cfg.client_secret.clone())))
            .set_redirect_uri(RedirectUrl::new(self.cfg.redirect_url.clone()).map_err(unavailable)?);
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
        let (url, state, nonce) = client
            .authorize_url(CoreAuthenticationFlow::AuthorizationCode, CsrfToken::new_random, Nonce::new_random)
            .add_scope(Scope::new("email".into()))
            .add_scope(Scope::new("profile".into()))
            .set_pkce_challenge(challenge)
            .add_extra_param("ui_locales", lang.code())
            .url();
        Ok(AuthRequest { url: url.to_string(), state: state.secret().clone(), nonce: nonce.secret().clone(), pkce_verifier: verifier.secret().clone() })
    }

    async fn exchange(&self, code: &str, pkce_verifier: &str, nonce: &str) -> AppResult<VerifiedLogin> {
        let (metadata, _) = self.discovered(false).await?;
        let client = CoreClient::from_provider_metadata(metadata.clone(), ClientId::new(self.cfg.client_id.clone()), Some(ClientSecret::new(self.cfg.client_secret.clone())))
            .set_redirect_uri(RedirectUrl::new(self.cfg.redirect_url.clone()).map_err(unavailable)?);
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
                verify_id_token(id_token, &self.verifier(fresh)?, nonce).map_err(|e| AppError::Forbidden(format!("ID-токен не прошёл проверку: {e} ({first})")))
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
        let resp = self.http.delete(url).header("Authorization", format!("API-Key {key}")).send().await.map_err(unavailable)?;
        if resp.status().is_success() || resp.status() == reqwest::StatusCode::NOT_FOUND {
            Ok(true)
        } else {
            Err(unavailable(format!("удаление пользователя: HTTP {}", resp.status())))
        }
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
}

#[cfg(test)]
mod tests {
    //! Проверка ID-токена (раздел 13 ТЗ): неверный nonce, чужой aud, чужой iss, истёкший срок,
    //! чужой ключ подписи → отказ. Ключи в testdata — только для тестов.
    use chrono::{Duration as ChronoDuration, Utc};
    use openidconnect::core::{CoreIdTokenClaims, CoreJsonWebKeySet, CoreJwsSigningAlgorithm, CoreRsaPrivateSigningKey};
    use openidconnect::{Audience, AuthenticationMethodReference, EmptyAdditionalClaims, EndUserEmail, JsonWebKeyId, PrivateSigningKey, StandardClaims, SubjectIdentifier};

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
            StandardClaims::new(SubjectIdentifier::new("rauthy-user-1".into())).set_email(Some(EndUserEmail::new("anna@example.test".into()))),
            EmptyAdditionalClaims {},
        )
        .set_nonce(Some(Nonce::new(nonce.into())))
        .set_auth_method_refs(Some(vec![AuthenticationMethodReference::new("pwd".into()), AuthenticationMethodReference::new("otp".into())]));
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
}
