//! Входящий адаптер: страница «Курсы валют».

use std::sync::Arc;

use askama::Template;
use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Extension, Form, Router};
use kernel::error::AppError;
use kernel::web::{Choice, Flash, PageCtx, PageError, PageResult, render, see_other, see_other_flash};
use rust_decimal::Decimal;
use serde::Deserialize;
use uuid::Uuid;

use crate::dto::{RateDto, RateInput};
use crate::service::RateService;

type Svc = State<Arc<RateService>>;

pub fn router(service: Arc<RateService>) -> Router {
    Router::new()
        .route("/", get(list).post(create))
        .route("/{id}/delete", post(delete))
        .route("/undo/{token}", post(undo))
        .with_state(service)
}

#[derive(Template)]
#[template(path = "rates/list.html")]
struct ListPage {
    page: PageCtx,
    rows: Vec<RateRow>,
    currencies: Vec<CurrencyOption>,
    today: chrono::NaiveDate,
    error: Option<String>,
    /// Введённое в форму (после ошибки форма открывается с ним же).
    form: RateForm,
    /// Направление ввода: `false` — «1 валюта = x базовой», `true` — «1 базовая = x валюты».
    inverted: bool,
    /// Валюта в форме и её последний курс (подсказка «последний: …» и placeholder).
    currency: String,
    last: Option<LastRate>,
}

impl ListPage {
    /// Левая и правая валюта подписи «1 <from> = [   ] <to>» (символы: «1 ₽ = [   ] ฿»).
    fn left_cur(&self) -> &str {
        self.page.cur(if self.inverted { &self.page.session.base_currency } else { &self.currency })
    }

    fn right_cur(&self) -> &str {
        self.page.cur(if self.inverted { &self.currency } else { &self.page.session.base_currency })
    }

    /// Последний курс в выбранном направлении: число как ввели или пересчитанное.
    fn last_value(&self) -> String {
        self.last
            .as_ref()
            .map(|l| {
                if l.inverted == self.inverted {
                    l.entered.clone()
                } else if self.inverted {
                    l.inverse.clone()
                } else {
                    l.rate.clone()
                }
            })
            .unwrap_or_default()
    }
}

/// Строка таблицы: курс в направлении ввода и обратный.
struct RateRow {
    id: Uuid,
    rate_date: chrono::NaiveDate,
    currency_code: String,
    main: String,
    other: String,
}

/// Валюта в выпадающем списке с её последним курсом (для подсказок на клиенте).
struct CurrencyOption {
    code: String,
    label: String,
    last: Option<LastRate>,
}

#[derive(Clone)]
struct LastRate {
    /// Введённое число, без искажений.
    entered: String,
    inverted: bool,
    /// Базовой за 1 единицу валюты.
    rate: String,
    /// Валюты за 1 единицу базовой.
    inverse: String,
    date: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RateForm {
    rate_date: String,
    currency_code: String,
    rate: String,
    inverted: String,
}

/// Курс для показа: введённое число — как есть, пересчитанное — 4 знака после запятой или
/// 4 значащие цифры для чисел меньше 1 (0.0274, 0.4032).
fn fmt_rate(v: Decimal) -> String {
    let v = v.normalize();
    let mut dp = 4;
    let mut x = v;
    while x < Decimal::new(1, 1) && !x.is_zero() && dp < crate::domain::RATE_DP {
        x *= Decimal::TEN;
        dp += 1;
    }
    v.round_dp(dp).normalize().to_string()
}

/// `cur` и `base` — как показать валюту курса и базовую (символы: «1 ฿ = 2.5 ₽»).
fn row(r: &RateDto, cur: &str, base: &str) -> RateRow {
    let entered = r.entered.normalize();
    let (main, other) = if r.inverted {
        (format!("1 {base} = {entered} {cur}"), format!("1 {cur} = {} {base}", fmt_rate(r.rate)))
    } else {
        (format!("1 {cur} = {entered} {base}"), format!("1 {base} = {} {cur}", fmt_rate(r.inverse_rate)))
    };
    RateRow { id: r.id, rate_date: r.rate_date, currency_code: r.currency_code.clone(), main, other }
}

/// Курс из формы: пробелы и запятая допускаются, выражения — нет.
fn parse_rate(value: &str) -> Result<Decimal, String> {
    let cleaned: String = value
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '\u{202F}' && *c != '\u{00A0}')
        .collect::<String>()
        .replace(',', ".");
    if cleaned.is_empty() {
        return Err("rate: введите курс".into());
    }
    cleaned.parse().map_err(|_| format!("rate: '{value}' — не число"))
}

async fn show(svc: &RateService, page: PageCtx, error: Option<String>, form: RateForm) -> PageResult {
    let p = page.session.project_id;
    let base = page.session.base_currency.clone();
    let all = svc.all(p).await?;
    let latest = svc.latest_by_currency(p).await?;
    let currencies: Vec<CurrencyOption> = svc
        .ref_choices(p, page.lang)
        .await?
        .into_iter()
        .filter(|c| c.code != base)
        .map(|c: Choice| {
            let last = latest.iter().find(|r| r.currency_code == c.code).map(|r| LastRate {
                entered: r.entered.normalize().to_string(),
                inverted: r.inverted,
                rate: fmt_rate(r.rate),
                inverse: fmt_rate(r.inverse_rate),
                date: page.date(&r.rate_date),
            });
            CurrencyOption { code: c.code, label: c.label, last }
        })
        .collect();
    let currency = currencies
        .iter()
        .find(|c| c.code == form.currency_code)
        .or(currencies.first())
        .map(|c| c.code.clone())
        .unwrap_or_default();
    // После ошибки — направление, в котором вводили; иначе — как у прошлого курса валюты.
    let inverted =
        if form.currency_code.is_empty() { svc.default_inverted(p, &currency).await? } else { form.inverted == "true" };
    let last = currencies.iter().find(|c| c.code == currency).and_then(|c| c.last.clone());
    let rows = all.iter().map(|r| row(r, page.cur(&r.currency_code), page.cur(&base))).collect();
    let page = page.for_feature(crate::NAME, crate::i18n::dict);
    render(&ListPage {
        page,
        rows,
        currencies,
        today: chrono::Local::now().date_naive(),
        error,
        form,
        inverted,
        currency,
        last,
    })
}

async fn list(State(svc): Svc, Extension(page): Extension<PageCtx>) -> PageResult {
    show(&svc, page, None, RateForm::default()).await
}

async fn create(State(svc): Svc, Extension(page): Extension<PageCtx>, Form(f): Form<RateForm>) -> PageResult {
    let result = async {
        let input = RateInput {
            rate_date: f.rate_date.parse().map_err(|_| AppError::Validation("rate_date: неверная дата".into()))?,
            currency_code: f.currency_code.clone(),
            rate: parse_rate(&f.rate).map_err(AppError::Validation)?,
            inverted: f.inverted == "true",
        };
        svc.create(page.session.project_id, input).await
    }
    .await;
    match result {
        Ok(_) => Ok(see_other("/rates")),
        Err(AppError::Validation(m) | AppError::Conflict(m)) => show(&svc, page, Some(m), f).await,
        Err(e) => Err(PageError(e)),
    }
}

/// Удаление без «Вы уверены?»: тост с «Отменить» на 5 секунд.
async fn delete(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(id): Path<Uuid>) -> PageResult {
    let token = svc.delete_with_undo(page.session.project_id, id).await?;
    Ok(see_other_flash(
        &page.config,
        "/rates",
        Flash::with_undo(page.t("toast.deleted"), format!("/rates/undo/{token}")),
    ))
}

async fn undo(State(svc): Svc, Extension(page): Extension<PageCtx>, Path(token): Path<Uuid>) -> PageResult {
    let flash = match svc.undo_delete(page.session.project_id, token).await {
        Ok(()) => Flash::ok(page.t("toast.restored")),
        Err(AppError::NotFound(_)) => Flash::error(page.t("toast.undo_expired")),
        Err(e) => return Err(PageError(e)),
    };
    Ok(see_other_flash(&page.config, "/rates", flash))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Decimal {
        s.parse().unwrap()
    }

    #[test]
    fn rates_are_shown_in_both_directions() {
        let dto = |entered: &str, inverted: bool, rate: &str, inverse: &str| RateDto {
            id: Uuid::nil(),
            rate_date: chrono::NaiveDate::from_ymd_opt(2026, 10, 16).unwrap(),
            currency_code: if inverted { "RUB".into() } else { "USD".into() },
            rate: d(rate),
            entered: d(entered),
            inverted,
            inverse_rate: d(inverse),
        };
        let rub = row(&dto("2.50", true, "0.4", "2.5"), "₽", "฿");
        assert_eq!((rub.main.as_str(), rub.other.as_str()), ("1 ฿ = 2.5 ₽", "1 ₽ = 0.4 ฿"));
        assert_eq!(rub.currency_code, "RUB");
        let rub = row(&dto("2.48", true, "0.403225806452", "2.48"), "₽", "฿");
        assert_eq!(rub.other, "1 ₽ = 0.4032 ฿");
        let usd = row(&dto("36.5", false, "36.5", "0.027397260274"), "$", "฿");
        assert_eq!((usd.main.as_str(), usd.other.as_str()), ("1 $ = 36.5 ฿", "1 ฿ = 0.0274 $"));
        assert_eq!(fmt_rate(d("36.123456")), "36.1235");
        assert_eq!(fmt_rate(d("0.000021234567")), "0.00002123");
    }

    #[test]
    fn rate_field_accepts_comma_and_spaces() {
        assert_eq!(parse_rate(" 2,50 ").unwrap(), d("2.50"));
        assert_eq!(parse_rate("1 000.5").unwrap(), d("1000.5"));
        assert!(parse_rate("").is_err());
        assert!(parse_rate("2.5+1").is_err());
    }
}
