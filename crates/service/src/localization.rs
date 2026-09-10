use serde_json::Value;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Language {
    Russian,
    English,
    Arabic,
    Spanish,
    German,
    French,
}

fn current_language() -> Language {
    let preference = std::env::var_os("APPDATA")
        .map(std::path::PathBuf::from)
        .map(|path| path.join("EvertyDisplay").join("ui-settings.json"))
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|value| value.get("language")?.as_str().map(str::to_owned));

    match preference.as_deref() {
        Some("russian") => Language::Russian,
        Some("english") => Language::English,
        Some("arabic") => Language::Arabic,
        Some("spanish") => Language::Spanish,
        Some("german") => Language::German,
        Some("french") => Language::French,
        _ => system_language(),
    }
}

fn system_language() -> Language {
    let lang_id = unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() };
    match lang_id & 0x03ff {
        0x01 => Language::Arabic,
        0x0a => Language::Spanish,
        0x07 => Language::German,
        0x0c => Language::French,
        0x19 => Language::Russian,
        _ => Language::English,
    }
}

pub fn translate(input: impl Into<String>) -> String {
    let input = input.into();
    match current_language() {
        Language::Russian => input,
        Language::English => translate_dynamic(
            &input,
            &[
                ("Экран", "Display"),
                ("Гц", "Hz"),
                ("Окно приложения", "Application window"),
            ],
            english_exact(&input),
        ),
        Language::Arabic => translate_dynamic(
            &input,
            &[
                ("Экран", "الشاشة"),
                ("Гц", "هرتز"),
                ("Окно приложения", "نافذة التطبيق"),
            ],
            arabic_exact(&input),
        ),
        Language::Spanish => translate_dynamic(
            &input,
            &[
                ("Экран", "Pantalla"),
                ("Гц", "Hz"),
                ("Окно приложения", "Ventana de aplicación"),
            ],
            spanish_exact(&input),
        ),
        Language::German => translate_dynamic(
            &input,
            &[
                ("Экран", "Bildschirm"),
                ("Гц", "Hz"),
                ("Окно приложения", "Anwendungsfenster"),
            ],
            german_exact(&input),
        ),
        Language::French => translate_dynamic(
            &input,
            &[
                ("Экран", "Écran"),
                ("Гц", "Hz"),
                ("Окно приложения", "Fenêtre d’application"),
            ],
            french_exact(&input),
        ),
    }
}

fn translate_dynamic(input: &str, replacements: &[(&str, &str)], exact: Option<&str>) -> String {
    if let Some(exact) = exact {
        return exact.to_string();
    }
    replacements
        .iter()
        .fold(input.to_string(), |text, (from, to)| text.replace(from, to))
}

fn english_exact(input: &str) -> Option<&'static str> {
    Some(match input {
        "⏸ Пауза переходов: ВКЛ" => "⏸ Display transitions: PAUSED",
        "Переключение мыши временно отключено" => {
            "Mouse transitions are temporarily disabled"
        }
        "▶ Переходы мыши: АКТИВНЫ" => "▶ Mouse transitions: ACTIVE",
        "Переключение мыши возобновлено" => {
            "Mouse transitions have resumed"
        }
        "🚀 Автозагрузка: ВКЛ" => "🚀 Start with Windows: ON",
        "Multitor будет запускаться вместе с Windows" => {
            "EvertyDisplay will start with Windows"
        }
        "🚀 Автозагрузка: ВЫКЛ" => "🚀 Start with Windows: OFF",
        "Multitor удален из автозагрузки" => {
            "EvertyDisplay was removed from startup"
        }
        "📺 Режим PiP: ВКЛ" => "📺 PiP mode: ON",
        "Превью виртуального экрана (Win+Alt+V)" => {
            "Virtual display preview (Win+Alt+V)"
        }
        "📺 Режим PiP: ВЫКЛ" => "📺 PiP mode: OFF",
        "Превью скрыто" => "Preview hidden",
        "⚡ Окно перенесено" | "🪟 Окно перенесено" => {
            "🪟 Window moved"
        }
        "📺 PiP скрыт" => "📺 PiP hidden",
        "Правая кнопка мыши → Показать PiP" => "Right-click → Show PiP",
        "🎮 Игровой режим" => "🎮 Gaming Mode",
        "Полноэкранная игра — переходы на паузе" => {
            "Fullscreen game — transitions paused"
        }
        "🎮 Игровой режим выключен" => "🎮 Gaming Mode off",
        "Переходы мыши снова активны" => {
            "Mouse transitions are active again"
        }
        _ => return None,
    })
}

fn arabic_exact(input: &str) -> Option<&'static str> {
    Some(match input {
        "⏸ Пауза переходов: ВКЛ" => "⏸ انتقال الشاشة: متوقف",
        "Переключение мыши временно отключено" => {
            "تم إيقاف انتقال المؤشر مؤقتًا"
        }
        "▶ Переходы мыши: АКТИВНЫ" => "▶ انتقال المؤشر: نشط",
        "Переключение мыши возобновлено" => "تم استئناف انتقال المؤشر",
        "🚀 Автозагрузка: ВКЛ" => "🚀 التشغيل مع Windows: مفعّل",
        "Multitor будет запускаться вместе с Windows" => {
            "سيبدأ EvertyDisplay مع Windows"
        }
        "🚀 Автозагрузка: ВЫКЛ" => "🚀 التشغيل مع Windows: متوقف",
        "Multitor удален из автозагрузки" => {
            "تمت إزالة EvertyDisplay من بدء التشغيل"
        }
        "📺 Режим PiP: ВКЛ" => "📺 وضع PiP: مفعّل",
        "Превью виртуального экрана (Win+Alt+V)" => {
            "معاينة الشاشة الافتراضية (Win+Alt+V)"
        }
        "📺 Режим PiP: ВЫКЛ" => "📺 وضع PiP: متوقف",
        "Превью скрыто" => "المعاينة مخفية",
        "⚡ Окно перенесено" | "🪟 Окно перенесено" => {
            "🪟 تم نقل النافذة"
        }
        "📺 PiP скрыт" => "📺 تم إخفاء PiP",
        "Правая кнопка мыши → Показать PiP" => {
            "زر الفأرة الأيمن ← إظهار PiP"
        }
        "🎮 Игровой режим" => "🎮 وضع الألعاب",
        "Полноэкранная игра — переходы на паузе" => {
            "لعبة بملء الشاشة — الانتقالات متوقفة"
        }
        "🎮 Игровой режим выключен" => "🎮 وضع الألعاب متوقف",
        "Переходы мыши снова активны" => "انتقالات المؤشر نشطة مجددًا",
        _ => return None,
    })
}

fn spanish_exact(input: &str) -> Option<&'static str> {
    Some(match input {
        "⏸ Пауза переходов: ВКЛ" => "⏸ Transiciones de pantalla: EN PAUSA",
        "Переключение мыши временно отключено" => {
            "Las transiciones del ratón están desactivadas temporalmente"
        }
        "▶ Переходы мыши: АКТИВНЫ" => "▶ Transiciones del ratón: ACTIVAS",
        "Переключение мыши возобновлено" => {
            "Las transiciones del ratón se han reanudado"
        }
        "🚀 Автозагрузка: ВКЛ" => "🚀 Iniciar con Windows: ACTIVADO",
        "Multitor будет запускаться вместе с Windows" => {
            "EvertyDisplay se iniciará con Windows"
        }
        "🚀 Автозагрузка: ВЫКЛ" => "🚀 Iniciar con Windows: DESACTIVADO",
        "Multitor удален из автозагрузки" => {
            "EvertyDisplay se eliminó del inicio de Windows"
        }
        "📺 Режим PiP: ВКЛ" => "📺 Modo PiP: ACTIVADO",
        "Превью виртуального экрана (Win+Alt+V)" => {
            "Vista previa de la pantalla virtual (Win+Alt+V)"
        }
        "📺 Режим PiP: ВЫКЛ" => "📺 Modo PiP: DESACTIVADO",
        "Превью скрыто" => "Vista previa oculta",
        "⚡ Окно перенесено" | "🪟 Окно перенесено" => {
            "🪟 Ventana movida"
        }
        "📺 PiP скрыт" => "📺 PiP oculto",
        "Правая кнопка мыши → Показать PiP" => {
            "Botón derecho del ratón → Mostrar PiP"
        }
        "🎮 Игровой режим" => "🎮 Modo de juego",
        "Полноэкранная игра — переходы на паузе" => {
            "Juego a pantalla completa: transiciones en pausa"
        }
        "🎮 Игровой режим выключен" => "🎮 Modo de juego desactivado",
        "Переходы мыши снова активны" => {
            "Las transiciones del ratón vuelven a estar activas"
        }
        _ => return None,
    })
}

fn german_exact(input: &str) -> Option<&'static str> {
    Some(match input {
        "⏸ Пауза переходов: ВКЛ" => "⏸ Bildschirmübergänge: PAUSIERT",
        "Переключение мыши временно отключено" => {
            "Mausübergänge sind vorübergehend deaktiviert"
        }
        "▶ Переходы мыши: АКТИВНЫ" => "▶ Mausübergänge: AKTIV",
        "Переключение мыши возобновлено" => {
            "Mausübergänge wurden fortgesetzt"
        }
        "🚀 Автозагрузка: ВКЛ" => "🚀 Mit Windows starten: EIN",
        "Multitor будет запускаться вместе с Windows" => {
            "EvertyDisplay wird mit Windows gestartet"
        }
        "🚀 Автозагрузка: ВЫКЛ" => "🚀 Mit Windows starten: AUS",
        "Multitor удален из автозагрузки" => {
            "EvertyDisplay wurde aus dem Autostart entfernt"
        }
        "📺 Режим PiP: ВКЛ" => "📺 PiP-Modus: EIN",
        "Превью виртуального экрана (Win+Alt+V)" => {
            "Vorschau des virtuellen Bildschirms (Win+Alt+V)"
        }
        "📺 Режим PiP: ВЫКЛ" => "📺 PiP-Modus: AUS",
        "Превью скрыто" => "Vorschau ausgeblendet",
        "⚡ Окно перенесено" | "🪟 Окно перенесено" => {
            "🪟 Fenster verschoben"
        }
        "📺 PiP скрыт" => "📺 PiP ausgeblendet",
        "Правая кнопка мыши → Показать PiP" => {
            "Rechtsklick → PiP anzeigen"
        }
        "🎮 Игровой режим" => "🎮 Spielmodus",
        "Полноэкранная игра — переходы на паузе" => {
            "Vollbildspiel — Übergänge pausiert"
        }
        "🎮 Игровой режим выключен" => "🎮 Spielmodus aus",
        "Переходы мыши снова активны" => "Mausübergänge sind wieder aktiv",
        _ => return None,
    })
}

fn french_exact(input: &str) -> Option<&'static str> {
    Some(match input {
        "⏸ Пауза переходов: ВКЛ" => "⏸ Transitions d’écran : EN PAUSE",
        "Переключение мыши временно отключено" => {
            "Les transitions de la souris sont temporairement désactivées"
        }
        "▶ Переходы мыши: АКТИВНЫ" => "▶ Transitions de la souris : ACTIVES",
        "Переключение мыши возобновлено" => {
            "Les transitions de la souris ont repris"
        }
        "🚀 Автозагрузка: ВКЛ" => "🚀 Démarrer avec Windows : ACTIVÉ",
        "Multitor будет запускаться вместе с Windows" => {
            "EvertyDisplay démarrera avec Windows"
        }
        "🚀 Автозагрузка: ВЫКЛ" => "🚀 Démarrer avec Windows : DÉSACTIVÉ",
        "Multitor удален из автозагрузки" => {
            "EvertyDisplay a été retiré du démarrage automatique"
        }
        "📺 Режим PiP: ВКЛ" => "📺 Mode PiP : ACTIVÉ",
        "Превью виртуального экрана (Win+Alt+V)" => {
            "Aperçu de l’écran virtuel (Win+Alt+V)"
        }
        "📺 Режим PiP: ВЫКЛ" => "📺 Mode PiP : DÉSACTIVÉ",
        "Превью скрыто" => "Aperçu masqué",
        "⚡ Окно перенесено" | "🪟 Окно перенесено" => {
            "🪟 Fenêtre déplacée"
        }
        "📺 PiP скрыт" => "📺 PiP masqué",
        "Правая кнопка мыши → Показать PiP" => {
            "Clic droit → Afficher PiP"
        }
        "🎮 Игровой режим" => "🎮 Mode jeu",
        "Полноэкранная игра — переходы на паузе" => {
            "Jeu en plein écran — transitions en pause"
        }
        "🎮 Игровой режим выключен" => "🎮 Mode jeu désactivé",
        "Переходы мыши снова активны" => {
            "Les transitions de la souris sont de nouveau actives"
        }
        _ => return None,
    })
}

pub fn tray_text(
    russian: &'static str,
    english: &'static str,
    arabic: &'static str,
    spanish: &'static str,
    german: &'static str,
    french: &'static str,
) -> Vec<u16> {
    let value = match current_language() {
        Language::Russian => russian,
        Language::English => english,
        Language::Arabic => arabic,
        Language::Spanish => spanish,
        Language::German => german,
        Language::French => french,
    };
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dynamic_display_units_are_localizable() {
        assert_eq!(
            translate_dynamic(
                "🖥 Экран #3 — 180 Гц",
                &[("Экран", "Display"), ("Гц", "Hz")],
                None
            ),
            "🖥 Display #3 — 180 Hz"
        );
    }

    #[test]
    fn spanish_osd_catalog_covers_state_notifications() {
        assert_eq!(
            spanish_exact("▶ Переходы мыши: АКТИВНЫ"),
            Some("▶ Transiciones del ratón: ACTIVAS")
        );
        assert_eq!(spanish_exact("📺 PiP скрыт"), Some("📺 PiP oculto"));
        assert_eq!(
            translate_dynamic(
                "🖥 Экран #3 — 180 Гц",
                &[("Экран", "Pantalla"), ("Гц", "Hz")],
                None
            ),
            "🖥 Pantalla #3 — 180 Hz"
        );
    }

    #[test]
    fn german_and_french_osd_catalogs_cover_state_notifications() {
        assert_eq!(
            german_exact("▶ Переходы мыши: АКТИВНЫ"),
            Some("▶ Mausübergänge: AKTIV")
        );
        assert_eq!(
            french_exact("▶ Переходы мыши: АКТИВНЫ"),
            Some("▶ Transitions de la souris : ACTIVES")
        );
    }
}
