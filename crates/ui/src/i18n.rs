use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LanguagePreference {
    #[default]
    System,
    Russian,
    English,
    Arabic,
    Spanish,
    German,
    French,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Russian,
    English,
    Arabic,
    Spanish,
    German,
    French,
}

#[derive(Debug, Serialize, Deserialize)]
struct UiSettings {
    #[serde(default)]
    language: LanguagePreference,
}

static CURRENT_LANGUAGE: AtomicU8 = AtomicU8::new(0);

pub fn initialize() -> LanguagePreference {
    let preference = load_preference();
    apply(preference);
    preference
}

pub fn apply(preference: LanguagePreference) {
    let language = match preference {
        LanguagePreference::System => system_language(),
        LanguagePreference::Russian => Language::Russian,
        LanguagePreference::English => Language::English,
        LanguagePreference::Arabic => Language::Arabic,
        LanguagePreference::Spanish => Language::Spanish,
        LanguagePreference::German => Language::German,
        LanguagePreference::French => Language::French,
    };
    let value = match language {
        Language::Russian => 0,
        Language::English => 1,
        Language::Arabic => 2,
        Language::Spanish => 3,
        Language::German => 4,
        Language::French => 5,
    };
    CURRENT_LANGUAGE.store(value, Ordering::Relaxed);
}

pub fn current() -> Language {
    match CURRENT_LANGUAGE.load(Ordering::Relaxed) {
        0 => Language::Russian,
        2 => Language::Arabic,
        3 => Language::Spanish,
        4 => Language::German,
        5 => Language::French,
        _ => Language::English,
    }
}

pub fn display_name(number: usize) -> String {
    match current() {
        Language::Russian => format!("Экран {number}"),
        Language::Arabic => format!("الشاشة {number}"),
        Language::Spanish => format!("Pantalla {number}"),
        Language::German => format!("Bildschirm {number}"),
        Language::French => format!("Écran {number}"),
        Language::English => format!("Display {number}"),
    }
}

pub fn removing_display(id: u32) -> String {
    match current() {
        Language::Russian => format!("Удаление виртуального экрана {id}…"),
        Language::English => format!("Removing virtual display {id}…"),
        Language::Arabic => format!("جارٍ إزالة الشاشة الافتراضية {id}…"),
        Language::Spanish => format!("Eliminando la pantalla virtual {id}…"),
        Language::German => format!("Virtueller Bildschirm {id} wird entfernt…"),
        Language::French => format!("Suppression de l’écran virtuel {id}…"),
    }
}

pub fn removal_progress(id: u32) -> String {
    match current() {
        Language::Russian => format!("Удаляем виртуальный экран {id}. Windows перенастраивает дисплеи — это может занять до 30 секунд…"),
        Language::English => format!("Removing virtual display {id}. Windows is reconfiguring the displays; this may take up to 30 seconds…"),
        Language::Arabic => format!("جارٍ إزالة الشاشة الافتراضية {id}. يعيد Windows تهيئة الشاشات وقد يستغرق ذلك حتى 30 ثانية…"),
        Language::Spanish => format!("Eliminando la pantalla virtual {id}. Windows está reconfigurando las pantallas; puede tardar hasta 30 segundos…"),
        Language::German => format!("Virtueller Bildschirm {id} wird entfernt. Windows konfiguriert die Anzeigen neu; dies kann bis zu 30 Sekunden dauern…"),
        Language::French => format!("Suppression de l’écran virtuel {id}. Windows reconfigure les écrans ; cela peut prendre jusqu’à 30 secondes…"),
    }
}

pub fn display_removed(id: u32) -> String {
    match current() {
        Language::Russian => format!("Виртуальный экран {id} удалён"),
        Language::English => format!("Virtual display {id} was removed"),
        Language::Arabic => format!("تمت إزالة الشاشة الافتراضية {id}"),
        Language::Spanish => format!("Se eliminó la pantalla virtual {id}"),
        Language::German => format!("Virtueller Bildschirm {id} wurde entfernt"),
        Language::French => format!("L’écran virtuel {id} a été supprimé"),
    }
}

pub fn display_remove_failed(id: u32, error: &str) -> String {
    match current() {
        Language::Russian => format!("Не удалось удалить экран {id}: {error}"),
        Language::English => format!("Could not remove display {id}: {error}"),
        Language::Arabic => format!("تعذرت إزالة الشاشة {id}: {error}"),
        Language::Spanish => format!("No se pudo eliminar la pantalla {id}: {error}"),
        Language::German => format!("Bildschirm {id} konnte nicht entfernt werden: {error}"),
        Language::French => format!("Impossible de supprimer l’écran {id} : {error}"),
    }
}

pub fn removal_timeout() -> &'static str {
    match current() {
        Language::Russian => "Удаление не завершилось за 65 секунд. Драйвер мог перестать отвечать",
        Language::English => "Removal did not finish within 65 seconds. The driver may have stopped responding",
        Language::Arabic => "لم تكتمل الإزالة خلال 65 ثانية. ربما توقف برنامج التشغيل عن الاستجابة",
        Language::Spanish => "La eliminación no terminó en 65 segundos. Es posible que el controlador no responda",
        Language::German => "Das Entfernen wurde nicht innerhalb von 65 Sekunden abgeschlossen. Der Treiber reagiert möglicherweise nicht mehr",
        Language::French => "La suppression ne s’est pas terminée en 65 secondes. Le pilote ne répond peut-être plus",
    }
}

pub fn confirmation_message(seconds: u8) -> String {
    match current() {
        Language::Russian => format!("Отображение работает корректно? Если не нажать «Ок» через {seconds} с — монитор будет автоматически удалён."),
        Language::Arabic => format!("هل تعمل الشاشة بشكل صحيح؟ إذا لم تضغط «موافق» خلال {seconds} ثانية فستُزال تلقائيًا."),
        Language::Spanish => format!("¿La pantalla funciona correctamente? Si no pulsas Aceptar en {seconds} s, se eliminará automáticamente."),
        Language::German => format!("Funktioniert der Bildschirm korrekt? Wenn Sie nicht innerhalb von {seconds} s auf OK klicken, wird er automatisch entfernt."),
        Language::French => format!("L’écran fonctionne-t-il correctement ? Sans confirmation dans {seconds} s, il sera automatiquement supprimé."),
        Language::English => format!("Is the display working correctly? If you do not click OK within {seconds} s, it will be removed automatically."),
    }
}

pub fn confirmation_button(seconds: u8) -> String {
    match current() {
        Language::Russian => format!("Ок ({seconds}с)"),
        Language::Arabic => format!("موافق ({seconds} ث)"),
        Language::Spanish => format!("Aceptar ({seconds}s)"),
        Language::German => format!("OK ({seconds}s)"),
        Language::French => format!("OK ({seconds}s)"),
        Language::English => format!("OK ({seconds}s)"),
    }
}

pub fn hertz(value: u32) -> String {
    match current() {
        Language::Russian => format!("{value} Гц"),
        Language::Arabic => format!("{value} هرتز"),
        Language::Spanish => format!("{value} Hz"),
        Language::German | Language::French => format!("{value} Hz"),
        Language::English => format!("{value} Hz"),
    }
}

pub fn milliseconds(value: u64) -> String {
    match current() {
        Language::Russian => format!("{value} мс"),
        Language::Arabic => format!("{value} مللي ثانية"),
        Language::Spanish => format!("{value} ms"),
        Language::German | Language::French => format!("{value} ms"),
        Language::English => format!("{value} ms"),
    }
}

pub fn active_displays(count: usize) -> String {
    match current() {
        Language::Russian => format!("{count} активных"),
        Language::Arabic => format!("{count} نشطة"),
        Language::Spanish => format!("{count} activas"),
        Language::German => format!("{count} aktiv"),
        Language::French => format!("{count} actifs"),
        Language::English => format!("{count} active"),
    }
}

pub fn virtual_display_limit(max: u32) -> String {
    match current() {
        Language::Russian => format!("В этой редакции доступно виртуальных экранов: {max}"),
        Language::English => format!("This edition supports {max} virtual display(s)"),
        Language::Arabic => format!("يدعم هذا الإصدار {max} شاشة افتراضية"),
        Language::Spanish => format!("Esta edición admite {max} pantalla(s) virtual(es)"),
        Language::German => format!("Diese Edition unterstützt {max} virtuelle Bildschirm(e)"),
        Language::French => format!("Cette édition prend en charge {max} écran(s) virtuel(s)"),
    }
}

pub fn display_counts(virtual_count: usize, physical_count: usize, offline_count: usize) -> String {
    match current() {
        Language::Russian => {
            format!("{virtual_count} вирт. / {physical_count} физ. / {offline_count} откл.")
        }
        Language::Arabic => {
            format!("{virtual_count} افتراضية / {physical_count} فعلية / {offline_count} غير متصلة")
        }
        Language::Spanish => {
            format!("{virtual_count} virtuales / {physical_count} físicas / {offline_count} desconectadas")
        }
        Language::German => {
            format!(
                "{virtual_count} virtuell / {physical_count} physisch / {offline_count} offline"
            )
        }
        Language::French => {
            format!("{virtual_count} virtuels / {physical_count} physiques / {offline_count} hors ligne")
        }
        Language::English => {
            format!("{virtual_count} virtual / {physical_count} physical / {offline_count} offline")
        }
    }
}

pub fn edge_delay(value: u64) -> String {
    match current() {
        Language::Russian => format!("Задержка края: {value} мс"),
        Language::Arabic => format!("تأخير الحافة: {value} مللي ثانية"),
        Language::Spanish => format!("Retardo del borde: {value} ms"),
        Language::German => format!("Randverzögerung: {value} ms"),
        Language::French => format!("Délai du bord : {value} ms"),
        Language::English => format!("Edge delay: {value} ms"),
    }
}

pub fn display_duration(value: u32) -> String {
    match current() {
        Language::Russian => format!("Длительность показа: {value} мс"),
        Language::Arabic => format!("مدة العرض: {value} مللي ثانية"),
        Language::Spanish => format!("Duración: {value} ms"),
        Language::German => format!("Anzeigedauer: {value} ms"),
        Language::French => format!("Durée d’affichage : {value} ms"),
        Language::English => format!("Display duration: {value} ms"),
    }
}

pub fn pip_scale(value: u32) -> String {
    match current() {
        Language::Russian => format!("Масштаб миниатюры PiP: {value} %"),
        Language::Arabic => format!("حجم معاينة PiP: {value}%"),
        Language::Spanish => format!("Escala de PiP: {value}%"),
        Language::German => format!("PiP-Vorschaugröße: {value}%"),
        Language::French => format!("Échelle de l’aperçu PiP : {value}%"),
        Language::English => format!("PiP preview scale: {value}%"),
    }
}

pub fn selected_display(id: u32, name: &str) -> String {
    match current() {
        Language::Russian => format!("Выбран экран #{id}: {name}"),
        Language::Arabic => format!("الشاشة المحددة #{id}: {name}"),
        Language::Spanish => format!("Pantalla seleccionada #{id}: {name}"),
        Language::German => format!("Ausgewählter Bildschirm #{id}: {name}"),
        Language::French => format!("Écran sélectionné nº{id} : {name}"),
        Language::English => format!("Selected display #{id}: {name}"),
    }
}

pub fn monitor_name_placeholder() -> &'static str {
    match current() {
        Language::Russian => "Например: CODE, WEB, CHAT",
        Language::Arabic => "مثال: CODE، WEB، CHAT",
        Language::Spanish => "Ejemplo: CODE, WEB, CHAT",
        Language::German => "Beispiel: CODE, WEB, CHAT",
        Language::French => "Exemple : CODE, WEB, CHAT",
        Language::English => "For example: CODE, WEB, CHAT",
    }
}

pub fn neighbor_summary(
    left: Option<u32>,
    right: Option<u32>,
    top: Option<u32>,
    bottom: Option<u32>,
) -> String {
    match current() {
        Language::Russian => format!("Соседи перехода мыши: [Слева: {left:?}]  [Справа: {right:?}]  [Сверху: {top:?}]  [Снизу: {bottom:?}]"),
        Language::Arabic => format!("الشاشات المجاورة: [يسار: {left:?}]  [يمين: {right:?}]  [أعلى: {top:?}]  [أسفل: {bottom:?}]"),
        Language::Spanish => format!("Pantallas vecinas: [Izquierda: {left:?}]  [Derecha: {right:?}]  [Arriba: {top:?}]  [Abajo: {bottom:?}]"),
        Language::German => format!("Benachbarte Bildschirme: [Links: {left:?}]  [Rechts: {right:?}]  [Oben: {top:?}]  [Unten: {bottom:?}]"),
        Language::French => format!("Écrans voisins : [Gauche : {left:?}]  [Droite : {right:?}]  [Haut : {top:?}]  [Bas : {bottom:?}]"),
        Language::English => format!("Mouse transition neighbors: [Left: {left:?}]  [Right: {right:?}]  [Top: {top:?}]  [Bottom: {bottom:?}]"),
    }
}

pub fn save_preference(preference: LanguagePreference) -> Result<(), String> {
    let path = settings_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let bytes = serde_json::to_vec_pretty(&UiSettings {
        language: preference,
    })
    .map_err(|error| error.to_string())?;
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, bytes).map_err(|error| error.to_string())?;

    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;
        use windows::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };
        let source: Vec<u16> = temporary
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let destination: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        unsafe {
            MoveFileExW(
                PCWSTR(source.as_ptr()),
                PCWSTR(destination.as_ptr()),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        }
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    #[cfg(not(windows))]
    std::fs::rename(temporary, path).map_err(|error| error.to_string())
}

fn load_preference() -> LanguagePreference {
    settings_path()
        .ok()
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice::<UiSettings>(&bytes).ok())
        .map(|settings| settings.language)
        .unwrap_or_default()
}

fn settings_path() -> Result<std::path::PathBuf, String> {
    std::env::var_os("APPDATA")
        .map(std::path::PathBuf::from)
        .map(|path| path.join("EvertyDisplay").join("ui-settings.json"))
        .ok_or_else(|| "APPDATA is not available".to_string())
}

#[cfg(windows)]
fn system_language() -> Language {
    // Use the primary LANGID so every Arabic regional locale (Saudi Arabia,
    // Egypt, UAE, etc.) resolves to the same catalog.
    let lang_id = unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() };
    language_from_windows_lang_id(lang_id)
}

fn language_from_windows_lang_id(lang_id: u16) -> Language {
    match lang_id & 0x03ff {
        0x01 => Language::Arabic,
        0x0a => Language::Spanish,
        0x07 => Language::German,
        0x0c => Language::French,
        0x19 => Language::Russian,
        _ => Language::English,
    }
}

#[cfg(not(windows))]
fn system_language() -> Language {
    let locale = std::env::var("LANG")
        .unwrap_or_default()
        .to_ascii_lowercase();
    if locale.starts_with("ru") {
        Language::Russian
    } else if locale.starts_with("ar") {
        Language::Arabic
    } else if locale.starts_with("es") {
        Language::Spanish
    } else if locale.starts_with("de") {
        Language::German
    } else if locale.starts_with("fr") {
        Language::French
    } else {
        Language::English
    }
}

pub fn translate<'a>(russian: impl Into<Cow<'a, str>>) -> Cow<'a, str> {
    let russian = russian.into();
    let language = current();
    if language == Language::Russian {
        return russian;
    }

    for (prefix, english, arabic, spanish, german, french) in [
        (
            "Служба EvertyDisplay отключена",
            "EvertyDisplay service is offline",
            "خدمة EvertyDisplay غير متصلة",
            "El servicio EvertyDisplay está desconectado",
            "Der EvertyDisplay-Dienst ist offline",
            "Le service EvertyDisplay est hors ligne",
        ),
        (
            "Операция с монитором не выполнена",
            "Display operation failed",
            "فشلت عملية الشاشة",
            "La operación de pantalla falló",
            "Der Bildschirmvorgang ist fehlgeschlagen",
            "L’opération sur l’écran a échoué",
        ),
        (
            "Не удалось добавить экран",
            "Could not add display",
            "تعذرت إضافة الشاشة",
            "No se pudo añadir la pantalla",
            "Bildschirm konnte nicht hinzugefügt werden",
            "Impossible d’ajouter l’écran",
        ),
        (
            "Служба EvertyDisplay не ответила",
            "EvertyDisplay service did not respond",
            "لم تستجب خدمة EvertyDisplay",
            "El servicio EvertyDisplay no respondió",
            "Der EvertyDisplay-Dienst antwortet nicht",
            "Le service EvertyDisplay ne répond pas",
        ),
        (
            "не удалось запустить фоновую службу",
            "could not start the background service",
            "تعذر تشغيل خدمة الخلفية",
            "no se pudo iniciar el servicio en segundo plano",
            "der Hintergrunddienst konnte nicht gestartet werden",
            "impossible de démarrer le service en arrière-plan",
        ),
    ] {
        if let Some(details) = russian.as_ref().strip_prefix(prefix) {
            let translated = match language {
                Language::Arabic => arabic,
                Language::Spanish => spanish,
                Language::German => german,
                Language::French => french,
                _ => english,
            };
            return Cow::Owned(format!("{translated}{details}"));
        }
    }

    let english = match russian.as_ref() {
        "Добавить экран" => "Add display",
        "Добавить виртуальный монитор в систему" => "Add a virtual display to Windows",
        "Драйвер готов" => "Driver ready",
        "Виртуальный видеоадаптер IddCx активен. Нажмите для переустановки." => "The IddCx virtual display adapter is active. Click to reinstall.",
        "Активировать драйвер" => "Activate driver",
        "Требуется разовая системная активация для создания виртуальных экранов." => "One-time system activation is required to create virtual displays.",
        "Мониторы и топология" => "Displays and topology",
        "Интерактивное пространственное расположение экранов" => "Interactive spatial display layout",
        "Пространственные виртуальные дисплеи" => "Spatial Virtual Displays",
        "Настройки и возможности" => "Settings and features",
        "Конфигурация OSD, Live PiP, автозапуска и горячих клавиш" => "OSD, Live PiP, startup and hotkey configuration",
        "О нас" => "About",
        "Автор и контакты проекта" => "Author and project contacts",
        "Светлая тема" => "Light theme",
        "Темная тема" => "Dark theme",
        "Переключить тему оформления приложения" => "Switch application theme",
        "Служба активна" => "Service active",
        "Служба отключена" => "Service offline",
        "Адаптер готов" => "Adapter ready",
        "Адаптер не активен" => "Adapter inactive",
        "Подключение к службе EvertyDisplay..." => "Connecting to the EvertyDisplay service...",
        "Обновление данных..." => "Refreshing data...",
        "Запуск фоновой службы EvertyDisplay..." => "Starting the EvertyDisplay background service...",
        "Перезапуск фоновой службы EvertyDisplay..." => "Restarting the EvertyDisplay background service...",
        "Удаление видеодрайвера EvertyDisplay (UAC)..." => "Removing the EvertyDisplay display driver (UAC)...",
        "Активация видеодрайвера EvertyDisplay (UAC)..." => "Activating the EvertyDisplay display driver (UAC)...",
        "Служба EvertyDisplay и видеодрайвер активны" => "EvertyDisplay service and display driver are active",
        "Служба активна; видеодрайвер пока не подключен" => "Service active; display driver is not connected yet",
        "Добавление виртуального экрана..." => "Adding a virtual display...",
        "Расположение мониторов и масштаб холста сброшены" => "Display layout and canvas zoom have been reset",
        "Загрузка топологии мониторов..." => "Loading display topology...",
        "Добавить виртуальный монитор?" => "Add a virtual display?",
        "Будет добавлен новый виртуальный дисплей через драйвер. После добавления у вас будет 15 секунд чтобы подтвердить, что всё в порядке. Если не нажать «Ок» — монитор будет автоматически удалён." => "A new virtual display will be added through the driver. You will have 15 seconds to confirm that it works. If you do not click OK, the display will be removed automatically.",
        "Продолжить" => "Continue",
        "Отмена" => "Cancel",
        "Монитор добавлен — всё в порядке?" => "Display added — is everything working?",
        "Отмена / Откатить" => "Cancel / Revert",
        "Активация виртуального дисплея (UAC)" => "Virtual display activation (UAC)",
        "Будет запущен сценарий PowerShell от имени Администратора для установки и регистрации драйвера виртуального монитора IddCx. Экран Windows может кратковременно моргнуть при добавлении виртуального адаптера." => "An elevated PowerShell task will install and register the IddCx virtual display driver. Windows may briefly flicker while the adapter is added.",
        "Продолжить и установить (UAC)" => "Continue and install (UAC)",
        "Внимание: Полное удаление драйвера виртуального дисплея" => "Warning: Complete virtual display driver removal",
        "Будет запущен сценарий деинсталляции с повышенными привилегиями (UAC). Драйвер IddCx (MttVDD) будет полностью удален из Windows Driver Store и реестра, а все активные виртуальные мониторы будут закрыты. Экран может кратковременно моргнуть." => "An elevated uninstall task will completely remove the IddCx (MttVDD) driver from the Windows Driver Store and registry. All active virtual displays will close and the screen may briefly flicker.",
        "Да, удалить драйвер из системы (UAC)" => "Yes, remove driver from the system (UAC)",
        "Включить аппаратный Viewport?" => "Enable hardware Viewport?",
        "Выключить Viewport?" => "Disable Viewport?",
        "Включить Viewport" => "Enable Viewport",
        "Выключить Viewport" => "Disable Viewport",
        "Режим аппаратного Viewport захватывает рабочий стол виртуального монитора через Direct3D 11 и отображает его на вашем основном экране с нулевой задержкой. Для быстрого сворачивания/разворачивания используйте Win + Alt + V." => "Hardware Viewport captures the virtual display through Direct3D 11 and presents it on the primary screen with minimal latency. Use Win+Alt+V to switch modes.",
        "Viewport будет отключен. Отображение вернется к стандартному физическому рабочему столу." => "Viewport will be disabled and the standard physical desktop will return.",
        "Включить игровой режим (Пауза мыши)?" => "Enable Gaming Mode (pause mouse switching)?",
        "Возобновить переключение мыши?" => "Resume mouse switching?",
        "Включить режим" => "Enable mode",
        "Возобновить мышь" => "Resume mouse",
        "Курсор мыши будет зафиксирован в пределах текущего монитора. Это предотвращает случайный вылет курсора в 3D-играх и шутерах. Для быстрой паузы/возобновления используйте Win + Alt + P." => "The pointer will stay within the current display, preventing accidental transitions in fullscreen games. Use Win+Alt+P to pause or resume quickly.",
        "Свободное пространственное перемещение курсора между мониторами будет возобновлено." => "Free spatial pointer movement between displays will resume.",
        "Служба EvertyDisplay сейчас отключена" => "EvertyDisplay service is currently offline",
        "Фоновая служба обеспечивает мгновенный переход мыши, OSD и виртуальные мониторы." => "The background service provides instant mouse transitions, OSD and virtual displays.",
        "Запустить службу EvertyDisplay" => "Start EvertyDisplay service",
        "Активировать виртуальный дисплей в Windows (UAC)" => "Activate virtual display in Windows (UAC)",
        "Топология мониторов" => "Display topology",
        "Пространственное расположение и переключение экранов" => "Spatial display layout and switching",
        "Конфигурация OSD, игрового режима, фокуса и автозапуска" => "OSD, Gaming Mode, focus and startup configuration",
        "Информация об авторе и контакты проекта" => "Author and project contact information",
        "Обновить" => "Refresh",
        "Обновить конфигурацию и экраны из системы" => "Refresh configuration and displays from Windows",
        "Viewport: Вкл" => "Viewport: On",
        "Viewport: Выкл" => "Viewport: Off",
        "Аппаратный захват виртуального дисплея (Win+Alt+V)" => "Hardware virtual display capture (Win+Alt+V)",
        "Игровой режим" => "Gaming Mode",
        "Временная фиксация мыши для 3D-игр (Win+Alt+P)" => "Temporarily lock the mouse for 3D games (Win+Alt+P)",
        "Физический" => "Physical",
        "Основной" => "Primary",
        "АКТИВНЫЙ VIEWPORT" => "ACTIVE VIEWPORT",
        "ВСЕГО ДИСПЛЕЕВ" => "TOTAL DISPLAYS",
        "ЧАСТОТА РАЗВЕРТКИ" => "REFRESH RATE",
        "ЗАДЕРЖКА ПЕРЕХОДА" => "EDGE DELAY",
        "Порог активации" => "Activation threshold",
        "В ряд" => "Horizontal",
        "Расположить мониторы горизонтально в одну линию" => "Arrange displays in one horizontal row",
        "Сверху вниз" => "Vertical",
        "Расположить мониторы вертикально друг над другом" => "Arrange displays vertically",
        "Сетка 2x2" => "2x2 grid",
        "Расположить мониторы сеткой 2 на 2" => "Arrange displays in a 2 by 2 grid",
        "Циклический переход краев (1 <-> N)" => "Wrap edge transitions (1 <-> N)",
        "Собрать экраны (Сброс)" => "Gather displays (Reset)",
        "Сбросить масштаб холста и собрать все мониторы в один ряд" => "Reset canvas zoom and gather all displays into one row",
        "Быстрое выравнивание:" => "Quick arrangement:",
        "Видеодрайвер Windows требует подтверждения активации (UAC)" => "The Windows display driver requires activation confirmation (UAC)",
        "Нажмите «Активировать драйвер», чтобы система создавала реальные виртуальные мониторы Windows" => "Click ‘Activate driver’ so Windows can create real virtual displays",
        "Активировать драйвер (UAC)" => "Activate driver (UAC)",
        "Имя экрана:" => "Display name:",
        "Например: CODE, WEB, CHAT" => "For example: CODE, WEB, CHAT",
        "Сохранить" => "Save",
        "Переключить физический экран на этот монитор" => "Switch the physical screen to this display",
        "Забыть отключённый" => "Forget disconnected",
        "Физический экран" => "Physical display",
        "Сначала удалите последний" => "Remove the last one first",
        "Удалить" => "Remove",
        "Сдвинуть левее" => "Move left",
        "Сдвинуть правее" => "Move right",
        "Интерактивный 2D холст (колесико мыши: зум, зажмите фон: перемещение):" => "Interactive 2D canvas (mouse wheel: zoom, drag background: pan):",
        "EvertyDisplay — пространственное управление физическими и виртуальными дисплеями" => "EvertyDisplay — spatial control for physical and virtual displays",
        "Автор" => "Author",
        "Сайт" => "Website",
        "Электронная почта" => "Email",
        "Версия" => "Version",
        "Язык интерфейса" => "Interface language",
        "Следовать языку интерфейса Windows или выбрать его вручную." => "Follow the Windows display language or choose one manually.",
        "Как в Windows" => "Windows default",
        "Русский" => "Russian",
        "Английский" => "English",
        "Арабский" => "Arabic",
        "Испанский" => "Spanish",
        "Немецкий" => "German",
        "Французский" => "French",
        "Включить всплывающие уведомления (OSD HUD) при смене активного экрана" => "Show pop-up notifications (OSD HUD) when the active display changes",
        "Всплывающие уведомления (OSD HUD)" => "Pop-up notifications (OSD HUD)",
        "Показывать компактную схему экранов и выделять активный экран" => "Show a compact display layout and highlight the active display",
        "Не показывать уведомления при переключении между физическими дисплеями" => "Do not show notifications when switching between physical displays",
        "Минимум один виртуальный" => "At least one virtual display",
        "Отображает полупрозрачный индикатор в центре экрана с именем монитора и подсказкой при переключении." => "Shows a translucent indicator with the display name and a switching hint.",
        "Auto-Gaming Guard: Автоматически блокировать переход мыши в полноэкранных 3D-играх" => "Auto-Gaming Guard: Block mouse transitions in fullscreen 3D games",
        "Игровой режим (Auto-Gaming Guard)" => "Gaming Mode (Auto-Gaming Guard)",
        "Служба проверяет запуск игр в полноэкранном режиме и блокирует случайный вылет курсора на соседние мониторы. Быстрая пауза: Win+Alt+P." => "The service detects fullscreen games and prevents accidental mouse transitions to adjacent displays. Quick pause: Win+Alt+P.",
        "Smart Auto-Focus: Автоматически передавать фокус окну под курсором при переходе на монитор" => "Smart Auto-Focus: Focus the window under the pointer after a display transition",
        "Drag-to-Teleport: Мгновенно переносить окно на монитор при зажатой ЛКМ на краю экрана" => "Drag-to-Teleport: Move a dragged window to the adjacent display at the edge",
        "Использовать разрешение MAIN для виртуальных дисплеев (частоту не менять)" => "Use MAIN resolution for virtual displays (keep their refresh rate)",
        "Переходить с виртуального дисплея к окну, открывшемуся на физическом дисплее" => "Follow a window that opens on a physical display from a virtual display",
        "При активации окна на виртуальном дисплее:" => "When activating a window on a virtual display:",
        "Перейти на дисплей" => "Switch to display",
        "Перенести окно сюда" => "Bring window here",
        "Работает при выборе окна на панели задач и через Alt+Tab." => "Works when selecting a window from the taskbar or with Alt+Tab.",
        "Управление окнами и фокусом" => "Window and focus control",
        "Обеспечивает естественное взаимодействие с окнами при пространственном переключении мониторов." => "Provides natural window interaction across spatial display transitions.",
        "Включить режим Live PiP (компактная миниатюра монитора в углу экрана)" => "Enable Live PiP (a compact display preview)",
        "Картинка-в-картинке (Live PiP)" => "Picture-in-Picture (Live PiP)",
        "Позволяет непрерывно видеть виртуальный экран в компактном окне. Окно можно свободно растягивать мышью за любые края и перетаскивать за центр в любое место экрана. Хоткей: Win+Alt+V." => "Continuously shows the virtual display in a compact window. Resize it from any edge and drag it from the center to any display. Hotkey: Win+Alt+V.",
        "Служба: Активна (работает)" => "Service: Active (running)",
        "Перезапустить службу" => "Restart service",
        "Служба: Не подключена" => "Service: Not connected",
        "Запустить службу" => "Start service",
        "Запускать фоновую службу EvertyDisplay автоматически при входе в Windows (HKCU Run)" => "Start the EvertyDisplay background service automatically at Windows sign-in (HKCU Run)",
        "Автозапуск и системные службы" => "Startup and system services",
        "Служба работает в фоне в системном трее Windows без консольных окон, обеспечивая бесшовное перемещение курсора, хоткеи и виртуальные мониторы." => "The service runs in the Windows system tray without console windows and provides seamless cursor movement, hotkeys and virtual displays.",
        "Памятка горячих клавиш EvertyDisplay" => "EvertyDisplay hotkeys",
        "Все комбинации работают глобально в любых приложениях:" => "All shortcuts work globally in any application:",
        "Win + Shift + Стрелки (Влево / Вправо / Вверх / Вниз)" => "Win + Shift + Arrow keys (Left / Right / Up / Down)",
        "— Телепортация активного окна на соседний экран" => "— Move the active window to an adjacent display",
        "Win + Alt + Стрелки (Влево / Вправо / Вверх / Вниз)" => "Win + Alt + Arrow keys (Left / Right / Up / Down)",
        "— Мгновенное переключение экрана Viewport" => "— Instantly switch the Viewport display",
        "— Пауза / возобновление переключения мыши (Игровой режим)" => "— Pause / resume mouse switching (Gaming Mode)",
        "— Переключение режима Viewport (Полный экран / PiP / Выкл)" => "— Switch Viewport mode (Fullscreen / PiP / Off)",
        "Зажатая ЛКМ на краю экрана" => "Hold the left mouse button at a display edge",
        "— Перетаскивание окна на соседний экран (Drag-to-Teleport)" => "— Drag a window to an adjacent display (Drag-to-Teleport)",
        "Опасная зона: Удаление драйвера" => "Danger zone: Remove driver",
        "Полное удаление драйвера виртуального монитора IddCx (MttVDD) из Windows Driver Store и системного реестра. Все виртуальные экраны будут немедленно отключены. Нажмите для запуска деинсталляции с правами Администратора." => "Completely removes the IddCx (MttVDD) virtual display driver from the Windows Driver Store and registry. All virtual displays will be disconnected immediately. Administrator rights are required.",
        "Удалить драйвер виртуального дисплея..." => "Remove virtual display driver...",
        _ => return russian,
    };
    match language {
        Language::Arabic => Cow::Borrowed(arabic_translation(russian.as_ref()).unwrap_or(english)),
        Language::Spanish => {
            Cow::Borrowed(spanish_translation(russian.as_ref()).unwrap_or(english))
        }
        Language::German => Cow::Owned(german_translation(english)),
        Language::French => Cow::Owned(french_translation(english)),
        _ => Cow::Borrowed(english),
    }
}

fn arabic_translation(russian: &str) -> Option<&'static str> {
    Some(match russian {
        "Добавить экран" => "إضافة شاشة",
        "Добавить виртуальный монитор в систему" => "إضافة شاشة افتراضية إلى Windows",
        "Драйвер готов" => "برنامج التشغيل جاهز",
        "Виртуальный видеоадаптер IddCx активен. Нажмите для переустановки." => "محوّل العرض الافتراضي IddCx نشط. اضغط لإعادة التثبيت.",
        "Активировать драйвер" => "تفعيل برنامج التشغيل",
        "Требуется разовая системная активация для создания виртуальных экранов." => "يلزم تفعيل النظام مرة واحدة لإنشاء الشاشات الافتراضية.",
        "Мониторы и топология" => "الشاشات والتخطيط",
        "Интерактивное пространственное расположение экранов" => "تخطيط مكاني تفاعلي للشاشات",
        "Пространственные виртуальные дисплеи" => "شاشات افتراضية مكانية",
        "Настройки и возможности" => "الإعدادات والميزات",
        "Конфигурация OSD, Live PiP, автозапуска и горячих клавиш" => "إعدادات الإشعارات وLive PiP والتشغيل التلقائي والاختصارات",
        "О нас" => "حول البرنامج",
        "Автор и контакты проекта" => "المؤلف وبيانات التواصل",
        "Светлая тема" => "السمة الفاتحة",
        "Темная тема" => "السمة الداكنة",
        "Переключить тему оформления приложения" => "تبديل سمة التطبيق",
        "Служба активна" => "الخدمة نشطة",
        "Служба отключена" => "الخدمة غير متصلة",
        "Адаптер готов" => "المحوّل جاهز",
        "Адаптер не активен" => "المحوّل غير نشط",
        "Подключение к службе EvertyDisplay..." => "جارٍ الاتصال بخدمة EvertyDisplay...",
        "Обновление данных..." => "جارٍ تحديث البيانات...",
        "Запуск фоновой службы EvertyDisplay..." => "جارٍ تشغيل خدمة EvertyDisplay في الخلفية...",
        "Перезапуск фоновой службы EvertyDisplay..." => "جارٍ إعادة تشغيل خدمة EvertyDisplay...",
        "Удаление видеодрайвера EvertyDisplay (UAC)..." => "جارٍ إزالة برنامج تشغيل EvertyDisplay ‏(UAC)...",
        "Активация видеодрайвера EvertyDisplay (UAC)..." => "جارٍ تفعيل برنامج تشغيل EvertyDisplay ‏(UAC)...",
        "Служба EvertyDisplay и видеодрайвер активны" => "خدمة EvertyDisplay وبرنامج تشغيل العرض نشطان",
        "Служба активна; видеодрайвер пока не подключен" => "الخدمة نشطة؛ برنامج تشغيل العرض غير متصل بعد",
        "Добавление виртуального экрана..." => "جارٍ إضافة شاشة افتراضية...",
        "Расположение мониторов и масштаб холста сброшены" => "تمت إعادة تعيين تخطيط الشاشات ومقياس اللوحة",
        "Загрузка топологии мониторов..." => "جارٍ تحميل تخطيط الشاشات...",
        "Добавить виртуальный монитор?" => "هل تريد إضافة شاشة افتراضية؟",
        "Будет добавлен новый виртуальный дисплей через драйвер. После добавления у вас будет 15 секунд чтобы подтвердить, что всё в порядке. Если не нажать «Ок» — монитор будет автоматически удалён." => "ستتم إضافة شاشة افتراضية جديدة عبر برنامج التشغيل. لديك 15 ثانية لتأكيد أنها تعمل، وإلا فستُزال تلقائيًا.",
        "Продолжить" => "متابعة",
        "Отмена" => "إلغاء",
        "Монитор добавлен — всё в порядке?" => "تمت إضافة الشاشة — هل تعمل بشكل صحيح؟",
        "Отмена / Откатить" => "إلغاء / تراجع",
        "Активация виртуального дисплея (UAC)" => "تفعيل الشاشة الافتراضية (UAC)",
        "Будет запущен сценарий PowerShell от имени Администратора для установки и регистрации драйвера виртуального монитора IddCx. Экран Windows может кратковременно моргнуть при добавлении виртуального адаптера." => "سيتم تشغيل مهمة PowerShell بصلاحيات المسؤول لتثبيت برنامج تشغيل الشاشة الافتراضية IddCx وتسجيله. قد تومض الشاشة للحظة.",
        "Продолжить и установить (UAC)" => "متابعة التثبيت (UAC)",
        "Внимание: Полное удаление драйвера виртуального дисплея" => "تحذير: إزالة برنامج تشغيل الشاشة الافتراضية بالكامل",
        "Будет запущен сценарий деинсталляции с повышенными привилегиями (UAC). Драйвер IddCx (MttVDD) будет полностью удален из Windows Driver Store и реестра, а все активные виртуальные мониторы будут закрыты. Экран может кратковременно моргнуть." => "ستؤدي مهمة إزالة بصلاحيات مرتفعة إلى حذف برنامج IddCx ‏(MttVDD) من مخزن برامج تشغيل Windows والسجل، وإغلاق جميع الشاشات الافتراضية. قد تومض الشاشة للحظة.",
        "Да, удалить драйвер из системы (UAC)" => "نعم، إزالة برنامج التشغيل من النظام (UAC)",
        "Включить аппаратный Viewport?" => "هل تريد تشغيل Viewport المسرّع؟",
        "Выключить Viewport?" => "هل تريد إيقاف Viewport؟",
        "Включить Viewport" => "تشغيل Viewport",
        "Выключить Viewport" => "إيقاف Viewport",
        "Режим аппаратного Viewport захватывает рабочий стол виртуального монитора через Direct3D 11 и отображает его на вашем основном экране с нулевой задержкой. Для быстрого сворачивания/разворачивания используйте Win + Alt + V." => "يلتقط Viewport سطح مكتب الشاشة الافتراضية عبر Direct3D 11 ويعرضه على الشاشة الرئيسية بأقل زمن انتقال. استخدم Win+Alt+V لتبديل الأوضاع.",
        "Viewport будет отключен. Отображение вернется к стандартному физическому рабочему столу." => "سيتم إيقاف Viewport والعودة إلى سطح المكتب الفعلي العادي.",
        "Включить игровой режим (Пауза мыши)?" => "هل تريد تشغيل وضع الألعاب (إيقاف انتقال المؤشر)؟",
        "Возобновить переключение мыши?" => "هل تريد استئناف انتقال المؤشر؟",
        "Включить режим" => "تشغيل الوضع",
        "Возобновить мышь" => "استئناف المؤشر",
        "Курсор мыши будет зафиксирован в пределах текущего монитора. Это предотвращает случайный вылет курсора в 3D-играх и шутерах. Для быстрой паузы/возобновления используйте Win + Alt + P." => "سيبقى المؤشر داخل الشاشة الحالية لمنع انتقاله بالخطأ أثناء الألعاب. استخدم Win+Alt+P للإيقاف أو الاستئناف بسرعة.",
        "Свободное пространственное перемещение курсора между мониторами будет возобновлено." => "سيُستأنف انتقال المؤشر بحرية بين الشاشات.",
        "Служба EvertyDisplay сейчас отключена" => "خدمة EvertyDisplay غير متصلة حاليًا",
        "Фоновая служба обеспечивает мгновенный переход мыши, OSD и виртуальные мониторы." => "توفر خدمة الخلفية انتقال المؤشر والإشعارات والشاشات الافتراضية.",
        "Запустить службу EvertyDisplay" => "تشغيل خدمة EvertyDisplay",
        "Активировать виртуальный дисплей в Windows (UAC)" => "تفعيل الشاشة الافتراضية في Windows ‏(UAC)",
        "Топология мониторов" => "تخطيط الشاشات",
        "Пространственное расположение и переключение экранов" => "ترتيب الشاشات والتبديل المكاني بينها",
        "Конфигурация OSD, игрового режима, фокуса и автозапуска" => "إعداد الإشعارات ووضع الألعاب والتركيز والتشغيل التلقائي",
        "Информация об авторе и контакты проекта" => "معلومات المؤلف وبيانات التواصل",
        "Обновить" => "تحديث",
        "Обновить конфигурацию и экраны из системы" => "تحديث الإعدادات والشاشات من Windows",
        "Viewport: Вкл" => "Viewport: تشغيل",
        "Viewport: Выкл" => "Viewport: إيقاف",
        "Аппаратный захват виртуального дисплея (Win+Alt+V)" => "التقاط الشاشة الافتراضية المسرّع (Win+Alt+V)",
        "Игровой режим" => "وضع الألعاب",
        "Временная фиксация мыши для 3D-игр (Win+Alt+P)" => "تقييد المؤشر مؤقتًا للألعاب (Win+Alt+P)",
        "Физический" => "فعلية",
        "Основной" => "رئيسية",
        "АКТИВНЫЙ VIEWPORT" => "VIEWPORT النشط",
        "ВСЕГО ДИСПЛЕЕВ" => "إجمالي الشاشات",
        "ЧАСТОТА РАЗВЕРТКИ" => "معدل التحديث",
        "ЗАДЕРЖКА ПЕРЕХОДА" => "تأخير الانتقال",
        "Порог активации" => "حد التفعيل",
        "В ряд" => "أفقيًا",
        "Расположить мониторы горизонтально в одну линию" => "ترتيب الشاشات أفقيًا في صف واحد",
        "Сверху вниз" => "عموديًا",
        "Расположить мониторы вертикально друг над другом" => "ترتيب الشاشات عموديًا",
        "Сетка 2x2" => "شبكة 2×2",
        "Расположить мониторы сеткой 2 на 2" => "ترتيب الشاشات في شبكة 2×2",
        "Циклический переход краев (1 <-> N)" => "التفاف الانتقال عند الحواف (1 ↔ N)",
        "Собрать экраны (Сброс)" => "تجميع الشاشات (إعادة تعيين)",
        "Сбросить масштаб холста и собрать все мониторы в один ряд" => "إعادة مقياس اللوحة وتجميع الشاشات في صف واحد",
        "Быстрое выравнивание:" => "ترتيب سريع:",
        "Видеодрайвер Windows требует подтверждения активации (UAC)" => "يتطلب برنامج تشغيل Windows تأكيد التفعيل (UAC)",
        "Нажмите «Активировать драйвер», чтобы система создавала реальные виртуальные мониторы Windows" => "اضغط «تفعيل برنامج التشغيل» لإنشاء شاشات Windows افتراضية حقيقية",
        "Активировать драйвер (UAC)" => "تفعيل برنامج التشغيل (UAC)",
        "Имя экрана:" => "اسم الشاشة:",
        "Например: CODE, WEB, CHAT" => "مثال: CODE، WEB، CHAT",
        "Сохранить" => "حفظ",
        "Переключить физический экран на этот монитор" => "الانتقال إلى هذه الشاشة",
        "Забыть отключённый" => "نسيان الشاشة غير المتصلة",
        "Физический экран" => "شاشة فعلية",
        "Сначала удалите последний" => "أزل الشاشة الأخيرة أولًا",
        "Удалить" => "إزالة",
        "Сдвинуть левее" => "تحريك إلى اليسار",
        "Сдвинуть правее" => "تحريك إلى اليمين",
        "Интерактивный 2D холст (колесико мыши: зум, зажмите фон: перемещение):" => "لوحة ثنائية الأبعاد تفاعلية (عجلة الفأرة: تكبير، سحب الخلفية: تحريك):",
        "EvertyDisplay — пространственное управление физическими и виртуальными дисплеями" => "EvertyDisplay — إدارة مكانية للشاشات الفعلية والافتراضية",
        "Автор" => "المؤلف",
        "Сайт" => "الموقع",
        "Электронная почта" => "البريد الإلكتروني",
        "Версия" => "الإصدار",
        "Язык интерфейса" => "لغة الواجهة",
        "Следовать языку интерфейса Windows или выбрать его вручную." => "استخدم لغة واجهة Windows أو اختر اللغة يدويًا.",
        "Как в Windows" => "لغة Windows",
        "Русский" => "الروسية",
        "Английский" => "الإنجليزية",
        "Арабский" => "العربية",
        "Испанский" => "الإسبانية",
        "Немецкий" => "الألمانية",
        "Французский" => "الفرنسية",
        "Включить всплывающие уведомления (OSD HUD) при смене активного экрана" => "إظهار الإشعارات عند تغيير الشاشة النشطة",
        "Всплывающие уведомления (OSD HUD)" => "الإشعارات المنبثقة (OSD HUD)",
        "Показывать компактную схему экранов и выделять активный экран" => "إظهار مخطط مصغر للشاشات وتمييز الشاشة النشطة",
        "Не показывать уведомления при переключении между физическими дисплеями" => "عدم إظهار إشعارات عند الانتقال بين الشاشات الفعلية",
        "Минимум один виртуальный" => "شاشة افتراضية واحدة على الأقل",
        "Отображает полупрозрачный индикатор в центре экрана с именем монитора и подсказкой при переключении." => "يعرض مؤشرًا شفافًا جزئيًا يتضمن اسم الشاشة عند التبديل.",
        "Auto-Gaming Guard: Автоматически блокировать переход мыши в полноэкранных 3D-играх" => "حماية الألعاب التلقائية: منع انتقال المؤشر في ألعاب ملء الشاشة",
        "Игровой режим (Auto-Gaming Guard)" => "وضع الألعاب (الحماية التلقائية)",
        "Служба проверяет запуск игр в полноэкранном режиме и блокирует случайный вылет курсора на соседние мониторы. Быстрая пауза: Win+Alt+P." => "تكتشف الخدمة ألعاب ملء الشاشة وتمنع انتقال المؤشر إلى شاشة مجاورة بالخطأ. إيقاف سريع: Win+Alt+P.",
        "Smart Auto-Focus: Автоматически передавать фокус окну под курсором при переходе на монитор" => "التركيز الذكي: تركيز النافذة تحت المؤشر بعد الانتقال إلى شاشة",
        "Drag-to-Teleport: Мгновенно переносить окно на монитор при зажатой ЛКМ на краю экрана" => "السحب للنقل: نقل النافذة إلى الشاشة المجاورة عند سحبها عبر الحافة",
        "Использовать разрешение MAIN для виртуальных дисплеев (частоту не менять)" => "استخدام دقة MAIN للشاشات الافتراضية (مع الاحتفاظ بمعدل التحديث)",
        "Переходить с виртуального дисплея к окну, открывшемуся на физическом дисплее" => "الانتقال من الشاشة الافتراضية إلى نافذة فُتحت على شاشة فعلية",
        "При активации окна на виртуальном дисплее:" => "عند تنشيط نافذة على شاشة افتراضية:",
        "Перейти на дисплей" => "الانتقال إلى الشاشة",
        "Перенести окно сюда" => "إحضار النافذة إلى هنا",
        "Работает при выборе окна на панели задач и через Alt+Tab." => "يعمل عند اختيار نافذة من شريط المهام أو باستخدام Alt+Tab.",
        "Управление окнами и фокусом" => "إدارة النوافذ والتركيز",
        "Обеспечивает естественное взаимодействие с окнами при пространственном переключении мониторов." => "يوفر تفاعلًا طبيعيًا مع النوافذ أثناء التنقل بين الشاشات.",
        "Включить режим Live PiP (компактная миниатюра монитора в углу экрана)" => "تشغيل Live PiP (معاينة مصغرة للشاشة)",
        "Картинка-в-картинке (Live PiP)" => "صورة داخل صورة (Live PiP)",
        "Позволяет непрерывно видеть виртуальный экран в компактном окне. Окно можно свободно растягивать мышью за любые края и перетаскивать за центр в любое место экрана. Хоткей: Win+Alt+V." => "يعرض الشاشة الافتراضية باستمرار في نافذة صغيرة. يمكنك تغيير حجمها من الحواف وسحبها إلى أي شاشة. الاختصار: Win+Alt+V.",
        "Служба: Активна (работает)" => "الخدمة: نشطة",
        "Перезапустить службу" => "إعادة تشغيل الخدمة",
        "Служба: Не подключена" => "الخدمة: غير متصلة",
        "Запустить службу" => "تشغيل الخدمة",
        "Запускать фоновую службу EvertyDisplay автоматически при входе в Windows (HKCU Run)" => "تشغيل خدمة EvertyDisplay تلقائيًا عند تسجيل الدخول إلى Windows",
        "Автозапуск и системные службы" => "التشغيل التلقائي وخدمات النظام",
        "Служба работает в фоне в системном трее Windows без консольных окон, обеспечивая бесшовное перемещение курсора, хоткеи и виртуальные мониторы." => "تعمل الخدمة في الخلفية من علبة نظام Windows وتوفر انتقال المؤشر والاختصارات والشاشات الافتراضية.",
        "Памятка горячих клавиш EvertyDisplay" => "اختصارات EvertyDisplay",
        "Все комбинации работают глобально в любых приложениях:" => "تعمل جميع الاختصارات في كل التطبيقات:",
        "Win + Shift + Стрелки (Влево / Вправо / Вверх / Вниз)" => "Win + Shift + الأسهم (يسار / يمين / أعلى / أسفل)",
        "— Телепортация активного окна на соседний экран" => "— نقل النافذة النشطة إلى شاشة مجاورة",
        "Win + Alt + Стрелки (Влево / Вправо / Вверх / Вниз)" => "Win + Alt + الأسهم (يسار / يمين / أعلى / أسفل)",
        "— Мгновенное переключение экрана Viewport" => "— تبديل شاشة Viewport فورًا",
        "— Пауза / возобновление переключения мыши (Игровой режим)" => "— إيقاف / استئناف انتقال المؤشر (وضع الألعاب)",
        "— Переключение режима Viewport (Полный экран / PiP / Выкл)" => "— تبديل Viewport (ملء الشاشة / PiP / إيقاف)",
        "Зажатая ЛКМ на краю экрана" => "سحب بزر الفأرة الأيسر عند حافة الشاشة",
        "— Перетаскивание окна на соседний экран (Drag-to-Teleport)" => "— سحب النافذة إلى شاشة مجاورة",
        "Опасная зона: Удаление драйвера" => "منطقة خطرة: إزالة برنامج التشغيل",
        "Полное удаление драйвера виртуального монитора IddCx (MttVDD) из Windows Driver Store и системного реестра. Все виртуальные экраны будут немедленно отключены. Нажмите для запуска деинсталляции с правами Администратора." => "إزالة برنامج IddCx ‏(MttVDD) بالكامل من مخزن برامج التشغيل والسجل. ستُفصل جميع الشاشات الافتراضية فورًا، ويلزم إذن المسؤول.",
        "Удалить драйвер виртуального дисплея..." => "إزالة برنامج تشغيل الشاشة الافتراضية...",
        _ => return None,
    })
}

fn spanish_translation(russian: &str) -> Option<&'static str> {
    Some(match russian {
        "Добавить экран" => "Añadir pantalla",
        "Добавить виртуальный монитор в систему" => "Añadir una pantalla virtual a Windows",
        "Драйвер готов" => "Controlador listo",
        "Виртуальный видеоадаптер IddCx активен. Нажмите для переустановки." => "El adaptador de pantalla virtual IddCx está activo. Pulsa para reinstalarlo.",
        "Активировать драйвер" => "Activar controlador",
        "Требуется разовая системная активация для создания виртуальных экранов." => "Se requiere una activación única del sistema para crear pantallas virtuales.",
        "Мониторы и топология" => "Pantallas y topología",
        "Интерактивное пространственное расположение экранов" => "Distribución espacial interactiva de pantallas",
        "Пространственные виртуальные дисплеи" => "Pantallas virtuales espaciales",
        "Настройки и возможности" => "Ajustes y funciones",
        "Конфигурация OSD, Live PiP, автозапуска и горячих клавиш" => "Configuración de OSD, Live PiP, inicio e indicadores",
        "О нас" => "Acerca de",
        "Автор и контакты проекта" => "Autor y contacto del proyecto",
        "Светлая тема" => "Tema claro",
        "Темная тема" => "Tema oscuro",
        "Переключить тему оформления приложения" => "Cambiar el tema de la aplicación",
        "Служба активна" => "Servicio activo",
        "Служба отключена" => "Servicio desconectado",
        "Адаптер готов" => "Adaptador listo",
        "Адаптер не активен" => "Adaptador inactivo",
        "Подключение к службе EvertyDisplay..." => "Conectando con el servicio EvertyDisplay...",
        "Обновление данных..." => "Actualizando datos...",
        "Запуск фоновой службы EvertyDisplay..." => "Iniciando el servicio EvertyDisplay...",
        "Перезапуск фоновой службы EvertyDisplay..." => "Reiniciando el servicio EvertyDisplay...",
        "Удаление видеодрайвера EvertyDisplay (UAC)..." => "Eliminando el controlador de EvertyDisplay (UAC)...",
        "Активация видеодрайвера EvertyDisplay (UAC)..." => "Activando el controlador de EvertyDisplay (UAC)...",
        "Служба EvertyDisplay и видеодрайвер активны" => "El servicio y el controlador de EvertyDisplay están activos",
        "Служба активна; видеодрайвер пока не подключен" => "El servicio está activo; el controlador aún no está conectado",
        "Добавление виртуального экрана..." => "Añadiendo pantalla virtual...",
        "Расположение мониторов и масштаб холста сброшены" => "Se restablecieron la distribución y la escala del lienzo",
        "Загрузка топологии мониторов..." => "Cargando la topología de pantallas...",
        "Добавить виртуальный монитор?" => "¿Añadir una pantalla virtual?",
        "Будет добавлен новый виртуальный дисплей через драйвер. После добавления у вас будет 15 секунд чтобы подтвердить, что всё в порядке. Если не нажать «Ок» — монитор будет автоматически удалён." => "Se añadirá una pantalla virtual mediante el controlador. Tendrás 15 segundos para confirmar que funciona; de lo contrario, se eliminará automáticamente.",
        "Продолжить" => "Continuar",
        "Отмена" => "Cancelar",
        "Монитор добавлен — всё в порядке?" => "Pantalla añadida — ¿funciona correctamente?",
        "Отмена / Откатить" => "Cancelar / Revertir",
        "Активация виртуального дисплея (UAC)" => "Activación de pantalla virtual (UAC)",
        "Будет запущен сценарий PowerShell от имени Администратора для установки и регистрации драйвера виртуального монитора IddCx. Экран Windows может кратковременно моргнуть при добавлении виртуального адаптера." => "Se ejecutará PowerShell como administrador para instalar y registrar el controlador IddCx. Windows puede parpadear brevemente.",
        "Продолжить и установить (UAC)" => "Continuar e instalar (UAC)",
        "Внимание: Полное удаление драйвера виртуального дисплея" => "Advertencia: eliminación completa del controlador virtual",
        "Будет запущен сценарий деинсталляции с повышенными привилегиями (UAC). Драйвер IddCx (MttVDD) будет полностью удален из Windows Driver Store и реестра, а все активные виртуальные мониторы будут закрыты. Экран может кратковременно моргнуть." => "Una tarea con privilegios eliminará IddCx (MttVDD) del almacén de controladores y del registro. Todas las pantallas virtuales se cerrarán y la pantalla puede parpadear.",
        "Да, удалить драйвер из системы (UAC)" => "Sí, eliminar el controlador (UAC)",
        "Включить аппаратный Viewport?" => "¿Activar Viewport acelerado?",
        "Выключить Viewport?" => "¿Desactivar Viewport?",
        "Включить Viewport" => "Activar Viewport",
        "Выключить Viewport" => "Desactivar Viewport",
        "Режим аппаратного Viewport захватывает рабочий стол виртуального монитора через Direct3D 11 и отображает его на вашем основном экране с нулевой задержкой. Для быстрого сворачивания/разворачивания используйте Win + Alt + V." => "Viewport captura la pantalla virtual mediante Direct3D 11 y la muestra en la pantalla principal con latencia mínima. Usa Win+Alt+V para cambiar de modo.",
        "Viewport будет отключен. Отображение вернется к стандартному физическому рабочему столу." => "Viewport se desactivará y volverá el escritorio físico normal.",
        "Включить игровой режим (Пауза мыши)?" => "¿Activar el modo de juego?",
        "Возобновить переключение мыши?" => "¿Reanudar las transiciones del ratón?",
        "Включить режим" => "Activar modo",
        "Возобновить мышь" => "Reanudar ratón",
        "Курсор мыши будет зафиксирован в пределах текущего монитора. Это предотвращает случайный вылет курсора в 3D-играх и шутерах. Для быстрой паузы/возобновления используйте Win + Alt + P." => "El puntero permanecerá en la pantalla actual para evitar saltos accidentales durante los juegos. Usa Win+Alt+P para pausar o reanudar.",
        "Свободное пространственное перемещение курсора между мониторами будет возобновлено." => "Se reanudará el movimiento libre del puntero entre pantallas.",
        "Служба EvertyDisplay сейчас отключена" => "El servicio EvertyDisplay está desconectado",
        "Фоновая служба обеспечивает мгновенный переход мыши, OSD и виртуальные мониторы." => "El servicio proporciona transiciones del ratón, OSD y pantallas virtuales.",
        "Запустить службу EvertyDisplay" => "Iniciar servicio EvertyDisplay",
        "Активировать виртуальный дисплей в Windows (UAC)" => "Activar pantalla virtual en Windows (UAC)",
        "Топология мониторов" => "Topología de pantallas",
        "Пространственное расположение и переключение экранов" => "Distribución y cambio espacial de pantallas",
        "Конфигурация OSD, игрового режима, фокуса и автозапуска" => "Configuración de OSD, juego, foco e inicio",
        "Информация об авторе и контакты проекта" => "Información del autor y contacto",
        "Обновить" => "Actualizar",
        "Обновить конфигурацию и экраны из системы" => "Actualizar configuración y pantallas desde Windows",
        "Viewport: Вкл" => "Viewport: activado",
        "Viewport: Выкл" => "Viewport: desactivado",
        "Аппаратный захват виртуального дисплея (Win+Alt+V)" => "Captura acelerada de pantalla virtual (Win+Alt+V)",
        "Игровой режим" => "Modo de juego",
        "Временная фиксация мыши для 3D-игр (Win+Alt+P)" => "Bloquear temporalmente el ratón para juegos (Win+Alt+P)",
        "Физический" => "Física",
        "Основной" => "Principal",
        "АКТИВНЫЙ VIEWPORT" => "VIEWPORT ACTIVO",
        "ВСЕГО ДИСПЛЕЕВ" => "TOTAL DE PANTALLAS",
        "ЧАСТОТА РАЗВЕРТКИ" => "FRECUENCIA",
        "ЗАДЕРЖКА ПЕРЕХОДА" => "RETARDO DEL BORDE",
        "Порог активации" => "Umbral de activación",
        "В ряд" => "Horizontal",
        "Расположить мониторы горизонтально в одну линию" => "Colocar las pantallas en una fila horizontal",
        "Сверху вниз" => "Vertical",
        "Расположить мониторы вертикально друг над другом" => "Colocar las pantallas verticalmente",
        "Сетка 2x2" => "Cuadrícula 2×2",
        "Расположить мониторы сеткой 2 на 2" => "Colocar las pantallas en una cuadrícula 2×2",
        "Циклический переход краев (1 <-> N)" => "Transición cíclica en los bordes (1 ↔ N)",
        "Собрать экраны (Сброс)" => "Reunir pantallas (Restablecer)",
        "Сбросить масштаб холста и собрать все мониторы в один ряд" => "Restablecer el lienzo y reunir las pantallas en una fila",
        "Быстрое выравнивание:" => "Distribución rápida:",
        "Видеодрайвер Windows требует подтверждения активации (UAC)" => "El controlador de Windows requiere confirmación (UAC)",
        "Нажмите «Активировать драйвер», чтобы система создавала реальные виртуальные мониторы Windows" => "Pulsa «Activar controlador» para crear pantallas virtuales reales de Windows",
        "Активировать драйвер (UAC)" => "Activar controlador (UAC)",
        "Имя экрана:" => "Nombre de la pantalla:",
        "Например: CODE, WEB, CHAT" => "Ejemplo: CODE, WEB, CHAT",
        "Сохранить" => "Guardar",
        "Переключить физический экран на этот монитор" => "Cambiar a esta pantalla",
        "Забыть отключённый" => "Olvidar desconectada",
        "Физический экран" => "Pantalla física",
        "Сначала удалите последний" => "Elimina primero la última",
        "Удалить" => "Eliminar",
        "Сдвинуть левее" => "Mover a la izquierda",
        "Сдвинуть правее" => "Mover a la derecha",
        "Интерактивный 2D холст (колесико мыши: зум, зажмите фон: перемещение):" => "Lienzo 2D interactivo (rueda: zoom, arrastrar fondo: mover):",
        "EvertyDisplay — пространственное управление физическими и виртуальными дисплеями" => "EvertyDisplay — gestión espacial de pantallas físicas y virtuales",
        "Автор" => "Autor",
        "Сайт" => "Sitio web",
        "Электронная почта" => "Correo electrónico",
        "Версия" => "Versión",
        "Язык интерфейса" => "Idioma de la interfaz",
        "Следовать языку интерфейса Windows или выбрать его вручную." => "Usa el idioma de Windows o selecciónalo manualmente.",
        "Как в Windows" => "Idioma de Windows",
        "Русский" => "Ruso",
        "Английский" => "Inglés",
        "Арабский" => "Árabe",
        "Испанский" => "Español",
        "Немецкий" => "Alemán",
        "Французский" => "Francés",
        "Включить всплывающие уведомления (OSD HUD) при смене активного экрана" => "Mostrar notificaciones al cambiar la pantalla activa",
        "Всплывающие уведомления (OSD HUD)" => "Notificaciones emergentes (OSD HUD)",
        "Показывать компактную схему экранов и выделять активный экран" => "Mostrar un esquema compacto y resaltar la pantalla activa",
        "Не показывать уведомления при переключении между физическими дисплеями" => "No mostrar notificaciones entre pantallas físicas",
        "Минимум один виртуальный" => "Al menos una pantalla virtual",
        "Отображает полупрозрачный индикатор в центре экрана с именем монитора и подсказкой при переключении." => "Muestra un indicador semitransparente con el nombre de la pantalla al cambiar.",
        "Auto-Gaming Guard: Автоматически блокировать переход мыши в полноэкранных 3D-играх" => "Protección de juego: bloquear transiciones en juegos a pantalla completa",
        "Игровой режим (Auto-Gaming Guard)" => "Modo de juego (protección automática)",
        "Служба проверяет запуск игр в полноэкранном режиме и блокирует случайный вылет курсора на соседние мониторы. Быстрая пауза: Win+Alt+P." => "El servicio detecta juegos a pantalla completa y evita saltos accidentales del puntero. Pausa rápida: Win+Alt+P.",
        "Smart Auto-Focus: Автоматически передавать фокус окну под курсором при переходе на монитор" => "Foco inteligente: enfocar la ventana bajo el puntero al cambiar de pantalla",
        "Drag-to-Teleport: Мгновенно переносить окно на монитор при зажатой ЛКМ на краю экрана" => "Arrastrar para mover: enviar la ventana a la pantalla vecina al cruzar el borde",
        "Использовать разрешение MAIN для виртуальных дисплеев (частоту не менять)" => "Usar la resolución de MAIN en pantallas virtuales (mantener su frecuencia)",
        "Переходить с виртуального дисплея к окну, открывшемуся на физическом дисплее" => "Seguir una ventana que se abra en una pantalla física",
        "При активации окна на виртуальном дисплее:" => "Al activar una ventana en una pantalla virtual:",
        "Перейти на дисплей" => "Ir a la pantalla",
        "Перенести окно сюда" => "Traer la ventana aquí",
        "Работает при выборе окна на панели задач и через Alt+Tab." => "Funciona desde la barra de tareas y con Alt+Tab.",
        "Управление окнами и фокусом" => "Gestión de ventanas y foco",
        "Обеспечивает естественное взаимодействие с окнами при пространственном переключении мониторов." => "Permite trabajar con las ventanas de forma natural entre pantallas.",
        "Включить режим Live PiP (компактная миниатюра монитора в углу экрана)" => "Activar Live PiP (vista previa compacta)",
        "Картинка-в-картинке (Live PiP)" => "Imagen en imagen (Live PiP)",
        "Позволяет непрерывно видеть виртуальный экран в компактном окне. Окно можно свободно растягивать мышью за любые края и перетаскивать за центр в любое место экрана. Хоткей: Win+Alt+V." => "Muestra la pantalla virtual en una ventana compacta. Puedes cambiar su tamaño y arrastrarla a cualquier pantalla. Atajo: Win+Alt+V.",
        "Служба: Активна (работает)" => "Servicio: activo",
        "Перезапустить службу" => "Reiniciar servicio",
        "Служба: Не подключена" => "Servicio: desconectado",
        "Запустить службу" => "Iniciar servicio",
        "Запускать фоновую службу EvertyDisplay автоматически при входе в Windows (HKCU Run)" => "Iniciar EvertyDisplay automáticamente al entrar en Windows",
        "Автозапуск и системные службы" => "Inicio automático y servicios",
        "Служба работает в фоне в системном трее Windows без консольных окон, обеспечивая бесшовное перемещение курсора, хоткеи и виртуальные мониторы." => "El servicio se ejecuta en la bandeja de Windows y proporciona transiciones, atajos y pantallas virtuales.",
        "Памятка горячих клавиш EvertyDisplay" => "Atajos de EvertyDisplay",
        "Все комбинации работают глобально в любых приложениях:" => "Todos los atajos funcionan en cualquier aplicación:",
        "Win + Shift + Стрелки (Влево / Вправо / Вверх / Вниз)" => "Win + Shift + Flechas (izquierda / derecha / arriba / abajo)",
        "— Телепортация активного окна на соседний экран" => "— Mover la ventana activa a una pantalla vecina",
        "Win + Alt + Стрелки (Влево / Вправо / Вверх / Вниз)" => "Win + Alt + Flechas (izquierda / derecha / arriba / abajo)",
        "— Мгновенное переключение экрана Viewport" => "— Cambiar instantáneamente la pantalla de Viewport",
        "— Пауза / возобновление переключения мыши (Игровой режим)" => "— Pausar / reanudar transiciones (modo de juego)",
        "— Переключение режима Viewport (Полный экран / PiP / Выкл)" => "— Cambiar Viewport (pantalla completa / PiP / apagado)",
        "Зажатая ЛКМ на краю экрана" => "Mantener el botón izquierdo al borde",
        "— Перетаскивание окна на соседний экран (Drag-to-Teleport)" => "— Arrastrar una ventana a una pantalla vecina",
        "Опасная зона: Удаление драйвера" => "Zona peligrosa: eliminar controlador",
        "Полное удаление драйвера виртуального монитора IddCx (MttVDD) из Windows Driver Store и системного реестра. Все виртуальные экраны будут немедленно отключены. Нажмите для запуска деинсталляции с правами Администратора." => "Elimina por completo IddCx (MttVDD) del almacén de controladores y del registro. Todas las pantallas virtuales se desconectarán inmediatamente. Se requieren permisos de administrador.",
        "Удалить драйвер виртуального дисплея..." => "Eliminar controlador de pantalla virtual...",
        _ => return None,
    })
}

fn german_translation(english: &str) -> String {
    let exact = match english {
        "Add display" => "Bildschirm hinzufügen",
        "Driver ready" => "Treiber bereit",
        "Activate driver" => "Treiber aktivieren",
        "Displays and topology" => "Bildschirme und Topologie",
        "Settings and features" => "Einstellungen und Funktionen",
        "About" => "Über uns",
        "Light theme" => "Helles Design",
        "Dark theme" => "Dunkles Design",
        "Service active" => "Dienst aktiv",
        "Service offline" => "Dienst offline",
        "Adapter ready" => "Adapter bereit",
        "Adapter inactive" => "Adapter inaktiv",
        "Continue" => "Weiter",
        "Cancel" => "Abbrechen",
        "Cancel / Revert" => "Abbrechen / Rückgängig",
        "Enable Viewport" => "Viewport aktivieren",
        "Disable Viewport" => "Viewport deaktivieren",
        "Enable mode" => "Modus aktivieren",
        "Resume mouse" => "Mausübergänge fortsetzen",
        "Refresh" => "Aktualisieren",
        "Viewport: On" => "Viewport: Ein",
        "Viewport: Off" => "Viewport: Aus",
        "Gaming Mode" => "Spielmodus",
        "Physical" => "Physisch",
        "Primary" => "Primär",
        "ACTIVE VIEWPORT" => "AKTIVER VIEWPORT",
        "TOTAL DISPLAYS" => "BILDSCHIRME GESAMT",
        "REFRESH RATE" => "BILDWIEDERHOLRATE",
        "EDGE DELAY" => "RANDVERZÖGERUNG",
        "Activation threshold" => "Aktivierungsschwelle",
        "Horizontal" => "Horizontal",
        "Vertical" => "Vertikal",
        "2x2 grid" => "2x2-Raster",
        "Quick arrangement:" => "Schnelle Anordnung:",
        "Display name:" => "Bildschirmname:",
        "Save" => "Speichern",
        "Forget disconnected" => "Getrennten vergessen",
        "Physical display" => "Physischer Bildschirm",
        "Remove the last one first" => "Zuerst den letzten entfernen",
        "Remove" => "Entfernen",
        "Move left" => "Nach links verschieben",
        "Move right" => "Nach rechts verschieben",
        "Author" => "Autor",
        "Website" => "Webseite",
        "Email" => "E-Mail",
        "Version" => "Version",
        "Interface language" => "Oberflächensprache",
        "Windows default" => "Wie in Windows",
        "Russian" => "Russisch",
        "English" => "Englisch",
        "Arabic" => "Arabisch",
        "Spanish" => "Spanisch",
        "German" => "Deutsch",
        "French" => "Französisch",
        "At least one virtual display" => "Mindestens ein virtueller Bildschirm",
        "Switch to display" => "Zum Bildschirm wechseln",
        "Bring window here" => "Fenster hierher verschieben",
        "Window and focus control" => "Fenster- und Fokussteuerung",
        "Service: Active (running)" => "Dienst: Aktiv",
        "Restart service" => "Dienst neu starten",
        "Service: Not connected" => "Dienst: Nicht verbunden",
        "Start service" => "Dienst starten",
        "EvertyDisplay hotkeys" => "EvertyDisplay-Tastenkürzel",
        "Danger zone: Remove driver" => "Gefahrenbereich: Treiber entfernen",
        "Remove virtual display driver..." => "Treiber für virtuelle Bildschirme entfernen...",
        _ => return replace_phrases(english, GERMAN_PHRASES),
    };
    exact.to_string()
}

const GERMAN_PHRASES: &[(&str, &str)] = &[
    ("virtual display", "virtuellen Bildschirm"),
    ("Virtual display", "Virtueller Bildschirm"),
    ("virtual displays", "virtuelle Bildschirme"),
    ("display topology", "Bildschirmtopologie"),
    ("display layout", "Bildschirmanordnung"),
    ("display", "Bildschirm"),
    ("Display", "Bildschirm"),
    ("screens", "Bildschirme"),
    ("screen", "Bildschirm"),
    ("service", "Dienst"),
    ("Service", "Dienst"),
    ("driver", "Treiber"),
    ("Driver", "Treiber"),
    ("window", "Fenster"),
    ("Window", "Fenster"),
    ("mouse transitions", "Mausübergänge"),
    ("mouse", "Maus"),
    ("pointer", "Mauszeiger"),
    ("settings", "Einstellungen"),
    ("Settings", "Einstellungen"),
    ("configuration", "Konfiguration"),
    ("startup", "Autostart"),
    ("Start", "Starten"),
    ("Enable", "Aktivieren"),
    ("Disable", "Deaktivieren"),
    ("Switch", "Wechseln"),
    ("Move", "Verschieben"),
    ("Show", "Anzeigen"),
    ("Hide", "Ausblenden"),
    ("Remove", "Entfernen"),
    ("Adding", "Hinzufügen"),
    ("Loading", "Laden"),
    ("Starting", "Starten"),
    ("Connecting", "Verbinden"),
    ("active", "aktiv"),
    ("inactive", "inaktiv"),
    ("offline", "offline"),
    ("fullscreen", "Vollbild"),
    ("physical", "physisch"),
    ("virtual", "virtuell"),
    ("adjacent", "benachbart"),
    ("left", "links"),
    ("right", "rechts"),
    ("top", "oben"),
    ("bottom", "unten"),
];

fn french_translation(english: &str) -> String {
    let exact = match english {
        "Add display" => "Ajouter un écran",
        "Driver ready" => "Pilote prêt",
        "Activate driver" => "Activer le pilote",
        "Displays and topology" => "Écrans et topologie",
        "Settings and features" => "Paramètres et fonctionnalités",
        "About" => "À propos",
        "Light theme" => "Thème clair",
        "Dark theme" => "Thème sombre",
        "Service active" => "Service actif",
        "Service offline" => "Service hors ligne",
        "Adapter ready" => "Adaptateur prêt",
        "Adapter inactive" => "Adaptateur inactif",
        "Continue" => "Continuer",
        "Cancel" => "Annuler",
        "Cancel / Revert" => "Annuler / Rétablir",
        "Enable Viewport" => "Activer Viewport",
        "Disable Viewport" => "Désactiver Viewport",
        "Enable mode" => "Activer le mode",
        "Resume mouse" => "Réactiver la souris",
        "Refresh" => "Actualiser",
        "Viewport: On" => "Viewport : activé",
        "Viewport: Off" => "Viewport : désactivé",
        "Gaming Mode" => "Mode jeu",
        "Physical" => "Physique",
        "Primary" => "Principal",
        "ACTIVE VIEWPORT" => "VIEWPORT ACTIF",
        "TOTAL DISPLAYS" => "TOTAL DES ÉCRANS",
        "REFRESH RATE" => "FRÉQUENCE D’ACTUALISATION",
        "EDGE DELAY" => "DÉLAI DU BORD",
        "Activation threshold" => "Seuil d’activation",
        "Horizontal" => "Horizontal",
        "Vertical" => "Vertical",
        "2x2 grid" => "Grille 2x2",
        "Quick arrangement:" => "Disposition rapide :",
        "Display name:" => "Nom de l’écran :",
        "Save" => "Enregistrer",
        "Forget disconnected" => "Oublier l’écran déconnecté",
        "Physical display" => "Écran physique",
        "Remove the last one first" => "Supprimez d’abord le dernier",
        "Remove" => "Supprimer",
        "Move left" => "Déplacer à gauche",
        "Move right" => "Déplacer à droite",
        "Author" => "Auteur",
        "Website" => "Site web",
        "Email" => "E-mail",
        "Version" => "Version",
        "Interface language" => "Langue de l’interface",
        "Windows default" => "Comme dans Windows",
        "Russian" => "Russe",
        "English" => "Anglais",
        "Arabic" => "Arabe",
        "Spanish" => "Espagnol",
        "German" => "Allemand",
        "French" => "Français",
        "At least one virtual display" => "Au moins un écran virtuel",
        "Switch to display" => "Passer à l’écran",
        "Bring window here" => "Déplacer la fenêtre ici",
        "Window and focus control" => "Gestion des fenêtres et du focus",
        "Service: Active (running)" => "Service : actif",
        "Restart service" => "Redémarrer le service",
        "Service: Not connected" => "Service : non connecté",
        "Start service" => "Démarrer le service",
        "EvertyDisplay hotkeys" => "Raccourcis EvertyDisplay",
        "Danger zone: Remove driver" => "Zone dangereuse : supprimer le pilote",
        "Remove virtual display driver..." => "Supprimer le pilote d’écran virtuel...",
        _ => return replace_phrases(english, FRENCH_PHRASES),
    };
    exact.to_string()
}

const FRENCH_PHRASES: &[(&str, &str)] = &[
    ("virtual display", "écran virtuel"),
    ("Virtual display", "Écran virtuel"),
    ("virtual displays", "écrans virtuels"),
    ("display topology", "topologie des écrans"),
    ("display layout", "disposition des écrans"),
    ("display", "écran"),
    ("Display", "Écran"),
    ("screens", "écrans"),
    ("screen", "écran"),
    ("service", "service"),
    ("Service", "Service"),
    ("driver", "pilote"),
    ("Driver", "Pilote"),
    ("window", "fenêtre"),
    ("Window", "Fenêtre"),
    ("mouse transitions", "transitions de la souris"),
    ("mouse", "souris"),
    ("pointer", "pointeur"),
    ("settings", "paramètres"),
    ("Settings", "Paramètres"),
    ("configuration", "configuration"),
    ("startup", "démarrage automatique"),
    ("Start", "Démarrer"),
    ("Enable", "Activer"),
    ("Disable", "Désactiver"),
    ("Switch", "Changer"),
    ("Move", "Déplacer"),
    ("Show", "Afficher"),
    ("Hide", "Masquer"),
    ("Remove", "Supprimer"),
    ("Adding", "Ajout"),
    ("Loading", "Chargement"),
    ("Starting", "Démarrage"),
    ("Connecting", "Connexion"),
    ("active", "actif"),
    ("inactive", "inactif"),
    ("offline", "hors ligne"),
    ("fullscreen", "plein écran"),
    ("physical", "physique"),
    ("virtual", "virtuel"),
    ("adjacent", "voisin"),
    ("left", "gauche"),
    ("right", "droite"),
    ("top", "haut"),
    ("bottom", "bas"),
];

fn replace_phrases(input: &str, replacements: &[(&str, &str)]) -> String {
    replacements
        .iter()
        .fold(input.to_string(), |text, (from, to)| text.replace(from, to))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static LANGUAGE_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn explicit_language_is_applied_immediately() {
        let _language_guard = LANGUAGE_TEST_LOCK.lock().unwrap();
        apply(LanguagePreference::English);
        assert_eq!(translate("Добавить экран"), "Add display");
        apply(LanguagePreference::Arabic);
        assert_eq!(translate("Добавить экран"), "إضافة شاشة");
        assert_eq!(translate("Арабский"), "العربية");
        assert_eq!(confirmation_button(15), "موافق (15 ث)");
        apply(LanguagePreference::Spanish);
        assert_eq!(translate("Добавить экран"), "Añadir pantalla");
        assert_eq!(translate("Испанский"), "Español");
        assert_eq!(confirmation_button(15), "Aceptar (15s)");
        apply(LanguagePreference::German);
        assert_eq!(translate("Добавить экран"), "Bildschirm hinzufügen");
        assert_eq!(translate("Немецкий"), "Deutsch");
        assert_eq!(confirmation_button(15), "OK (15s)");
        apply(LanguagePreference::French);
        assert_eq!(translate("Добавить экран"), "Ajouter un écran");
        assert_eq!(translate("Французский"), "Français");
        assert_eq!(confirmation_button(15), "OK (15s)");
        apply(LanguagePreference::Russian);
        assert_eq!(translate("Добавить экран"), "Добавить экран");
    }

    #[test]
    fn windows_language_detection_covers_arabic_regions() {
        assert_eq!(language_from_windows_lang_id(0x0401), Language::Arabic);
        assert_eq!(language_from_windows_lang_id(0x0c01), Language::Arabic);
        assert_eq!(language_from_windows_lang_id(0x0419), Language::Russian);
        assert_eq!(language_from_windows_lang_id(0x0409), Language::English);
        assert_eq!(language_from_windows_lang_id(0x0c0a), Language::Spanish);
        assert_eq!(language_from_windows_lang_id(0x0407), Language::German);
        assert_eq!(language_from_windows_lang_id(0x040c), Language::French);
    }

    #[test]
    fn removal_progress_and_results_follow_selected_language() {
        let _language_guard = LANGUAGE_TEST_LOCK.lock().unwrap();
        apply(LanguagePreference::English);
        assert!(removing_display(3).starts_with("Removing"));
        assert!(removal_progress(3).contains("30 seconds"));
        assert!(display_removed(3).contains("was removed"));
        assert!(display_remove_failed(3, "driver error").contains("driver error"));

        apply(LanguagePreference::Arabic);
        assert!(removing_display(3).contains('3'));
        apply(LanguagePreference::Spanish);
        assert!(removal_progress(3).contains("30 segundos"));
        apply(LanguagePreference::German);
        assert!(display_removed(3).contains("entfernt"));
        apply(LanguagePreference::French);
        assert!(display_remove_failed(3, "erreur").contains("erreur"));
        apply(LanguagePreference::Russian);
    }
}
