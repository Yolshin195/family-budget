//! Лимит запросов на сессию, API-токен и IP: фиксированное окно в минуту, в памяти процесса.
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const WINDOW: Duration = Duration::from_secs(60);
/// Потолок числа ключей: больше — и новые ключи в этом окне не принимаются (память не растёт
/// от потока запросов с разных адресов).
const MAX_KEYS: usize = 50_000;

pub struct RateLimiter {
    windows: Mutex<HashMap<String, (Instant, u32)>>,
    max_keys: usize,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self { windows: Mutex::default(), max_keys: MAX_KEYS }
    }
}

impl RateLimiter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Учесть запрос по ключу; `false` — лимит `per_minute` в текущем окне исчерпан.
    pub fn allow(&self, key: &str, per_minute: u32) -> bool {
        self.allow_at(key, per_minute, Instant::now())
    }

    fn allow_at(&self, key: &str, per_minute: u32, now: Instant) -> bool {
        // Паника другого потока под замком не выключает лимит: данные окна остаются годными.
        let mut map = self.windows.lock().unwrap_or_else(|e| e.into_inner());
        if map.len() >= self.max_keys && !map.contains_key(key) {
            map.retain(|_, (start, _)| now.duration_since(*start) < WINDOW);
            if map.len() >= self.max_keys {
                return false;
            }
        }
        let entry = map.entry(key.to_string()).or_insert((now, 0));
        if now.duration_since(entry.0) >= WINDOW {
            *entry = (now, 0);
        }
        entry.1 += 1;
        entry.1 <= per_minute
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_resets() {
        let l = RateLimiter::new();
        let t = Instant::now();
        assert!(l.allow_at("s:1", 2, t) && l.allow_at("s:1", 2, t));
        assert!(!l.allow_at("s:1", 2, t), "третий запрос в окне");
        assert!(l.allow_at("s:2", 2, t), "ключи независимы");
        assert!(l.allow_at("s:1", 2, t + WINDOW), "новое окно");
    }

    /// Б4: карта ключей не растёт без предела — при заполнении новые ключи ждут следующего окна.
    #[test]
    fn keys_are_capped() {
        let l = RateLimiter { max_keys: 3, ..Default::default() };
        let t = Instant::now();
        for k in ["ip:1", "ip:2", "ip:3"] {
            assert!(l.allow_at(k, 10, t));
        }
        assert!(!l.allow_at("ip:4", 10, t), "карта полна");
        assert!(l.allow_at("ip:1", 10, t), "известный ключ работает");
        assert_eq!(l.windows.lock().unwrap().len(), 3);
        assert!(l.allow_at("ip:4", 10, t + WINDOW), "старые окна вычищены");
    }
}
