//! Лимит запросов на сессию и API-токен: фиксированное окно в минуту, в памяти процесса.
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const WINDOW: Duration = Duration::from_secs(60);

#[derive(Default)]
pub struct RateLimiter {
    windows: Mutex<HashMap<String, (Instant, u32)>>,
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
        let Ok(mut map) = self.windows.lock() else { return true };
        if map.len() > 50_000 {
            map.retain(|_, (start, _)| now.duration_since(*start) < WINDOW);
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
}
