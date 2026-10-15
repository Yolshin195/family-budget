//! Строки страницы «Ассистенты (MCP)».
use kernel::i18n::Tr;

pub fn dict(key: &str) -> Option<Tr> {
    Some(match key {
        "title" => ("Ассистенты (MCP)", "Assistants (MCP)", "ผู้ช่วย (MCP)"),
        "profile" => ("Профиль", "Profile", "โปรไฟล์"),
        "security" => ("Безопасность", "Security", "ความปลอดภัย"),
        "intro" => (
            "Claude может вести бюджет за вас: записывать расходы голосом или по фото чека, отвечать, сколько потрачено и сколько осталось. Он работает под вашим аккаунтом, только в выбранном проекте и только с отмеченными правами. Удалять через ассистента ничего нельзя, а права меняются только здесь.",
            "Claude can keep the budget for you: record expenses by voice or from a receipt photo, tell you how much was spent and what is left. It acts under your account, only in the chosen project and only with the permissions you tick. Nothing can be deleted through the assistant, and permissions are changed only here.",
            "Claude ช่วยจัดการงบให้คุณได้: บันทึกรายจ่ายด้วยเสียงหรือรูปใบเสร็จ บอกยอดใช้จ่ายและยอดคงเหลือ โดยทำงานภายใต้บัญชีของคุณ เฉพาะโปรเจกต์และสิทธิ์ที่เลือก ลบข้อมูลผ่านผู้ช่วยไม่ได้ และเปลี่ยนสิทธิ์ได้ที่นี่เท่านั้น",
        ),
        "access" => ("Доступ Claude", "Claude access", "สิทธิ์ของ Claude"),
        "status_on" => ("Доступ открыт", "Access granted", "เปิดสิทธิ์แล้ว"),
        "status_off" => ("Доступ не открыт", "No access yet", "ยังไม่ได้เปิดสิทธิ์"),
        "status_off_hint" => (
            "Пока доступ не открыт, подключённый Claude видит только подсказку открыть эту страницу.",
            "Until access is granted, a connected Claude only sees a hint to open this page.",
            "ก่อนเปิดสิทธิ์ Claude ที่เชื่อมต่อแล้วจะเห็นเพียงคำแนะนำให้เปิดหน้านี้",
        ),
        "project" => ("Проект", "Project", "โปรเจกต์"),
        "permissions" => ("Права", "Permissions", "สิทธิ์"),
        "granted_on" => ("Открыт", "Granted", "เปิดเมื่อ"),
        "last_used" => ("Последнее обращение", "Last used", "ใช้ล่าสุด"),
        "never" => ("ещё не было", "not yet", "ยังไม่เคย"),
        "project_gone" => ("проект недоступен", "project unavailable", "ไม่มีสิทธิ์ในโปรเจกต์"),
        "scope.read" => ("Просмотр всего", "View everything", "ดูทั้งหมด"),
        "scope.transactions:write" => ("Операции: добавлять и исправлять", "Transactions: add and edit", "รายการ: เพิ่มและแก้ไข"),
        "scope.planning:write" => ("Плановые платежи: оплачивать и пропускать", "Scheduled payments: pay and skip", "รายการตามแผน: ชำระและข้าม"),
        "scope_read_always" => ("всегда", "always", "เสมอ"),
        "viewer_hint" => ("Роль VIEWER в проекте — только просмотр.", "VIEWER role in the project — view only.", "บทบาท VIEWER ในโปรเจกต์ — ดูอย่างเดียว"),
        "no_delete" => ("Удаление через ассистента недоступно.", "Deleting through the assistant is not available.", "ลบผ่านผู้ช่วยไม่ได้"),
        "save" => ("Сохранить доступ", "Save access", "บันทึกสิทธิ์"),
        "revoke" => ("Отключить", "Revoke", "ยกเลิกสิทธิ์"),
        "saved" => ("Доступ ассистента сохранён", "Assistant access saved", "บันทึกสิทธิ์ผู้ช่วยแล้ว"),
        "revoked" => ("Доступ ассистента отключён", "Assistant access revoked", "ยกเลิกสิทธิ์ผู้ช่วยแล้ว"),
        "no_such_project" => ("Выберите проект из списка", "Choose a project from the list", "เลือกโปรเจกต์จากรายการ"),
        "connect" => ("Как подключить", "How to connect", "วิธีเชื่อมต่อ"),
        "connect_claude" => ("Claude на сайте, в приложении на телефоне и компьютере", "Claude on the web, phone and desktop apps", "Claude บนเว็บ แอปโทรศัพท์ และคอมพิวเตอร์"),
        "step1" => (
            "claude.ai → «Настроить» → «Коннекторы» → «+» → «Добавить свой коннектор».",
            "claude.ai → Customize → Connectors → “+” → Add custom connector.",
            "claude.ai → Customize → Connectors → “+” → Add custom connector",
        ),
        "step2" => ("Адрес сервера:", "Server URL:", "URL ของเซิร์ฟเวอร์:"),
        "step3" => (
            "В «Дополнительных настройках» выберите свой OAuth-клиент и укажите Client ID (секрет оставьте пустым):",
            "In advanced settings choose your own OAuth client and enter the Client ID (leave the secret empty):",
            "ในการตั้งค่าขั้นสูง เลือก OAuth client ของคุณเองและใส่ Client ID (เว้นว่าง secret):",
        ),
        "step4" => (
            "Нажмите «Подключить» и войдите своим аккаунтом. Коннектор сразу появится и в приложении Claude на телефоне.",
            "Click Connect and sign in with your account. The connector also appears in the Claude phone app.",
            "กด Connect แล้วเข้าสู่ระบบด้วยบัญชีของคุณ ตัวเชื่อมต่อจะปรากฏในแอป Claude บนโทรศัพท์ด้วย",
        ),
        "oauth_off" => (
            "Вход через провайдера не настроен на этом сервере — подключение через claude.ai недоступно. Используйте личный API-токен в Claude Code.",
            "Sign-in provider is not configured on this server — connecting from claude.ai is unavailable. Use a personal API token in Claude Code.",
            "เซิร์ฟเวอร์นี้ยังไม่ได้ตั้งค่าผู้ให้บริการเข้าสู่ระบบ — ใช้โทเคน API ส่วนตัวใน Claude Code",
        ),
        "connect_code" => ("Claude Code (с личным API-токеном)", "Claude Code (with a personal API token)", "Claude Code (ด้วยโทเคน API ส่วนตัว)"),
        "code_hint" => (
            "Создайте токен в «Безопасности» (проект и права — у токена) и выполните:",
            "Create a token in Security (the token defines project and permissions) and run:",
            "สร้างโทเคนในหน้าความปลอดภัย (โปรเจกต์และสิทธิ์อยู่ที่โทเคน) แล้วรัน:",
        ),
        "to_tokens" => ("К API-токенам", "API tokens", "โทเคน API"),
        "guest" => ("Демо-гостю ассистенты недоступны.", "Assistants are not available to the demo guest.", "ผู้เยี่ยมชมเดโมใช้ผู้ช่วยไม่ได้"),
        "copy" => ("Копировать", "Copy", "คัดลอก"),
        "copied" => ("Скопировано", "Copied", "คัดลอกแล้ว"),
        _ => return None,
    })
}
