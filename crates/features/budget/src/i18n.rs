//! Строки интерфейса фичи «Бюджет».
use kernel::i18n::Tr;

pub fn dict(key: &str) -> Option<Tr> {
    Some(match key {
        "nav" => ("Бюджет", "Budget", "งบประมาณ"),
        "title" => ("Бюджет", "Budget", "งบประมาณ"),
        "limit" => ("Лимит", "Limit", "วงเงิน"),
        "fact" => ("Факт", "Actual", "จริง"),
        "plan" => ("План", "Planned", "ตามแผน"),
        "forecast" => ("Прогноз", "Forecast", "คาดการณ์"),
        "available" => ("Доступно", "Available", "คงเหลือใช้ได้"),
        "expenses" => ("Расходы", "Expenses", "รายจ่าย"),
        "incomes" => ("Доходы", "Income", "รายรับ"),
        "net" => ("Итог периода (прогноз)", "Period net (forecast)", "สุทธิ (คาดการณ์)"),
        "accounts" => ("Остатки на конец периода", "Balances at period end", "ยอดคงเหลือสิ้นงวด"),
        "upcoming" => ("Ещё предстоит по планам", "Still planned", "ยังรอตามแผน"),
        "nothing_planned" => ("Все плановые операции периода решены", "All planned items of the period are settled", "รายการตามแผนของงวดนี้ครบแล้ว"),
        "missing_rates" => ("Нет курса к базовой валюте — суммы посчитаны 1:1:", "No rate to base currency — counted 1:1:", "ไม่มีอัตราแลกเปลี่ยน — คิด 1:1:"),
        "children_over" => ("Сумма лимитов подкатегорий больше лимита категории", "Subcategory limits exceed this category's limit", "วงเงินหมวดย่อยรวมเกินวงเงินหมวดนี้"),
        "inherited" => ("ограничено лимитом родителя", "limited by parent's limit", "จำกัดโดยวงเงินหมวดหลัก"),
        "all_members" => ("Все участники", "All members", "สมาชิกทั้งหมด"),
        "base_currency" => ("Суммы в базовой валюте", "Amounts in base currency", "จำนวนเงินในสกุลหลัก"),
        "expand" => ("Развернуть всё", "Expand all", "ขยายทั้งหมด"),
        "set_limits" => ("Настроить лимиты", "Set limits", "ตั้งวงเงิน"),
        _ => return None,
    })
}
