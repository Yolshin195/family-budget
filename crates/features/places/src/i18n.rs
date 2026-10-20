//! Строки интерфейса и ошибки фичи «Места».
use kernel::i18n::Tr;

pub fn dict(key: &str) -> Option<Tr> {
    Some(match key {
        "nav" => ("Места", "Places", "สถานที่"),
        "title" => ("Места", "Places", "สถานที่"),
        "new" => ("Новое место", "New place", "สถานที่ใหม่"),
        "edit_title" => ("Место", "Place", "สถานที่"),
        "intro" => (
            "Магазины, кафе и рынки, где вы платите. Место подставляет в операцию категорию и счёт: выберите его в поиске быстрого ввода. Конкретный магазин можно вложить в сеть — он возьмёт у неё категорию и счёт.",
            "Shops, cafés and markets where you pay. A place fills in the category and account: pick it in the quick entry search. Put a specific shop inside its chain, and it takes the chain's category and account.",
            "ร้านค้า คาเฟ่ และตลาดที่คุณจ่ายเงิน สถานที่จะใส่หมวดหมู่และบัญชีให้ เลือกได้ในช่องค้นหาของการบันทึกด่วน ใส่สาขาไว้ในเครือร้านได้ แล้วสาขาจะใช้หมวดหมู่และบัญชีของเครือ",
        ),
        "code_hint" => ("Латиница, цифры и _: SEVEN_ELEVEN", "Latin letters, digits and _: SEVEN_ELEVEN", "อักษรละติน ตัวเลข และ _: SEVEN_ELEVEN"),
        "name_hint" => ("Как на вывеске; не переводится", "As on the sign; not translated", "ตามป้ายร้าน ไม่ต้องแปล"),
        "aliases" => ("Варианты написания", "Other spellings", "ชื่อเรียกอื่น"),
        "aliases_hint" => ("Через запятую — для поиска: 7-11, เซเว่น", "Comma-separated, for search: 7-11, เซเว่น", "คั่นด้วยจุลภาค ใช้สำหรับค้นหา: 7-11, เซเว่น"),
        "parent" => ("Входит в", "Part of", "อยู่ใน"),
        "parent_hint" => (
            "Сеть или торговый центр: магазин берёт у неё категорию и счёт, если свои не выбраны",
            "A chain or mall: the shop takes its category and account unless you pick others",
            "เครือร้านหรือห้าง: ร้านจะใช้หมวดหมู่และบัญชีของเครือ ถ้าไม่ได้เลือกเอง",
        ),
        "root" => ("— верхний уровень —", "— top level —", "— ระดับบนสุด —"),
        "add_child" => ("магазин", "branch", "สาขา"),
        "inherit" => ("— как у «{name}» —", "— same as “{name}” —", "— เหมือน “{name}” —"),
        "inherited" => ("от сети", "from the chain", "จากเครือ"),
        "location" => ("Филиал или адрес", "Branch or address", "สาขาหรือที่อยู่"),
        "location_hint" => ("Чтобы различать филиалы: Сукхумвит 24", "To tell branches apart: Sukhumvit 24", "เพื่อแยกสาขา: สุขุมวิท 24"),
        "place_kind" => ("Тип", "Type", "ประเภท"),
        "kind.SHOP" => ("Магазин", "Shop", "ร้านค้า"),
        "kind.CAFE" => ("Кафе", "Café", "คาเฟ่"),
        "kind.MARKET" => ("Рынок", "Market", "ตลาด"),
        "kind.ONLINE" => ("Онлайн", "Online", "ออนไลน์"),
        "kind.SERVICE" => ("Услуги", "Services", "บริการ"),
        "kind.OTHER" => ("Другое", "Other", "อื่น ๆ"),
        "defaults" => ("Подставляет", "Fills in", "ใส่ให้"),
        "default_category" => ("Категория по умолчанию", "Default category", "หมวดหมู่เริ่มต้น"),
        "default_account" => ("Счёт по умолчанию", "Default account", "บัญชีเริ่มต้น"),
        "defaults_hint" => (
            "Подставляются в операцию с этим местом, если их не выбрали",
            "Used in a transaction at this place unless you pick others",
            "ใช้ในรายการของสถานที่นี้ ถ้าไม่ได้เลือกเอง",
        ),
        "none" => ("— нет —", "— none —", "— ไม่มี —"),
        "empty_title" => ("Мест пока нет", "No places yet", "ยังไม่มีสถานที่"),
        "empty_text" => (
            "Добавьте «7-Eleven» с категорией «Продукты» — и в быстром вводе одно касание заполнит категорию и счёт.",
            "Add 7-Eleven with the Groceries category, and one tap in quick entry fills in the category and account.",
            "เพิ่ม 7-Eleven พร้อมหมวด ของชำ แล้วแตะครั้งเดียวในการบันทึกด่วนก็ใส่หมวดหมู่และบัญชีให้",
        ),
        // Ошибки (техдолг #9): на языке запроса.
        "err.code_taken" => ("код «{code}» уже занят", "the code “{code}” is already taken", "รหัส “{code}” ถูกใช้แล้ว"),
        "err.not_found" => ("место «{code}» не найдено", "place “{code}” not found", "ไม่พบสถานที่ “{code}”"),
        "err.too_long" => ("не длиннее {max} знаков", "{max} characters at most", "ไม่เกิน {max} ตัวอักษร"),
        "err.too_many_aliases" => ("не больше {max} вариантов", "{max} spellings at most", "ไม่เกิน {max} ชื่อ"),
        "err.parent_self" => ("место не может входить само в себя", "a place cannot be part of itself", "สถานที่อยู่ในตัวเองไม่ได้"),
        "err.parent_cycle" => (
            "нельзя перенести место внутрь его собственного магазина",
            "a place cannot be moved inside its own branch",
            "ย้ายสถานที่เข้าไปในสาขาของตัวเองไม่ได้",
        ),
        "err.parent_archived" => ("«{name}» в архиве", "“{name}” is archived", "“{name}” ถูกเก็บถาวรแล้ว"),
        "err.depth" => ("не больше {max} уровней вложенности", "{max} levels of nesting at most", "ซ้อนได้ไม่เกิน {max} ระดับ"),
        "err.archive_children" => (
            "В «{name}» есть активные магазины — сначала архивируйте или перенесите их",
            "“{name}” has active branches — archive or move them first",
            "“{name}” มีสาขาที่ใช้งานอยู่ — เก็บถาวรหรือย้ายสาขาก่อน",
        ),
        "err.restore_parent" => (
            "Чтобы вернуть «{name}», сначала верните из архива «{parent}»",
            "To restore “{name}”, restore “{parent}” first",
            "หากต้องการกู้คืน “{name}” ให้กู้คืน “{parent}” ก่อน",
        ),
        "err.kind" => ("тип — одно из: {values}", "the type is one of: {values}", "ประเภทต้องเป็นหนึ่งใน: {values}"),
        _ => return None,
    })
}
