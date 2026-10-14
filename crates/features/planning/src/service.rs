//! Use cases фичи «Планирование». Знает только домен и порты.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::NaiveDate;
use kernel::code::ensure_ref;
use kernel::error::{AppError, AppResult};
use kernel::i18n::Lang;
use kernel::web::Choice;
use uuid::Uuid;

use crate::domain::{Plan, status};
use crate::dto::{OccurrenceDto, PlanDto, PlanInput};
use crate::ports::{Deps, PlanRepository};

pub struct PlanningService {
    repo: Arc<dyn PlanRepository>,
    deps: Deps,
    /// Удалённые планы с их пропусками — для «Отменить» в тосте.
    undo: kernel::undo::UndoBuffer<(Plan, Vec<NaiveDate>)>,
}

/// Окно, в котором ищутся пропуски удаляемого плана (весь разумный диапазон дат).
fn all_dates() -> (NaiveDate, NaiveDate) {
    (NaiveDate::from_ymd_opt(1900, 1, 1).unwrap_or_default(), NaiveDate::from_ymd_opt(2999, 12, 31).unwrap_or_default())
}

/// Окно расчёта вхождений — не больше 3 лет за запрос.
const MAX_RANGE_DAYS: i64 = 3 * 366;

impl PlanningService {
    pub fn new(repo: Arc<dyn PlanRepository>, deps: Deps) -> Self {
        Self { repo, deps, undo: Default::default() }
    }

    pub async fn create(&self, project_id: Uuid, user_id: Uuid, input: PlanInput) -> AppResult<PlanDto> {
        let plan = Plan::create(project_id, user_id, input.into_domain().map_err(AppError::Validation)?).map_err(AppError::Validation)?;
        self.check_refs(project_id, &plan).await?;
        self.repo.insert(&plan).await?;
        Ok(plan.into())
    }

    pub async fn get(&self, project_id: Uuid, id: Uuid) -> AppResult<PlanDto> {
        Ok(self.load(project_id, id).await?.into())
    }

    /// Все планы проекта: активные сначала, затем по названию.
    pub async fn list(&self, project_id: Uuid) -> AppResult<Vec<PlanDto>> {
        let mut plans = self.repo.list(project_id).await?;
        plans.sort_by(|a, b| b.active.cmp(&a.active).then(a.name.text.cmp(&b.name.text)));
        Ok(plans.into_iter().map(Into::into).collect())
    }

    /// Изменение влияет на будущие нерешённые вхождения: оплаты живут в транзакциях, пропуски — по дате.
    pub async fn update(&self, project_id: Uuid, id: Uuid, input: PlanInput) -> AppResult<PlanDto> {
        let mut plan = self.load(project_id, id).await?;
        plan.change(input.into_domain().map_err(AppError::Validation)?).map_err(AppError::Validation)?;
        self.check_refs(project_id, &plan).await?;
        self.repo.update(&plan).await?;
        Ok(plan.into())
    }

    /// Удаление плана; оплаченные им транзакции остаются (ссылка plan_id просто перестаёт резолвиться).
    pub async fn delete(&self, project_id: Uuid, id: Uuid) -> AppResult<()> {
        if self.repo.delete(project_id, id).await? { Ok(()) } else { Err(not_found(id)) }
    }

    /// Удаление из интерфейса: план и его пропуски запоминаются — токен для «Отменить».
    pub async fn delete_with_undo(&self, project_id: Uuid, id: Uuid) -> AppResult<Uuid> {
        let plan = self.load(project_id, id).await?;
        let (from, to) = all_dates();
        let skips = self.repo.skips(project_id, from, to).await?.into_iter().filter(|(p, _)| *p == id).map(|(_, d)| d).collect();
        self.delete(project_id, id).await?;
        Ok(self.undo.put(project_id, (plan, skips)))
    }

    /// «Отменить»: вернуть план с тем же id и его пропуски.
    pub async fn undo_delete(&self, project_id: Uuid, user_id: Uuid, token: Uuid) -> AppResult<()> {
        let (plan, skips) = self.undo.take(project_id, token).ok_or_else(|| AppError::NotFound("отменить уже нельзя".into()))?;
        self.repo.insert(&plan).await?;
        for d in skips {
            self.repo.add_skip(project_id, plan.id, d, user_id).await?;
        }
        Ok(())
    }

    /// Остановить или возобновить план (остановленный показывает только решённые вхождения).
    pub async fn set_active(&self, project_id: Uuid, id: Uuid, active: bool) -> AppResult<()> {
        let mut plan = self.load(project_id, id).await?;
        plan.active = active;
        self.repo.update(&plan).await
    }

    /// Удалить все планы проекта (удаление проекта или аккаунта).
    pub async fn purge_project(&self, project_id: Uuid) -> AppResult<u64> {
        self.repo.purge(project_id).await
    }

    /// Вхождения всех активных планов в периоде со статусами, по дате.
    pub async fn occurrences(&self, project_id: Uuid, from: NaiveDate, to: NaiveDate, today: NaiveDate) -> AppResult<Vec<OccurrenceDto>> {
        if to < from || (to - from).num_days() > MAX_RANGE_DAYS {
            return Err(AppError::Validation(format!("период: от 1 дня до {MAX_RANGE_DAYS} дней")));
        }
        let plans = self.repo.list(project_id).await?;
        let paid: HashMap<(Uuid, NaiveDate), _> = self
            .deps
            .payments
            .paid(project_id, from, to)
            .await?
            .into_iter()
            .map(|p| ((p.plan_id, p.plan_date), (p.transaction_id, p.amount, p.paid_on)))
            .collect();
        let skips: HashSet<(Uuid, NaiveDate)> = self.repo.skips(project_id, from, to).await?.into_iter().collect();
        let mut out = Vec::new();
        for plan in plans {
            for date in plan.dates(from, to) {
                let p = paid.get(&(plan.id, date)).copied();
                // Остановленный план показывает только уже решённые вхождения.
                if !plan.active && p.is_none() && !skips.contains(&(plan.id, date)) {
                    continue;
                }
                let st = status(date, today, p, skips.contains(&(plan.id, date)));
                let (transaction_id, paid_amount, paid_on) = match &st {
                    crate::domain::Status::Paid { transaction_id, amount, paid_on } => (Some(*transaction_id), Some(*amount), Some(*paid_on)),
                    _ => (None, None, None),
                };
                out.push(OccurrenceDto { plan: plan.clone().into(), date, status: st.code().into(), transaction_id, paid_amount, paid_on });
            }
        }
        out.sort_by(|a, b| a.date.cmp(&b.date).then(a.plan.name.cmp(&b.plan.name)));
        Ok(out)
    }

    /// Одно вхождение (для страницы подтверждения).
    pub async fn occurrence(&self, project_id: Uuid, plan_id: Uuid, date: NaiveDate, today: NaiveDate) -> AppResult<OccurrenceDto> {
        let plan = self.load(project_id, plan_id).await?;
        if !plan.occurs_on(date) {
            return Err(AppError::NotFound(format!("план {plan_id} не приходится на {date}")));
        }
        self.occurrences(project_id, date, date, today)
            .await?
            .into_iter()
            .find(|o| o.plan.id == plan_id)
            .ok_or_else(|| AppError::NotFound(format!("вхождение {plan_id} / {date} не найдено")))
    }

    /// Пропустить вхождение (без транзакции).
    pub async fn skip(&self, project_id: Uuid, user_id: Uuid, plan_id: Uuid, date: NaiveDate) -> AppResult<()> {
        let today = chrono::Local::now().date_naive();
        let occ = self.occurrence(project_id, plan_id, date, today).await?;
        if occ.status == "PAID" {
            return Err(AppError::Conflict("вхождение уже оплачено — сначала удалите транзакцию".into()));
        }
        self.repo.add_skip(project_id, plan_id, date, user_id).await
    }

    /// Вернуть пропущенное вхождение в «ожидается».
    pub async fn unskip(&self, project_id: Uuid, plan_id: Uuid, date: NaiveDate) -> AppResult<()> {
        self.load(project_id, plan_id).await?;
        self.repo.remove_skip(project_id, plan_id, date).await
    }

    // ---- для страниц ----

    pub async fn account_choices(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<Choice>> {
        self.deps.accounts.choices(project_id, lang).await
    }

    pub async fn category_choices(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<Choice>> {
        self.deps.categories.choices(project_id, lang).await
    }

    pub async fn category_titles(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<(String, String)>> {
        self.deps.categories.titles(project_id, lang).await
    }

    async fn load(&self, project_id: Uuid, id: Uuid) -> AppResult<Plan> {
        self.repo.find(project_id, id).await?.ok_or_else(|| not_found(id))
    }

    async fn check_refs(&self, project_id: Uuid, plan: &Plan) -> AppResult<()> {
        ensure_ref(self.deps.accounts.exists(project_id, &plan.account_code).await?, "account_code", &plan.account_code)?;
        if let Some(to) = &plan.to_account_code {
            ensure_ref(self.deps.accounts.exists(project_id, to).await?, "to_account_code", to)?;
        }
        let Some(code) = &plan.category_code else { return Ok(()) };
        match self.deps.categories.kind_of(project_id, code).await? {
            None => ensure_ref(false, "category_code", code),
            Some(k) if k != plan.tx_kind => Err(AppError::Validation(format!("category_code: категория '{code}' — {k}, а план — {}", plan.tx_kind))),
            Some(_) => Ok(()),
        }
    }
}

fn not_found(id: Uuid) -> AppError {
    AppError::NotFound(format!("План {id} не найден"))
}
