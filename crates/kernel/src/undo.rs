//! «Отменить» в тосте после удаления (ТЗ, раздел 9): удалённая запись живёт в памяти пару минут
//! и восстанавливается тем же id. Без модальных «Вы уверены?».
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use uuid::Uuid;

/// Сколько хранится удалённое (тост висит 5 секунд, запас — на медленную сеть).
pub const UNDO_TTL: Duration = Duration::from_secs(120);

pub struct UndoBuffer<T> {
    items: Mutex<HashMap<Uuid, (Uuid, Instant, T)>>,
}

impl<T> Default for UndoBuffer<T> {
    fn default() -> Self {
        Self { items: Mutex::new(HashMap::new()) }
    }
}

impl<T> UndoBuffer<T> {
    /// Запомнить удалённое; возвращает токен для «Отменить».
    pub fn put(&self, project_id: Uuid, item: T) -> Uuid {
        let token = Uuid::new_v4();
        if let Ok(mut m) = self.items.lock() {
            let now = Instant::now();
            m.retain(|_, (_, at, _)| now.duration_since(*at) < UNDO_TTL);
            m.insert(token, (project_id, now, item));
        }
        token
    }

    /// Забрать удалённое по токену — только в своём проекте и пока не истёк срок.
    pub fn take(&self, project_id: Uuid, token: Uuid) -> Option<T> {
        let mut m = self.items.lock().ok()?;
        match m.get(&token) {
            Some((p, at, _)) if *p == project_id && at.elapsed() < UNDO_TTL => m.remove(&token).map(|x| x.2),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_own_project() {
        let b = UndoBuffer::default();
        let (p, other) = (Uuid::new_v4(), Uuid::new_v4());
        let t = b.put(p, "x");
        assert_eq!(b.take(other, t), None);
        assert_eq!(b.take(p, t), Some("x"));
        assert_eq!(b.take(p, t), None, "второй раз не восстановить");
    }
}
