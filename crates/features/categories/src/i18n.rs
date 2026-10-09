//! Строки интерфейса фичи «Категории».
use kernel::i18n::Tr;

pub fn dict(key: &str) -> Option<Tr> {
    Some(match key {
        "nav" => ("Категории", "Categories", "หมวดหมู่"),
        "title" => ("Категории", "Categories", "หมวดหมู่"),
        "new" => ("Новая категория", "New category", "หมวดหมู่ใหม่"),
        "edit" => ("Категория", "Category", "หมวดหมู่"),
        "expenses" => ("Расходы", "Expenses", "รายจ่าย"),
        "incomes" => ("Доходы", "Income", "รายรับ"),
        "parent" => ("Родитель", "Parent", "หมวดหมู่หลัก"),
        "root" => ("— корень —", "— top level —", "— ระดับบนสุด —"),
        "sort_order" => ("Порядок", "Order", "ลำดับ"),
        "add_child" => ("+ подкатегория", "+ subcategory", "+ หมวดหมู่ย่อย"),
        "code_hint" => ("Латиница, без пробелов: FOOD_CAFE", "Latin, no spaces: FOOD_CAFE", "อักษรละติน ไม่มีช่องว่าง: FOOD_CAFE"),
        "tree_hint" => ("Лимиты и бюджет учитывают дерево: расход в подкатегории входит в сумму родителя.",
                        "Limits and budget follow the tree: spending in a subcategory counts toward its parent.",
                        "งบและวงเงินคิดตามโครงสร้าง: รายจ่ายในหมวดย่อยนับรวมในหมวดหลัก"),
        _ => return None,
    })
}
