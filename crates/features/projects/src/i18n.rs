//! Строки интерфейса фичи «Проекты»: вход, онбординг, профиль, безопасность, проекты и участники.
use kernel::i18n::Tr;

pub fn dict(key: &str) -> Option<Tr> {
    Some(match key {
        "title" => ("Проекты и участники", "Projects & members", "โปรเจกต์และสมาชิก"),
        "tagline" => {
            ("Семейный бюджет, который ведут вместе", "A family budget you keep together", "งบครอบครัวที่ดูแลร่วมกัน")
        }
        // вход
        "login" => ("Вход", "Sign in", "เข้าสู่ระบบ"),
        "login_rauthy" => ("Войти", "Sign in", "เข้าสู่ระบบ"),
        "login_failed" => ("Не удалось войти", "Sign-in failed", "เข้าสู่ระบบไม่สำเร็จ"),
        "login_failed_hint" => (
            "Начните вход заново. Если ошибка повторяется — проверьте, что в браузере разрешены cookie.",
            "Start signing in again. If it keeps failing, make sure cookies are allowed.",
            "เริ่มเข้าสู่ระบบใหม่ หากยังไม่ได้ ตรวจสอบว่าเบราว์เซอร์อนุญาตคุกกี้",
        ),
        "try_again" => ("Войти заново", "Sign in again", "เข้าสู่ระบบอีกครั้ง"),
        "oidc_off" => (
            "Вход через Rauthy не настроен: задайте OIDC_ISSUER, OIDC_CLIENT_ID и OIDC_CLIENT_SECRET.",
            "Rauthy sign-in isn't configured: set OIDC_ISSUER, OIDC_CLIENT_ID and OIDC_CLIENT_SECRET.",
            "ยังไม่ได้ตั้งค่าการเข้าสู่ระบบผ่าน Rauthy: กำหนด OIDC_ISSUER, OIDC_CLIENT_ID และ OIDC_CLIENT_SECRET",
        ),
        "dev_login" => ("Вход для разработки", "Developer sign-in", "เข้าสู่ระบบสำหรับนักพัฒนา"),
        "dev_login_hint" => (
            "Только в режиме разработки (APP_ENV=dev). В продакшене вход — через Rauthy.",
            "Development mode only (APP_ENV=dev). In production, sign-in goes through Rauthy.",
            "เฉพาะโหมดพัฒนา (APP_ENV=dev) ในระบบจริงใช้ Rauthy",
        ),
        "dev_as" => ("Войти как", "Sign in as", "เข้าสู่ระบบเป็น"),
        "logout_confirm" => {
            ("Выйти из приложения и из Rauthy?", "Sign out of the app and Rauthy?", "ออกจากระบบแอปและ Rauthy?")
        }
        "email" => ("Email", "Email", "อีเมล"),
        // онбординг
        "onboarding" => ("Добро пожаловать!", "Welcome!", "ยินดีต้อนรับ!"),
        "onboarding_hint" => (
            "Три вопроса — и личный бюджет готов. Семью пригласите потом в «Проекты и участники».",
            "Three questions and your personal budget is ready. Invite your family later in Projects & members.",
            "ตอบสามข้อแล้วงบส่วนตัวจะพร้อม เชิญครอบครัวภายหลังได้ที่ โปรเจกต์และสมาชิก",
        ),
        "your_name" => ("Как вас зовут", "Your name", "ชื่อของคุณ"),
        "base_currency" => ("Базовая валюта", "Base currency", "สกุลเงินหลัก"),
        "base_currency_hint" => (
            "В ней считается бюджет. Код ISO 4217: THB, RUB, USD…",
            "The budget is counted in it. ISO 4217 code: THB, RUB, USD…",
            "ใช้คำนวณงบ รหัส ISO 4217: THB, RUB, USD…",
        ),
        "interface_lang" => ("Язык интерфейса", "Interface language", "ภาษาของหน้าจอ"),
        "start" => ("Создать бюджет", "Create budget", "สร้างงบประมาณ"),
        // проекты
        "PERSONAL" => ("Личный", "Personal", "ส่วนตัว"),
        "FAMILY" => ("Семейный", "Family", "ครอบครัว"),
        "role" => ("Роль", "Role", "บทบาท"),
        "members" => ("Участники", "Members", "สมาชิก"),
        "new_project" => ("Новый проект", "New project", "โปรเจกต์ใหม่"),
        "new_project_hint" => (
            "Семейный проект — общий бюджет на несколько человек. Вы станете владельцем.",
            "A family project is a shared budget for several people. You'll be the owner.",
            "โปรเจกต์ครอบครัวคืองบร่วมของหลายคน คุณจะเป็นเจ้าของ",
        ),
        "current" => ("активный", "active", "กำลังใช้"),
        "switch" => ("Перейти", "Switch", "สลับ"),
        "leave" => ("Выйти из проекта", "Leave project", "ออกจากโปรเจกต์"),
        "leave_confirm" => (
            "Вы потеряете доступ к данным проекта. Вернуть его сможет только владелец.",
            "You'll lose access to the project's data. Only the owner can add you back.",
            "คุณจะไม่เห็นข้อมูลของโปรเจกต์ มีเพียงเจ้าของที่เพิ่มคุณกลับได้",
        ),
        "leave_yes" => ("Да, выйти", "Yes, leave", "ใช่ ออก"),
        "remove" => ("Убрать", "Remove", "นำออก"),
        "you" => ("вы", "you", "คุณ"),
        "settings_project" => ("Настройки проекта", "Project settings", "ตั้งค่าโปรเจกต์"),
        "rename" => ("Переименовать", "Rename", "เปลี่ยนชื่อ"),
        "require_2fa" => ("Требовать 2FA от участников", "Require 2FA from members", "บังคับให้สมาชิกใช้ 2FA"),
        "require_2fa_hint" => (
            "Участник без второго фактора (код из письма или passkey) увидит просьбу включить 2FA вместо данных.",
            "Members without a second factor (email code or passkey) see a request to enable 2FA instead of data.",
            "สมาชิกที่ไม่มีปัจจัยที่สองจะเห็นคำขอให้เปิด 2FA แทนข้อมูล",
        ),
        "require_2fa_on" => ("2FA обязательна", "2FA required", "บังคับ 2FA"),
        "danger_zone" => ("Удаление проекта", "Delete project", "ลบโปรเจกต์"),
        "delete_project_hint" => (
            "Удалятся все операции, планы, лимиты и справочники проекта. Действие необратимо. Введите название проекта для подтверждения.",
            "All transactions, plans, limits and directories of the project will be deleted. This can't be undone. Type the project name to confirm.",
            "รายการ แผน วงเงิน และข้อมูลอ้างอิงทั้งหมดจะถูกลบถาวร พิมพ์ชื่อโปรเจกต์เพื่อยืนยัน",
        ),
        "delete_project" => ("Удалить проект навсегда", "Delete project forever", "ลบโปรเจกต์ถาวร"),
        "project_deleted" => ("Проект удалён", "Project deleted", "ลบโปรเจกต์แล้ว"),
        "left_project" => ("Вы вышли из проекта", "You left the project", "คุณออกจากโปรเจกต์แล้ว"),
        // приглашение по ссылке и QR-коду
        "invite_title" => ("Пригласить в семью", "Invite your family", "เชิญครอบครัว"),
        "invite_hint" => (
            "Создайте ссылку и отправьте её в мессенджер или покажите QR-код — человек войдёт или зарегистрируется и сразу окажется в бюджете.",
            "Create a link and send it in a messenger or show the QR code — the person signs in or signs up and lands right in the budget.",
            "สร้างลิงก์แล้วส่งทางแชตหรือให้สแกน QR — ผู้รับเข้าสู่ระบบหรือสมัครแล้วเข้างบได้ทันที",
        ),
        "invite_create" => ("Создать приглашение", "Create invite", "สร้างคำเชิญ"),
        "invite_link" => ("Приглашение", "Invite", "คำเชิญ"),
        "invite_until" => ("до", "until", "ถึง"),
        "invite_qr" => ("QR-код приглашения", "Invite QR code", "QR คำเชิญ"),
        "invite_scan" => (
            "Наведите камеру телефона на QR-код или отправьте ссылку:",
            "Point a phone camera at the QR code or send the link:",
            "สแกน QR ด้วยกล้องโทรศัพท์หรือส่งลิงก์:",
        ),
        "invite_share" => ("Поделиться", "Share", "แชร์"),
        "invite_share_text" => {
            ("Присоединяйся к нашему бюджету «{project}»", "Join our budget “{project}”", "มาร่วมงบ “{project}” ของเรา")
        }
        "invite_once" => (
            "Действует 7 дней для одного человека. Ссылку переслали не тому — отзовите её.",
            "Valid for 7 days, for one person. Sent it to the wrong person? Revoke it.",
            "ใช้ได้ 7 วัน สำหรับหนึ่งคน หากส่งผิดคน ให้เพิกถอน",
        ),
        "invite_revoke" => ("Отозвать приглашение", "Revoke invite", "เพิกถอนคำเชิญ"),
        "invite_revoked" => ("Приглашение отозвано", "Invite revoked", "เพิกถอนคำเชิญแล้ว"),
        "make_family" => ("Сделать бюджет семейным", "Make it a family budget", "เปลี่ยนเป็นงบครอบครัว"),
        "make_family_hint" => (
            "Личный бюджет — только для вас. Сделайте его семейным, чтобы пригласить близких: они увидят все операции и счета этого бюджета.",
            "A personal budget is just for you. Make it a family budget to invite your family: they'll see all transactions and accounts in it.",
            "งบส่วนตัวใช้ได้คนเดียว เปลี่ยนเป็นงบครอบครัวเพื่อเชิญคนในบ้าน พวกเขาจะเห็นทุกรายการและบัญชีในงบนี้",
        ),
        "made_family" => (
            "Теперь это семейный бюджет — пригласите близких",
            "It's a family budget now — invite your family",
            "เป็นงบครอบครัวแล้ว เชิญคนในบ้านได้เลย",
        ),
        "joined" => ("Добро пожаловать в семейный бюджет!", "Welcome to the family budget!", "ยินดีต้อนรับสู่งบครอบครัว!"),
        "join_title" => ("Приглашение", "Invitation", "คำเชิญ"),
        "join_from" => ("Вас приглашает", "Invited by", "เชิญโดย"),
        "join_hint" => (
            "Вас зовут вести этот бюджет вместе. Ваша роль —",
            "You're invited to keep this budget together. Your role:",
            "คุณได้รับเชิญให้ดูแลงบนี้ร่วมกัน บทบาทของคุณ:",
        ),
        "join_as_EDITOR" => {
            ("редактор: добавляете операции и планы", "editor: add transactions and plans", "ผู้แก้ไข: เพิ่มรายการและแผน")
        }
        "join_as_VIEWER" => ("наблюдатель: только просмотр", "viewer: read only", "ผู้ชม: ดูอย่างเดียว"),
        "join_as_OWNER" => ("владелец", "owner", "เจ้าของ"),
        "join_accept" => ("Присоединиться", "Join", "เข้าร่วม"),
        "join_register" => ("Создать аккаунт и присоединиться", "Sign up and join", "สมัครและเข้าร่วม"),
        "join_login" => ("У меня уже есть аккаунт", "I already have an account", "ฉันมีบัญชีแล้ว"),
        "join_login_hint" => (
            "После входа вы вернётесь сюда — останется нажать «Присоединиться».",
            "After signing in you'll come back here — just press Join.",
            "หลังเข้าสู่ระบบ คุณจะกลับมาที่นี่ แล้วกด เข้าร่วม",
        ),
        "join_member" => {
            ("Вы уже участник этого бюджета.", "You're already a member of this budget.", "คุณเป็นสมาชิกของงบนี้อยู่แล้ว")
        }
        "join_open" => ("Открыть бюджет", "Open budget", "เปิดงบ"),
        "join_used" => (
            "Этим приглашением уже воспользовались. Попросите новое у того, кто вас пригласил.",
            "This invite has already been used. Ask the person who invited you for a new one.",
            "คำเชิญนี้ถูกใช้แล้ว ขอคำเชิญใหม่จากผู้เชิญ",
        ),
        "join_expired" => (
            "Срок приглашения истёк. Попросите новое у того, кто вас пригласил.",
            "This invite has expired. Ask the person who invited you for a new one.",
            "คำเชิญหมดอายุแล้ว ขอคำเชิญใหม่จากผู้เชิญ",
        ),
        "join_revoked" => (
            "Приглашение отозвано. Попросите новое у того, кто вас пригласил.",
            "This invite was revoked. Ask the person who invited you for a new one.",
            "คำเชิญถูกเพิกถอนแล้ว ขอคำเชิญใหม่จากผู้เชิญ",
        ),
        "join_missing" => (
            "Ссылка неверная или приглашение удалено. Проверьте, что ссылка скопирована целиком, или попросите новую.",
            "The link is wrong or the invite was deleted. Make sure the whole link was copied, or ask for a new one.",
            "ลิงก์ไม่ถูกต้องหรือคำเชิญถูกลบ ตรวจสอบว่าคัดลอกลิงก์ครบ หรือขอลิงก์ใหม่",
        ),
        "join_home" => ("На главную", "Go to home page", "ไปหน้าแรก"),
        // профиль
        "profile" => ("Профиль", "Profile", "โปรไฟล์"),
        "profile_hint" => (
            "Имя видят участники ваших проектов.",
            "Members of your projects see this name.",
            "สมาชิกในโปรเจกต์ของคุณจะเห็นชื่อนี้",
        ),
        "theme" => ("Тема", "Theme", "ธีม"),
        "security" => ("Безопасность", "Security", "ความปลอดภัย"),
        "security_hint" => (
            "Устройства, API-токены и журнал входов.",
            "Devices, API tokens and sign-in log.",
            "อุปกรณ์ โทเคน API และบันทึกการเข้าใช้",
        ),
        "rauthy_account" => {
            ("Пароль, 2FA, GitHub и Facebook", "Password, 2FA, GitHub and Facebook", "รหัสผ่าน 2FA GitHub และ Facebook")
        }
        "rauthy_account_hint" => (
            "Пароль, второй фактор (код по email, passkey) и привязки GitHub и Facebook — в личном кабинете Rauthy. Рекомендуем добавить passkey: код по email не защитит, если взломана сама почта.",
            "Password, second factor (email code, passkey) and GitHub/Facebook links live in your Rauthy account. We recommend adding a passkey: an email code won't help if the mailbox itself is hacked.",
            "รหัสผ่าน ปัจจัยที่สอง (รหัสทางอีเมล, passkey) และการเชื่อม GitHub/Facebook อยู่ในบัญชี Rauthy แนะนำให้เพิ่ม passkey เพราะรหัสทางอีเมลไม่ช่วยหากอีเมลถูกเจาะ",
        ),
        "open_rauthy" => ("Открыть кабинет Rauthy", "Open Rauthy account", "เปิดบัญชี Rauthy"),
        "devices" => ("Устройства", "Devices", "อุปกรณ์"),
        "this_device" => ("это устройство", "this device", "อุปกรณ์นี้"),
        "last_seen" => ("Последняя активность", "Last active", "ใช้งานล่าสุด"),
        "signed_in" => ("Вход", "Signed in", "เข้าสู่ระบบ"),
        "with_2fa" => ("со вторым фактором", "with second factor", "ด้วยปัจจัยที่สอง"),
        "revoke" => ("Завершить", "Sign out", "ออกจากระบบ"),
        "logout_all" => {
            ("Выйти на всех других устройствах", "Sign out of all other devices", "ออกจากระบบในอุปกรณ์อื่นทั้งหมด")
        }
        "logged_out_all" => ("Другие сессии завершены", "Other sessions signed out", "ออกจากระบบเซสชันอื่นแล้ว"),
        "unknown_device" => ("Неизвестный браузер", "Unknown browser", "เบราว์เซอร์ที่ไม่รู้จัก"),
        "tokens" => ("API-токены", "API tokens", "โทเคน API"),
        "tokens_hint" => (
            "Для клиентов без браузера: синхронизация, Telegram-бот. Токен действует в одном проекте с выбранными правами.",
            "For clients without a browser: sync apps, a Telegram bot. A token works in one project with chosen permissions.",
            "สำหรับไคลเอนต์ที่ไม่ใช่เบราว์เซอร์ โทเคนใช้ได้ในโปรเจกต์เดียวตามสิทธิ์ที่เลือก",
        ),
        "token_name" => ("Название", "Name", "ชื่อ"),
        "token_project" => ("Проект", "Project", "โปรเจกต์"),
        "token_scopes" => ("Права", "Permissions", "สิทธิ์"),
        "token_expires" => ("Срок", "Expires", "หมดอายุ"),
        "token_days_30" => ("30 дней", "30 days", "30 วัน"),
        "token_days_90" => ("90 дней", "90 days", "90 วัน"),
        "token_days_365" => ("1 год", "1 year", "1 ปี"),
        "token_days_0" => ("Бессрочно", "Never", "ไม่หมดอายุ"),
        "scope_read" => ("Чтение всех данных", "Read all data", "อ่านข้อมูลทั้งหมด"),
        "scope_write" => ("Изменение всех данных", "Change all data", "แก้ไขข้อมูลทั้งหมด"),
        "scope_transactions_write" => ("Только добавлять операции", "Add transactions only", "เพิ่มรายการเท่านั้น"),
        "create_token" => ("Выпустить токен", "Create token", "สร้างโทเคน"),
        "token_created" => (
            "Токен создан. Скопируйте его сейчас — больше он не покажется.",
            "Token created. Copy it now — it won't be shown again.",
            "สร้างโทเคนแล้ว คัดลอกตอนนี้ — จะไม่แสดงอีก",
        ),
        "token_usage" => ("Заголовок запроса", "Request header", "ส่วนหัวคำขอ"),
        "copy" => ("Скопировать", "Copy", "คัดลอก"),
        "copied" => ("Скопировано", "Copied", "คัดลอกแล้ว"),
        "no_tokens" => ("Токенов нет", "No tokens", "ไม่มีโทเคน"),
        "revoked" => ("отозван", "revoked", "เพิกถอนแล้ว"),
        "expired" => ("истёк", "expired", "หมดอายุแล้ว"),
        "never_used" => ("не использовался", "never used", "ยังไม่เคยใช้"),
        "revoke_token" => ("Отозвать", "Revoke", "เพิกถอน"),
        "audit" => ("Журнал безопасности", "Security log", "บันทึกความปลอดภัย"),
        "audit_hint" => (
            "Входы, участники и роли, токены — за последний год. События 2FA ведёт Rauthy.",
            "Sign-ins, members and roles, tokens — for the last year. 2FA events are logged by Rauthy.",
            "การเข้าใช้ สมาชิกและบทบาท โทเคน — ย้อนหลังหนึ่งปี เหตุการณ์ 2FA บันทึกโดย Rauthy",
        ),
        "no_events" => ("Событий пока нет", "No events yet", "ยังไม่มีเหตุการณ์"),
        "act.login" => ("Вход", "Sign-in", "เข้าสู่ระบบ"),
        "act.login_failed" => ("Неудачный вход", "Failed sign-in", "เข้าสู่ระบบไม่สำเร็จ"),
        "act.logout" => ("Выход", "Sign-out", "ออกจากระบบ"),
        "act.logout_all" => ("Выход на всех устройствах", "Signed out everywhere", "ออกจากระบบทุกอุปกรณ์"),
        "act.session_revoked" => ("Сессия завершена", "Session ended", "สิ้นสุดเซสชัน"),
        "act.project_created" => ("Создан проект", "Project created", "สร้างโปรเจกต์"),
        "act.project_deleted" => ("Удалён проект", "Project deleted", "ลบโปรเจกต์"),
        // Добавления по email больше нет; ключ — для старых записей журнала.
        "act.member_added" => ("Добавлен участник", "Member added", "เพิ่มสมาชิก"),
        "act.member_role_changed" => ("Изменена роль", "Role changed", "เปลี่ยนบทบาท"),
        "act.member_removed" => ("Участник удалён", "Member removed", "นำสมาชิกออก"),
        "act.member_joined" => ("Вход в проект по приглашению", "Joined via invite", "เข้าร่วมด้วยคำเชิญ"),
        "act.invite_created" => ("Создано приглашение", "Invite created", "สร้างคำเชิญ"),
        "act.invite_revoked" => ("Отозвано приглашение", "Invite revoked", "เพิกถอนคำเชิญ"),
        "act.project_made_family" => ("Проект стал семейным", "Project made family", "เปลี่ยนเป็นโปรเจกต์ครอบครัว"),
        "act.require_2fa_changed" => ("Изменено требование 2FA", "2FA requirement changed", "เปลี่ยนข้อกำหนด 2FA"),
        "act.api_token_created" => ("Выпущен API-токен", "API token created", "สร้างโทเคน API"),
        "act.api_token_revoked" => ("Отозван API-токен", "API token revoked", "เพิกถอนโทเคน API"),
        "act.account_deleted" => ("Аккаунт удалён", "Account deleted", "ลบบัญชี"),
        "act.mcp_access_granted" => {
            ("Ассистенту открыт доступ (MCP)", "Assistant access granted (MCP)", "ให้สิทธิ์ผู้ช่วย (MCP)")
        }
        "act.mcp_access_revoked" => {
            ("Доступ ассистента отключён (MCP)", "Assistant access revoked (MCP)", "ยกเลิกสิทธิ์ผู้ช่วย (MCP)")
        }
        "act.mcp_write" => ("Изменение через ассистента (MCP)", "Change made by assistant (MCP)", "แก้ไขโดยผู้ช่วย (MCP)"),
        "assistants" => ("Ассистенты (MCP)", "Assistants (MCP)", "ผู้ช่วย (MCP)"),
        "assistants_hint" => (
            "Доступ Claude и других AI-ассистентов к бюджету: проект, права, подключение.",
            "Access for Claude and other AI assistants: project, permissions, connection.",
            "การเข้าถึงของ Claude และผู้ช่วย AI: โปรเจกต์ สิทธิ์ การเชื่อมต่อ",
        ),
        "open_assistants" => ("Настроить", "Configure", "ตั้งค่า"),
        "delete_account" => ("Удаление аккаунта", "Delete account", "ลบบัญชี"),
        "delete_account_hint" => (
            "Удалятся ваши личные проекты со всеми данными, вы выйдете из семейных проектов, пользователь будет удалён и в Rauthy. Если вы единственный владелец семейного проекта с другими участниками — сначала назначьте второго владельца. Введите свой email для подтверждения.",
            "Your personal projects and all their data are deleted, you leave family projects, and your Rauthy user is deleted too. If you're the only owner of a family project with other members, make someone else an owner first. Type your email to confirm.",
            "โปรเจกต์ส่วนตัวและข้อมูลทั้งหมดจะถูกลบ คุณจะออกจากโปรเจกต์ครอบครัว และบัญชี Rauthy จะถูกลบ หากคุณเป็นเจ้าของคนเดียวของโปรเจกต์ครอบครัวที่มีสมาชิกอื่น ให้ตั้งเจ้าของคนที่สองก่อน พิมพ์อีเมลเพื่อยืนยัน",
        ),
        "delete_account_btn" => ("Удалить аккаунт навсегда", "Delete my account forever", "ลบบัญชีถาวร"),
        "confirm_email_mismatch" => (
            "confirm: введите свой email для подтверждения",
            "confirm: type your email to confirm",
            "confirm: พิมพ์อีเมลเพื่อยืนยัน",
        ),
        _ => return None,
    })
}
