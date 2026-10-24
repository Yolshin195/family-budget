//! Тексты лендинга и правовых страниц на трёх языках (ТЗ, раздел 11). Слово «бесплатно» и тарифы
//! не используются, пока не решён вопрос монетизации.
use kernel::i18n::Tr;

pub fn dict(key: &str) -> Option<Tr> {
    Some(match key {
        "meta.title" => (
            "Семейный бюджет — ведите расходы вместе",
            "Family budget — track spending together",
            "งบครอบครัว — จัดการค่าใช้จ่ายร่วมกัน",
        ),
        "meta.description" => (
            "Общий и личный бюджет семьи: расход за 5 секунд или голосом через Claude, счета в разных валютах, переводы и обмен, лимиты и регулярные платежи. Русский, английский и тайский.",
            "A shared and personal family budget: add an expense in 5 seconds or by voice with Claude, accounts in any currency, transfers and exchange, limits and regular payments. English, Russian and Thai.",
            "งบครอบครัวทั้งส่วนรวมและส่วนตัว: เพิ่มรายจ่ายใน 5 วินาทีหรือสั่งด้วยเสียงผ่าน Claude บัญชีหลายสกุลเงิน โอนและแลกเงิน วงเงิน และการชำระประจำ รองรับไทย อังกฤษ และรัสเซีย",
        ),
        "nav.login" => ("Войти", "Sign in", "เข้าสู่ระบบ"),
        "cta.create" => ("Создать бюджет", "Create a budget", "สร้างงบประมาณ"),
        "cta.have_account" => ("Уже есть аккаунт?", "Already have an account?", "มีบัญชีอยู่แล้ว?"),
        "cta.demo" => ("Посмотреть демо", "View the demo", "ดูเดโม"),
        "cta.demo_hint" => ("Без регистрации, только просмотр", "No sign-up, view only", "ไม่ต้องสมัคร ดูอย่างเดียว"),
        "hero.badge" => (
            "Новое: траты голосом и по фото чека через Claude",
            "New: log spending by voice or receipt photo with Claude",
            "ใหม่: บันทึกรายจ่ายด้วยเสียงหรือรูปใบเสร็จผ่าน Claude",
        ),
        "hero.title" => {
            ("Семейный бюджет, который ведут вместе", "A family budget you keep together", "งบประมาณครอบครัวที่ดูแลร่วมกัน")
        }
        "hero.subtitle" => (
            "Расходы, счета в разных валютах, лимиты и регулярные платежи — в одном месте и на понятном каждому языке. Записать трату — 5 секунд, а можно просто сказать Claude.",
            "Spending, accounts in different currencies, limits and regular payments — in one place and in everyone's language. Logging an expense takes 5 seconds, or just tell Claude.",
            "รายจ่าย บัญชีหลายสกุลเงิน วงเงิน และการชำระประจำ — รวมไว้ที่เดียวในภาษาที่ทุกคนเข้าใจ บันทึกรายจ่ายใน 5 วินาที หรือแค่บอก Claude",
        ),
        "hero.alt" => (
            "Экран «Обзор»: бюджет месяца по категориям с лимитами",
            "The Overview screen: the month's budget by category with limits",
            "หน้าภาพรวม: งบของเดือนตามหมวดหมู่พร้อมวงเงิน",
        ),
        "benefits.title" => ("Что умеет", "What it does", "ทำอะไรได้บ้าง"),
        "b1.title" => ("Общий и личный бюджет", "Shared and personal budgets", "งบส่วนรวมและส่วนตัว"),
        "b1.text" => (
            "Семья видит общие траты, личные остаются личными. Роли: владелец, редактор, наблюдатель. Пригласить — ссылкой или QR-кодом.",
            "The family sees shared spending, personal stays personal. Roles: owner, editor, viewer. Invite with a link or a QR code.",
            "ครอบครัวเห็นค่าใช้จ่ายร่วม ส่วนตัวยังคงเป็นส่วนตัว บทบาท: เจ้าของ ผู้แก้ไข ผู้ชม เชิญด้วยลิงก์หรือคิวอาร์โค้ด",
        ),
        "b2.title" => ("Расход за 5 секунд", "An expense in 5 seconds", "บันทึกรายจ่ายใน 5 วินาที"),
        "b2.text" => (
            "Сумма, категория, Enter — с любой страницы. Частые категории под рукой, «120+45» посчитается само. Клавиша N на компьютере, «+» на телефоне.",
            "Amount, category, Enter — from any page. Frequent categories at hand, and “120+45” adds up by itself. The N key on desktop, + on the phone.",
            "จำนวน หมวดหมู่ Enter — จากทุกหน้า หมวดที่ใช้บ่อยพร้อมใช้ “120+45” คำนวณให้เอง ปุ่ม N บนคอมพิวเตอร์ ปุ่ม + บนมือถือ",
        ),
        "b3.title" => ("Чек по категориям", "Receipts by category", "แยกใบเสร็จตามหมวด"),
        "b3.text" => (
            "Одна покупка раскладывается на позиции: продукты, бытовая химия, кафе. Возврат уменьшает расходы своей категории.",
            "One purchase splits into items: groceries, household, cafe. A refund reduces spending in its category.",
            "การซื้อครั้งเดียวแยกเป็นรายการ: ของชำ ของใช้ในบ้าน ร้านกาแฟ การคืนเงินจะหักออกจากรายจ่ายของหมวดนั้น",
        ),
        "b4.title" => {
            ("Счета, переводы и обмен валют", "Accounts, transfers and currency exchange", "บัญชี การโอน และการแลกเงิน")
        }
        "b4.text" => (
            "Карта, наличные, копилка. Перевод или обмен с комиссией — одна операция по реальному курсу. Выписка по счёту и сверка остатка с банком.",
            "Card, cash, savings. A transfer or exchange with a fee is one transaction at the real rate. Account statements and balance reconciliation with the bank.",
            "บัตร เงินสด เงินออม การโอนหรือแลกเงินพร้อมค่าธรรมเนียมเป็นรายการเดียวตามอัตราจริง ดูรายการเดินบัญชีและกระทบยอดกับธนาคาร",
        ),
        "b5.title" => ("Лимиты по дереву категорий", "Limits on the category tree", "วงเงินตามโครงสร้างหมวดหมู่"),
        "b5.text" => (
            "Лимит на «Еду» учитывает «Продукты» и «Кафе». Сколько останется — видно ещё до сохранения; предупреждение при 80 % и при превышении.",
            "A Food limit covers Groceries and Cafes. You see what's left before saving; a warning at 80% and when it's exceeded.",
            "วงเงินอาหารรวมของชำและร้านอาหาร เห็นยอดคงเหลือก่อนบันทึก เตือนที่ 80% และเมื่อเกิน",
        ),
        "b6.title" => ("Бюджет недели, месяца и года", "Budget for the week, month and year", "งบรายสัปดาห์ เดือน และปี"),
        "b6.text" => (
            "Факт, план и прогноз по каждой категории — и сколько ещё можно потратить до конца периода. Остатки всех счетов в базовой валюте.",
            "Actual, planned and forecast for every category — and how much you can still spend before the period ends. All account balances in your base currency.",
            "ยอดจริง แผน และคาดการณ์ของทุกหมวด — และยังใช้ได้อีกเท่าไรก่อนสิ้นงวด ยอดคงเหลือทุกบัญชีในสกุลเงินหลัก",
        ),
        "b7.title" => ("Регулярные платежи в календаре", "Regular payments in a calendar", "การชำระประจำในปฏิทิน"),
        "b7.text" => (
            "Аренда, подписки, зарплата и ежемесячный обмен валют видны заранее. «Оплатил» — одно нажатие, сумму можно поправить.",
            "Rent, subscriptions, salary and the monthly currency exchange show up in advance. “Paid” is one tap, and the amount is editable.",
            "ค่าเช่า ค่าสมาชิก เงินเดือน และการแลกเงินรายเดือนเห็นล่วงหน้า กด “ชำระแล้ว” ครั้งเดียว แก้ไขจำนวนได้",
        ),
        "b8.title" => ("Три языка, любые валюты", "Three languages, any currency", "สามภาษา ทุกสกุลเงิน"),
        "b8.text" => (
            "Русский, английский, тайский — интерфейс, категории и описания операций. Бат, рубль, доллар — по курсу на дату операции.",
            "Russian, English, Thai — the interface, categories and transaction notes. Baht, ruble, dollar — at the rate on the transaction date.",
            "ไทย อังกฤษ รัสเซีย — ทั้งหน้าจอ หมวดหมู่ และคำอธิบายรายการ บาท รูเบิล ดอลลาร์ — ตามอัตราของวันที่ทำรายการ",
        ),
        "b9.title" => ("Ошибиться не страшно", "Mistakes are easy to undo", "ผิดพลาดก็แก้ได้"),
        "b9.text" => (
            "Удалили лишнее — «Отменить» в течение 5 секунд, без «Вы уверены?». Тёмная тема, горячие клавиши, на телефоне — как приложение.",
            "Deleted the wrong thing? “Undo” for 5 seconds, no “Are you sure?”. Dark theme, keyboard shortcuts, and it works like an app on your phone.",
            "ลบผิด? กด “เลิกทำ” ได้ภายใน 5 วินาที ไม่ต้องถาม “แน่ใจไหม?” ธีมมืด ปุ่มลัด และใช้บนมือถือได้เหมือนแอป",
        ),
        // Claude (MCP)
        "ai.title" => ("Claude ведёт бюджет за вас", "Claude keeps the budget for you", "Claude ช่วยดูแลงบให้คุณ"),
        "ai.lead" => (
            "Подключите Claude к своему бюджету — и записывайте траты голосом или по фото чека, спрашивайте, сколько осталось, оплачивайте плановые платежи. Прямо из чата.",
            "Connect Claude to your budget — log spending by voice or from a receipt photo, ask what's left and pay scheduled bills. Right from the chat.",
            "เชื่อม Claude กับงบของคุณ — บันทึกรายจ่ายด้วยเสียงหรือรูปใบเสร็จ ถามยอดคงเหลือ และชำระรายการตามแผน ได้จากแชทเลย",
        ),
        "ai1.title" => ("Голосом", "By voice", "ด้วยเสียง"),
        "ai1.text" => (
            "«Кофе 120 бат наличными» — расход записан на «Кафе и рестораны» со счёта «Наличные».",
            "“Coffee, 120 baht, cash” — the expense goes to Cafes & restaurants from the Cash account.",
            "“กาแฟ 120 บาท จ่ายเงินสด” — บันทึกรายจ่ายหมวดร้านอาหารจากบัญชีเงินสด",
        ),
        "ai2.title" => ("По фото чека", "From a receipt photo", "จากรูปใบเสร็จ"),
        "ai2.text" => (
            "Сфотографируйте чек — Claude разложит покупку на позиции по вашим категориям.",
            "Snap a receipt — Claude splits the purchase into items across your categories.",
            "ถ่ายรูปใบเสร็จ — Claude จะแยกรายการตามหมวดหมู่ของคุณ",
        ),
        "ai3.title" => ("Вопросы о деньгах", "Questions about money", "ถามเรื่องเงิน"),
        "ai3.text" => (
            "«Сколько осталось на еду до конца месяца?» — ответ по лимитам, плану и фактам.",
            "“How much is left for food this month?” — answered from your limits, plans and actual spending.",
            "“เดือนนี้เหลืองบค่าอาหารเท่าไร?” — ตอบจากวงเงิน แผน และยอดจริง",
        ),
        "ai4.title" => ("Плановые платежи", "Scheduled payments", "รายการตามแผน"),
        "ai4.text" => (
            "«Оплати аренду» — платёж из календаря отмечен оплаченным, сумму можно поправить.",
            "“Pay the rent” — the payment from the calendar is marked as paid, and the amount can be adjusted.",
            "“จ่ายค่าเช่า” — รายการในปฏิทินถูกทำเครื่องหมายว่าชำระแล้ว แก้ไขจำนวนได้",
        ),
        "ai.works_in" => ("Работает в", "Works in", "ใช้ได้ใน"),
        "ai.phone" => ("Claude на телефоне", "Claude on your phone", "Claude บนมือถือ"),
        // пример переписки: чек Big C из демо-данных (категории — как в дереве по умолчанию), лимит «Еды» — 18 000 ฿
        "chat.label" => ("Пример переписки с Claude", "A sample chat with Claude", "ตัวอย่างแชทกับ Claude"),
        "chat.photo" => ("Чек Big C.jpg", "Big C receipt.jpg", "ใบเสร็จ Big C.jpg"),
        "chat.q1" => ("Запиши, платила картой SCB", "Log this, I paid with the SCB card", "บันทึกให้หน่อย จ่ายด้วยบัตร SCB"),
        "chat.tool" => ("добавлен расход", "expense added", "เพิ่มรายจ่ายแล้ว"),
        "chat.a1" => (
            "Готово: расход 3\u{a0}110\u{a0}฿ с карты SCB.",
            "Done: a ฿3,110 expense from the SCB card.",
            "เรียบร้อย: รายจ่าย ฿3,110 จากบัตร SCB",
        ),
        "chat.l1" => ("Продукты", "Groceries", "ของชำ"),
        "chat.s1" => ("2\u{a0}180\u{a0}฿", "฿2,180", "฿2,180"),
        "chat.l2" => ("Дети", "Kids", "ลูก"),
        "chat.s2" => ("690\u{a0}฿", "฿690", "฿690"),
        "chat.l3" => ("Хозяйство", "Household", "ของใช้ในบ้าน"),
        "chat.s3" => ("240\u{a0}฿", "฿240", "฿240"),
        "chat.q2" => (
            "Сколько осталось на еду до конца месяца?",
            "How much is left for food this month?",
            "เดือนนี้เหลืองบค่าอาหารเท่าไร?",
        ),
        "chat.a2_pre" => ("На «Еду» в этом месяце осталось", "Food has", "หมวดอาหารเดือนนี้เหลือ"),
        "chat.a2_amount" => ("4\u{a0}820\u{a0}฿", "฿4,820", "฿4,820"),
        "chat.a2_post" => (
            "из 18\u{a0}000\u{a0}฿. До конца месяца 15 дней — примерно 320\u{a0}฿ в день.",
            "left of ฿18,000 this month. 15 days to go — about ฿320 a day.",
            "จาก ฿18,000 อีก 15 วันจะสิ้นเดือน — ประมาณวันละ ฿320",
        ),
        "trust1.title" => ("Только то, что вы разрешили", "Only what you allow", "เฉพาะที่คุณอนุญาต"),
        "trust1.text" => (
            "Проект и права вы выбираете в профиле: просмотр, операции, плановые операции, справочники, лимиты.",
            "You choose the project and permissions in your profile: view, transactions, scheduled operations, directories, limits.",
            "คุณเลือกโปรเจกต์และสิทธิ์ในโปรไฟล์: ดูข้อมูล รายการ รายการตามแผน ข้อมูลหลัก วงเงิน",
        ),
        "trust2.title" => ("Без удаления", "No deleting", "ลบไม่ได้"),
        "trust2.text" => (
            "Удалить через Claude ничего нельзя, а права меняются только в браузере.",
            "Nothing can be deleted through Claude, and permissions change only in the browser.",
            "ลบข้อมูลผ่าน Claude ไม่ได้ และเปลี่ยนสิทธิ์ได้เฉพาะในเบราว์เซอร์",
        ),
        "trust3.title" => ("Всё в журнале", "Everything is logged", "บันทึกทุกอย่าง"),
        "trust3.text" => (
            "Каждое изменение через ассистента записывается, отключение доступа действует сразу.",
            "Every change made through the assistant is logged, and revoking access takes effect at once.",
            "ทุกการเปลี่ยนแปลงผ่านผู้ช่วยถูกบันทึก และการยกเลิกสิทธิ์มีผลทันที",
        ),
        "how.title" => ("Как это работает", "How it works", "ใช้งานอย่างไร"),
        "s1.title" => ("Создайте бюджет", "Create a budget", "สร้างงบประมาณ"),
        "s1.text" => (
            "Зарегистрируйтесь по email — личный бюджет создастся сам.",
            "Sign up with your email — your personal budget is created automatically.",
            "สมัครด้วยอีเมล — งบส่วนตัวจะถูกสร้างให้อัตโนมัติ",
        ),
        "s2.title" => ("Пригласите семью", "Invite your family", "เชิญครอบครัว"),
        "s2.text" => (
            "Отправьте ссылку в Telegram или WhatsApp, покажите QR-код или добавьте по email.",
            "Send a link via Telegram or WhatsApp, show a QR code or add members by email.",
            "ส่งลิงก์ทาง Telegram หรือ WhatsApp แสดงคิวอาร์โค้ด หรือเพิ่มด้วยอีเมล",
        ),
        "s3.title" => ("Добавляйте расходы", "Add expenses", "บันทึกรายจ่าย"),
        "s3.text" => (
            "Сами за 5 секунд или через Claude — и смотрите, сколько осталось на месяц по каждой категории.",
            "Yourself in 5 seconds or through Claude — and see what's left for the month in every category.",
            "บันทึกเองใน 5 วินาทีหรือผ่าน Claude — แล้วดูว่าแต่ละหมวดเหลือเท่าไรในเดือนนี้",
        ),
        "sec.title" => ("Безопасность и приватность", "Security and privacy", "ความปลอดภัยและความเป็นส่วนตัว"),
        "sec1.title" => ("Пароли — не у нас", "We don't keep passwords", "เราไม่เก็บรหัสผ่าน"),
        "sec1.text" => (
            "Вход — через отдельный сервер Rauthy. Приложение паролей не видит и не хранит.",
            "Sign-in goes through a separate Rauthy server. The app never sees or stores passwords.",
            "การเข้าสู่ระบบผ่านเซิร์ฟเวอร์ Rauthy แยกต่างหาก แอปไม่เห็นและไม่เก็บรหัสผ่าน",
        ),
        "sec2.title" => ("Второй фактор", "Second factor", "การยืนยันสองชั้น"),
        "sec2.text" => (
            "Код из письма или passkey. Владелец может потребовать 2FA от всех участников семейного бюджета.",
            "An email code or a passkey. An owner can require 2FA from every member of a family budget.",
            "รหัสทางอีเมลหรือพาสคีย์ เจ้าของสามารถบังคับใช้ 2FA กับสมาชิกทุกคนในงบครอบครัว",
        ),
        "sec3.title" => ("Устройства под контролем", "Your devices, under control", "ควบคุมอุปกรณ์ได้"),
        "sec3.text" => (
            "Список устройств, выход на всех других устройствах и журнал безопасности за год — в профиле.",
            "Your devices, signing out of all other devices and a year-long security log — in your profile.",
            "รายการอุปกรณ์ การออกจากระบบในอุปกรณ์อื่นทั้งหมด และบันทึกความปลอดภัยย้อนหลังหนึ่งปี — ในโปรไฟล์",
        ),
        "sec4.title" => ("Без рекламы и трекеров", "No ads, no trackers", "ไม่มีโฆษณาและตัวติดตาม"),
        "sec4.text" => (
            "Данные видят только участники проекта. Резервные копии зашифрованы, аккаунт можно удалить в любой момент.",
            "Only project members see the data. Backups are encrypted, and you can delete your account at any time.",
            "เฉพาะสมาชิกโปรเจกต์เห็นข้อมูล ข้อมูลสำรองเข้ารหัส และลบบัญชีได้ทุกเมื่อ",
        ),
        "demo.title" => ("Посмотрите без регистрации", "Look around without signing up", "ลองดูโดยไม่ต้องสมัคร"),
        "demo.text" => (
            "Демо-семья с двумя месяцами операций: чеки по позициям, переводы и обмен валют, лимиты и календарь платежей. Только просмотр; данные сбрасываются раз в сутки.",
            "A demo family with two months of transactions: itemised receipts, transfers and currency exchange, limits and a payment calendar. View only; the data resets daily.",
            "ครอบครัวตัวอย่างพร้อมรายการสองเดือน: ใบเสร็จแยกรายการ การโอนและแลกเงิน วงเงิน และปฏิทินการชำระ ดูอย่างเดียว ข้อมูลรีเซ็ตทุกวัน",
        ),
        "faq.title" => ("Вопросы и ответы", "Questions and answers", "คำถามที่พบบ่อย"),
        "q1" => (
            "Где хранятся данные и кто их видит?",
            "Where is my data stored and who can see it?",
            "ข้อมูลเก็บที่ไหนและใครเห็นได้บ้าง?",
        ),
        "a1" => (
            "На нашем сервере, резервные копии зашифрованы. Данные проекта видят только его участники. Пароли хранит сервер входа Rauthy — приложение их не видит.",
            "On our server, with encrypted backups. Only a project's members see its data. Passwords are kept by the Rauthy sign-in server — the app never sees them.",
            "เก็บบนเซิร์ฟเวอร์ของเรา สำรองข้อมูลแบบเข้ารหัส เฉพาะสมาชิกโปรเจกต์เห็นข้อมูล รหัสผ่านเก็บที่เซิร์ฟเวอร์ Rauthy แอปไม่เห็น",
        ),
        "q2" => ("Как пригласить семью?", "How do I invite my family?", "เชิญครอบครัวอย่างไร?"),
        "a2" => (
            "Создайте семейный бюджет или сделайте семейным личный, затем пригласите участников ссылкой в мессенджере, QR-кодом или по email. Роли: владелец, редактор, наблюдатель. Ссылка действует 7 дней и подходит одному человеку.",
            "Create a family budget or turn your personal one into a family budget, then invite members with a link in a messenger, a QR code or by email. Roles: owner, editor, viewer. A link is valid for 7 days and works for one person.",
            "สร้างงบครอบครัวหรือเปลี่ยนงบส่วนตัวเป็นงบครอบครัว แล้วเชิญสมาชิกด้วยลิงก์ในแอปแชท คิวอาร์โค้ด หรืออีเมล บทบาท: เจ้าของ ผู้แก้ไข ผู้ชม ลิงก์ใช้ได้ 7 วันและสำหรับหนึ่งคน",
        ),
        "q3" => (
            "Как подключить Claude и что он может сделать с моими данными?",
            "How do I connect Claude, and what can it do with my data?",
            "เชื่อมต่อ Claude อย่างไร และ Claude ทำอะไรกับข้อมูลของฉันได้บ้าง?",
        ),
        "a3" => (
            "В «Профиль → Безопасность → Ассистенты» выберите проект и права, затем добавьте коннектор в Claude. Он сможет смотреть бюджет, добавлять и исправлять операции, вести плановые операции, справочники (валюты, счета, категории, курсы) и лимиты — только то, что вы отметили. Удалять через Claude ничего нельзя.",
            "In Profile → Security → Assistants choose a project and permissions, then add the connector in Claude. It can view the budget, add and edit transactions, manage scheduled operations, directories (currencies, accounts, categories, rates) and limits — only what you ticked. Nothing can be deleted through Claude.",
            "ที่ โปรไฟล์ → ความปลอดภัย → ผู้ช่วย เลือกโปรเจกต์และสิทธิ์ แล้วเพิ่มตัวเชื่อมต่อใน Claude จากนั้น Claude จะดูงบ เพิ่มและแก้ไขรายการ จัดการรายการตามแผน ข้อมูลหลัก (สกุลเงิน บัญชี หมวดหมู่ อัตราแลกเปลี่ยน) และวงเงินได้ — เฉพาะที่คุณเลือก ลบข้อมูลผ่าน Claude ไม่ได้",
        ),
        "q4" => ("Какие валюты поддерживаются?", "Which currencies are supported?", "รองรับสกุลเงินใดบ้าง?"),
        "a4" => (
            "Любые по коду ISO 4217: бат, рубль, доллар, евро и другие. Каждый счёт ведётся в своей валюте, обмен между счетами — одна операция с комиссией и реальным курсом. Бюджет пересчитывается в базовую валюту по курсу на дату операции.",
            "Any ISO 4217 code: baht, ruble, dollar, euro and more. Each account keeps its own currency, and an exchange between accounts is one transaction with the fee and the real rate. The budget converts to your base currency at the rate on the transaction date.",
            "ทุกสกุลตามรหัส ISO 4217: บาท รูเบิล ดอลลาร์ ยูโร และอื่น ๆ แต่ละบัญชีใช้สกุลเงินของตัวเอง การแลกเงินระหว่างบัญชีเป็นรายการเดียวพร้อมค่าธรรมเนียมและอัตราจริง งบจะแปลงเป็นสกุลหลักตามอัตราของวันที่ทำรายการ",
        ),
        "q5" => ("Можно пользоваться с телефона?", "Can I use it on my phone?", "ใช้บนมือถือได้ไหม?"),
        "a5" => (
            "Да: внизу экрана — основные разделы и круглая «+» для расхода. Сайт можно добавить на главный экран, и он откроется как приложение. А через Claude на телефоне траты можно надиктовать.",
            "Yes: the main sections and a round + button for expenses sit at the bottom of the screen. Add the site to your home screen and it opens like an app. And with Claude on your phone you can dictate expenses.",
            "ได้: เมนูหลักและปุ่ม + สำหรับรายจ่ายอยู่ด้านล่างของหน้าจอ เพิ่มเว็บไซต์ไว้ที่หน้าจอหลักแล้วจะเปิดเหมือนแอป และใช้ Claude บนมือถือเพื่อบอกรายจ่ายด้วยเสียงได้",
        ),
        "q6" => ("Можно выгрузить свои данные?", "Can I export my data?", "ส่งออกข้อมูลของฉันได้ไหม?"),
        "a6" => (
            "Да: создайте личный API-токен в «Профиль → Безопасность» и получайте операции, счета и справочники через REST API. Токен работает в одном проекте, с выбранными правами и сроком.",
            "Yes: create a personal API token in Profile → Security and fetch transactions, accounts and directories via the REST API. A token works in one project, with the permissions and expiry you choose.",
            "ได้: สร้างโทเคน API ส่วนตัวที่ โปรไฟล์ → ความปลอดภัย แล้วดึงรายการ บัญชี และข้อมูลอ้างอิงผ่าน REST API โทเคนใช้ได้ในโปรเจกต์เดียว ตามสิทธิ์และอายุที่คุณเลือก",
        ),
        "q7" => ("Как удалить аккаунт?", "How do I delete my account?", "ลบบัญชีอย่างไร?"),
        "a7" => (
            "В профиле нажмите «Удалить аккаунт». Личные данные удаляются в приложении и на сервере входа. Подробнее — на странице «Удаление данных».",
            "Press “Delete account” in your profile. Personal data is deleted in the app and on the sign-in server. See the Data deletion page for details.",
            "กด “ลบบัญชี” ในโปรไฟล์ ข้อมูลส่วนตัวจะถูกลบทั้งในแอปและเซิร์ฟเวอร์เข้าสู่ระบบ ดูรายละเอียดที่หน้า การลบข้อมูล",
        ),
        "final.title" => ("Начните вести бюджет вместе", "Start budgeting together", "เริ่มทำงบด้วยกัน"),
        "final.text" => (
            "Пара минут на настройку — и месяц виден целиком ещё до первой оплаты.",
            "A couple of minutes to set up — and the whole month is visible before the first payment.",
            "ตั้งค่าเพียงไม่กี่นาที แล้วจะเห็นทั้งเดือนก่อนการชำระครั้งแรก",
        ),
        "footer.privacy" => ("Политика конфиденциальности", "Privacy policy", "นโยบายความเป็นส่วนตัว"),
        "footer.terms" => ("Условия использования", "Terms of use", "ข้อกำหนดการใช้งาน"),
        "footer.deletion" => ("Удаление данных", "Data deletion", "การลบข้อมูล"),
        "footer.contacts" => ("Контакты", "Contact", "ติดต่อ"),
        "footer.home" => ("На главную", "Home", "หน้าแรก"),
        "updated" => ("Обновлено", "Updated", "ปรับปรุงเมื่อ"),
        // политика конфиденциальности
        "privacy.title" => ("Политика конфиденциальности", "Privacy policy", "นโยบายความเป็นส่วนตัว"),
        "privacy.intro" => (
            "Мы собираем только то, без чего бюджет не работает, и не передаём данные рекламным сетям.",
            "We collect only what the budget needs to work and never share data with ad networks.",
            "เราเก็บเฉพาะข้อมูลที่จำเป็นต่อการทำงานของงบ และไม่ส่งต่อให้เครือข่ายโฆษณา",
        ),
        "privacy.h1" => ("Какие данные мы храним", "What we store", "ข้อมูลที่เราเก็บ"),
        "privacy.p1" => (
            "Имя, email и аватар из вашего аккаунта входа; язык и тему интерфейса; данные, которые вы вносите сами: проекты, счета, категории, операции, планы и лимиты. Для безопасности — список устройств (браузер, IP) и журнал входов, он хранится один год.",
            "Your name, email and avatar from the sign-in account; interface language and theme; the data you enter: projects, accounts, categories, transactions, plans and limits. For security — a list of devices (browser, IP) and a sign-in log kept for one year.",
            "ชื่อ อีเมล และรูปโปรไฟล์จากบัญชีเข้าสู่ระบบ ภาษาและธีม ข้อมูลที่คุณบันทึกเอง: โปรเจกต์ บัญชี หมวดหมู่ รายการ แผน และวงเงิน เพื่อความปลอดภัย — รายการอุปกรณ์ (เบราว์เซอร์ IP) และบันทึกการเข้าใช้ เก็บไว้หนึ่งปี",
        ),
        "privacy.h2" => ("Пароли и вход", "Passwords and sign-in", "รหัสผ่านและการเข้าสู่ระบบ"),
        "privacy.p2" => (
            "Вход, пароли, второй фактор и привязки GitHub и Facebook обслуживает отдельный сервер входа Rauthy. Приложение не получает и не хранит пароли — только подтверждение, что вы вошли.",
            "Sign-in, passwords, the second factor and GitHub/Facebook links are handled by a separate Rauthy sign-in server. The app never receives or stores passwords — only the confirmation that you signed in.",
            "การเข้าสู่ระบบ รหัสผ่าน ปัจจัยที่สอง และการเชื่อม GitHub/Facebook ดูแลโดยเซิร์ฟเวอร์ Rauthy แยกต่างหาก แอปไม่ได้รับหรือเก็บรหัสผ่าน",
        ),
        "privacy.h3" => ("Кто видит ваши данные", "Who sees your data", "ใครเห็นข้อมูลของคุณ"),
        "privacy.p3" => (
            "Данные проекта видят только его участники в пределах своей роли. Мы не продаём и не передаём данные третьим лицам, на сайте нет сторонних трекеров. Статистика посещений считается без персональных данных.",
            "Only a project's members see its data, within their role. We don't sell or share data with third parties, and the site has no third-party trackers. Visit statistics are counted without personal data.",
            "เฉพาะสมาชิกโปรเจกต์เห็นข้อมูลตามบทบาท เราไม่ขายหรือแบ่งปันข้อมูลกับบุคคลที่สาม ไม่มีตัวติดตามภายนอก สถิติการเข้าชมนับโดยไม่ใช้ข้อมูลส่วนบุคคล",
        ),
        "privacy.h4" => ("Cookie", "Cookies", "คุกกี้"),
        "privacy.p4" => (
            "Только необходимые: сессия входа, выбранный проект и язык. Рекламных и аналитических cookie нет.",
            "Only essential ones: the sign-in session, the selected project and language. No advertising or analytics cookies.",
            "เฉพาะที่จำเป็น: เซสชันเข้าสู่ระบบ โปรเจกต์ที่เลือก และภาษา ไม่มีคุกกี้โฆษณาหรือวิเคราะห์",
        ),
        "privacy.h5" => ("Ваши права", "Your rights", "สิทธิของคุณ"),
        "privacy.p5" => (
            "Вы можете изменить данные в профиле, выгрузить их через API и удалить аккаунт в любой момент (PDPA Таиланда, GDPR). Вопросы — на адрес ниже.",
            "You can edit your data in the profile, export it via the API and delete your account at any time (Thailand's PDPA, GDPR). Questions — to the address below.",
            "คุณแก้ไขข้อมูลในโปรไฟล์ ส่งออกผ่าน API และลบบัญชีได้ทุกเมื่อ (PDPA ของไทย, GDPR) สอบถามที่อีเมลด้านล่าง",
        ),
        // условия использования
        "terms.title" => ("Условия использования", "Terms of use", "ข้อกำหนดการใช้งาน"),
        "terms.intro" => (
            "Пользуясь сервисом, вы соглашаетесь с этими условиями.",
            "By using the service you agree to these terms.",
            "เมื่อใช้บริการ ถือว่าคุณยอมรับข้อกำหนดเหล่านี้",
        ),
        "terms.h1" => ("Сервис", "The service", "บริการ"),
        "terms.p1" => (
            "«Семейный бюджет» помогает вести личные и общие финансы. Это инструмент учёта, а не финансовая или налоговая консультация.",
            "Family budget helps keep personal and shared finances. It's a bookkeeping tool, not financial or tax advice.",
            "งบครอบครัวช่วยจัดการการเงินส่วนตัวและส่วนรวม เป็นเครื่องมือบันทึก ไม่ใช่คำแนะนำทางการเงินหรือภาษี",
        ),
        "terms.h2" => ("Аккаунт и проекты", "Account and projects", "บัญชีและโปรเจกต์"),
        "terms.p2" => (
            "Вы отвечаете за безопасность своего входа и за то, кого приглашаете в проекты. Владелец проекта управляет участниками и может удалить проект.",
            "You're responsible for securing your sign-in and for whom you invite to projects. A project owner manages members and can delete the project.",
            "คุณรับผิดชอบความปลอดภัยของการเข้าสู่ระบบและผู้ที่คุณเชิญ เจ้าของโปรเจกต์จัดการสมาชิกและลบโปรเจกต์ได้",
        ),
        "terms.h3" => ("Допустимое использование", "Acceptable use", "การใช้งานที่ยอมรับได้"),
        "terms.p3" => (
            "Нельзя пытаться получить доступ к чужим данным, перегружать сервис или использовать его для незаконной деятельности.",
            "Don't try to access other people's data, overload the service or use it for unlawful activity.",
            "ห้ามพยายามเข้าถึงข้อมูลของผู้อื่น ทำให้บริการทำงานหนักเกิน หรือใช้เพื่อกิจกรรมที่ผิดกฎหมาย",
        ),
        "terms.h4" => ("Ответственность", "Liability", "ความรับผิดชอบ"),
        "terms.p4" => (
            "Мы делаем ежедневные резервные копии и стараемся, чтобы сервис был доступен, но не гарантируем бесперебойную работу. Изменения условий публикуются на этой странице.",
            "We make daily backups and keep the service available, but can't guarantee uninterrupted operation. Changes to the terms are published on this page.",
            "เราสำรองข้อมูลทุกวันและดูแลให้บริการพร้อมใช้งาน แต่ไม่รับประกันว่าจะไม่หยุดชะงัก การเปลี่ยนแปลงข้อกำหนดจะประกาศที่หน้านี้",
        ),
        // удаление данных
        "deletion.title" => ("Удаление данных", "Data deletion", "การลบข้อมูล"),
        "deletion.intro" => (
            "Вы можете удалить аккаунт и личные данные в любой момент — в том числе если входили через Facebook или GitHub.",
            "You can delete your account and personal data at any time — including if you signed in with Facebook or GitHub.",
            "คุณลบบัญชีและข้อมูลส่วนตัวได้ทุกเมื่อ รวมถึงกรณีที่เข้าสู่ระบบด้วย Facebook หรือ GitHub",
        ),
        "deletion.h1" => ("Как удалить", "How to delete", "วิธีลบ"),
        "deletion.p1" => (
            "Войдите и откройте «Профиль» → «Удаление аккаунта», введите свой email и подтвердите. Если войти не получается — напишите нам с адреса аккаунта.",
            "Sign in, open Profile → Delete account, type your email and confirm. If you can't sign in, email us from the account's address.",
            "เข้าสู่ระบบ เปิด โปรไฟล์ → ลบบัญชี พิมพ์อีเมลแล้วยืนยัน หากเข้าสู่ระบบไม่ได้ ให้อีเมลหาเราจากที่อยู่ของบัญชี",
        ),
        "deletion.h2" => ("Что удаляется", "What is deleted", "อะไรจะถูกลบ"),
        "deletion.p2" => (
            "Ваши личные проекты со всеми операциями, планами и справочниками, сессии и API-токены, пользователь на сервере входа Rauthy вместе с привязками Facebook и GitHub. Журнал безопасности обезличивается.",
            "Your personal projects with all transactions, plans and directories, sessions and API tokens, and your user on the Rauthy sign-in server with its Facebook and GitHub links. The security log is anonymised.",
            "โปรเจกต์ส่วนตัวพร้อมรายการ แผน และข้อมูลอ้างอิงทั้งหมด เซสชันและโทเคน API และผู้ใช้บนเซิร์ฟเวอร์ Rauthy พร้อมการเชื่อม Facebook และ GitHub บันทึกความปลอดภัยจะไม่ระบุตัวตน",
        ),
        "deletion.h3" => ("Семейные проекты", "Family projects", "โปรเจกต์ครอบครัว"),
        "deletion.p3" => (
            "Общие проекты остаются другим участникам. Если вы единственный владелец, сначала назначьте второго владельца или удалите проект.",
            "Shared projects stay with the other members. If you're the only owner, make someone else an owner or delete the project first.",
            "โปรเจกต์ร่วมยังคงอยู่กับสมาชิกคนอื่น หากคุณเป็นเจ้าของคนเดียว ให้ตั้งเจ้าของคนใหม่หรือลบโปรเจกต์ก่อน",
        ),
        "deletion.h4" => ("Сроки", "Timing", "ระยะเวลา"),
        "deletion.p4" => (
            "Данные удаляются сразу; из зашифрованных резервных копий — в течение 30 дней.",
            "Data is deleted immediately; from encrypted backups — within 30 days.",
            "ข้อมูลจะถูกลบทันที และจากข้อมูลสำรองที่เข้ารหัสภายใน 30 วัน",
        ),
        "deletion.done" => (
            "Аккаунт удалён. Спасибо, что пользовались сервисом.",
            "Your account has been deleted. Thank you for using the service.",
            "ลบบัญชีแล้ว ขอบคุณที่ใช้บริการ",
        ),
        "deletion.rauthy_manual" => (
            "Пользователя на сервере входа Rauthy удалите в его личном кабинете или напишите нам.",
            "Delete your user on the Rauthy sign-in server in its account page, or email us.",
            "ลบผู้ใช้บนเซิร์ฟเวอร์ Rauthy ในหน้าบัญชีของ Rauthy หรืออีเมลหาเรา",
        ),
        "contacts.text" => {
            ("Вопросы о данных и удалении:", "Questions about data and deletion:", "สอบถามเรื่องข้อมูลและการลบ:")
        }
        _ => return None,
    })
}
