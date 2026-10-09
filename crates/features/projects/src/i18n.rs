//! Строки интерфейса фичи «Проекты».
use kernel::i18n::Tr;

pub fn dict(key: &str) -> Option<Tr> {
    Some(match key {
        "title" => ("Проекты и участники", "Projects & members", "โปรเจกต์และสมาชิก"),
        "login" => ("Вход", "Sign in", "เข้าสู่ระบบ"),
        "register" => ("Регистрация", "Sign up", "สมัครสมาชิก"),
        "email" => ("Email", "Email", "อีเมล"),
        "password" => ("Пароль", "Password", "รหัสผ่าน"),
        "your_name" => ("Имя", "Your name", "ชื่อ"),
        "base_currency" => ("Базовая валюта", "Base currency", "สกุลเงินหลัก"),
        "no_account" => ("Нет аккаунта?", "No account?", "ยังไม่มีบัญชี?"),
        "have_account" => ("Уже есть аккаунт?", "Already registered?", "มีบัญชีแล้ว?"),
        "demo_hint" => ("Демо: aleksey@demo.local или anna@demo.local, пароль demo123",
                        "Demo: aleksey@demo.local or anna@demo.local, password demo123",
                        "เดโม: aleksey@demo.local หรือ anna@demo.local รหัสผ่าน demo123"),
        "tagline" => ("Общий и личный бюджет семьи на трёх языках", "Shared and personal family budget in three languages", "งบครอบครัวทั้งส่วนรวมและส่วนตัว สามภาษา"),
        "PERSONAL" => ("Личный", "Personal", "ส่วนตัว"),
        "FAMILY" => ("Семейный", "Family", "ครอบครัว"),
        "OWNER" => ("Владелец", "Owner", "เจ้าของ"),
        "EDITOR" => ("Редактор", "Editor", "ผู้แก้ไข"),
        "VIEWER" => ("Наблюдатель", "Viewer", "ผู้ชม"),
        "role" => ("Роль", "Role", "บทบาท"),
        "members" => ("Участники", "Members", "สมาชิก"),
        "add_member" => ("Добавить участника", "Add member", "เพิ่มสมาชิก"),
        "add_member_hint" => ("Человек должен быть зарегистрирован.", "The person must be registered.", "ผู้ใช้ต้องสมัครสมาชิกก่อน"),
        "new_project" => ("Новый проект", "New project", "โปรเจกต์ใหม่"),
        "current" => ("активный", "active", "กำลังใช้"),
        "switch" => ("Перейти", "Switch", "สลับ"),
        "leave" => ("Выйти из проекта", "Leave project", "ออกจากโปรเจกต์"),
        "remove" => ("Убрать", "Remove", "ลบ"),
        "you" => ("вы", "you", "คุณ"),
        "rename" => ("Переименовать", "Rename", "เปลี่ยนชื่อ"),
        _ => return None,
    })
}
