//! Строки интерфейса и ошибки фичи «Люди».
use kernel::i18n::Tr;

pub fn dict(key: &str) -> Option<Tr> {
    Some(match key {
        "nav" => ("Люди", "People", "บุคคล"),
        "title" => ("Люди", "People", "บุคคล"),
        "new" => ("Новый человек", "New person", "บุคคลใหม่"),
        "person" => ("Человек", "Person", "บุคคล"),
        "person_name" => ("Имя", "Name", "ชื่อ"),
        "intro" => (
            "Для кого покупки: участники проекта и те, у кого нет аккаунта, — дети, няня. В подробной карточке операции у позиции можно выбрать «Для кого»; по умолчанию — «Для всех».",
            "Who purchases are for: project members and people without an account, such as kids or a nanny. In the full transaction details, each item can have “For whom”; the default is “For everyone”.",
            "ซื้อให้ใคร: สมาชิกโปรเจกต์และคนที่ไม่มีบัญชี เช่น ลูกหรือพี่เลี้ยง ในรายละเอียดรายการ แต่ละรายการเลือก “สำหรับใคร” ได้ ค่าเริ่มต้นคือ “สำหรับทุกคน”",
        ),
        "account_col" => ("Аккаунт", "Account", "บัญชีผู้ใช้"),
        "member" => ("Участник проекта", "Project member", "สมาชิกโปรเจกต์"),
        "no_account" => ("Без аккаунта", "No account", "ไม่มีบัญชีผู้ใช้"),
        "account_deleted" => ("Аккаунт удалён", "Account deleted", "ลบบัญชีผู้ใช้แล้ว"),
        "you" => ("это вы", "you", "คุณ"),
        "former_member" => ("Удалённый участник", "Former member", "อดีตสมาชิก"),
        "code_hint" => {
            ("Латиница, цифры и _: PINPAN", "Latin letters, digits and _: PINPAN", "อักษรละติน ตัวเลข และ _: PINPAN")
        }
        "name_hint" => {
            ("Как вы его называете; не переводится", "As you call them; not translated", "ชื่อที่คุณเรียก ไม่ต้องแปล")
        }
        "former_hint" => (
            "Участник удалил аккаунт, его имя стёрто. Пустое имя — «Удалённый участник»",
            "The member deleted their account and their name was erased. An empty name shows “Former member”",
            "สมาชิกลบบัญชีแล้วและชื่อถูกลบ ถ้าเว้นว่างจะแสดง “อดีตสมาชิก”",
        ),
        "aliases" => ("Другие имена", "Other names", "ชื่ออื่น"),
        "aliases_hint" => (
            "Через запятую — для поиска: Пинпан, ปิ่นปั่น",
            "Comma-separated, for search: Pinpan, ปิ่นปั่น",
            "คั่นด้วยจุลภาค ใช้สำหรับค้นหา: Pinpan, ปิ่นปั่น",
        ),
        "member_hint" => (
            "Запись участника проекта создаётся сама, когда он вступает в проект, и уходит в архив, когда выходит. Имя можно поправить — в профиле оно не изменится.",
            "A project member’s record is created when they join the project and archived when they leave. You can edit the name here; their profile stays the same.",
            "บันทึกของสมาชิกสร้างขึ้นเองเมื่อเข้าร่วมโปรเจกต์ และเก็บถาวรเมื่อออก แก้ชื่อที่นี่ได้ โปรไฟล์ของเขาจะไม่เปลี่ยน",
        ),
        "empty_text" => (
            "Добавьте тех, у кого нет аккаунта, — например, ребёнка: тогда у покупки в подробной карточке можно выбрать «Для кого».",
            "Add people without an account, such as a child: then you can choose “For whom” for a purchase in the full details.",
            "เพิ่มคนที่ไม่มีบัญชี เช่น ลูก แล้วจะเลือก “สำหรับใคร” ให้รายการซื้อในรายละเอียดได้",
        ),
        // Ошибки (техдолг #9): на языке запроса.
        "err.code_taken" => ("код «{code}» уже занят", "the code “{code}” is already taken", "รหัส “{code}” ถูกใช้แล้ว"),
        "err.code_reserved" => (
            "код «{code}» зарезервирован: так в запросах обозначают «Для всех»",
            "the code “{code}” is reserved: it means “For everyone” in requests",
            "รหัส “{code}” สงวนไว้ หมายถึง “สำหรับทุกคน” ในคำขอ",
        ),
        "err.not_found" => ("человек «{code}» не найден", "person “{code}” not found", "ไม่พบบุคคล “{code}”"),
        "err.too_long" => ("не длиннее {max} знаков", "{max} characters at most", "ไม่เกิน {max} ตัวอักษร"),
        "err.too_many_aliases" => ("не больше {max} имён", "{max} names at most", "ไม่เกิน {max} ชื่อ"),
        "err.member_archive" => (
            "запись участника проекта уходит в архив сама, когда он выходит из проекта",
            "a project member’s record is archived automatically when they leave the project",
            "บันทึกของสมาชิกจะเก็บถาวรเองเมื่อออกจากโปรเจกต์",
        ),
        _ => return None,
    })
}
