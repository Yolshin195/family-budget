//! Use cases фичи «Транзакции». Знает только домен и порты — ничего об HTTP и SQL.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::NaiveDate;
use kernel::code::normalize;
use kernel::error::{AppError, AppResult, Problem, codes};
use kernel::i18n::Lang;
use kernel::pagination::{PagedResponse, Pagination};
use kernel::web::Choice;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::domain::{
    Body, Description, Draft, ExternalRef, ItemDraft, ItemsDraft, RefDraft, RefKind, RefLink, Transaction,
    TransactionData, TxKind,
};
use crate::dto::{
    ExternalRefDto, ExternalRefFoundDto, ExternalRefInput, ItemInput, ListQuery, QuickAccountDto, QuickCategoryDto,
    QuickDefaultsDto, QuickPlaceDto, TransactionDto, TransactionInput, TranslationInput, recipient_of,
};
use crate::ports::{
    CategoryInfo, Deps, ExchangeLine, FactLine, Filter, Payment, PersonInfo, PlaceInfo, TransactionRepository, taken,
};

/// Частых категорий в форме быстрого расхода.
pub const FREQUENT: usize = 6;

pub struct TransactionService {
    repo: Arc<dyn TransactionRepository>,
    deps: Deps,
    /// Удалённые операции для «Отменить» в тосте.
    undo: kernel::undo::UndoBuffer<Transaction>,
}

/// Частые категории типа `kind`: сначала по привычкам пользователя, затем — корни дерева.
fn frequent(stats: Vec<String>, cats: &[CategoryInfo], kind: &str) -> Vec<String> {
    let active = |code: &str| cats.iter().any(|c| c.code == code && c.kind == kind);
    let mut out: Vec<String> = stats.into_iter().filter(|c| active(c)).take(FREQUENT).collect();
    for c in cats.iter().filter(|c| c.kind == kind && c.depth == 0) {
        if out.len() >= FREQUENT {
            break;
        }
        if !out.contains(&c.code) {
            out.push(c.code.clone());
        }
    }
    out
}

impl TransactionService {
    pub fn new(repo: Arc<dyn TransactionRepository>, deps: Deps) -> Self {
        Self { repo, deps, undo: Default::default() }
    }

    /// Создать транзакцию. `created_by` — текущий пользователь, не из тела запроса.
    pub async fn create(
        &self,
        project_id: Uuid,
        created_by: Uuid,
        input: TransactionInput,
    ) -> AppResult<TransactionDto> {
        Ok(self.create_once(project_id, created_by, input).await?.0)
    }

    /// Создать транзакцию; с `external_ref` — не больше одной на внешнюю запись (docs/external-refs.md, 2.2).
    /// `false` — операция по этой записи уже есть: ничего не создано, возвращена она (тело запроса с ней не
    /// сравнивается). Операция по записи была и удалена — `EXTERNAL_REF_DELETED`.
    pub async fn create_once(
        &self,
        project_id: Uuid,
        created_by: Uuid,
        mut input: TransactionInput,
    ) -> AppResult<(TransactionDto, bool)> {
        let source = input.external_ref.take().map(|r| r.into_draft("external_ref.")).transpose()?;
        if let Some(r) = &source {
            if TxKind::parse(&input.tx_kind) == Some(TxKind::Opening) {
                return Err(err("external_ref", "err.external_ref_opening", &[]).into());
            }
            if let Some(existing) = self.by_ref(project_id, r).await? {
                return Ok((existing, false));
            }
        }
        self.apply_place(project_id, &mut input, None).await?;
        let people = self.check_header_person(project_id, &input, None).await?;
        let data = input.into_domain()?;
        let accounts = self.deps.accounts.currencies(project_id).await?;
        let item = Transaction::create(project_id, created_by, data, &accounts)?;
        self.check_refs(project_id, &item, None).await?;
        check_people(&people, &item, None)?;
        self.check_plan(&item, None).await?;
        self.check_opening(&item).await?;
        let Some(r) = source else {
            self.repo.insert(&item).await.map_err(|e| conflict(e, &item))?;
            return Ok((item.into(), true));
        };
        let link = r.link(project_id, item.id, RefLink::Created, created_by);
        match self.repo.insert_with_ref(&item, &link).await {
            Ok(()) => Ok((with_refs(item.into(), vec![link]), true)),
            // Гонка: ту же запись только что записал параллельный запрос — его SQL-транзакция победила, наша
            // откатилась целиком. Ответ — как на повтор.
            Err(AppError::Conflict(m)) if m == taken::EXTERNAL_REF => match self.by_ref(project_id, &r).await? {
                Some(existing) => Ok((existing, false)),
                None => Err(ref_taken("external_ref")),
            },
            Err(e) => Err(conflict(e, &item)),
        }
    }

    /// Операция, которую создала или подтвердила внешняя запись; связи нет — `None`, операция удалена — ошибка.
    async fn by_ref(&self, project_id: Uuid, r: &RefDraft) -> AppResult<Option<TransactionDto>> {
        let Some(link) = self.repo.find_ref(project_id, &r.provider, &r.external_id).await? else { return Ok(None) };
        match self.repo.find(project_id, link.transaction_id).await? {
            Some(t) => Ok(Some(self.dto(t).await?)),
            None => Err(AppError::ConflictWith(Problem::new(
                codes::EXTERNAL_REF_DELETED,
                "external_ref",
                tr("err.external_ref_deleted", &[]),
            ))),
        }
    }

    /// Прикрепить внешнюю запись к существующей операции (2.3): связь `MATCHED`, операция не меняется.
    /// `false` — запись уже прикреплена к этой операции, возвращена прежняя связь; к другой — `EXTERNAL_REF_TAKEN`.
    pub async fn attach(
        &self,
        project_id: Uuid,
        user_id: Uuid,
        id: Uuid,
        input: ExternalRefInput,
    ) -> AppResult<(ExternalRefDto, bool)> {
        let r = input.into_draft("")?;
        let item = self.load(project_id, id).await?;
        if item.tx_kind == TxKind::Opening {
            return Err(err("", "err.external_ref_opening", &[]).into());
        }
        let link = r.link(project_id, id, RefLink::Matched, user_id);
        match self.repo.insert_ref(&link).await {
            Ok(()) => Ok((link.into(), true)),
            // Занято — этой операцией (повтор, в том числе параллельный) или другой.
            Err(AppError::Conflict(m)) if m == taken::EXTERNAL_REF => {
                match self.repo.find_ref(project_id, &r.provider, &r.external_id).await? {
                    Some(found) if found.transaction_id == id => Ok((found.into(), false)),
                    _ => Err(ref_taken("")),
                }
            }
            Err(e) => Err(e),
        }
    }

    /// Связь внешней записи (2.4) — для клиента после 409: к какой операции и есть ли она ещё.
    pub async fn find_ref(
        &self,
        project_id: Uuid,
        provider: &str,
        external_id: &str,
    ) -> AppResult<ExternalRefFoundDto> {
        let r = RefDraft::new("", provider, external_id, None)?;
        let link = self
            .repo
            .find_ref(project_id, &r.provider, &r.external_id)
            .await?
            .ok_or_else(|| AppError::NotFound(tr("err.external_ref_not_found", &[])))?;
        let transaction_exists = self.repo.find(project_id, link.transaction_id).await?.is_some();
        Ok(ExternalRefFoundDto { external_ref: link.into(), transaction_exists })
    }

    pub async fn get(&self, project_id: Uuid, id: Uuid) -> AppResult<TransactionDto> {
        let item = self.load(project_id, id).await?;
        self.dto(item).await
    }

    pub async fn list(
        &self,
        project_id: Uuid,
        query: ListQuery,
        pagination: Pagination,
    ) -> AppResult<PagedResponse<TransactionDto>> {
        let filter = filter_of(query)?;
        let items = self.repo.list(project_id, &filter, pagination.limit(), pagination.offset()).await?;
        let total = self.repo.count(project_id, &filter).await?;
        Ok(PagedResponse::new(self.dtos(project_id, items).await?, &pagination, total))
    }

    /// Операции со связями с внешними записями — одним запросом связей на страницу.
    async fn dtos(&self, project_id: Uuid, items: Vec<Transaction>) -> AppResult<Vec<TransactionDto>> {
        let ids: Vec<Uuid> = items.iter().map(|t| t.id).collect();
        let mut refs: HashMap<Uuid, Vec<ExternalRef>> = HashMap::new();
        if !ids.is_empty() {
            for r in self.repo.refs_of(project_id, &ids).await? {
                refs.entry(r.transaction_id).or_default().push(r);
            }
        }
        Ok(items
            .into_iter()
            .map(|t| {
                let mine = refs.remove(&t.id).unwrap_or_default();
                with_refs(t.into(), mine)
            })
            .collect())
    }

    async fn dto(&self, item: Transaction) -> AppResult<TransactionDto> {
        let refs = self.repo.refs_of(item.project_id, &[item.id]).await?;
        Ok(with_refs(item.into(), refs))
    }

    /// Лента страницы за период (техдолги #25, #26): операции — не больше [`MAX_FEED`], и тогда `truncated`;
    /// итоги и выписка считаются по всем операциям фильтра, а не по показанным.
    pub async fn feed(&self, project_id: Uuid, query: ListQuery) -> AppResult<Feed> {
        let filter = filter_of(query)?;
        let mut items = self.repo.list(project_id, &filter, MAX_FEED as i64 + 1, 0).await?;
        let truncated = items.len() > MAX_FEED;
        items.truncate(MAX_FEED);
        let totals = self.repo.category_totals(project_id, &filter).await?;
        // С фильтром «Для кого» выписки нет: проводки на счёт не делятся по позициям, итоги — доля человека.
        let statement = match filter.account_code.as_ref().filter(|_| filter.for_person.is_none()) {
            None => None,
            Some(acc) => {
                let start = match filter.from.and_then(|d| d.pred_opt()) {
                    Some(d) => self.balance(project_id, acc, d).await?,
                    None => Decimal::ZERO,
                };
                let end = match filter.to {
                    Some(d) => self.balance(project_id, acc, d).await?,
                    None => self
                        .balances(project_id, NaiveDate::MAX)
                        .await?
                        .into_iter()
                        .find(|b| &b.0 == acc)
                        .map(|b| b.1)
                        .unwrap_or_default(),
                };
                let (inflow, outflow) = self.repo.account_flows(project_id, &filter, acc).await?;
                // От остатка на конец назад: верно и для показанной части обрезанной ленты.
                let mut running = Vec::with_capacity(items.len());
                let mut bal = end;
                for t in &items {
                    running.push((t.id, bal));
                    bal -= t.amount_on(acc);
                }
                Some(Statement { start, end, inflow, outflow, running })
            }
        };
        Ok(Feed { items: self.dtos(project_id, items).await?, truncated, totals, statement })
    }

    pub async fn update(&self, project_id: Uuid, id: Uuid, mut input: TransactionInput) -> AppResult<TransactionDto> {
        let mut item = self.load(project_id, id).await?;
        let before = item.clone();
        self.apply_place(project_id, &mut input, before.place_code.as_deref()).await?;
        let people = self.check_header_person(project_id, &input, Some(&before)).await?;
        let data = input.into_domain()?;
        let accounts = self.deps.accounts.currencies(project_id).await?;
        item.change(data, &accounts)?;
        self.check_refs(project_id, &item, Some(&before)).await?;
        check_people(&people, &item, Some(&before))?;
        self.check_plan(&item, Some(&before)).await?;
        self.check_opening(&item).await?;
        self.repo.update(&item).await.map_err(|e| conflict(e, &item))?;
        self.dto(item).await
    }

    /// Место операции (docs/group-1.md, 5.2): если счёт и категории новых позиций не указаны — берутся из места.
    /// Новое место — только активное; место, которое у операции уже было, остаётся и после архивации.
    /// У перевода и начального остатка места нет.
    async fn apply_place(&self, project_id: Uuid, input: &mut TransactionInput, before: Option<&str>) -> AppResult<()> {
        let flow = matches!(TxKind::parse(&input.tx_kind), Some(TxKind::Expense | TxKind::Income));
        let code = input.place_code.take().map(|v| v.trim().to_uppercase()).filter(|v| flow && !v.is_empty());
        let Some(code) = code else { return Ok(()) };
        let places = self.deps.places.all(project_id).await?;
        let place =
            places.into_iter().find(|p| p.code == code && (!p.archived || before == Some(code.as_str()))).ok_or_else(
                || Problem::new(codes::NOT_FOUND, "place_code", tr("err.place_not_found", &[("code", &code)])),
            )?;
        // Подставляется только то, что можно выбрать: активный счёт и активная категория типа операции.
        if input.account_code.trim().is_empty()
            && let Some(account) = place.default_account_code
            && self.deps.accounts.exists(project_id, &account).await?
        {
            input.account_code = account;
        }
        if let Some(category) = place.default_category_code {
            let kind = input.tx_kind.trim().to_uppercase();
            let usable = self
                .deps
                .categories
                .kind_of(project_id, &category)
                .await?
                .is_some_and(|c| !c.archived && c.kind == kind);
            let empty =
                |x: &ItemInput| x.id.is_none() && x.category_code.as_deref().is_none_or(|c| c.trim().is_empty());
            for x in input.items.iter_mut().filter(|x| usable && empty(x)) {
                x.category_code = Some(category.clone());
            }
        }
        input.place_code = Some(code);
        Ok(())
    }

    /// «Для кого по умолчанию» из запроса — человек из справочника (раздел 5.9). Возвращает людей проекта для
    /// проверки позиций; если получателей нет нигде, справочник не читается.
    async fn check_header_person(
        &self,
        project_id: Uuid,
        input: &TransactionInput,
        before: Option<&Transaction>,
    ) -> AppResult<Vec<PersonInfo>> {
        // У перевода и начального остатка получателя нет: поле не проверяется, а отбрасывается (как место).
        if !matches!(TxKind::parse(&input.tx_kind), Some(TxKind::Expense | TxKind::Income)) {
            return Ok(Vec::new());
        }
        let named = input.items.iter().any(|x| recipient_of(x.for_person_code.clone()).flatten().is_some());
        let header = recipient_of(input.for_person_code.clone()).flatten();
        let had = before.is_some_and(|b| b.items.iter().any(|x| x.for_person.is_some()));
        if header.is_none() && !named && !had {
            return Ok(Vec::new());
        }
        let people = self.deps.people.all(project_id, Lang::default()).await?;
        if let Some(code) = header.map(|c| c.trim().to_uppercase()) {
            let kept = before.is_some_and(|b| b.items.iter().any(|x| x.for_person.as_deref() == Some(code.as_str())));
            if !kept && !people.iter().any(|p| p.code == code && !p.archived) {
                return Err(Problem::new(
                    codes::NOT_FOUND,
                    "for_person_code",
                    tr("err.person_not_found", &[("code", &code)]),
                )
                .into());
            }
        }
        Ok(people)
    }

    /// Изменить только переводы описания.
    pub async fn translate(&self, project_id: Uuid, id: Uuid, input: TranslationInput) -> AppResult<TransactionDto> {
        let mut item = self.load(project_id, id).await?;
        if item.description.text.is_none() {
            return Err(err("description", "err.no_description", &[]).into());
        }
        item.translate(input.description_ru, input.description_en, input.description_th);
        self.repo.update(&item).await?;
        self.dto(item).await
    }

    /// Удаление. Если транзакция оплачивала вхождение плана — оно снова «ожидается».
    pub async fn delete(&self, project_id: Uuid, id: Uuid) -> AppResult<()> {
        if self.repo.delete(project_id, id).await? { Ok(()) } else { Err(not_found(id)) }
    }

    /// Удаление из интерфейса: операция запоминается на пару минут — токен для «Отменить».
    pub async fn delete_with_undo(&self, project_id: Uuid, id: Uuid) -> AppResult<Uuid> {
        let item = self.load(project_id, id).await?;
        self.delete(project_id, id).await?;
        Ok(self.undo.put(project_id, item))
    }

    /// «Отменить»: вернуть операцию с тем же id и проводками; связи с внешними записями снова указывают на неё.
    pub async fn undo_delete(&self, project_id: Uuid, token: Uuid) -> AppResult<TransactionDto> {
        let item = self.undo.take(project_id, token).ok_or_else(|| AppError::NotFound(tr("err.undo_expired", &[])))?;
        self.repo.insert(&item).await.map_err(|e| conflict(e, &item))?;
        self.dto(item).await
    }

    /// Пользователи, внёсшие первую операцию за период — для воронки лендинга (только число).
    pub async fn first_expenses_between(
        &self,
        from: chrono::DateTime<chrono::Utc>,
        to: chrono::DateTime<chrono::Utc>,
    ) -> AppResult<i64> {
        self.repo.count_first(from, to).await
    }

    /// Удалить все операции проекта и их связи с внешними записями (удаление проекта или аккаунта).
    pub async fn purge_project(&self, project_id: Uuid) -> AppResult<u64> {
        self.repo.purge(project_id).await
    }

    // ---- Счета: начальный остаток, остатки, сверка ----

    /// Начальный остаток счёта и его дата.
    pub async fn opening(&self, project_id: Uuid, account_code: &str) -> AppResult<Option<(Decimal, NaiveDate)>> {
        let code = normalize("account_code", account_code).map_err(AppError::Validation)?;
        Ok(self.repo.find_opening(project_id, &code).await?.map(|t| (t.amount_on(&code), t.occurred_on)))
    }

    /// Задать начальный остаток из формы счёта: транзакция `OPENING` создаётся, меняется или
    /// удаляется (нулевой остаток).
    pub async fn set_opening(
        &self,
        project_id: Uuid,
        user_id: Uuid,
        account_code: &str,
        amount: Decimal,
        date: NaiveDate,
    ) -> AppResult<()> {
        let code = normalize("account_code", account_code).map_err(AppError::Validation)?;
        let existing = self.repo.find_opening(project_id, &code).await?;
        if amount.is_zero() {
            if let Some(t) = existing {
                self.repo.delete(project_id, t.id).await?;
            }
            return Ok(());
        }
        let accounts = self.deps.accounts.currencies(project_id).await?;
        let body = Draft::Opening { account: code.clone(), amount };
        match existing {
            Some(mut t) => {
                let data = TransactionData {
                    occurred_on: date,
                    description: t.description.clone(),
                    plan: None,
                    place_code: None,
                    body,
                };
                t.change(data, &accounts).map_err(opening_error)?;
                self.repo.update(&t).await
            }
            None => {
                let data = TransactionData {
                    occurred_on: date,
                    description: Description::default(),
                    plan: None,
                    place_code: None,
                    body: body.clone(),
                };
                let t = Transaction::create(project_id, user_id, data, &accounts).map_err(opening_error)?;
                match self.repo.insert(&t).await {
                    // Повторное нажатие «Сохранить»: остаток успел записать параллельный запрос — обновляем его.
                    Err(AppError::Conflict(m)) if m == taken::OPENING => {
                        let mut t =
                            self.repo.find_opening(project_id, &code).await?.ok_or_else(|| AppError::Conflict(m))?;
                        let data = TransactionData {
                            occurred_on: date,
                            description: t.description.clone(),
                            plan: None,
                            place_code: None,
                            body,
                        };
                        t.change(data, &accounts).map_err(opening_error)?;
                        self.repo.update(&t).await
                    }
                    other => other,
                }
            }
        }
    }

    /// Остатки счетов на дату включительно: (счёт, остаток).
    pub async fn balances(&self, project_id: Uuid, date: NaiveDate) -> AppResult<Vec<(String, Decimal)>> {
        self.repo.balances(project_id, date).await
    }

    pub async fn balance(&self, project_id: Uuid, account_code: &str, date: NaiveDate) -> AppResult<Decimal> {
        Ok(self
            .balances(project_id, date)
            .await?
            .into_iter()
            .find(|b| b.0 == account_code)
            .map(|b| b.1)
            .unwrap_or_default())
    }

    /// Есть ли по счёту проводки (тогда его валюту менять нельзя).
    pub async fn has_postings(&self, project_id: Uuid, account_code: &str) -> AppResult<bool> {
        self.repo.has_postings(project_id, account_code).await
    }

    /// Есть ли проводки на категорию (для категорий: тогда тип не меняется).
    pub async fn uses_category(&self, project_id: Uuid, code: &str) -> AppResult<bool> {
        self.repo.uses_category(project_id, code).await
    }

    /// Есть ли проводки в валюте (для валют: тогда точность не уменьшается).
    pub async fn uses_currency(&self, project_id: Uuid, currency: &str) -> AppResult<bool> {
        self.repo.uses_currency(project_id, currency).await
    }

    /// «Сверить остаток»: разница между остатком из банка и остатком по проводкам вносится расходом
    /// или доходом на выбранную категорию. `None` — остатки совпали, вносить нечего.
    #[allow(clippy::too_many_arguments)]
    pub async fn reconcile(
        &self,
        project_id: Uuid,
        user_id: Uuid,
        account_code: &str,
        actual: Decimal,
        date: NaiveDate,
        category_code: &str,
        description: Description,
    ) -> AppResult<Option<TransactionDto>> {
        let code = normalize("account_code", account_code).map_err(AppError::Validation)?;
        let diff = actual - self.balance(project_id, &code, date).await?;
        if diff.is_zero() {
            return Ok(None);
        }
        let accounts = self.deps.accounts.currencies(project_id).await?;
        let items = ItemsDraft::replace(vec![ItemDraft::simple(category_code, diff.abs())]);
        let body = Draft::Flow { income: diff > Decimal::ZERO, account: code, items, refund: false };
        let data = TransactionData { occurred_on: date, description, plan: None, place_code: None, body };
        let item = Transaction::create(project_id, user_id, data, &accounts)?;
        self.check_refs(project_id, &item, None).await.map_err(|e| match e {
            AppError::Invalid(_) | AppError::Validation(_) if diff > Decimal::ZERO => {
                err("category_code", "err.reconcile_need_income", &[]).into()
            }
            AppError::Invalid(_) | AppError::Validation(_) => {
                err("category_code", "err.reconcile_need_expense", &[]).into()
            }
            other => other,
        })?;
        self.repo.insert(&item).await?;
        Ok(Some(item.into()))
    }

    /// Быстрый расход (раздел 10 ТЗ): счёт по умолчанию, частые категории за 30 дней, дерево
    /// категорий для поиска на трёх языках, остатки лимитов за месяц даты и курсы к базовой валюте.
    pub async fn quick_defaults(
        &self,
        project_id: Uuid,
        user_id: Uuid,
        base_currency: &str,
        lang: Lang,
        date: NaiveDate,
    ) -> AppResult<QuickDefaultsDto> {
        let since = date - chrono::Days::new(30);
        let expense = self.repo.quick_stats(project_id, user_id, "EXPENSE", since).await?;
        let income = self.repo.quick_stats(project_id, user_id, "INCOME", since).await?;
        let accounts = self.deps.accounts.infos(project_id, lang).await?;
        let cats = self.deps.categories.infos(project_id, lang).await?;
        let limits = self.deps.budget.limits(project_id, base_currency, date, lang).await?;
        let rates = self.deps.budget.rates(project_id, base_currency, date).await?;
        // Без курса — пусто, а не 1 (этап 0, Ц3): форма и MCP предупредят «нет курса».
        let rate = |cur: &str| {
            if cur == base_currency { Some(Decimal::ONE) } else { rates.iter().find(|r| r.0 == cur).map(|r| r.1) }
        };
        let active = |code: &str| accounts.iter().any(|a| a.code == code);
        let default_account = expense
            .last_account
            .clone()
            .or(income.last_account.clone())
            .filter(|a| active(a))
            .or_else(|| accounts.first().map(|a| a.code.clone()));
        let filter = Filter { kind: Some(TxKind::Transfer), created_by: Some(user_id), ..Default::default() };
        let last_transfer =
            self.repo.list(project_id, &filter, 1, 0).await?.into_iter().next().and_then(|t| match t.body() {
                Body::Transfer { from, to, .. } if active(&from) && active(&to) => Some((from, to)),
                _ => None,
            });
        // Места: сначала частые в расходах пользователя, затем по названию; подставляют только активное.
        let mut places: Vec<PlaceInfo> =
            self.deps.places.all(project_id).await?.into_iter().filter(|p| !p.archived).collect();
        let rank = |code: &str| expense.places.iter().position(|c| c == code).unwrap_or(usize::MAX);
        places.sort_by_key(|p| (rank(&p.code), p.title.to_lowercase()));
        let frequent_places: Vec<String> =
            places.iter().filter(|p| rank(&p.code) != usize::MAX).map(|p| p.code.clone()).collect();
        let places = places
            .into_iter()
            .map(|p| QuickPlaceDto {
                category: p.default_category_code.filter(|c| cats.iter().any(|x| &x.code == c)),
                account: p.default_account_code.filter(|a| active(a)),
                code: p.code,
                title: p.title,
                search: p.search,
            })
            .collect();
        Ok(QuickDefaultsDto {
            places,
            frequent_places,
            date,
            base_currency: base_currency.to_string(),
            default_account,
            last_transfer,
            frequent_expense: frequent(expense.frequent, &cats, "EXPENSE"),
            frequent_income: frequent(income.frequent, &cats, "INCOME"),
            accounts: accounts
                .into_iter()
                .map(|a| QuickAccountDto {
                    rate_to_base: rate(&a.currency),
                    code: a.code,
                    title: a.title,
                    currency: a.currency,
                    symbol: a.symbol,
                })
                .collect(),
            categories: cats
                .into_iter()
                .map(|c| {
                    let l = limits.iter().find(|l| l.code == c.code);
                    QuickCategoryDto {
                        limit: l.and_then(|l| l.limit),
                        forecast: l.map(|l| l.forecast).unwrap_or_default(),
                        available: l.and_then(|l| l.available),
                        code: c.code,
                        parent_code: c.parent_code,
                        kind: c.kind,
                        title: c.title,
                        depth: c.depth,
                        search: c.names,
                    }
                })
                .collect(),
        })
    }

    // ---- Публичный контракт для других фич (бюджет, планирование, счета) ----

    /// Проводки на категории за период включительно.
    pub async fn lines(&self, project_id: Uuid, from: NaiveDate, to: NaiveDate) -> AppResult<Vec<FactLine>> {
        self.repo.lines(project_id, from, to).await
    }

    /// Проводки «Обмен валют» за период — курсовая разница.
    pub async fn exchange(&self, project_id: Uuid, from: NaiveDate, to: NaiveDate) -> AppResult<Vec<ExchangeLine>> {
        self.repo.exchange(project_id, from, to).await
    }

    /// Оплаченные вхождения планов за период (по дате вхождения).
    pub async fn payments(&self, project_id: Uuid, from: NaiveDate, to: NaiveDate) -> AppResult<Vec<Payment>> {
        self.repo.payments(project_id, from, to).await
    }

    // ---- Для страниц ----

    /// Точность валюты каждого счёта (включая архивные): (счёт, знаков после запятой).
    pub async fn minor_units(&self, project_id: Uuid) -> AppResult<Vec<(String, u32)>> {
        Ok(self.deps.accounts.currencies(project_id).await?.into_iter().map(|a| (a.code, a.minor_units)).collect())
    }

    pub async fn account_choices(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<Choice>> {
        self.deps.accounts.choices(project_id, lang).await
    }

    pub async fn category_choices(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<Choice>> {
        self.deps.categories.choices(project_id, lang).await
    }

    /// Люди проекта, с архивными (форма показывает активных и получателей самой операции), имена — на языке `lang`.
    pub async fn people(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<PersonInfo>> {
        self.deps.people.all(project_id, lang).await
    }

    /// Места проекта, с архивными (форма показывает активные и место самой операции).
    pub async fn places(&self, project_id: Uuid) -> AppResult<Vec<PlaceInfo>> {
        self.deps.places.all(project_id).await
    }

    pub async fn account_titles(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<(String, String)>> {
        self.deps.accounts.titles(project_id, lang).await
    }

    pub async fn category_titles(&self, project_id: Uuid, lang: Lang) -> AppResult<Vec<(String, String)>> {
        self.deps.categories.titles(project_id, lang).await
    }

    /// Курсы валют к базовой на дату — для курса справочника рядом с курсом перевода.
    pub async fn rates(
        &self,
        project_id: Uuid,
        base_currency: &str,
        date: NaiveDate,
    ) -> AppResult<Vec<(String, Decimal)>> {
        self.deps.budget.rates(project_id, base_currency, date).await
    }

    async fn load(&self, project_id: Uuid, id: Uuid) -> AppResult<Transaction> {
        self.repo.find(project_id, id).await?.ok_or_else(|| not_found(id))
    }

    /// Оплата вхождения плана (этап 0, Ц7): план есть в проекте, дата — из его правила, тип операции —
    /// как у плана, вхождение не пропущено. Вторую оплату того же вхождения не даёт уникальный индекс.
    /// Если операция и раньше оплачивала это вхождение, проверка не повторяется.
    async fn check_plan(&self, item: &Transaction, before: Option<&Transaction>) -> AppResult<()> {
        let Some((plan_id, date)) = item.plan else { return Ok(()) };
        if before.is_some_and(|b| b.plan == item.plan) {
            return Ok(());
        }
        let Some(occ) = self.deps.plans.occurrence(item.project_id, plan_id, date).await? else {
            return Err(
                Problem::new(codes::NOT_FOUND, "plan_date", tr("err.plan_no_occurrence", &[("date", &date)])).into()
            );
        };
        if occ.kind != item.tx_kind.as_str() {
            return Err(Problem::new(codes::KIND_MISMATCH, "plan_id", tr("err.plan_kind", &[])).into());
        }
        if occ.skipped {
            return Err(err("plan_date", "err.plan_skipped", &[("date", &date)]).into());
        }
        Ok(())
    }

    /// У счёта один начальный остаток: он ведётся из формы счёта.
    async fn check_opening(&self, item: &Transaction) -> AppResult<()> {
        if item.tx_kind != TxKind::Opening {
            return Ok(());
        }
        for code in item.accounts() {
            if self.repo.find_opening(item.project_id, code).await?.is_some_and(|t| t.id != item.id) {
                return Err(AppError::Conflict(tr("err.opening_exists", &[("code", &code)])));
            }
        }
        Ok(())
    }

    /// Ссылки на счета и категории: активные записи нужного типа. Ссылки, которые уже были в
    /// прежней версии транзакции, остаются валидными, даже если запись с тех пор в архиве; тип категории
    /// проверяется всегда (техдолг #3: правка не превращает доход в расход на категории дохода).
    async fn check_refs(&self, project_id: Uuid, item: &Transaction, before: Option<&Transaction>) -> AppResult<()> {
        let kept = |kind: RefKind, code: &str| {
            before.is_some_and(|b| b.postings.iter().any(|p| p.ref_kind == kind && p.ref_code == code))
        };
        let body = item.body();
        let (account_fields, cat_kind): (Vec<(&str, String)>, &str) = match &body {
            Body::Flow { income, account, .. } => {
                (vec![("account_code", account.clone())], if *income { "INCOME" } else { "EXPENSE" })
            }
            Body::Transfer { from, to, .. } => {
                (vec![("account_code", from.clone()), ("to_account_code", to.clone())], "EXPENSE")
            }
            Body::Opening { account, .. } => (vec![("account_code", account.clone())], "EXPENSE"),
        };
        for (field, code) in account_fields {
            if !kept(RefKind::Account, &code) && !self.deps.accounts.exists(project_id, &code).await? {
                return Err(
                    Problem::new(codes::NOT_FOUND, field, tr("err.account_not_found", &[("code", &code)])).into()
                );
            }
        }
        let cat_fields: Vec<(String, String)> = match body {
            Body::Flow { items, .. } => items
                .into_iter()
                .enumerate()
                .map(|(i, x)| (format!("items[{}].category_code", i + 1), x.category_code))
                .collect(),
            Body::Transfer { fee, .. } => {
                fee.into_iter().map(|f| ("fee_category_code".to_string(), f.category_code)).collect()
            }
            Body::Opening { .. } => Vec::new(),
        };
        for (field, code) in cat_fields {
            match self.deps.categories.kind_of(project_id, &code).await? {
                Some(c) if c.archived && !kept(RefKind::Category, &code) => {
                    return Err(Problem::new(
                        codes::NOT_FOUND,
                        &field,
                        tr("err.category_not_found", &[("code", &code)]),
                    )
                    .into());
                }
                None => {
                    return Err(Problem::new(
                        codes::NOT_FOUND,
                        &field,
                        tr("err.category_not_found", &[("code", &code)]),
                    )
                    .into());
                }
                Some(c) if c.kind != cat_kind => {
                    let key =
                        if cat_kind == "INCOME" { "err.need_income_category" } else { "err.need_expense_category" };
                    return Err(Problem::new(codes::KIND_MISMATCH, &field, tr(key, &[("code", &code)])).into());
                }
                Some(_) => {}
            }
        }
        Ok(())
    }
}

/// Операций в ленте страницы — не больше (защита от огромных периодов; сверх — признак `truncated`).
pub const MAX_FEED: usize = 5000;

/// Лента страницы.
pub struct Feed {
    /// Новые сначала, не больше [`MAX_FEED`].
    pub items: Vec<TransactionDto>,
    /// Операций в периоде больше, чем показано.
    pub truncated: bool,
    /// Итоги по категориям всех операций фильтра: (валюта, доходы, расходы).
    pub totals: Vec<(String, Decimal, Decimal)>,
    /// Выписка — если фильтр по счёту.
    pub statement: Option<Statement>,
}

/// Выписка по счёту за период.
pub struct Statement {
    pub start: Decimal,
    pub end: Decimal,
    pub inflow: Decimal,
    pub outflow: Decimal,
    /// Остаток после каждой показанной операции.
    pub running: Vec<(Uuid, Decimal)>,
}

fn filter_of(query: ListQuery) -> AppResult<Filter> {
    let kind = match query.tx_kind.as_deref().filter(|v| !v.is_empty()) {
        None => None,
        Some(k) => Some(TxKind::parse(k).ok_or_else(|| err("tx_kind", "err.kind_invalid", &[("value", &k)]))?),
    };
    Ok(Filter {
        from: query.from,
        to: query.to,
        account_code: query.account_code.filter(|v| !v.is_empty()),
        kind,
        created_by: query.created_by,
        untranslated: query.untranslated.as_deref().and_then(Lang::parse),
        place_code: query.place_code.as_deref().map(str::trim).filter(|v| !v.is_empty()).map(str::to_uppercase),
        for_person: recipient_of(query.for_person_code.filter(|v| !v.trim().is_empty()))
            .map(|who| who.map(|c| c.to_uppercase())),
        external_provider: query
            .external_provider
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_lowercase),
    })
}

fn with_refs(dto: TransactionDto, refs: Vec<ExternalRef>) -> TransactionDto {
    TransactionDto { external_refs: refs.into_iter().map(Into::into).collect(), ..dto }
}

/// Внешняя запись связана с другой операцией. `field` — `external_ref` в теле операции, пусто — в теле прикрепления.
fn ref_taken(field: &str) -> AppError {
    AppError::ConflictWith(Problem::new(codes::EXTERNAL_REF_TAKEN, field, tr("err.external_ref_taken", &[])))
}

/// Получатели позиций (раздел 5.9): новый — активный человек проекта; получатель, который у операции уже был,
/// остаётся и после архивации (как место и категории).
pub(crate) fn check_people(people: &[PersonInfo], item: &Transaction, before: Option<&Transaction>) -> AppResult<()> {
    for (i, x) in item.items.iter().enumerate() {
        let Some(code) = x.for_person.as_deref() else { continue };
        let kept = before.is_some_and(|b| b.items.iter().any(|y| y.for_person.as_deref() == Some(code)));
        if !kept && !people.iter().any(|p| p.code == code && !p.archived) {
            return Err(Problem::new(
                codes::NOT_FOUND,
                &format!("items[{}].for_person_code", i + 1),
                tr("err.person_not_found", &[("code", &code)]),
            )
            .into());
        }
    }
    Ok(())
}

/// Строка словаря фичи на языке запроса.
fn tr(key: &str, args: &[(&str, &dyn std::fmt::Display)]) -> String {
    kernel::i18n::tr(Some(crate::i18n::dict), key, args)
}

fn err(field: &str, key: &str, args: &[(&str, &dyn std::fmt::Display)]) -> Problem {
    Problem::validation(field, tr(key, args))
}

fn not_found(id: Uuid) -> AppError {
    AppError::NotFound(tr("err.not_found", &[("id", &id)]))
}

/// Ошибка начального остатка — на поле формы счёта (форма счёта разбирает `поле: текст`).
fn opening_error(p: Problem) -> AppError {
    let field = if p.field == "amount" { "opening_balance".to_string() } else { p.field };
    AppError::Validation(Problem { field, ..p }.to_string())
}

/// Занятый уникальный ключ: вхождение плана уже оплачено или у счёта уже есть начальный остаток.
fn conflict(e: AppError, item: &Transaction) -> AppError {
    match e {
        AppError::Conflict(m) if m == taken::OPENING => {
            AppError::Conflict(tr("err.opening_exists", &[("code", &item.opening_account().unwrap_or_default())]))
        }
        AppError::Conflict(_) => AppError::Conflict(tr("err.plan_paid", &[])),
        other => other,
    }
}
