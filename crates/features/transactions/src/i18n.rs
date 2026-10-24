//! Строки интерфейса фичи «Транзакции».
use kernel::i18n::Tr;

pub fn dict(key: &str) -> Option<Tr> {
    Some(match key {
        "nav" => ("Операции", "Transactions", "รายการ"),
        "title" => ("Операции", "Transactions", "รายการ"),
        "new" => ("Новая операция", "New transaction", "รายการใหม่"),
        "edit_title" => ("Операция", "Transaction", "รายการ"),
        "lines" => ("Позиции", "Items", "รายการย่อย"),
        "add_line" => ("Добавить позицию", "Add item", "เพิ่มรายการ"),
        // Позиции чека (docs/group-1.md, раздел 8).
        "item_name" => ("Название", "Name", "ชื่อรายการ"),
        "x_price" => ("× цена", "× price", "× ราคา"),
        "quantity" => ("Количество", "Quantity", "จำนวน"),
        "unit" => ("Единица", "Unit", "หน่วย"),
        "price" => ("Цена", "Price", "ราคา"),
        "discount" => ("Скидка", "Discount", "ส่วนลด"),
        "unit.pcs" => ("шт", "pcs", "ชิ้น"),
        "unit.kg" => ("кг", "kg", "กก."),
        "unit.l" => ("л", "l", "ล."),
        "unit.g" => ("г", "g", "ก."),
        "more" => ("Ещё", "More", "เพิ่มเติม"),
        "feed_truncated" => (
            "Показаны последние 5 000 операций месяца — итоги посчитаны по всем. Уточните фильтр, чтобы увидеть остальные.",
            "Showing the latest 5,000 transactions of the month — totals include all of them. Narrow the filter to see the rest.",
            "แสดง 5,000 รายการล่าสุดของเดือน — ยอดรวมนับทุกรายการ กรองให้แคบลงเพื่อดูรายการที่เหลือ",
        ),
        "discount_total" => ("Скидка на чек", "Receipt discount", "ส่วนลดทั้งใบเสร็จ"),
        "discount_total_hint" => (
            "Делится по позициям пропорционально сумме",
            "Split across items in proportion to their amounts",
            "แบ่งให้แต่ละรายการตามสัดส่วนยอดเงิน",
        ),
        "note" => ("Уточнение", "Note", "หมายเหตุ"),
        "remove" => ("Убрать", "Remove", "ลบ"),
        "author" => ("Кто внёс", "Added by", "ผู้บันทึก"),
        "all_accounts" => ("Все счета", "All accounts", "ทุกบัญชี"),
        "all_members" => ("Все участники", "All members", "สมาชิกทั้งหมด"),
        "untranslated" => ("Описание не на моём языке", "Description not in my language", "คำอธิบายยังไม่มีภาษาของฉัน"),
        // «Перевод» в операциях — перевод между счетами; перевод текста — «Перевести описание».
        "translate" => ("Перевести описание", "Translate description", "แปลคำอธิบาย"),
        "translate_title" => ("Перевод описания", "Translate description", "แปลคำอธิบาย"),
        "lang.translations" => ("Переводы описания", "Description translations", "คำแปลคำอธิบาย"),
        "original" => ("Оригинал", "Original", "ต้นฉบับ"),
        "from_plan" => ("по плану", "planned", "ตามแผน"),
        "paid_plan" => ("Оплата по плану", "Planned payment", "ชำระตามแผน"),
        "income_total" => ("Доходы", "Income", "รายรับ"),
        "expense_total" => ("Расходы", "Expenses", "รายจ่าย"),
        "net" => ("Итог", "Net", "สุทธิ"),
        "confirm_delete" => ("Удалить операцию?", "Delete this transaction?", "ลบรายการนี้?"),
        "desc_hint" => (
            "Пишите на любом из трёх языков; переводы можно добавить позже.",
            "Write in any of the three languages; translations can be added later.",
            "เขียนได้ทั้งสามภาษา เพิ่มคำแปลภายหลังได้",
        ),
        "no_desc" => ("Без описания", "No description", "ไม่มีคำอธิบาย"),
        "apply" => ("Показать", "Apply", "แสดง"),
        "month_nav" => ("Выбор месяца", "Month navigation", "เลือกเดือน"),
        "prev_month" => ("Предыдущий месяц", "Previous month", "เดือนก่อน"),
        "next_month" => ("Следующий месяц", "Next month", "เดือนถัดไป"),
        "add_first" => ("Добавить расход", "Add an expense", "เพิ่มรายจ่าย"),
        "empty_title" => ("В этом месяце операций нет", "No transactions this month", "เดือนนี้ยังไม่มีรายการ"),
        "empty_text" => (
            "Добавьте первый расход: сумма, категория, Enter — меньше пяти секунд. Клавиша N работает на любой странице, T — перевод между счетами.",
            "Add your first expense: amount, category, Enter — under five seconds. The N key works on any page, T adds a transfer between accounts.",
            "เพิ่มรายจ่ายแรก: จำนวน หมวดหมู่ Enter — ไม่ถึงห้าวินาที กด N ได้ทุกหน้า และ T เพื่อโอนระหว่างบัญชี",
        ),
        "empty_filtered" => ("Ничего не найдено", "Nothing found", "ไม่พบรายการ"),
        "empty_filtered_text" => (
            "Сбросьте фильтры или выберите другой месяц.",
            "Clear the filters or pick another month.",
            "ล้างตัวกรองหรือเลือกเดือนอื่น",
        ),
        "transfer" => ("Перевод", "Transfer", "โอน"),
        "opening" => ("Начальный остаток", "Opening balance", "ยอดยกมา"),
        "from" => ("Откуда", "From", "จากบัญชี"),
        "to" => ("Куда", "To", "ไปยังบัญชี"),
        "sent" => ("Списано", "Debited", "ตัดออก"),
        "sent_hint" => (
            "Всего ушло со счёта, вместе с комиссией.",
            "Total taken from the account, fee included.",
            "ยอดที่ออกจากบัญชีทั้งหมด รวมค่าธรรมเนียม",
        ),
        "received" => ("Зачислено", "Credited", "เข้าบัญชี"),
        "received_hint" => (
            "Сколько пришло на счёт в его валюте.",
            "What arrived, in the account's currency.",
            "ยอดที่เข้าบัญชีในสกุลเงินของบัญชี",
        ),
        "rate" => ("Курс", "Rate", "อัตรา"),
        "rate_ref" => ("по справочнику", "reference", "ตามข้อมูลอ้างอิง"),
        "fee" => ("Комиссия", "Fee", "ค่าธรรมเนียม"),
        "fee_short" => ("комиссия", "fee", "ค่าธรรมเนียม"),
        "fee_category" => ("Категория комиссии", "Fee category", "หมวดค่าธรรมเนียม"),
        "fee_hint" => (
            "Комиссия — расход: попадает в бюджет и лимиты. Курс считается без неё.",
            "The fee is an expense: it counts in the budget and limits. The rate excludes it.",
            "ค่าธรรมเนียมเป็นรายจ่าย นับในงบประมาณและวงเงิน อัตราคำนวณโดยไม่รวมค่าธรรมเนียม",
        ),
        "refund" => ("Возврат", "Refund", "คืนเงิน"),
        "refund_hint" => (
            "Деньги вернулись: суммы уменьшат факт категорий.",
            "Money came back: amounts reduce the categories' actuals.",
            "เงินคืนกลับมา: ยอดจะลดยอดจริงของหมวดหมู่",
        ),
        "postings" => ("Проводки", "Postings", "รายการบัญชีคู่"),
        "postings_hint" => (
            "Двойная запись: сумма проводок по каждой валюте равна нулю.",
            "Double entry: postings in each currency add up to zero.",
            "บัญชีคู่: ผลรวมรายการในแต่ละสกุลเงินเท่ากับศูนย์",
        ),
        "sys.EXCHANGE" => ("Обмен валют", "Currency exchange", "แลกเปลี่ยนเงินตรา"),
        "sys.OPENING" => ("Начальные остатки", "Opening balances", "ยอดยกมา"),
        "all_kinds" => ("Все типы", "All types", "ทุกประเภท"),
        "kinds.EXPENSE" => ("Расходы", "Expenses", "รายจ่าย"),
        "kinds.INCOME" => ("Доходы", "Income", "รายรับ"),
        "kinds.TRANSFER" => ("Переводы", "Transfers", "การโอน"),
        "kinds.OPENING" => ("Начальные остатки", "Opening balances", "ยอดยกมา"),
        "statement" => ("выписка", "statement", "รายการเดินบัญชี"),
        "running" => ("остаток", "balance", "คงเหลือ"),
        "balance_start" => ("Остаток на начало", "Opening balance", "ยอดต้นงวด"),
        "balance_end" => ("Остаток на конец", "Closing balance", "ยอดปลายงวด"),
        "inflow" => ("Поступило", "Money in", "เงินเข้า"),
        "outflow" => ("Списано", "Money out", "เงินออก"),
        "reconcile" => ("Сверить остаток", "Reconcile balance", "กระทบยอดคงเหลือ"),
        "reconcile_text" => (
            "Введите остаток из банка или кошелька. Разница внесётся расходом или доходом на выбранную категорию.",
            "Enter the balance from your bank or wallet. The difference is recorded as an expense or income in the chosen category.",
            "ใส่ยอดคงเหลือจากธนาคารหรือกระเป๋าเงิน ส่วนต่างจะบันทึกเป็นรายจ่ายหรือรายรับในหมวดที่เลือก",
        ),
        "book_balance" => ("Остаток по операциям", "Balance per records", "ยอดตามรายการ"),
        "actual_balance" => ("Остаток на самом деле", "Actual balance", "ยอดจริง"),
        "diff_more" => ("Больше на", "More by", "มากกว่า"),
        "diff_less" => ("Меньше на", "Less by", "น้อยกว่า"),
        "reconcile_category_hint" => (
            "Не хватает — категория расхода, больше — категория дохода.",
            "Short — an expense category; extra — an income category.",
            "ขาด — หมวดรายจ่าย, เกิน — หมวดรายรับ",
        ),
        "reconcile_desc" => ("Сверка остатка", "Balance reconciliation", "กระทบยอดคงเหลือ"),
        "reconcile_done" => ("Остаток сверен", "Balance reconciled", "กระทบยอดแล้ว"),
        "reconcile_same" => {
            ("Остатки совпадают — вносить нечего", "Balances match — nothing to record", "ยอดตรงกัน ไม่ต้องบันทึก")
        }
        "nav_accounts" => ("Счета", "Accounts", "บัญชี"),
        // Ошибки (техдолг #9): текст после «поле: », параметры — {имя}.
        "err.kind_invalid" => (
            "недопустимое значение '{value}', ожидается EXPENSE, INCOME, TRANSFER или OPENING",
            "invalid value '{value}', expected EXPENSE, INCOME, TRANSFER or OPENING",
            "ค่า '{value}' ไม่ถูกต้อง ต้องเป็น EXPENSE, INCOME, TRANSFER หรือ OPENING",
        ),
        "err.lang_invalid" => (
            "недопустимое значение '{value}', ожидается RU, EN или TH",
            "invalid value '{value}', expected RU, EN or TH",
            "ค่า '{value}' ไม่ถูกต้อง ต้องเป็น RU, EN หรือ TH",
        ),
        "err.plan_pair" => (
            "plan_id и plan_date задаются вместе",
            "plan_id and plan_date go together",
            "ต้องระบุ plan_id และ plan_date คู่กัน",
        ),
        "err.to_account_required" => ("выберите счёт зачисления", "choose the account to credit", "เลือกบัญชีที่รับเงิน"),
        "err.amount_required" => ("введите сумму списания", "enter the debited amount", "กรอกยอดที่ตัดออก"),
        "err.fee_category_required" => ("выберите категорию комиссии", "choose a fee category", "เลือกหมวดค่าธรรมเนียม"),
        "err.fee_amount_required" => ("введите сумму комиссии", "enter the fee amount", "กรอกค่าธรรมเนียม"),
        "err.opening_required" => ("введите остаток", "enter the balance", "กรอกยอดคงเหลือ"),
        "err.lines_and_items" => (
            "передайте позиции в items или в lines, но не в обоих",
            "send items either in items or in lines, not both",
            "ส่งรายการใน items หรือ lines อย่างใดอย่างหนึ่ง",
        ),
        "err.precision" => (
            "у {currency} не больше {n} знаков после запятой",
            "{currency} allows at most {n} decimal places",
            "{currency} มีทศนิยมได้ไม่เกิน {n} ตำแหน่ง",
        ),
        "err.positive" => {
            ("сумма должна быть больше нуля", "the amount must be greater than zero", "จำนวนเงินต้องมากกว่าศูนย์")
        }
        "err.account_not_found" => ("счёт '{code}' не найден", "account '{code}' not found", "ไม่พบบัญชี '{code}'"),
        "place" => ("Место", "Place", "สถานที่"),
        "for_whom" => ("Для кого", "For whom", "สำหรับใคร"),
        "for_default" => ("Для кого по умолчанию", "For whom by default", "สำหรับใคร (ค่าเริ่มต้น)"),
        "for_all" => ("Для всех", "For everyone", "สำหรับทุกคน"),
        "for_me" => ("Для себя", "For myself", "สำหรับตัวเอง"),
        "for_split" => ("Разные получатели в чеке", "Different recipients in one receipt", "หลายผู้รับในใบเสร็จเดียว"),
        "for_differ" => ("разные получатели", "different recipients", "หลายผู้รับ"),
        "for_category" => ("Всей категории «{category}»", "Whole category “{category}”", "ทั้งหมวด “{category}”"),
        "for_reset" => ("Сбросить всё на «{value}»?", "Reset all to “{value}”?", "ตั้งทั้งหมดเป็น “{value}”?"),
        "for_reset_yes" => ("Сбросить", "Reset", "รีเซ็ต"),
        "for_reset_no" => ("Оставить разными", "Keep different", "คงไว้ต่างกัน"),
        "all_recipients" => ("Все получатели", "All recipients", "ผู้รับทั้งหมด"),
        "err.person_not_found" => (
            "человек '{code}' не найден или в архиве",
            "person '{code}' not found or archived",
            "ไม่พบบุคคล '{code}' หรือถูกเก็บถาวร",
        ),
        "err.place_not_found" => (
            "место '{code}' не найдено или в архиве",
            "place '{code}' not found or archived",
            "ไม่พบสถานที่ '{code}' หรือถูกเก็บถาวร",
        ),
        "err.items_empty" => ("нужна хотя бы одна позиция", "add at least one item", "ต้องมีอย่างน้อยหนึ่งรายการ"),
        "err.items_max" => ("не больше {n} позиций", "at most {n} items", "ไม่เกิน {n} รายการ"),
        "err.category_required" => ("выберите категорию", "choose a category", "เลือกหมวดหมู่"),
        "err.amount_or_price" => ("введите сумму или цену", "enter an amount or a price", "กรอกจำนวนเงินหรือราคา"),
        "err.quantity_positive" => {
            ("количество должно быть больше нуля", "the quantity must be greater than zero", "จำนวนต้องมากกว่าศูนย์")
        }
        "err.qty_scale" => {
            ("не больше {n} знаков после запятой", "at most {n} decimal places", "ทศนิยมได้ไม่เกิน {n} ตำแหน่ง")
        }
        "err.unit_invalid" => {
            ("единица — шт, кг, л или г", "the unit is pcs, kg, l or g", "หน่วยต้องเป็น ชิ้น กก. ล. หรือ ก.")
        }
        "err.price_negative" => ("цена не может быть отрицательной", "the price cannot be negative", "ราคาติดลบไม่ได้"),
        "err.discount_negative" => {
            ("скидка не может быть отрицательной", "the discount cannot be negative", "ส่วนลดติดลบไม่ได้")
        }
        "err.discount_too_big" => (
            "скидка съедает сумму позиции целиком",
            "the discount takes the whole item amount",
            "ส่วนลดเท่ากับหรือเกินยอดของรายการ",
        ),
        "err.receipt_discount_too_big" => (
            "скидка на чек должна быть меньше суммы позиций",
            "the receipt discount must be less than the items' total",
            "ส่วนลดทั้งใบเสร็จต้องน้อยกว่ายอดรวมรายการ",
        ),
        "err.amount_mismatch" => (
            "сумма {amount} не сходится с ценой: должно быть {computed}",
            "the amount {amount} does not match the price: expected {computed}",
            "ยอด {amount} ไม่ตรงกับราคา ควรเป็น {computed}",
        ),
        "err.item_unknown" => {
            ("позиции {id} нет в этой операции", "item {id} is not in this transaction", "ไม่มีรายการ {id} ในธุรกรรมนี้")
        }
        "err.item_twice" => ("позиция {id} указана дважды", "item {id} is listed twice", "ระบุรายการ {id} ซ้ำ"),
        "err.same_account" => (
            "счёт зачисления должен отличаться от счёта списания",
            "the credited account must differ from the debited one",
            "บัญชีรับต้องต่างจากบัญชีที่ตัด",
        ),
        "err.fee_too_big" => (
            "комиссия должна быть меньше суммы списания",
            "the fee must be less than the debited amount",
            "ค่าธรรมเนียมต้องน้อยกว่ายอดที่ตัด",
        ),
        "err.to_amount_same" => (
            "при одной валюте зачисляется списанное за вычетом комиссии",
            "in one currency the credit is the debit minus the fee",
            "สกุลเงินเดียวกัน ยอดรับเท่ากับยอดตัดหักค่าธรรมเนียม",
        ),
        "err.to_amount_required" => (
            "при разных валютах укажите, сколько зачислено",
            "for different currencies enter the credited amount",
            "สกุลเงินต่างกัน กรอกยอดที่เข้าบัญชี",
        ),
        "err.opening_zero" => {
            ("начальный остаток не может быть нулевым", "the opening balance cannot be zero", "ยอดยกมาเป็นศูนย์ไม่ได้")
        }
        "err.opening_plan" => {
            ("начальный остаток не оплачивает план", "an opening balance cannot pay a plan", "ยอดยกมาใช้ชำระตามแผนไม่ได้")
        }
        "err.no_description" => (
            "у операции нет описания — переводить нечего",
            "the transaction has no description to translate",
            "รายการนี้ไม่มีคำอธิบายให้แปล",
        ),
        "err.need_expense_category" => (
            "категория '{code}' — доходная, а нужна категория расхода",
            "'{code}' is an income category, an expense category is needed",
            "'{code}' เป็นหมวดรายรับ ต้องใช้หมวดรายจ่าย",
        ),
        "err.need_income_category" => (
            "категория '{code}' — расходная, а нужна категория дохода",
            "'{code}' is an expense category, an income category is needed",
            "'{code}' เป็นหมวดรายจ่าย ต้องใช้หมวดรายรับ",
        ),
        "err.category_not_found" => {
            ("категория '{code}' не найдена", "category '{code}' not found", "ไม่พบหมวดหมู่ '{code}'")
        }
        "err.plan_no_occurrence" => {
            ("у плана нет платежа на {date}", "the plan has no payment on {date}", "แผนไม่มีการชำระวันที่ {date}")
        }
        "err.plan_kind" => (
            "тип операции не совпадает с планом",
            "the transaction type does not match the plan",
            "ประเภทรายการไม่ตรงกับแผน",
        ),
        "err.plan_skipped" => (
            "платёж {date} пропущен — сначала верните его в календаре",
            "the {date} payment is skipped — restore it in the calendar first",
            "การชำระวันที่ {date} ถูกข้าม — คืนค่าในปฏิทินก่อน",
        ),
        "err.opening_exists" => (
            "у счёта {code} уже есть начальный остаток — измените его в форме счёта",
            "account {code} already has an opening balance — change it in the account form",
            "บัญชี {code} มียอดยกมาแล้ว — แก้ในฟอร์มบัญชี",
        ),
        "err.plan_paid" => (
            "это вхождение плана уже оплачено другой операцией",
            "this plan payment is already paid by another transaction",
            "การชำระตามแผนนี้ถูกบันทึกโดยรายการอื่นแล้ว",
        ),
        "err.not_found" => ("операция {id} не найдена", "transaction {id} not found", "ไม่พบรายการ {id}"),
        "err.reconcile_need_income" => (
            "остаток больше — нужна категория дохода",
            "the balance is higher — choose an income category",
            "ยอดมากกว่า — เลือกหมวดรายรับ",
        ),
        "err.reconcile_need_expense" => (
            "остаток меньше — нужна категория расхода",
            "the balance is lower — choose an expense category",
            "ยอดน้อยกว่า — เลือกหมวดรายจ่าย",
        ),
        "err.undo_expired" => ("отменить уже нельзя", "it can no longer be undone", "ยกเลิกไม่ได้แล้ว"),
        "err.bad_date" => ("неверная дата", "invalid date", "วันที่ไม่ถูกต้อง"),
        "err.bad_id" => ("неверный id", "invalid id", "id ไม่ถูกต้อง"),
        // Связи с внешними записями (docs/external-refs.md, 2.6).
        "err.external_ref_deleted" => (
            "Операция по этой записи уже была и удалена — повторно не создаётся",
            "A transaction from this record existed and was deleted — it is not created again",
            "รายการจากข้อมูลนี้เคยมีและถูกลบแล้ว — จะไม่สร้างซ้ำ",
        ),
        "err.external_ref_taken" => (
            "Эта запись уже прикреплена к другой операции",
            "This record is already linked to another transaction",
            "ข้อมูลนี้ผูกกับรายการอื่นแล้ว",
        ),
        "err.external_ref_opening" => (
            "Начальный остаток нельзя связать с внешней записью",
            "An opening balance cannot be linked to an external record",
            "ยอดยกมาไม่สามารถผูกกับข้อมูลภายนอกได้",
        ),
        "err.external_ref_not_found" => ("Связи с этой записью нет", "No link to this record", "ไม่มีการผูกกับข้อมูลนี้"),
        "err.external_provider" => (
            "Источник: латиница в нижнем регистре, цифры и «_», до 40 знаков",
            "Source: lowercase Latin letters, digits and “_”, up to 40 characters",
            "แหล่งที่มา: ตัวอักษรละตินพิมพ์เล็ก ตัวเลข และ “_” ไม่เกิน 40 ตัว",
        ),
        "err.external_id" => (
            "Id внешней записи: от 1 до 200 знаков",
            "External record id: 1 to 200 characters",
            "รหัสข้อมูลภายนอก: 1 ถึง 200 ตัวอักษร",
        ),
        "err.external_received_at" => (
            "Время записи: дата и время RFC 3339 с часовым поясом",
            "Record time: RFC 3339 date and time with a time zone",
            "เวลาของข้อมูล: วันที่และเวลาแบบ RFC 3339 พร้อมเขตเวลา",
        ),
        // Значок «SMS» в ленте и карточке (раздел 3); {time} — время SMS или дата связи.
        "tx.sms_badge" => ("SMS", "SMS", "SMS"),
        "tx.sms_created" => ("Из SMS банка · {time}", "From bank SMS · {time}", "จาก SMS ธนาคาร · {time}"),
        "tx.sms_matched" => {
            ("Подтверждено SMS банка · {time}", "Confirmed by bank SMS · {time}", "ยืนยันด้วย SMS ธนาคาร · {time}")
        }
        _ => return None,
    })
}
