// «Боли семейного бюджета»: проблема → решение, ~30 с. Первый ролик, образец для новых сценариев.
scenario({
  scenes: [
    { type: 'hook', dur: 2.4, text: {
      ru: ['Зарплата пришла…', '…и куда-то делась', 'Знакомо?'],
      en: ['Payday…', '…and where did it all go?', 'Sound familiar?'],
      th: ['เงินเดือนออกแล้ว…', '…แล้วเงินหายไปไหน?', 'คุ้น ๆ ไหม?'],
    } },
    { type: 'shot', dur: 3.8, img: 'transactions/quick', aspect: 1.1,
      from: { x: 280, y: 70, w: 730 }, to: { x: 385, y: 150, w: 520 },
      rings: [{ x: 401, y: 183, w: 478, h: 64, at: 1.3, ok: true }],
      typing: { x: 410, y: 192, w: 300, h: 48, text: '350', at: 1.55 },
      text: {
        ru: ['Записывать траты — долго и лень', 'Расход за 5 секунд', 'Сумма, категория, Enter — с любой страницы'],
        en: ['Logging expenses is a chore', 'An expense in 5 seconds', 'Amount, category, Enter — from any page'],
        th: ['จดรายจ่ายทีไร ยุ่งยากจนขี้เกียจ', 'บันทึกรายจ่ายใน 5 วินาที', 'จำนวนเงิน หมวดหมู่ กด Enter — ได้จากทุกหน้า'],
      } },
    // Проезд по строкам «Обзора» от названий категорий к красной полосе перерасхода.
    { type: 'shot', dur: 3.8, img: 'overview/overview', aspect: 1.3,
      from: { x: 268, y: 335, w: 600 }, to: { x: 662, y: 335, w: 600 },
      rings: [{ x: 990, y: 731, w: 256, h: 44, at: 2.6 }],
      text: {
        ru: ['О перерасходе узнаёшь в конце месяца', 'Лимиты предупредят заранее', 'Факт, план и прогноз — сколько ещё можно потратить'],
        en: ['You find out you overspent at month-end', 'Limits warn you early', 'Actual, plan and forecast — what’s left to spend'],
        th: ['รู้ว่าใช้เกินงบก็ตอนสิ้นเดือน', 'วงเงินเตือนล่วงหน้า', 'ยอดจริง แผน และคาดการณ์ — รู้ว่ายังใช้ได้อีกเท่าไร'],
      } },
    { type: 'shot', dur: 3.8, img: 'calendar/calendar', aspect: 1.3,
      from: { x: 262, y: 110, w: 900 }, to: { x: 262, y: 354, w: 580 },
      rings: [{ x: 274, y: 581, w: 138, h: 47, at: 2.3 }, { x: 694, y: 581, w: 135, h: 47, at: 2.6 }],
      text: {
        ru: ['Опять забыли про аренду и подписки', 'Все платежи — в календаре', 'Видно заранее, просрочки — красным'],
        en: ['Forgot the rent and subscriptions again', 'Every payment in the calendar', 'See them coming, overdue ones in red'],
        th: ['ลืมจ่ายค่าเช่ากับค่าสมาชิกอีกแล้ว', 'ทุกการชำระอยู่ในปฏิทิน', 'เห็นล่วงหน้า รายการเลยกำหนดเป็นสีแดง'],
      } },
    { type: 'shot', dur: 3.8, img: 'first-budget/family', aspect: 1.3,
      from: { x: 262, y: 30, w: 1000 }, to: { x: 760, y: 60, w: 500 },
      rings: [{ x: 776, y: 252, w: 468, h: 128, at: 2.4, ok: true }],
      text: {
        ru: ['Каждый ведёт деньги в своих заметках', 'Один бюджет на всю семью', 'Пригласите ссылкой или QR-кодом, личное остаётся личным'],
        en: ['Everyone tracks money in their own notes', 'One budget for the whole family', 'Invite by link or QR code — personal stays personal'],
        th: ['ต่างคนต่างจดเงินในโน้ตของตัวเอง', 'งบเดียวสำหรับทั้งครอบครัว', 'เชิญด้วยลิงก์หรือคิวอาร์โค้ด เรื่องส่วนตัวยังเป็นส่วนตัว'],
      } },
    { type: 'shot', dur: 3.8, img: 'accounts/accounts', aspect: 1.3,
      from: { x: 262, y: 60, w: 720 }, to: { x: 262, y: 90, w: 560 }, coins: ['฿', '₽', '$'],
      text: {
        ru: ['Карты, наличные, баты, рубли — каша', 'Все счета и валюты в одном месте', 'Остатки в базовой валюте, обмен — по реальному курсу'],
        en: ['Cards, cash, baht, roubles — a mess', 'All accounts and currencies in one place', 'Balances in your base currency, exchange at the real rate'],
        th: ['บัตร เงินสด บาท รูเบิล — ปนกันวุ่น', 'ทุกบัญชีทุกสกุลเงินในที่เดียว', 'ยอดคงเหลือในสกุลเงินหลัก แลกเงินตามอัตราจริง'],
      } },
    { type: 'chat', dur: 4.8, text: {
      ru: { pain: 'Нет времени даже открыть приложение', title: 'Просто скажите Claude — он запишет', messages: [
        { receipt: 'Чек Big C', total: '3 110 ฿', at: 1.0 },
        { me: 'Запиши, платила картой SCB', at: 1.35 },
        { tool: 'add_expense · добавлен расход', at: 1.9 },
        { bot: 'Готово: расход 3 110 ฿ с карты SCB.', at: 2.2 },
        { me: 'Сколько осталось на еду до конца месяца?', at: 2.9 },
        { bot: 'На «Еду» осталось <b>4 820 ฿</b> из 18 000 ฿ — примерно 320 ฿ в день.', at: 3.5 },
      ] },
      en: { pain: 'No time to even open the app', title: 'Just tell Claude — it logs it for you', messages: [
        { receipt: 'Big C receipt', total: '฿3,110', at: 1.0 },
        { me: 'Log this, I paid with the SCB card', at: 1.35 },
        { tool: 'add_expense · expense added', at: 1.9 },
        { bot: 'Done: a ฿3,110 expense from the SCB card.', at: 2.2 },
        { me: 'How much is left for food this month?', at: 2.9 },
        { bot: 'Food has <b>฿4,820</b> left of ฿18,000 — about ฿320 a day.', at: 3.5 },
      ] },
      th: { pain: 'ไม่มีเวลาแม้แต่จะเปิดแอป', title: 'แค่บอก Claude แล้วจะบันทึกให้เอง', messages: [
        { receipt: 'ใบเสร็จ Big C', total: '฿3,110', at: 1.0 },
        { me: 'บันทึกให้หน่อย จ่ายด้วยบัตร SCB', at: 1.35 },
        { tool: 'add_expense · เพิ่มรายจ่ายแล้ว', at: 1.9 },
        { bot: 'เรียบร้อย: รายจ่าย ฿3,110 จากบัตร SCB', at: 2.2 },
        { me: 'เดือนนี้เหลืองบค่าอาหารเท่าไร?', at: 2.9 },
        { bot: 'หมวดอาหารเหลือ <b>฿4,820</b> จาก ฿18,000 — ประมาณวันละ ฿320', at: 3.5 },
      ] },
    } },
    { type: 'cta', dur: 3.6, text: {
      ru: { name: 'Семейный бюджет', tag: 'Деньги под контролем — всей семьёй', btn: 'Демо — без регистрации' },
      en: { name: 'Family budget', tag: 'Money under control — together', btn: 'Try the demo — no sign-up' },
      th: { name: 'งบประมาณครอบครัว', tag: 'คุมเงินไปด้วยกันทั้งครอบครัว', btn: 'ลองเดโม — ไม่ต้องสมัคร' },
    } },
  ],
});
