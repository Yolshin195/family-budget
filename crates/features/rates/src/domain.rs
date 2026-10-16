//! Доменная модель «Курсы валют». Никаких sqlx/axum/serde — только правила предметной области.
//!
//! Направление ввода (issue #22): курс вводят в любую сторону — «1 USD = 36.5 THB» или «1 THB = 2.50 RUB».
//! Направление выбирает пользователь кнопкой ⇄, система его только запоминает: следующий курс валюты
//! открывается в направлении её последнего курса, без курсов — прямое «1 валюта = x базовой». Правил под
//! конкретные валюты нет. `entered` — число как ввели, `rate` — по-прежнему «базовой за 1 единицу валюты»
//! (`inverted ? 1 / entered : entered`); потребители (`budget`, `transactions`, `mcp`) читают только `rate`.
//! Поведение формы (⇄, предпросмотр, предупреждение при расхождении больше чем в 5 раз) — `bindRateForms`
//! в `crates/app/static/app.js`. Тест миграции на живой БД: `TEST_DATABASE_URL=… cargo test -p rates migration`.

#[allow(unused_imports)]
use kernel::code::{normalize, require_text};
use rust_decimal::Decimal;
use uuid::Uuid;

/// Знаков после запятой у введённого курса (`entered`, колонка NUMERIC(18,6)).
pub const ENTERED_DP: u32 = 6;
/// Знаков после запятой у курса к базовой (`rate`, колонка NUMERIC(24,12)): `1/2.53` и обратно
/// не расходятся с введённым числом в 6-м знаке.
pub const RATE_DP: u32 = 12;

/// Запись журнала.
#[derive(Debug, Clone, PartialEq)]
pub struct Rate {
    pub id: Uuid,
    pub project_id: Uuid,
    pub rate_date: chrono::NaiveDate,
    pub currency_code: String,
    /// Сколько единиц базовой валюты за 1 единицу валюты. Считается из `entered` и `inverted`.
    pub rate: Decimal,
    /// Курс ровно в том виде, в каком его ввели.
    pub entered: Decimal,
    /// Направление ввода: `false` — 1 валюта = `entered` базовой; `true` — 1 базовая = `entered` валюты.
    pub inverted: bool,
}

/// Данные записи, которые задаёт пользователь (для создания и изменения).
#[derive(Debug, Clone)]
pub struct RateData {
    pub rate_date: chrono::NaiveDate,
    pub currency_code: String,
    pub entered: Decimal,
    pub inverted: bool,
}

impl Rate {
    pub fn create(project_id: Uuid, data: RateData) -> Result<Self, String> {
        let mut item = Self {
            id: Uuid::new_v4(),
            project_id,
            rate_date: data.rate_date,
            currency_code: data.currency_code.clone(),
            rate: Decimal::ONE,
            entered: Decimal::ONE,
            inverted: false,
        };
        item.change(data)?;
        Ok(item)
    }

    pub fn change(&mut self, data: RateData) -> Result<(), String> {
        let (entered, rate) = base_rate(data.entered, data.inverted)?;
        self.rate_date = data.rate_date;
        self.currency_code = normalize("currency_code", &data.currency_code)?;
        self.entered = entered;
        self.inverted = data.inverted;
        self.rate = rate;
        Ok(())
    }

    /// Обратный курс: сколько единиц валюты за 1 единицу базовой.
    pub fn inverse_rate(&self) -> Decimal {
        if self.inverted { self.entered } else { invert(self.rate) }
    }
}

/// Введённый курс → (введённое число без лишних нулей, курс к базовой «базовой за 1 единицу валюты»).
pub fn base_rate(entered: Decimal, inverted: bool) -> Result<(Decimal, Decimal), String> {
    let entered = entered.normalize();
    if entered <= Decimal::ZERO {
        return Err("rate: курс должен быть больше нуля".into());
    }
    if entered.scale() > ENTERED_DP {
        return Err(format!("rate: не больше {ENTERED_DP} знаков после запятой"));
    }
    let rate = if inverted { invert(entered) } else { entered };
    if rate.is_zero() {
        return Err("rate: слишком большой курс".into());
    }
    Ok((entered, rate))
}

/// `1 / x` с точностью курса к базовой.
pub fn invert(x: Decimal) -> Decimal {
    if x.is_zero() { Decimal::ZERO } else { (Decimal::ONE / x).round_dp(RATE_DP).normalize() }
}
