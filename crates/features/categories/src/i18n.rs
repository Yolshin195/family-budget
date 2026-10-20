//! Строки интерфейса фичи «Категории».
use kernel::i18n::Tr;

pub fn dict(key: &str) -> Option<Tr> {
    Some(match key {
        "nav" => ("Категории", "Categories", "หมวดหมู่"),
        "title" => ("Категории", "Categories", "หมวดหมู่"),
        "new" => ("Новая категория", "New category", "หมวดหมู่ใหม่"),
        "edit_title" => ("Категория", "Category", "หมวดหมู่"),
        "expenses" => ("Расходы", "Expenses", "รายจ่าย"),
        "incomes" => ("Доходы", "Income", "รายรับ"),
        "parent" => ("Родитель", "Parent", "หมวดหมู่หลัก"),
        "root" => ("— корень —", "— top level —", "— ระดับบนสุด —"),
        "sort_order" => ("Порядок", "Order", "ลำดับ"),
        "add_child" => ("+ подкатегория", "+ subcategory", "+ หมวดหมู่ย่อย"),
        "code_hint" => ("Латиница, цифры и _: FOOD_CAFE", "Latin letters, digits and _: FOOD_CAFE", "อักษรละติน ตัวเลข และ _: FOOD_CAFE"),
        "tree_hint" => ("Лимиты и бюджет учитывают дерево: расход в подкатегории входит в сумму родителя.",
                        "Limits and budget follow the tree: spending in a subcategory counts toward its parent.",
                        "งบและวงเงินคิดตามโครงสร้าง: รายจ่ายในหมวดย่อยนับรวมในหมวดหลัก"),
        "err.archive_children" => (
            "У категории «{name}» есть подкатегории — сначала архивируйте их",
            "Category “{name}” has subcategories — archive them first",
            "หมวดหมู่ “{name}” มีหมวดหมู่ย่อย — เก็บถาวรหมวดย่อยก่อน",
        ),
        "err.restore_parent" => (
            "Чтобы вернуть «{name}», сначала верните из архива «{parent}»",
            "To restore “{name}”, restore “{parent}” first",
            "หากต้องการกู้คืน “{name}” ให้กู้คืน “{parent}” ก่อน",
        ),
        "empty_title" => ("Категорий пока нет", "No categories yet", "ยังไม่มีหมวดหมู่"),
        "empty_text" => ("Категории — дерево до 5 уровней: «Еда → Кафе». Лимит на родителя учитывает все подкатегории.", "Categories form a tree up to 5 levels: Food → Cafe. A parent's limit covers all subcategories.", "หมวดหมู่เป็นโครงสร้างได้ถึง 5 ระดับ วงเงินของหมวดหลักครอบคลุมหมวดย่อยทั้งหมด"),
        _ => return None,
    })
}
