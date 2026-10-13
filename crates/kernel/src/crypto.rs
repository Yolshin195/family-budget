//! Криптографические примитивы приложения: случайные токены, хеши, подписи (HMAC-SHA256).
//!
//! - id сессий и API-токены — 256 случайных бит; в БД хранится только SHA-256 ([`sha256_hex`]).
//! - CSRF-токен и подписанные cookie — HMAC от секрета приложения ([`Signer`]).
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, Mac};
use rand::RngCore;
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

/// `bytes` случайных байт (CSPRNG ОС) в base64url без паддинга.
pub fn random_token(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rand::rngs::OsRng.fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

pub fn sha256_hex(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

/// Сравнение за постоянное время (не выдаёт длину общего префикса по времени ответа).
pub fn ct_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Подпись данных секретом приложения.
#[derive(Clone)]
pub struct Signer {
    key: Vec<u8>,
}

impl std::fmt::Debug for Signer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Signer(***)")
    }
}

impl Signer {
    pub fn new(key: &[u8]) -> Self {
        Self { key: key.to_vec() }
    }

    fn mac(&self, purpose: &str, data: &[u8]) -> Vec<u8> {
        let mut m = HmacSha256::new_from_slice(&self.key).expect("HMAC принимает ключ любой длины");
        m.update(purpose.as_bytes());
        m.update(&[0]);
        m.update(data);
        m.finalize().into_bytes().to_vec()
    }

    /// HMAC в base64url — например, CSRF-токен сессии: `sign("csrf", id_hash)`.
    pub fn sign(&self, purpose: &str, data: &str) -> String {
        URL_SAFE_NO_PAD.encode(self.mac(purpose, data.as_bytes()))
    }

    pub fn verify(&self, purpose: &str, data: &str, signature: &str) -> bool {
        ct_eq(&self.sign(purpose, data), signature)
    }

    /// Упаковать значение в подписанную строку `base64url(data).base64url(hmac)` — для cookie.
    pub fn seal(&self, purpose: &str, data: &str) -> String {
        let body = URL_SAFE_NO_PAD.encode(data.as_bytes());
        let sig = URL_SAFE_NO_PAD.encode(self.mac(purpose, body.as_bytes()));
        format!("{body}.{sig}")
    }

    /// Распаковать подписанную строку; `None`, если подпись не сходится.
    pub fn open(&self, purpose: &str, sealed: &str) -> Option<String> {
        let (body, sig) = sealed.split_once('.')?;
        let expected = URL_SAFE_NO_PAD.encode(self.mac(purpose, body.as_bytes()));
        if !ct_eq(&expected, sig) {
            return None;
        }
        String::from_utf8(URL_SAFE_NO_PAD.decode(body).ok()?).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_random_and_256_bit() {
        let a = random_token(32);
        assert_ne!(a, random_token(32));
        assert_eq!(URL_SAFE_NO_PAD.decode(&a).unwrap().len(), 32);
        assert_eq!(sha256_hex("abc").len(), 64);
    }

    #[test]
    fn seal_and_open() {
        let s = Signer::new(b"0123456789abcdef0123456789abcdef");
        let sealed = s.seal("oidc", r#"{"state":"x"}"#);
        assert_eq!(s.open("oidc", &sealed).as_deref(), Some(r#"{"state":"x"}"#));
        assert_eq!(s.open("other", &sealed), None, "подпись привязана к назначению");
        let tampered = format!("{}x", sealed);
        assert_eq!(s.open("oidc", &tampered), None);
        assert!(s.verify("csrf", "h", &s.sign("csrf", "h")));
        assert!(!s.verify("csrf", "h2", &s.sign("csrf", "h")));
        assert!(ct_eq("abc", "abc") && !ct_eq("abc", "abd") && !ct_eq("abc", "ab"));
    }
}
