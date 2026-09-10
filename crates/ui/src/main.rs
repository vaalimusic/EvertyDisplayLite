#![windows_subsystem = "windows"]

mod assets;
mod canvas;
mod driver_device;
mod i18n;
mod ipc_client;
mod styles;

use assets::*;
use i18n::LanguagePreference;
use iced::widget::{
    button, canvas::Canvas, checkbox, column, container, horizontal_space, image, mouse_area, row,
    scrollable, slider, text as iced_text, text_input, tooltip, Space,
};
use iced::{Alignment, Color, Element, Length, Subscription, Task, Theme};
use multitor_ipc::{
    ArrangeMode, DisplayInfo, IpcRequest, IpcResponse, ProductCapabilities, ProductEdition,
    TopologyConfig, VirtualWindowActivationAction,
};
use std::process::Command;
use std::time::{Duration, Instant};
use styles::{ThemeMode, DANGER, PRIMARY, SUCCESS};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

const CREATE_NO_WINDOW: u32 = 0x08000000;
const APP_WINDOW_TITLE: &str = "EvertyDisplay";

fn text<'a>(content: impl Into<std::borrow::Cow<'a, str>>) -> iced::widget::Text<'a> {
    iced_text(i18n::translate(content)).shaping(iced::widget::text::Shaping::Advanced)
}

mod text {
    pub use iced::widget::text::Style;
}

fn set_service_autostart(enable: bool) {
    #[cfg(windows)]
    unsafe {
        use windows::core::w;
        use windows::Win32::System::Registry::{
            RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
            KEY_WRITE, REG_SZ,
        };

        let run_key = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
        let app_name = w!("EvertyDisplayService");
        let old_name = w!("Multitor");

        let mut hkey = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, run_key, 0, KEY_WRITE, &mut hkey).is_ok() {
            let _ = RegDeleteValueW(hkey, old_name);
            if enable {
                if let Ok(cur_exe) = std::env::current_exe() {
                    let svc_path = cur_exe
                        .parent()
                        .map(|d| d.join("multitor-service.exe"))
                        .unwrap_or_else(|| cur_exe.clone());
                    let val = format!("\"{}\"", svc_path.to_string_lossy());
                    let wide: Vec<u16> = val.encode_utf16().chain(std::iter::once(0)).collect();
                    let byte_slice =
                        std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2);
                    let _ = RegSetValueExW(hkey, app_name, 0, REG_SZ, Some(byte_slice));
                }
            } else {
                let _ = RegDeleteValueW(hkey, app_name);
            }
            let _ = RegCloseKey(hkey);
        }
    }
}

fn find_driver_dir() -> Option<std::path::PathBuf> {
    if let Ok(cur_exe) = std::env::current_exe() {
        if let Some(parent) = cur_exe.parent() {
            let direct = parent.join("driver");
            if direct.exists() {
                return Some(direct);
            }
            if let Some(proj_root) = parent.parent().and_then(|p| p.parent()) {
                let from_root = proj_root.join("driver");
                if from_root.exists() {
                    return Some(from_root);
                }
            }
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        let from_cwd = cwd.join("driver");
        if from_cwd.exists() {
            return Some(from_cwd);
        }
    }
    None
}

fn run_native_driver_installer() -> i32 {
    // A normal application update must not touch a healthy display adapter or
    // its live XML. Reinstalling or rewriting an active IDD causes avoidable
    // display loss, races the driver settings writer and can create duplicates.
    if multitor_driver_manager::is_driver_pipe_ready() {
        return 0;
    }

    // Keep an existing installation's monitor count and custom options intact,
    // but merge newly supported refresh rates into its global mode list. The
    // driver is known to be stopped here, so updating the file cannot race it.
    let installed_settings = std::path::Path::new(r"C:\VirtualDisplayDriver\vdd_settings.xml");
    if let Ok(existing) = std::fs::read_to_string(installed_settings) {
        if let Some(updated) = add_global_refresh_rate_xml(&existing, 180) {
            let _ = std::fs::write(installed_settings, updated);
        }
    }

    let driver_dir = match find_driver_dir() {
        Some(d) => d,
        None => return 1,
    };

    let inf = driver_dir.join(r"signed_x64\MttVDD.inf");
    let settings = driver_dir.join(r"signed_x64\vdd_settings.xml");
    if !inf.is_file() {
        return 4;
    }

    // The driver pipe can be temporarily unavailable while an update is stopping
    // the old application. Device presence is authoritative: reinstalling an
    // already registered IDD can create another adapter and extra Windows screens.
    let installed_device_exists = match driver_device::is_present() {
        Ok(present) => present,
        // Never guess "absent" when SetupAPI cannot enumerate devices: doing
        // so could create a duplicate adapter during an update.
        Err(_) => return 3,
    };
    if installed_device_exists {
        // Repair the existing stopped device in place. Never install a second one.
        if driver_device::restart_existing().is_err() {
            return 3;
        }

        for _ in 0..40 {
            if multitor_driver_manager::is_driver_pipe_ready() {
                return 0;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        return 3;
    }

    // 1. Prepare C:\VirtualDisplayDriver for a genuinely new installation.
    let target_dir = std::path::Path::new(r"C:\VirtualDisplayDriver");
    let _ = std::fs::create_dir_all(target_dir);
    let target_settings = target_dir.join("vdd_settings.xml");
    let existing_settings_valid = std::fs::read_to_string(&target_settings)
        .map(|content| content.contains("<count>") && content.contains("</count>"))
        .unwrap_or(false);
    if target_settings.exists() && !existing_settings_valid {
        let _ = std::fs::rename(
            &target_settings,
            target_dir.join("vdd_settings.invalid.bak"),
        );
    }
    if settings.exists() && !existing_settings_valid {
        let _ = std::fs::copy(&settings, &target_settings);
    } else if !target_settings.exists() {
        let default_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<vdd_settings>
    <monitors><count>1</count></monitors>
    <gpu><friendlyname>default</friendlyname></gpu>
    <resolutions>
        <resolution><width>1920</width><height>1080</height><refresh_rate>60</refresh_rate><refresh_rate>180</refresh_rate></resolution>
        <resolution><width>2560</width><height>1440</height><refresh_rate>60</refresh_rate><refresh_rate>144</refresh_rate><refresh_rate>180</refresh_rate></resolution>
    </resolutions>
</vdd_settings>"#;
        let _ = std::fs::write(&target_settings, default_xml);
    }

    // 2. Register VDDPATH in Windows Registry
    #[cfg(windows)]
    unsafe {
        use windows::core::w;
        use windows::Win32::System::Registry::{
            RegCloseKey, RegCreateKeyExW, RegSetValueExW, HKEY, HKEY_LOCAL_MACHINE, KEY_WRITE,
            REG_OPTION_NON_VOLATILE, REG_SZ,
        };

        let reg_path = w!("SOFTWARE\\VirtualDisplayDriver");
        let val_name = w!("VDDPATH");
        let val_str = r"C:\VirtualDisplayDriver\";
        let val_wide: Vec<u16> = val_str.encode_utf16().chain(std::iter::once(0)).collect();
        let byte_slice =
            std::slice::from_raw_parts(val_wide.as_ptr() as *const u8, val_wide.len() * 2);

        let mut hkey = HKEY::default();
        if RegCreateKeyExW(
            HKEY_LOCAL_MACHINE,
            reg_path,
            0,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut hkey,
            None,
        )
        .is_ok()
        {
            let _ = RegSetValueExW(hkey, val_name, 0, REG_SZ, Some(byte_slice));
            let _ = RegCloseKey(hkey);
        }
    }

    // 3. Create the root device through SetupAPI and install its signed INF
    // with the inbox PnPUtil. No WDK redistributables are shipped.
    if driver_device::install(&inf).is_err() {
        return 2;
    }

    for _ in 0..40 {
        if multitor_driver_manager::is_driver_pipe_ready() {
            return 0;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    3
}

fn add_global_refresh_rate_xml(content: &str, refresh_rate: u32) -> Option<String> {
    let global_start = content.find("<global>")?;
    let relative_end = content[global_start..].find("</global>")?;
    let global_end = global_start + relative_end;
    let rate_tag = format!("<g_refresh_rate>{refresh_rate}</g_refresh_rate>");
    if content[global_start..global_end].contains(&rate_tag) {
        return None;
    }
    let mut updated = String::with_capacity(content.len() + rate_tag.len() + 4);
    updated.push_str(&content[..global_end]);
    updated.push_str(&format!("\t\t{rate_tag}\r\n\t"));
    updated.push_str(&content[global_end..]);
    Some(updated)
}

fn run_native_driver_uninstaller() -> i32 {
    if driver_device::remove_all().is_err() {
        return 2;
    }

    // Clean registry
    #[cfg(windows)]
    unsafe {
        use windows::core::w;
        use windows::Win32::System::Registry::{RegDeleteKeyW, HKEY_LOCAL_MACHINE};
        let reg_path = w!("SOFTWARE\\VirtualDisplayDriver");
        let _ = RegDeleteKeyW(HKEY_LOCAL_MACHINE, reg_path);
    }

    // Clean C:\VirtualDisplayDriver
    let target_dir = std::path::Path::new(r"C:\VirtualDisplayDriver");
    // Only remove the directory when it still looks like EvertyDisplay's own
    // driver state. Never recursively delete an arbitrary reused folder.
    if target_dir.join("vdd_settings.xml").is_file() {
        let _ = std::fs::remove_file(target_dir.join("vdd_settings.xml"));
        let _ = std::fs::remove_dir(target_dir);
    }

    // Remove only packages whose original INF file is exactly MttVDD.inf.
    // Provider-name wildcard deletion is intentionally forbidden because it
    // can match unrelated virtual display products.
    let package_cleanup = r#"$ErrorActionPreference='Stop'; try { Get-WindowsDriver -Online -All | Where-Object { [IO.Path]::GetFileName($_.OriginalFileName) -ieq 'MttVDD.inf' } | ForEach-Object { & pnputil.exe /delete-driver $_.Driver /uninstall /force | Out-Null; if ($LASTEXITCODE -ne 0) { throw "pnputil failed: $LASTEXITCODE" } }; exit 0 } catch { exit 2 }"#;
    let mut cleanup = Command::new("powershell.exe");
    cleanup.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        package_cleanup,
    ]);
    #[cfg(windows)]
    cleanup.creation_flags(CREATE_NO_WINDOW);
    if !matches!(cleanup.status(), Ok(status) if status.success()) {
        return 2;
    }

    0
}

fn elevate_and_install_driver() -> bool {
    let cur_exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return false,
    };

    #[cfg(windows)]
    unsafe {
        use windows::core::PCWSTR;
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

        let file: Vec<u16> = cur_exe
            .to_string_lossy()
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let verb: Vec<u16> = "runas\0".encode_utf16().collect();
        let param_str = "--install-driver\0";
        let params: Vec<u16> = param_str.encode_utf16().collect();
        let dir_str = format!(
            "{}\0",
            cur_exe
                .parent()
                .map(|p| p.display().to_string())
                .unwrap_or_default()
        );
        let dir: Vec<u16> = dir_str.encode_utf16().collect();

        let res = ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(file.as_ptr()),
            PCWSTR(params.as_ptr()),
            PCWSTR(dir.as_ptr()),
            SW_HIDE,
        );
        (res.0 as usize) > 32
    }
    #[cfg(not(windows))]
    false
}

fn elevate_and_uninstall_driver() -> bool {
    let cur_exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return false,
    };

    #[cfg(windows)]
    unsafe {
        use windows::core::PCWSTR;
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

        let file: Vec<u16> = cur_exe
            .to_string_lossy()
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let verb: Vec<u16> = "runas\0".encode_utf16().collect();
        let param_str = "--uninstall-driver\0";
        let params: Vec<u16> = param_str.encode_utf16().collect();
        let dir_str = format!(
            "{}\0",
            cur_exe
                .parent()
                .map(|p| p.display().to_string())
                .unwrap_or_default()
        );
        let dir: Vec<u16> = dir_str.encode_utf16().collect();

        let res = ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(file.as_ptr()),
            PCWSTR(params.as_ptr()),
            PCWSTR(dir.as_ptr()),
            SW_HIDE,
        );
        (res.0 as usize) > 32
    }
    #[cfg(not(windows))]
    false
}

fn find_script_path(script_name: &str) -> Option<std::path::PathBuf> {
    if let Ok(cur_exe) = std::env::current_exe() {
        if let Some(parent) = cur_exe.parent() {
            let direct = parent.join(script_name);
            if direct.exists() {
                return Some(direct);
            }
            if let Some(proj_root) = parent.parent().and_then(|p| p.parent()) {
                let from_root = proj_root.join(script_name);
                if from_root.exists() {
                    return Some(from_root);
                }
            }
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        let from_cwd = cwd.join(script_name);
        if from_cwd.exists() {
            return Some(from_cwd);
        }
    }
    None
}

fn run_script_elevated(script_name: &str) -> bool {
    let script_path = match find_script_path(script_name) {
        Some(p) => p,
        None => return false,
    };

    #[cfg(windows)]
    unsafe {
        use windows::core::PCWSTR;
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

        let file: Vec<u16> = "powershell.exe\0".encode_utf16().collect();
        let verb: Vec<u16> = "runas\0".encode_utf16().collect();
        let param_str = format!(
            "-NoProfile -ExecutionPolicy Bypass -File \"{}\"\0",
            script_path.display()
        );
        let params: Vec<u16> = param_str.encode_utf16().collect();
        let dir_str = format!(
            "{}\0",
            script_path
                .parent()
                .map(|p| p.display().to_string())
                .unwrap_or_default()
        );
        let dir: Vec<u16> = dir_str.encode_utf16().collect();

        let res = ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(file.as_ptr()),
            PCWSTR(params.as_ptr()),
            PCWSTR(dir.as_ptr()),
            SW_SHOWNORMAL,
        );
        (res.0 as usize) > 32
    }
    #[cfg(not(windows))]
    false
}

fn find_service_executable() -> Result<std::path::PathBuf, String> {
    let mut candidates = Vec::new();
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(dir) = current_exe.parent() {
            candidates.push(dir.join("multitor-service.exe"));
            if let Some(root) = dir.parent().and_then(|p| p.parent()) {
                candidates.push(root.join("target/release/multitor-service.exe"));
                candidates.push(root.join("target/debug/multitor-service.exe"));
            }
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("target/release/multitor-service.exe"));
        candidates.push(cwd.join("target/debug/multitor-service.exe"));
    }

    candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| {
            "рядом с EvertyDisplay.exe не найден multitor-service.exe; восстановите установку программы"
                .to_string()
        })
}

async fn ensure_service_ready() -> Result<(), String> {
    async fn responds() -> bool {
        matches!(
            tokio::time::timeout(
                Duration::from_millis(500),
                ipc_client::send_ipc_request(&IpcRequest::Ping),
            )
            .await,
            Ok(Ok(IpcResponse::Pong))
        )
    }

    if responds().await {
        return Ok(());
    }

    let service_path = find_service_executable()?;
    let mut command = Command::new(&service_path);
    command.current_dir(
        service_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new(".")),
    );
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    command
        .spawn()
        .map_err(|error| format!("не удалось запустить фоновую службу: {error}"))?;

    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(150)).await;
        if responds().await {
            return Ok(());
        }
    }

    Err("фоновая служба была запущена, но не стала готова за 10 секунд".to_string())
}

async fn bootstrap_and_fetch(
) -> Result<(TopologyConfig, Vec<DisplayInfo>, ProductCapabilities), String> {
    ensure_service_ready().await?;
    fetch_data().await
}

pub fn main() -> iced::Result {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--install-driver") {
        let code = run_native_driver_installer();
        std::process::exit(code);
    }
    if args.iter().any(|a| a == "--uninstall-driver") {
        let code = run_native_driver_uninstaller();
        std::process::exit(code);
    }
    if args.iter().any(|a| a == "--enable-autostart") {
        set_service_autostart(true);
        std::process::exit(0);
    }
    if args.iter().any(|a| a == "--disable-autostart") {
        set_service_autostart(false);
        std::process::exit(0);
    }

    // A second launch activates the existing editor instead of creating another
    // taskbar window and another topology polling loop.
    #[cfg(windows)]
    let _ui_instance_mutex = unsafe {
        use windows::core::{w, PCWSTR};
        use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
        use windows::Win32::System::Threading::CreateMutexW;
        use windows::Win32::UI::WindowsAndMessaging::{
            FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE,
        };

        let mutex = CreateMutexW(None, false, w!("Local\\EvertyDisplay.Ui.Singleton"));
        let already_running = mutex.is_ok() && GetLastError() == ERROR_ALREADY_EXISTS;
        if already_running {
            let title: Vec<u16> = APP_WINDOW_TITLE
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            if let Ok(hwnd) = FindWindowW(None, PCWSTR(title.as_ptr())) {
                let _ = ShowWindow(hwnd, SW_RESTORE);
                let _ = SetForegroundWindow(hwnd);
            }
            return Ok(());
        }
        mutex.ok()
    };

    #[cfg(windows)]
    unsafe {
        let _ = windows::Win32::System::Console::SetConsoleOutputCP(65001);
        let _ = windows::Win32::System::Console::SetConsoleCP(65001);
        let _ = windows::Win32::UI::HiDpi::SetProcessDpiAwarenessContext(
            windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        );
    }

    let icon = iced::window::icon::from_file_data(LOGO_ICON, None).ok();

    iced::application(APP_WINDOW_TITLE, App::update, App::view)
        .window(iced::window::Settings {
            icon,
            min_size: Some(iced::Size::new(1080.0, 720.0)),
            size: iced::Size::new(1280.0, 800.0),
            decorations: false,
            ..Default::default()
        })
        .subscription(App::subscription)
        .theme(|app| match app.theme_mode {
            ThemeMode::Dark => Theme::Dark,
            ThemeMode::Light => Theme::Light,
        })
        .run_with(App::new)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Monitors,
    Settings,
    About,
}

#[derive(Debug, Clone)]
pub enum Message {
    Tick,
    RefreshData,
    DataLoaded(Result<(TopologyConfig, Vec<DisplayInfo>, ProductCapabilities), String>),
    StartService,
    ActivateDriver,
    ToggleTheme,
    SetLanguage(LanguagePreference),
    SetTab(Tab),
    SelectMonitor(u32),
    SwitchToMonitor(u32),
    MonitorActionResult(Result<(), String>),
    TogglePause(bool),
    ToggleViewport(bool),
    ToggleWrap(bool),
    ChangeMonitorName(String),
    ChangeEdgeDelay(u64),
    ToggleOsd(bool),
    ToggleOsdLayout(bool),
    ToggleOsdPhysicalSwitches(bool),
    ChangeOsdDuration(u32),
    ToggleGamingGuard(bool),
    ToggleSmartFocus(bool),
    ToggleDragTeleport(bool),
    ToggleMatchVirtualMode(bool),
    ToggleFollowPhysicalWindow(bool),
    SetVirtualWindowActivation(VirtualWindowActivationAction),
    TogglePip(bool),
    ChangePipScale(u32),
    ToggleAutostart(bool),
    SaveTopology,
    AddVirtualMonitor,
    RemoveMonitor(u32),
    ForgetMonitor(u32),
    MoveLeft(u32),
    MoveRight(u32),
    AutoArrange(ArrangeMode),
    ResetLayout,
    UpdateMonitorPosition { id: u32, x: i32, y: i32 },
    // Add monitor confirmation flow
    RequestAddMonitor, // User clicked "Добавить экран" - show warning first
    ConfirmAddMonitor, // User clicked "Продолжить" in warning → actually add
    AddedMonitorConfirmStart(u32), // IPC returned after add → enter countdown with the new monitor's id
    AddedMonitorResult(Result<u32, String>),
    AddedMonitorConfirmTick, // Countdown tick every second after monitor added
    KeepAddedMonitor,        // User clicked OK in 15s confirm → keep it
    RevertAddedMonitor,      // User clicked Cancel OR timeout → remove last added monitor
    CancelAddMonitor,        // User clicked Cancel in initial warning banner
    // Window controls for custom titlebar
    WindowIdFound(Option<iced::window::Id>),
    MinimizeWindow,
    ToggleMaximizeWindow,
    CloseWindow,
    DragWindow,
    // Feature confirmation dialogs
    RequestActivateDriver,
    ConfirmActivateDriver,
    RequestUninstallDriver,
    ConfirmUninstallDriver,
    RestartService,
    RequestToggleViewport(bool),
    ConfirmToggleViewport(bool),
    RequestToggleGaming(bool),
    ConfirmToggleGaming(bool),
    DismissConfirmDialog,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConfirmDialogState {
    None,
    ActivateDriver,
    UninstallDriver,
    ToggleViewport(bool),
    ToggleGaming(bool),
}

/// State of the "add monitor" confirmation flow.
#[derive(Debug, Clone, PartialEq)]
pub enum AddMonitorFlowState {
    /// Nothing pending.
    Idle,
    /// Warning shown, waiting for user to confirm or cancel.
    WarnPending,
    /// Monitor was added; countdown to auto-revert if user doesn't click OK.
    Confirming { secs_left: u8, added_id: u32 },
}

pub struct App {
    is_connected: bool,
    is_driver_ready: bool,
    theme_mode: ThemeMode,
    language: LanguagePreference,
    active_tab: Tab,
    topology: Option<TopologyConfig>,
    displays: Vec<DisplayInfo>,
    product_capabilities: ProductCapabilities,
    selected_id: Option<u32>,
    status_text: String,
    editing_name: String,
    add_flow: AddMonitorFlowState,
    confirm_dialog: ConfirmDialogState,
    window_id: Option<iced::window::Id>,
    canvas_reset_tag: u32,
    is_refreshing: bool,
}

impl App {
    pub fn new() -> (Self, Task<Message>) {
        let language = i18n::initialize();
        (
            Self {
                is_connected: false,
                is_driver_ready: multitor_driver_manager::is_driver_pipe_ready(),
                theme_mode: ThemeMode::Light,
                language,
                active_tab: Tab::Monitors,
                topology: None,
                displays: Vec::new(),
                product_capabilities: ProductCapabilities {
                    edition: ProductEdition::Lite,
                    max_virtual_displays: 1,
                    multi_display_layouts: false,
                    cloud_features: false,
                },
                selected_id: None,
                status_text: "Подключение к службе EvertyDisplay...".to_string(),
                editing_name: String::new(),
                add_flow: AddMonitorFlowState::Idle,
                confirm_dialog: ConfirmDialogState::None,
                window_id: None,
                canvas_reset_tag: 0,
                is_refreshing: true,
            },
            Task::batch([
                Task::perform(bootstrap_and_fetch(), Message::DataLoaded),
                iced::window::get_latest().map(Message::WindowIdFound),
            ]),
        )
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let base = iced::time::every(Duration::from_millis(1500)).map(|_| Message::Tick);
        let win_events = iced::window::open_events().map(|id| Message::WindowIdFound(Some(id)));
        if matches!(self.add_flow, AddMonitorFlowState::Confirming { .. }) {
            let countdown =
                iced::time::every(Duration::from_secs(1)).map(|_| Message::AddedMonitorConfirmTick);
            Subscription::batch([base, countdown, win_events])
        } else {
            Subscription::batch([base, win_events])
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tick => {
                if self.is_refreshing {
                    Task::none()
                } else {
                    self.is_refreshing = true;
                    if self.is_connected {
                        Task::perform(fetch_data(), Message::DataLoaded)
                    } else {
                        Task::perform(bootstrap_and_fetch(), Message::DataLoaded)
                    }
                }
            }
            Message::RefreshData => {
                self.status_text = "Обновление данных...".to_string();
                self.is_refreshing = true;
                Task::perform(fetch_data(), Message::DataLoaded)
            }
            Message::ToggleTheme => {
                self.theme_mode = match self.theme_mode {
                    ThemeMode::Dark => ThemeMode::Light,
                    ThemeMode::Light => ThemeMode::Dark,
                };
                Task::none()
            }
            Message::SetLanguage(language) => {
                self.language = language;
                i18n::apply(language);
                if let Err(error) = i18n::save_preference(language) {
                    tracing::warn!(%error, "failed to save UI language");
                }
                Task::none()
            }
            Message::StartService => {
                self.status_text = "Запуск фоновой службы EvertyDisplay...".to_string();
                self.is_refreshing = true;
                Task::perform(bootstrap_and_fetch(), Message::DataLoaded)
            }
            Message::RestartService => {
                self.status_text = "Перезапуск фоновой службы EvertyDisplay...".to_string();
                let mut kill_cmd = Command::new("taskkill");
                kill_cmd.args(["/F", "/IM", "multitor-service.exe"]);
                #[cfg(windows)]
                kill_cmd.creation_flags(CREATE_NO_WINDOW);
                let _ = kill_cmd.status();

                Task::perform(
                    async {
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    },
                    |_| Message::StartService,
                )
            }
            Message::RequestUninstallDriver => {
                self.confirm_dialog = ConfirmDialogState::UninstallDriver;
                Task::none()
            }
            Message::ConfirmUninstallDriver => {
                self.confirm_dialog = ConfirmDialogState::None;
                self.status_text = "Удаление видеодрайвера EvertyDisplay (UAC)...".to_string();
                let launched = elevate_and_uninstall_driver();
                if !launched {
                    let _ = run_script_elevated("uninstall-display.ps1");
                }
                Task::perform(
                    async {
                        tokio::time::sleep(Duration::from_millis(3500)).await;
                    },
                    |_| Message::RefreshData,
                )
            }
            Message::RequestActivateDriver => {
                self.confirm_dialog = ConfirmDialogState::ActivateDriver;
                Task::none()
            }
            Message::ConfirmActivateDriver => {
                self.confirm_dialog = ConfirmDialogState::None;
                self.update(Message::ActivateDriver)
            }
            Message::RequestToggleViewport(val) => {
                self.confirm_dialog = ConfirmDialogState::ToggleViewport(val);
                Task::none()
            }
            Message::ConfirmToggleViewport(val) => {
                self.confirm_dialog = ConfirmDialogState::None;
                self.update(Message::ToggleViewport(val))
            }
            Message::RequestToggleGaming(val) => {
                self.confirm_dialog = ConfirmDialogState::ToggleGaming(val);
                Task::none()
            }
            Message::ConfirmToggleGaming(val) => {
                self.confirm_dialog = ConfirmDialogState::None;
                self.update(Message::TogglePause(val))
            }
            Message::DismissConfirmDialog => {
                self.confirm_dialog = ConfirmDialogState::None;
                Task::none()
            }
            Message::ActivateDriver => {
                self.status_text = "Активация видеодрайвера EvertyDisplay (UAC)...".to_string();
                let launched = elevate_and_install_driver();
                if !launched {
                    let _ = run_script_elevated("install-display.ps1");
                }
                Task::perform(
                    async {
                        tokio::time::sleep(Duration::from_millis(4000)).await;
                        ensure_service_ready().await?;
                        fetch_data().await
                    },
                    Message::DataLoaded,
                )
            }
            Message::DataLoaded(res) => {
                self.is_refreshing = false;
                self.is_driver_ready = multitor_driver_manager::is_driver_pipe_ready();
                match res {
                    Ok((top, disps, capabilities)) => {
                        self.is_connected = true;
                        self.displays = disps;
                        self.product_capabilities = capabilities;

                        if self.selected_id.is_none()
                            || !top.monitors.iter().any(|m| Some(m.id) == self.selected_id)
                        {
                            self.selected_id = top
                                .active_monitor_id
                                .or_else(|| top.monitors.first().map(|m| m.id));
                            if let Some(sel) = self.selected_id {
                                if let Some(m) = top.monitors.iter().find(|m| m.id == sel) {
                                    self.editing_name = m.name.clone();
                                }
                            }
                        }

                        self.topology = Some(top);
                        self.status_text = if self.is_driver_ready {
                            "Служба EvertyDisplay и видеодрайвер активны".to_string()
                        } else {
                            "Служба активна; видеодрайвер пока не подключен".to_string()
                        };
                    }
                    Err(err) => {
                        self.is_connected = false;
                        self.status_text = format!("Служба EvertyDisplay отключена ({})", err);
                    }
                }
                Task::none()
            }
            Message::SetTab(tab) => {
                self.active_tab = tab;
                Task::none()
            }
            Message::SelectMonitor(id) => {
                self.selected_id = Some(id);
                if let Some(top) = &self.topology {
                    if let Some(m) = top.monitors.iter().find(|m| m.id == id) {
                        self.editing_name = m.name.clone();
                    }
                }
                Task::none()
            }
            Message::SwitchToMonitor(id) => Task::perform(
                send_monitor_action(IpcRequest::SetActiveMonitor(id)),
                Message::MonitorActionResult,
            ),
            Message::MonitorActionResult(result) => match result {
                Ok(()) => Task::perform(fetch_data(), Message::DataLoaded),
                Err(message) => {
                    self.status_text = format!("Операция с монитором не выполнена: {message}");
                    Task::none()
                }
            },
            Message::TogglePause(pause) => {
                if let Some(top) = &mut self.topology {
                    top.pause_switching = pause;
                }
                Task::perform(
                    async move {
                        let _ = ipc_client::send_ipc_request(&IpcRequest::SetPauseSwitching(pause))
                            .await;
                    },
                    |_| Message::RefreshData,
                )
            }
            Message::ToggleViewport(enabled) => {
                if let Some(top) = &mut self.topology {
                    top.viewport_enabled = enabled;
                }
                Task::perform(
                    async move {
                        let _ =
                            ipc_client::send_ipc_request(&IpcRequest::SetViewportEnabled(enabled))
                                .await;
                    },
                    |_| Message::RefreshData,
                )
            }
            Message::ToggleWrap(wrap) => {
                if let Some(top) = &mut self.topology {
                    top.wrap_around = wrap;
                }
                self.update(Message::SaveTopology)
            }
            Message::ChangeMonitorName(new_name) => {
                self.editing_name = new_name.clone();
                if let (Some(top), Some(sel_id)) = (&mut self.topology, self.selected_id) {
                    if let Some(m) = top.monitors.iter_mut().find(|m| m.id == sel_id) {
                        m.name = new_name;
                    }
                }
                Task::none()
            }
            Message::ChangeEdgeDelay(delay) => {
                if let (Some(top), Some(sel_id)) = (&mut self.topology, self.selected_id) {
                    if let Some(m) = top.monitors.iter_mut().find(|m| m.id == sel_id) {
                        m.edge_activation_delay_ms = delay;
                    }
                }
                Task::none()
            }
            Message::ToggleOsd(enabled) => {
                if let Some(top) = &mut self.topology {
                    top.osd_enabled = enabled;
                }
                self.update(Message::SaveTopology)
            }
            Message::ToggleOsdLayout(enabled) => {
                if let Some(top) = &mut self.topology {
                    top.osd_show_layout = enabled;
                }
                self.update(Message::SaveTopology)
            }
            Message::ToggleOsdPhysicalSwitches(enabled) => {
                if let Some(top) = &mut self.topology {
                    top.osd_hide_physical_to_physical = enabled;
                }
                self.update(Message::SaveTopology)
            }
            Message::ChangeOsdDuration(duration) => {
                if let Some(top) = &mut self.topology {
                    top.osd_duration_ms = duration;
                }
                Task::none()
            }
            Message::ToggleGamingGuard(enabled) => {
                if let Some(top) = &mut self.topology {
                    top.gaming_guard_enabled = enabled;
                }
                self.update(Message::SaveTopology)
            }
            Message::ToggleSmartFocus(enabled) => {
                if let Some(top) = &mut self.topology {
                    top.smart_focus_enabled = enabled;
                }
                self.update(Message::SaveTopology)
            }
            Message::ToggleDragTeleport(enabled) => {
                if let Some(top) = &mut self.topology {
                    top.drag_teleport_enabled = enabled;
                }
                self.update(Message::SaveTopology)
            }
            Message::ToggleMatchVirtualMode(enabled) => {
                if let Some(top) = &mut self.topology {
                    top.match_virtual_mode_to_primary = enabled;
                }
                self.update(Message::SaveTopology)
            }
            Message::ToggleFollowPhysicalWindow(enabled) => {
                if let Some(top) = &mut self.topology {
                    top.follow_physical_window_activation = enabled;
                }
                self.update(Message::SaveTopology)
            }
            Message::SetVirtualWindowActivation(action) => {
                if let Some(top) = &mut self.topology {
                    top.virtual_window_activation_action = action;
                }
                self.update(Message::SaveTopology)
            }
            Message::TogglePip(enabled) => {
                if let Some(top) = &mut self.topology {
                    top.pip_enabled = enabled;
                }
                self.update(Message::SaveTopology)
            }
            Message::ChangePipScale(scale) => {
                if let Some(top) = &mut self.topology {
                    top.pip_scale_percent = scale;
                }
                Task::none()
            }
            Message::ToggleAutostart(enabled) => {
                if let Some(top) = &mut self.topology {
                    top.autostart_enabled = enabled;
                }
                set_service_autostart(enabled);
                self.update(Message::SaveTopology)
            }
            // Step 1: User clicks button → show warning first
            Message::RequestAddMonitor => {
                let virtual_count = self
                    .topology
                    .as_ref()
                    .map(|topology| {
                        topology
                            .monitors
                            .iter()
                            .filter(|monitor| monitor.is_virtual && monitor.is_enabled)
                            .count() as u32
                    })
                    .unwrap_or(0);
                if virtual_count >= self.product_capabilities.max_virtual_displays {
                    self.status_text =
                        i18n::virtual_display_limit(self.product_capabilities.max_virtual_displays);
                    return Task::none();
                }
                self.add_flow = AddMonitorFlowState::WarnPending;
                Task::none()
            }
            Message::ConfirmAddMonitor => {
                self.add_flow = AddMonitorFlowState::Idle;
                let next_number = self
                    .topology
                    .as_ref()
                    .map(|t| t.monitors.len() + 1)
                    .unwrap_or(1);
                let name = i18n::display_name(next_number);
                let (width, height, refresh_rate) = if let Some(top) = &self.topology {
                    let primary_device = self
                        .displays
                        .iter()
                        .find(|display| display.is_primary)
                        .map(|display| display.device_name.as_str());
                    primary_device
                        .and_then(|device| {
                            top.monitors
                                .iter()
                                .find(|monitor| monitor.device_name.eq_ignore_ascii_case(device))
                        })
                        .or_else(|| top.monitors.iter().find(|monitor| !monitor.is_virtual))
                        .or_else(|| top.monitors.first())
                        .map(|m| (m.bounds.width, m.bounds.height, m.refresh_rate))
                        .unwrap_or((2560, 1440, 60))
                } else {
                    (2560, 1440, 60)
                };

                let driver_ready = multitor_driver_manager::is_driver_pipe_ready();
                if !driver_ready {
                    self.status_text = "Активация видеодрайвера EvertyDisplay (UAC)...".to_string();
                    let launched = elevate_and_install_driver();
                    if !launched {
                        let _ = run_script_elevated("install-display.ps1");
                    }

                    return Task::perform(
                        async move {
                            // Wait for UAC and driver initialization
                            tokio::time::sleep(Duration::from_millis(4000)).await;
                            ensure_service_ready().await?;
                            let response = ipc_client::send_ipc_request(&IpcRequest::AddMonitor {
                                name,
                                width,
                                height,
                                refresh_rate,
                            })
                            .await
                            .map_err(|e| e.to_string())?;
                            match response {
                                IpcResponse::MonitorAdded(id) => Ok(id),
                                IpcResponse::Error(message) => Err(message),
                                _ => Err("Служба вернула неожиданный ответ".to_string()),
                            }
                        },
                        Message::AddedMonitorResult,
                    );
                }

                self.status_text = "Добавление виртуального экрана...".to_string();
                Task::perform(
                    async move {
                        ensure_service_ready().await?;
                        let response = ipc_client::send_ipc_request(&IpcRequest::AddMonitor {
                            name,
                            width,
                            height,
                            refresh_rate,
                        })
                        .await
                        .map_err(|e| e.to_string())?;
                        match response {
                            IpcResponse::MonitorAdded(id) => Ok(id),
                            IpcResponse::Error(message) => Err(message),
                            _ => Err("Служба вернула неожиданный ответ".to_string()),
                        }
                    },
                    Message::AddedMonitorResult,
                )
            }
            Message::AddedMonitorResult(result) => match result {
                Ok(new_id) => self.update(Message::AddedMonitorConfirmStart(new_id)),
                Err(message) => {
                    self.add_flow = AddMonitorFlowState::Idle;
                    self.status_text = format!("Не удалось добавить экран: {message}");
                    Task::none()
                }
            },
            Message::AddedMonitorConfirmStart(new_id) => {
                self.add_flow = AddMonitorFlowState::Confirming {
                    secs_left: 15,
                    added_id: new_id,
                };
                Task::perform(fetch_data(), Message::DataLoaded)
            }
            // Cancel warning dialog
            Message::CancelAddMonitor | Message::AddVirtualMonitor => {
                self.add_flow = AddMonitorFlowState::Idle;
                Task::none()
            }
            // Countdown tick every second
            Message::AddedMonitorConfirmTick => {
                if let AddMonitorFlowState::Confirming {
                    secs_left,
                    added_id,
                } = self.add_flow.clone()
                {
                    if secs_left <= 1 {
                        // Time's up → auto-revert
                        return self.update(Message::RevertAddedMonitor);
                    }
                    self.add_flow = AddMonitorFlowState::Confirming {
                        secs_left: secs_left - 1,
                        added_id,
                    };
                }
                Task::none()
            }
            // User confirmed "OK" → keep the monitor
            Message::KeepAddedMonitor => {
                let id = if let AddMonitorFlowState::Confirming { added_id, .. } = self.add_flow {
                    added_id
                } else {
                    self.add_flow = AddMonitorFlowState::Idle;
                    return Task::none();
                };
                self.add_flow = AddMonitorFlowState::Idle;
                Task::perform(
                    send_monitor_action(IpcRequest::ConfirmMonitor(id)),
                    Message::MonitorActionResult,
                )
            }
            // User cancelled or timed out → remove the added monitor
            Message::RevertAddedMonitor => {
                let id = if let AddMonitorFlowState::Confirming { added_id, .. } = self.add_flow {
                    added_id
                } else {
                    self.add_flow = AddMonitorFlowState::Idle;
                    return Task::none();
                };
                self.add_flow = AddMonitorFlowState::Idle;
                Task::perform(
                    send_monitor_action(IpcRequest::RemoveMonitor(id)),
                    Message::MonitorActionResult,
                )
            }
            Message::RemoveMonitor(id) => Task::perform(
                send_monitor_action(IpcRequest::RemoveMonitor(id)),
                Message::MonitorActionResult,
            ),
            Message::ForgetMonitor(id) => {
                if let Some(top) = &mut self.topology {
                    top.monitors
                        .retain(|monitor| monitor.id != id || monitor.is_enabled);
                    multitor_ipc::recompute_neighbors(&mut top.monitors, top.wrap_around);
                    if self.selected_id == Some(id) {
                        self.selected_id = top.active_monitor_id;
                    }
                }
                self.update(Message::SaveTopology)
            }
            Message::MoveLeft(id) => Task::perform(
                async move {
                    let _ = ipc_client::send_ipc_request(&IpcRequest::MoveMonitorLeft(id)).await;
                },
                |_| Message::RefreshData,
            ),
            Message::MoveRight(id) => Task::perform(
                async move {
                    let _ = ipc_client::send_ipc_request(&IpcRequest::MoveMonitorRight(id)).await;
                },
                |_| Message::RefreshData,
            ),
            Message::AutoArrange(mode) => {
                if mode == ArrangeMode::Grid2x2 && !self.product_capabilities.multi_display_layouts
                {
                    self.status_text =
                        i18n::virtual_display_limit(self.product_capabilities.max_virtual_displays);
                    Task::none()
                } else {
                    Task::perform(
                        async move {
                            let _ =
                                ipc_client::send_ipc_request(&IpcRequest::AutoArrange(mode)).await;
                        },
                        |_| Message::RefreshData,
                    )
                }
            }
            Message::ResetLayout => {
                if let Some(top) = &mut self.topology {
                    let mut cur_x = 0;
                    top.monitors.sort_by_key(|m| m.layout_bounds().x);
                    for m in &mut top.monitors {
                        m.layout_x = Some(cur_x);
                        m.layout_y = Some(0);
                        cur_x += m.bounds.width as i32;
                    }
                    let wrap = top.wrap_around;
                    multitor_ipc::recompute_neighbors(&mut top.monitors, wrap);
                }
                self.canvas_reset_tag = self.canvas_reset_tag.wrapping_add(1);
                self.status_text = "Расположение мониторов и масштаб холста сброшены".to_string();
                self.update(Message::SaveTopology)
            }
            Message::UpdateMonitorPosition { id, x, y } => {
                if let Some(top) = &mut self.topology {
                    if let Some(m) = top.monitors.iter_mut().find(|m| m.id == id) {
                        m.layout_x = Some(x);
                        m.layout_y = Some(y);
                    }
                    multitor_ipc::recompute_neighbors(&mut top.monitors, top.wrap_around);
                }
                self.update(Message::SaveTopology)
            }
            Message::SaveTopology => {
                if let Some(top) = self.topology.clone() {
                    Task::perform(
                        send_monitor_action(IpcRequest::UpdateTopology(top)),
                        Message::MonitorActionResult,
                    )
                } else {
                    Task::none()
                }
            }
            Message::WindowIdFound(id) => {
                self.window_id = id;
                Task::none()
            }
            Message::MinimizeWindow => {
                if let Some(id) = self.window_id {
                    iced::window::minimize(id, true)
                } else {
                    iced::window::get_latest().and_then(|id| iced::window::minimize(id, true))
                }
            }
            Message::ToggleMaximizeWindow => {
                if let Some(id) = self.window_id {
                    iced::window::toggle_maximize(id)
                } else {
                    iced::window::get_latest().and_then(iced::window::toggle_maximize)
                }
            }
            Message::CloseWindow => {
                if let Some(id) = self.window_id {
                    iced::window::close(id)
                } else {
                    iced::window::get_latest().and_then(iced::window::close)
                }
            }
            Message::DragWindow => {
                if let Some(id) = self.window_id {
                    iced::window::drag(id)
                } else {
                    iced::window::get_latest().and_then(iced::window::drag)
                }
            }
        }
    }

    pub fn view_titlebar(&self) -> Element<'_, Message> {
        let icon_color = if self.theme_mode == ThemeMode::Dark {
            "#F8FAFC"
        } else {
            "#17223B"
        };

        let app_brand = row![
            image(image::Handle::from_bytes(LOGO_ICON))
                .width(Length::Fixed(18.0))
                .height(Length::Fixed(18.0)),
            text("EvertyDisplay").size(13).style(move |_| text::Style {
                color: Some(self.theme_mode.text_primary()),
            }),
        ]
        .spacing(6)
        .align_y(Alignment::Center);

        // Keep one uninterrupted drag surface from the left edge through all free titlebar space.
        // Window buttons remain outside this mouse area and retain their normal behavior.
        let draggable_bar = mouse_area(
            row![Space::with_width(12), app_brand, horizontal_space(),]
                .width(Length::Fill)
                .height(Length::Fill)
                .align_y(Alignment::Center),
        )
        .on_press(Message::DragWindow);

        let btn_min = button(
            container(render_svg(ICON_MINUS, 12.0, Some(icon_color)))
                .center_x(Length::Fill)
                .center_y(Length::Fill),
        )
        .style(move |_, status| styles::window_control_button(self.theme_mode, false, status))
        .width(Length::Fixed(44.0))
        .height(Length::Fixed(32.0))
        .on_press(Message::MinimizeWindow);

        let btn_max = button(
            container(render_svg(ICON_MAXIMIZE, 11.0, Some(icon_color)))
                .center_x(Length::Fill)
                .center_y(Length::Fill),
        )
        .style(move |_, status| styles::window_control_button(self.theme_mode, false, status))
        .width(Length::Fixed(44.0))
        .height(Length::Fixed(32.0))
        .on_press(Message::ToggleMaximizeWindow);

        let btn_close = button(
            container(render_svg(ICON_CLOSE, 12.0, Some(icon_color)))
                .center_x(Length::Fill)
                .center_y(Length::Fill),
        )
        .style(move |_, status| styles::window_control_button(self.theme_mode, true, status))
        .width(Length::Fixed(44.0))
        .height(Length::Fixed(32.0))
        .on_press(Message::CloseWindow);

        let controls = row![btn_min, btn_max, btn_close].align_y(Alignment::Center);

        container(row![draggable_bar, controls,].align_y(Alignment::Center))
            .height(Length::Fixed(38.0))
            .width(Length::Fill)
            .style(move |_| styles::titlebar_style(self.theme_mode))
            .into()
    }

    pub fn view(&self) -> Element<'_, Message> {
        let titlebar = self.view_titlebar();
        let sidebar = self.view_sidebar();
        let content = self.view_content();

        let main_body = row![
            sidebar,
            container(content)
                .width(Length::Fill)
                .height(Length::Fill)
                .padding([16, 20])
                .style(|_| container::Style {
                    background: Some(iced::Background::Color(self.theme_mode.bg_app())),
                    ..Default::default()
                }),
        ]
        .height(Length::Fill);

        column![titlebar, main_body,]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn view_sidebar(&self) -> Element<'_, Message> {
        let logo = row![
            image(image::Handle::from_bytes(LOGO_IN_APP))
                .width(Length::Fixed(38.0))
                .height(Length::Fixed(38.0)),
            column![
                text("EvertyDisplay").size(18),
                text("Пространственные виртуальные дисплеи")
                    .size(11)
                    .style(|_| text::Style {
                        color: Some(self.theme_mode.text_muted()),
                    }),
            ]
            .spacing(1),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let virtual_count = self
            .topology
            .as_ref()
            .map(|topology| {
                topology
                    .monitors
                    .iter()
                    .filter(|monitor| monitor.is_virtual && monitor.is_enabled)
                    .count() as u32
            })
            .unwrap_or(0);
        let can_add = virtual_count < self.product_capabilities.max_virtual_displays;
        let add_btn = tooltip(
            button(
                row![
                    render_svg(ICON_PLUS, 16.0, Some("#FFFFFF")),
                    text("Добавить экран").size(14),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .style(|_, status| styles::primary_button(status))
            .padding([10, 16])
            .width(Length::Fill)
            .on_press_maybe(can_add.then_some(Message::RequestAddMonitor)),
            text(if can_add {
                "Добавить виртуальный монитор в систему".to_string()
            } else {
                i18n::virtual_display_limit(self.product_capabilities.max_virtual_displays)
            })
            .size(11),
            tooltip::Position::Right,
        )
        .gap(8)
        .padding(8)
        .style(|_| styles::tooltip_style(self.theme_mode));

        let driver_btn = if self.is_driver_ready {
            tooltip(
                button(
                    row![
                        render_svg(ICON_STATUS_CHECK, 14.0, Some("#22C55E")),
                        text("Драйвер готов").size(12).style(|_| text::Style {
                            color: Some(Color::from_rgb(0.13, 0.77, 0.37)),
                        }),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .style(|_, status| styles::secondary_button(self.theme_mode, status))
                .padding([8, 14])
                .width(Length::Fill)
                .on_press(Message::RequestActivateDriver),
                text("Виртуальный видеоадаптер IddCx активен. Нажмите для переустановки.").size(11),
                tooltip::Position::Right,
            )
            .gap(8)
            .padding(8)
            .style(|_| styles::tooltip_style(self.theme_mode))
        } else {
            tooltip(
                button(
                    row![
                        render_svg(ICON_ZAP, 14.0, Some("#F59E0B")),
                        text("Активировать драйвер")
                            .size(12)
                            .style(|_| text::Style {
                                color: Some(Color::from_rgb(0.96, 0.62, 0.04)),
                            }),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .style(|_, status| styles::secondary_button(self.theme_mode, status))
                .padding([8, 14])
                .width(Length::Fill)
                .on_press(Message::RequestActivateDriver),
                text("Требуется разовая системная активация для создания виртуальных экранов.")
                    .size(11),
                tooltip::Position::Right,
            )
            .gap(8)
            .padding(8)
            .style(|_| styles::tooltip_style(self.theme_mode))
        };

        let is_monitors_active = self.active_tab == Tab::Monitors;
        let nav_monitors = tooltip(
            button(
                row![
                    render_svg(
                        ICON_GRID,
                        16.0,
                        Some(if is_monitors_active {
                            "#5B4CFF"
                        } else {
                            "#667085"
                        }),
                    ),
                    text("Мониторы и топология").size(14),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            )
            .style(move |_, status| {
                styles::nav_item_button(self.theme_mode, is_monitors_active, status)
            })
            .padding([10, 14])
            .width(Length::Fill)
            .on_press(Message::SetTab(Tab::Monitors)),
            text("Интерактивное пространственное расположение экранов").size(11),
            tooltip::Position::Right,
        )
        .gap(8)
        .padding(8)
        .style(|_| styles::tooltip_style(self.theme_mode));

        let is_settings_active = self.active_tab == Tab::Settings;
        let nav_settings = tooltip(
            button(
                row![
                    render_svg(
                        ICON_SETTINGS,
                        16.0,
                        Some(if is_settings_active {
                            "#5B4CFF"
                        } else {
                            "#667085"
                        }),
                    ),
                    text("Настройки и возможности").size(14),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            )
            .style(move |_, status| {
                styles::nav_item_button(self.theme_mode, is_settings_active, status)
            })
            .padding([10, 14])
            .width(Length::Fill)
            .on_press(Message::SetTab(Tab::Settings)),
            text("Конфигурация OSD, Live PiP, автозапуска и горячих клавиш").size(11),
            tooltip::Position::Right,
        )
        .gap(8)
        .padding(8)
        .style(|_| styles::tooltip_style(self.theme_mode));

        let is_about_active = self.active_tab == Tab::About;
        let nav_about = tooltip(
            button(
                row![
                    render_svg(
                        ICON_INFO,
                        16.0,
                        Some(if is_about_active {
                            "#5B4CFF"
                        } else {
                            "#667085"
                        }),
                    ),
                    text("О нас").size(14),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            )
            .style(move |_, status| {
                styles::nav_item_button(self.theme_mode, is_about_active, status)
            })
            .padding([10, 14])
            .width(Length::Fill)
            .on_press(Message::SetTab(Tab::About)),
            text("Автор и контакты проекта").size(11),
            tooltip::Position::Right,
        )
        .gap(8)
        .padding(8)
        .style(|_| styles::tooltip_style(self.theme_mode));

        let icon_color = if self.theme_mode == ThemeMode::Dark {
            "#F8FAFC"
        } else {
            "#17223B"
        };
        let (theme_icon, theme_label) = match self.theme_mode {
            ThemeMode::Dark => (ICON_SUN, "Светлая тема"),
            ThemeMode::Light => (ICON_MOON, "Темная тема"),
        };
        let theme_btn = tooltip(
            button(
                row![
                    render_svg(theme_icon, 14.0, Some(icon_color)),
                    text(theme_label).size(12),
                ]
                .align_y(Alignment::Center)
                .spacing(6),
            )
            .style(|_, status| styles::secondary_button(self.theme_mode, status))
            .padding([6, 12])
            .width(Length::Fill)
            .on_press(Message::ToggleTheme),
            text("Переключить тему оформления приложения").size(11),
            tooltip::Position::Right,
        )
        .gap(8)
        .padding(8)
        .style(|_| styles::tooltip_style(self.theme_mode));

        let status_color = if self.is_connected { SUCCESS } else { DANGER };
        let status_text = if self.is_connected {
            "Служба активна"
        } else {
            "Служба отключена"
        };
        let driver_status_color = if self.is_driver_ready {
            SUCCESS
        } else {
            Color::from_rgb(0.96, 0.62, 0.04)
        };
        let driver_status_text = if self.is_driver_ready {
            "Адаптер готов"
        } else {
            "Адаптер не активен"
        };

        let status_card = container(
            column![
                row![
                    container(Space::with_width(8))
                        .height(Length::Fixed(8.0))
                        .width(Length::Fixed(8.0))
                        .style(move |_| container::Style {
                            background: Some(iced::Background::Color(status_color)),
                            border: iced::border::Border {
                                radius: 4.0.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        }),
                    text(status_text).size(12).style(move |_| text::Style {
                        color: Some(status_color),
                    }),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                row![
                    container(Space::with_width(8))
                        .height(Length::Fixed(8.0))
                        .width(Length::Fixed(8.0))
                        .style(move |_| container::Style {
                            background: Some(iced::Background::Color(driver_status_color)),
                            border: iced::border::Border {
                                radius: 4.0.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        }),
                    text(driver_status_text)
                        .size(12)
                        .style(move |_| text::Style {
                            color: Some(driver_status_color),
                        }),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                Space::with_height(4),
                theme_btn,
            ]
            .spacing(6),
        )
        .padding(12)
        .width(Length::Fill)
        .style(|_| styles::card_style(self.theme_mode));

        container(
            column![
                logo,
                Space::with_height(14),
                add_btn,
                Space::with_height(4),
                driver_btn,
                Space::with_height(18),
                nav_monitors,
                Space::with_height(4),
                nav_settings,
                Space::with_height(4),
                nav_about,
                Space::with_height(Length::Fill),
                status_card,
            ]
            .spacing(4)
            .width(Length::Fill)
            .height(Length::Fill),
        )
        .width(Length::Fixed(270.0))
        .height(Length::Fill)
        .padding(18)
        .style(|_| styles::sidebar_style(self.theme_mode))
        .into()
    }

    fn view_content(&self) -> Element<'_, Message> {
        if self.active_tab == Tab::About {
            return self.view_about_tab();
        }
        if !self.is_connected {
            return self.view_disconnected();
        }

        let Some(top) = &self.topology else {
            return container(text("Загрузка топологии мониторов...").size(18))
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into();
        };

        let header = self.view_header();
        let stats = self.view_stat_cards(top);
        let tab_content = match self.active_tab {
            Tab::Monitors => self.view_monitors_tab(top),
            Tab::Settings => self.view_settings_tab(top),
            Tab::About => unreachable!("about tab is handled before topology-dependent content"),
        };

        // --- Add Monitor confirmation flow banners ---
        let flow_banner: Option<Element<'_, Message>> = match &self.add_flow {
            AddMonitorFlowState::WarnPending => {
                let warn_card = container(
                    column![
                        row![
                            render_svg(ICON_STATUS_WARN, 20.0, Some("#F59E0B")),
                            text("Добавить виртуальный монитор?").size(16).style(|_| text::Style {
                                color: Some(self.theme_mode.text_primary()),
                            }),
                        ].spacing(8).align_y(Alignment::Center),
                        text("Будет добавлен новый виртуальный дисплей через драйвер. После добавления у вас будет 15 секунд чтобы подтвердить, что всё в порядке. Если не нажать «Ок» — монитор будет автоматически удалён.").size(12).style(|_| text::Style {
                            color: Some(self.theme_mode.text_muted()),
                        }),
                        Space::with_height(4),
                        row![
                            button(text("Продолжить").size(13))
                                .style(|_, status| styles::primary_button(status))
                                .padding([8, 18])
                                .on_press(Message::ConfirmAddMonitor),
                            button(text("Отмена").size(13))
                                .style(|_, status| styles::secondary_button(self.theme_mode, status))
                                .padding([8, 18])
                                .on_press(Message::CancelAddMonitor),
                        ].spacing(10),
                    ].spacing(10),
                )
                .padding(16)
                .width(Length::Fill)
                .style(|_| {
                    let mut s = styles::card_style(self.theme_mode);
                    s.border.color = Color::from_rgb(0.96, 0.62, 0.04); // amber border
                    s.border.width = 1.5;
                    s
                });
                Some(warn_card.into())
            }
            AddMonitorFlowState::Confirming { secs_left, .. } => {
                let secs = *secs_left;
                let confirm_card = container(
                    column![
                        row![
                            render_svg(ICON_STATUS_CHECK, 20.0, Some("#22C55E")),
                            text("Монитор добавлен — всё в порядке?")
                                .size(16)
                                .style(|_| text::Style {
                                    color: Some(self.theme_mode.text_primary()),
                                }),
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center),
                        text(i18n::confirmation_message(secs))
                            .size(12)
                            .style(move |_| text::Style {
                                color: Some(self.theme_mode.text_muted()),
                            }),
                        Space::with_height(4),
                        row![
                            button(
                                row![
                                    render_svg(ICON_STATUS_CHECK, 14.0, Some("#FFFFFF")),
                                    text(i18n::confirmation_button(secs)).size(13),
                                ]
                                .spacing(6)
                                .align_y(Alignment::Center),
                            )
                            .style(|_, status| styles::primary_button(status))
                            .padding([8, 18])
                            .on_press(Message::KeepAddedMonitor),
                            button(text("Отмена / Откатить").size(13))
                                .style(|_, status| styles::secondary_button(
                                    self.theme_mode,
                                    status
                                ))
                                .padding([8, 18])
                                .on_press(Message::RevertAddedMonitor),
                        ]
                        .spacing(10),
                    ]
                    .spacing(10),
                )
                .padding(16)
                .width(Length::Fill)
                .style(|_| {
                    let mut s = styles::card_style(self.theme_mode);
                    s.border.color = Color::from_rgb(0.13, 0.77, 0.37); // green border
                    s.border.width = 1.5;
                    s
                });
                Some(confirm_card.into())
            }
            AddMonitorFlowState::Idle => None,
        };

        // --- Confirmation Dialog Banners (Driver, Viewport, Gaming Guard) ---
        let dialog_banner: Option<Element<'_, Message>> = match &self.confirm_dialog {
            ConfirmDialogState::ActivateDriver => {
                let card = container(
                    column![
                        row![
                            render_svg(ICON_ZAP, 20.0, Some("#F59E0B")),
                            text("Активация виртуального дисплея (UAC)").size(16).style(|_| text::Style {
                                color: Some(self.theme_mode.text_primary()),
                            }),
                        ].spacing(8).align_y(Alignment::Center),
                        text("Будет запущен сценарий PowerShell от имени Администратора для установки и регистрации драйвера виртуального монитора IddCx. Экран Windows может кратковременно моргнуть при добавлении виртуального адаптера.").size(12).style(|_| text::Style {
                            color: Some(self.theme_mode.text_muted()),
                        }),
                        Space::with_height(4),
                        row![
                            button(text("Продолжить и установить (UAC)").size(13))
                                .style(|_, status| styles::primary_button(status))
                                .padding([8, 18])
                                .on_press(Message::ConfirmActivateDriver),
                            button(text("Отмена").size(13))
                                .style(|_, status| styles::secondary_button(self.theme_mode, status))
                                .padding([8, 18])
                                .on_press(Message::DismissConfirmDialog),
                        ].spacing(10),
                    ].spacing(10),
                )
                .padding(16)
                .width(Length::Fill)
                .style(|_| {
                    let mut s = styles::card_style(self.theme_mode);
                    s.border.color = Color::from_rgb(0.96, 0.62, 0.04);
                    s.border.width = 1.5;
                    s
                });
                Some(card.into())
            }
            ConfirmDialogState::UninstallDriver => {
                let card = container(
                    column![
                        row![
                            render_svg(ICON_TRASH, 20.0, Some("#EF4444")),
                            text("Внимание: Полное удаление драйвера виртуального дисплея").size(16).style(|_| text::Style {
                                color: Some(Color::from_rgb(0.94, 0.27, 0.27)),
                            }),
                        ].spacing(8).align_y(Alignment::Center),
                        text("Будет запущен сценарий деинсталляции с повышенными привилегиями (UAC). Драйвер IddCx (MttVDD) будет полностью удален из Windows Driver Store и реестра, а все активные виртуальные мониторы будут закрыты. Экран может кратковременно моргнуть.").size(12).style(|_| text::Style {
                            color: Some(self.theme_mode.text_muted()),
                        }),
                        Space::with_height(4),
                        row![
                            button(text("Да, удалить драйвер из системы (UAC)").size(13))
                                .style(|_, status| styles::danger_button(status))
                                .padding([8, 18])
                                .on_press(Message::ConfirmUninstallDriver),
                            button(text("Отмена").size(13))
                                .style(|_, status| styles::secondary_button(self.theme_mode, status))
                                .padding([8, 18])
                                .on_press(Message::DismissConfirmDialog),
                        ].spacing(10),
                    ].spacing(10),
                )
                .padding(16)
                .width(Length::Fill)
                .style(|_| {
                    let mut s = styles::card_style(self.theme_mode);
                    s.border.color = Color::from_rgb(0.94, 0.27, 0.27);
                    s.border.width = 1.5;
                    s
                });
                Some(card.into())
            }
            ConfirmDialogState::ToggleViewport(target) => {
                let is_enable = *target;
                let card = container(
                    column![
                        row![
                            render_svg(ICON_EYE, 20.0, Some("#5B4CFF")),
                            text(if is_enable { "Включить аппаратный Viewport?" } else { "Выключить Viewport?" }).size(16).style(|_| text::Style {
                                color: Some(self.theme_mode.text_primary()),
                            }),
                        ].spacing(8).align_y(Alignment::Center),
                        text(if is_enable {
                            "Режим аппаратного Viewport захватывает рабочий стол виртуального монитора через Direct3D 11 и отображает его на вашем основном экране с нулевой задержкой. Для быстрого сворачивания/разворачивания используйте Win + Alt + V."
                        } else {
                            "Viewport будет отключен. Отображение вернется к стандартному физическому рабочему столу."
                        }).size(12).style(|_| text::Style {
                            color: Some(self.theme_mode.text_muted()),
                        }),
                        Space::with_height(4),
                        row![
                            button(text(if is_enable { "Включить Viewport" } else { "Выключить Viewport" }).size(13))
                                .style(|_, status| styles::primary_button(status))
                                .padding([8, 18])
                                .on_press(Message::ConfirmToggleViewport(is_enable)),
                            button(text("Отмена").size(13))
                                .style(|_, status| styles::secondary_button(self.theme_mode, status))
                                .padding([8, 18])
                                .on_press(Message::DismissConfirmDialog),
                        ].spacing(10),
                    ].spacing(10),
                )
                .padding(16)
                .width(Length::Fill)
                .style(|_| {
                    let mut s = styles::card_style(self.theme_mode);
                    s.border.color = Color::from_rgb(0.36, 0.30, 1.0);
                    s.border.width = 1.5;
                    s
                });
                Some(card.into())
            }
            ConfirmDialogState::ToggleGaming(target) => {
                let is_pause = *target;
                let card = container(
                    column![
                        row![
                            render_svg(ICON_GAMEPAD, 20.0, Some("#22C55E")),
                            text(if is_pause { "Включить игровой режим (Пауза мыши)?" } else { "Возобновить переключение мыши?" }).size(16).style(|_| text::Style {
                                color: Some(self.theme_mode.text_primary()),
                            }),
                        ].spacing(8).align_y(Alignment::Center),
                        text(if is_pause {
                            "Курсор мыши будет зафиксирован в пределах текущего монитора. Это предотвращает случайный вылет курсора в 3D-играх и шутерах. Для быстрой паузы/возобновления используйте Win + Alt + P."
                        } else {
                            "Свободное пространственное перемещение курсора между мониторами будет возобновлено."
                        }).size(12).style(|_| text::Style {
                            color: Some(self.theme_mode.text_muted()),
                        }),
                        Space::with_height(4),
                        row![
                            button(text(if is_pause { "Включить режим" } else { "Возобновить мышь" }).size(13))
                                .style(|_, status| styles::primary_button(status))
                                .padding([8, 18])
                                .on_press(Message::ConfirmToggleGaming(is_pause)),
                            button(text("Отмена").size(13))
                                .style(|_, status| styles::secondary_button(self.theme_mode, status))
                                .padding([8, 18])
                                .on_press(Message::DismissConfirmDialog),
                        ].spacing(10),
                    ].spacing(10),
                )
                .padding(16)
                .width(Length::Fill)
                .style(|_| {
                    let mut s = styles::card_style(self.theme_mode);
                    s.border.color = Color::from_rgb(0.13, 0.77, 0.37);
                    s.border.width = 1.5;
                    s
                });
                Some(card.into())
            }
            ConfirmDialogState::None => None,
        };

        let active_banner = flow_banner.or(dialog_banner);

        if let Some(banner) = active_banner {
            column![
                banner,
                Space::with_height(8),
                header,
                Space::with_height(12),
                stats,
                Space::with_height(12),
                tab_content,
            ]
            .spacing(2)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        } else {
            column![
                header,
                Space::with_height(12),
                stats,
                Space::with_height(12),
                tab_content,
            ]
            .spacing(2)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        }
    }

    fn view_disconnected(&self) -> Element<'_, Message> {
        container(
            column![
                image(image::Handle::from_bytes(LOGO_IN_APP))
                    .width(Length::Fixed(84.0))
                    .height(Length::Fixed(84.0)),
                Space::with_height(8),
                text("Служба EvertyDisplay сейчас отключена").size(22).style(move |_| text::Style {
                    color: Some(self.theme_mode.text_primary()),
                }),
                Space::with_height(4),
                text("Фоновая служба обеспечивает мгновенный переход мыши, OSD и виртуальные мониторы.").size(14).style(|_| text::Style {
                    color: Some(self.theme_mode.text_muted()),
                }),
                Space::with_height(16),
                button(
                    row![
                        render_svg(ICON_ROCKET, 16.0, Some("#FFFFFF")),
                        text("Запустить службу EvertyDisplay").size(15),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .style(|_, status| styles::primary_button(status))
                .padding([12, 26])
                .on_press(Message::StartService),
                Space::with_height(8),
                button(
                    row![
                        render_svg(ICON_ZAP, 14.0, Some("#F59E0B")),
                        text("Активировать виртуальный дисплей в Windows (UAC)").size(13),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .style(|_, status| styles::secondary_button(self.theme_mode, status))
                .padding([10, 20])
                .on_press(Message::ActivateDriver),
            ]
            .spacing(8)
            .align_x(Alignment::Center),
        )
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
    }

    fn view_header(&self) -> Element<'_, Message> {
        let (title, subtitle) = match self.active_tab {
            Tab::Monitors => (
                "Топология мониторов",
                "Пространственное расположение и переключение экранов",
            ),
            Tab::Settings => (
                "Настройки и возможности",
                "Конфигурация OSD, игрового режима, фокуса и автозапуска",
            ),
            Tab::About => ("О нас", "Информация об авторе и контакты проекта"),
        };

        let is_viewport = self
            .topology
            .as_ref()
            .map(|t| t.viewport_enabled)
            .unwrap_or(true);
        let is_paused = self
            .topology
            .as_ref()
            .map(|t| t.pause_switching)
            .unwrap_or(false);

        let icon_color = if self.theme_mode == ThemeMode::Dark {
            "#F8FAFC"
        } else {
            "#17223B"
        };

        let refresh_btn = tooltip(
            button(
                row![
                    render_svg(ICON_REFRESH, 14.0, Some(icon_color)),
                    text("Обновить").size(12),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .style(|_, status| styles::secondary_button(self.theme_mode, status))
            .padding([8, 14])
            .on_press(Message::RefreshData),
            text("Обновить конфигурацию и экраны из системы").size(11),
            tooltip::Position::Bottom,
        )
        .gap(6)
        .padding(6)
        .style(|_| styles::tooltip_style(self.theme_mode));

        let viewport_btn = tooltip(
            button(
                row![
                    render_svg(
                        ICON_EYE,
                        14.0,
                        Some(if is_viewport { "#22C55E" } else { "#98A2B3" })
                    ),
                    text(if is_viewport {
                        "Viewport: Вкл"
                    } else {
                        "Viewport: Выкл"
                    })
                    .size(12),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .style(|_, status| styles::secondary_button(self.theme_mode, status))
            .padding([8, 14])
            .on_press(Message::RequestToggleViewport(!is_viewport)),
            text("Аппаратный захват виртуального дисплея (Win+Alt+V)").size(11),
            tooltip::Position::Bottom,
        )
        .gap(6)
        .padding(6)
        .style(|_| styles::tooltip_style(self.theme_mode));

        let pause_btn = tooltip(
            button(
                row![
                    render_svg(
                        if is_paused {
                            ICON_STATUS_PLAY
                        } else {
                            ICON_STATUS_PAUSE
                        },
                        14.0,
                        Some(if is_paused { "#F59E0B" } else { "#22C55E" }),
                    ),
                    text(if is_paused {
                        "Возобновить мышь"
                    } else {
                        "Игровой режим"
                    })
                    .size(12),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .style(|_, status| styles::secondary_button(self.theme_mode, status))
            .padding([8, 14])
            .on_press(Message::RequestToggleGaming(!is_paused)),
            text("Временная фиксация мыши для 3D-игр (Win+Alt+P)").size(11),
            tooltip::Position::Bottom,
        )
        .gap(6)
        .padding(6)
        .style(|_| styles::tooltip_style(self.theme_mode));

        container(
            row![
                column![
                    text(title).size(20).style(move |_| text::Style {
                        color: Some(self.theme_mode.text_primary()),
                    }),
                    text(subtitle).size(12).style(|_| text::Style {
                        color: Some(self.theme_mode.text_muted()),
                    }),
                ]
                .spacing(2),
                horizontal_space(),
                viewport_btn,
                pause_btn,
                refresh_btn,
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .padding([14, 18])
        .width(Length::Fill)
        .style(|_| styles::header_style(self.theme_mode))
        .into()
    }

    fn view_stat_cards(&self, top: &TopologyConfig) -> Element<'_, Message> {
        let active_m = top
            .monitors
            .iter()
            .find(|m| top.active_monitor_id == Some(m.id));
        let active_name = active_m.map(|m| m.name.as_str()).unwrap_or("Физический");
        let active_res = active_m
            .map(|m| format!("{}x{}", m.bounds.width, m.bounds.height))
            .unwrap_or_else(|| "Основной".to_string());
        let active_rate = active_m
            .map(|m| i18n::hertz(m.refresh_rate))
            .unwrap_or_else(|| i18n::hertz(180));
        let delay_ms = active_m
            .map(|m| i18n::milliseconds(m.edge_activation_delay_ms))
            .unwrap_or_else(|| i18n::milliseconds(15));

        let enabled_count = top.monitors.iter().filter(|m| m.is_enabled).count();
        let offline_count = top.monitors.len().saturating_sub(enabled_count);
        let total_count = i18n::active_displays(enabled_count);
        let virt_count = top
            .monitors
            .iter()
            .filter(|m| m.is_virtual && m.is_enabled)
            .count();
        let physical_count = top
            .monitors
            .iter()
            .filter(|m| !m.is_virtual && m.is_enabled)
            .count();
        let virt_sub = i18n::display_counts(virt_count, physical_count, offline_count);

        row![
            self.stat_card("АКТИВНЫЙ VIEWPORT", active_name.to_string(), active_res),
            self.stat_card("ВСЕГО ДИСПЛЕЕВ", total_count, virt_sub),
            self.stat_card(
                "ЧАСТОТА РАЗВЕРТКИ",
                active_rate,
                "Direct3D 11 Tear-Free".to_string()
            ),
            self.stat_card("ЗАДЕРЖКА ПЕРЕХОДА", delay_ms, "Порог активации".to_string()),
        ]
        .spacing(12)
        .width(Length::Fill)
        .into()
    }

    fn stat_card(&self, label: &str, value: String, sub: String) -> Element<'_, Message> {
        container(
            column![
                text(label.to_string()).size(10).style(|_| text::Style {
                    color: Some(self.theme_mode.text_muted()),
                }),
                Space::with_height(2),
                text(value).size(17).style(move |_| text::Style {
                    color: Some(self.theme_mode.text_primary()),
                }),
                Space::with_height(2),
                text(sub).size(11).style(|_| text::Style {
                    color: Some(PRIMARY),
                }),
            ]
            .spacing(2),
        )
        .padding([12, 16])
        .width(Length::FillPortion(1))
        .style(|_| styles::card_style(self.theme_mode))
        .into()
    }

    fn view_monitors_tab<'a>(&'a self, top: &'a TopologyConfig) -> Element<'a, Message> {
        let icon_color = if self.theme_mode == ThemeMode::Dark {
            "#F8FAFC"
        } else {
            "#17223B"
        };

        let arrange_h = tooltip(
            button(
                row![
                    render_svg(ICON_ARROW_RIGHT, 12.0, Some(icon_color)),
                    text("В ряд").size(12),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .style(|_, status| styles::secondary_button(self.theme_mode, status))
            .padding([6, 12])
            .on_press(Message::AutoArrange(ArrangeMode::Horizontal)),
            text("Расположить мониторы горизонтально в одну линию").size(11),
            tooltip::Position::Top,
        )
        .gap(6)
        .padding(6)
        .style(|_| styles::tooltip_style(self.theme_mode));

        let arrange_v = tooltip(
            button(
                row![text("Сверху вниз").size(12),]
                    .spacing(6)
                    .align_y(Alignment::Center),
            )
            .style(|_, status| styles::secondary_button(self.theme_mode, status))
            .padding([6, 12])
            .on_press(Message::AutoArrange(ArrangeMode::Vertical)),
            text("Расположить мониторы вертикально друг над другом").size(11),
            tooltip::Position::Top,
        )
        .gap(6)
        .padding(6)
        .style(|_| styles::tooltip_style(self.theme_mode));

        let is_wrap = top.wrap_around;
        let wrap_chk = checkbox(
            i18n::translate("Циклический переход краев (1 <-> N)"),
            is_wrap,
        )
        .size(16)
        .on_toggle(Message::ToggleWrap);

        let reset_btn = tooltip(
            button(
                row![
                    render_svg(
                        ICON_REFRESH,
                        13.0,
                        Some(if self.theme_mode == ThemeMode::Dark {
                            "#93C5FD"
                        } else {
                            "#1E40AF"
                        })
                    ),
                    text("Собрать экраны (Сброс)").size(12),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .style(|_, status| styles::secondary_button(self.theme_mode, status))
            .padding([6, 12])
            .on_press(Message::ResetLayout),
            text("Сбросить масштаб холста и собрать все мониторы в один ряд").size(11),
            tooltip::Position::Top,
        )
        .gap(6)
        .padding(6)
        .style(|_| styles::tooltip_style(self.theme_mode));

        let toolbar = row![
            text("Быстрое выравнивание:")
                .size(13)
                .style(|_| text::Style {
                    color: Some(self.theme_mode.text_secondary()),
                }),
            arrange_h,
            arrange_v,
            reset_btn,
            horizontal_space(),
            wrap_chk,
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let driver_warning = if !multitor_driver_manager::is_driver_pipe_ready() {
            container(
                row![
                    render_svg(ICON_STATUS_WARN, 18.0, Some("#F59E0B")),
                    column![
                        text("Видеодрайвер Windows требует подтверждения активации (UAC)").size(13).style(|_| text::Style {
                            color: Some(Color::from_rgb(0.85, 0.55, 0.1)),
                        }),
                        text("Нажмите «Активировать драйвер», чтобы система создавала реальные виртуальные мониторы Windows").size(11).style(|_| text::Style {
                            color: Some(self.theme_mode.text_muted()),
                        }),
                    ]
                    .spacing(2),
                    horizontal_space(),
                    button(
                        row![
                            render_svg(ICON_ZAP, 13.0, Some("#FFFFFF")),
                            text("Активировать драйвер (UAC)").size(12),
                        ]
                        .spacing(6)
                        .align_y(Alignment::Center)
                    )
                    .style(|_, status| styles::primary_button(status))
                    .padding([6, 14])
                    .on_press(Message::RequestActivateDriver),
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            )
            .padding(12)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(Color::from_rgba(0.96, 0.62, 0.12, 0.10))),
                border: iced::border::Border {
                    color: Color::from_rgba(0.96, 0.62, 0.12, 0.35),
                    width: 1.0,
                    radius: 8.0.into(),
                },
                ..Default::default()
            })
        } else {
            container(Space::with_height(0))
        };

        let spatial_canvas = container(
            Canvas::new(canvas::SpatialCanvas::new(
                &top.monitors,
                top.active_monitor_id,
                self.selected_id,
                self.theme_mode,
                top.wrap_around,
                self.canvas_reset_tag,
            ))
            .width(Length::Fill)
            .height(Length::Fixed(250.0)),
        )
        .padding(2)
        .style(|_| {
            let mut s = styles::card_style(self.theme_mode);
            s.border.radius = 12.0.into();
            s
        });

        let removable_virtual_id = top
            .monitors
            .iter()
            .filter(|monitor| monitor.is_virtual)
            .max_by_key(|monitor| monitor.id)
            .map(|monitor| monitor.id);
        let virtual_monitor_count = top
            .monitors
            .iter()
            .filter(|monitor| monitor.is_virtual && monitor.is_enabled)
            .count();

        let details_section = if let Some(sel_id) = self.selected_id {
            if let Some(m) = top.monitors.iter().find(|m| m.id == sel_id) {
                let name_input = row![
                    text("Имя экрана:").size(13),
                    text_input(i18n::monitor_name_placeholder(), &self.editing_name)
                        .style(|_, status| styles::input_style(self.theme_mode, status))
                        .padding([6, 10])
                        .on_input(Message::ChangeMonitorName)
                        .on_submit(Message::SaveTopology)
                        .width(Length::Fixed(200.0)),
                    button(text("Сохранить").size(12))
                        .style(|_, status| styles::primary_button(status))
                        .padding([6, 14])
                        .on_press(Message::SaveTopology),
                ]
                .spacing(8)
                .align_y(Alignment::Center);

                let delay_val = m.edge_activation_delay_ms;
                let delay_slider = row![
                    text(i18n::edge_delay(delay_val)).size(13),
                    slider(0..=250u32, delay_val as u32, |v| Message::ChangeEdgeDelay(
                        v as u64
                    ))
                    .on_release(Message::SaveTopology)
                    .width(Length::Fixed(180.0)),
                ]
                .spacing(10)
                .align_y(Alignment::Center);

                let switch_btn = button(
                    row![
                        render_svg(ICON_EYE, 14.0, Some("#FFFFFF")),
                        text("Переключить физический экран на этот монитор").size(13),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .style(|_, status| styles::primary_button(status))
                .padding([8, 16])
                .on_press_maybe(m.is_enabled.then_some(Message::SwitchToMonitor(m.id)));

                let remove_btn = button(
                    row![
                        render_svg(ICON_TRASH, 14.0, Some("#FFFFFF")),
                        text(if !m.is_enabled && !m.is_virtual {
                            "Забыть отключённый"
                        } else if !m.is_virtual {
                            "Физический экран"
                        } else if virtual_monitor_count <= 1 {
                            "Минимум один виртуальный"
                        } else if removable_virtual_id != Some(m.id) {
                            "Сначала удалите последний"
                        } else {
                            "Удалить"
                        })
                        .size(12),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                )
                .style(|_, status| styles::danger_button(status))
                .padding([6, 12])
                .on_press_maybe(if !m.is_enabled && !m.is_virtual {
                    Some(Message::ForgetMonitor(m.id))
                } else {
                    (m.is_virtual
                        && virtual_monitor_count > 1
                        && removable_virtual_id == Some(m.id))
                    .then_some(Message::RemoveMonitor(m.id))
                });

                let move_left_btn = button(
                    row![
                        render_svg(ICON_ARROW_LEFT, 12.0, Some(icon_color)),
                        text("Сдвинуть левее").size(12),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                )
                .style(|_, status| styles::secondary_button(self.theme_mode, status))
                .padding([6, 12])
                .on_press_maybe(m.is_enabled.then_some(Message::MoveLeft(m.id)));

                let move_right_btn = button(
                    row![
                        text("Сдвинуть правее").size(12),
                        render_svg(ICON_ARROW_RIGHT, 12.0, Some(icon_color)),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                )
                .style(|_, status| styles::secondary_button(self.theme_mode, status))
                .padding([6, 12])
                .on_press_maybe(m.is_enabled.then_some(Message::MoveRight(m.id)));

                let neighbor_info = i18n::neighbor_summary(
                    m.neighbors.left,
                    m.neighbors.right,
                    m.neighbors.top,
                    m.neighbors.bottom,
                );

                container(
                    column![
                        row![
                            text(i18n::selected_display(m.id, &m.name)).size(16),
                            horizontal_space(),
                            move_left_btn,
                            move_right_btn,
                            remove_btn,
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center),
                        Space::with_height(6),
                        row![name_input, Space::with_width(20), delay_slider]
                            .align_y(Alignment::Center),
                        Space::with_height(4),
                        text(neighbor_info).size(12).style(|_| text::Style {
                            color: Some(PRIMARY),
                        }),
                        Space::with_height(8),
                        switch_btn,
                    ]
                    .spacing(10),
                )
                .padding(16)
                .style(|_| styles::card_style(self.theme_mode))
            } else {
                container(Space::with_height(0))
            }
        } else {
            container(Space::with_height(0))
        };

        column![
            driver_warning,
            toolbar,
            Space::with_height(6),
            text("Интерактивный 2D холст (колесико мыши: зум, зажмите фон: перемещение):")
                .size(13)
                .style(|_| text::Style {
                    color: Some(self.theme_mode.text_muted()),
                }),
            spatial_canvas,
            Space::with_height(8),
            details_section,
        ]
        .spacing(8)
        .into()
    }

    fn view_about_tab(&self) -> Element<'_, Message> {
        let header = container(
            column![
                text("О нас").size(20).style(|_| text::Style {
                    color: Some(self.theme_mode.text_primary()),
                }),
                text("EvertyDisplay — пространственное управление физическими и виртуальными дисплеями")
                    .size(12)
                    .style(|_| text::Style {
                        color: Some(self.theme_mode.text_muted()),
                    }),
            ]
            .spacing(3),
        )
        .padding([14, 18])
        .width(Length::Fill)
        .style(|_| styles::header_style(self.theme_mode));

        let author = container(
            column![
                row![
                    render_svg(ICON_INFO, 20.0, Some("#5B4CFF")),
                    text("EvertyDisplay").size(20),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
                Space::with_height(8),
                text("Автор").size(11).style(|_| text::Style {
                    color: Some(self.theme_mode.text_muted()),
                }),
                text("Артур Валиев (Arthur Valiev)").size(17),
                Space::with_height(8),
                text("Сайт").size(11).style(|_| text::Style {
                    color: Some(self.theme_mode.text_muted()),
                }),
                text("desk.everty.ru").size(15).style(|_| text::Style {
                    color: Some(PRIMARY),
                }),
                Space::with_height(8),
                text("Электронная почта").size(11).style(|_| text::Style {
                    color: Some(self.theme_mode.text_muted()),
                }),
                text("info@everty.ru").size(15).style(|_| text::Style {
                    color: Some(PRIMARY),
                }),
                Space::with_height(8),
                text("Версия").size(11).style(|_| text::Style {
                    color: Some(self.theme_mode.text_muted()),
                }),
                text(format!("EvertyDisplay {}", env!("CARGO_PKG_VERSION"))).size(15),
            ]
            .spacing(4),
        )
        .padding(24)
        .width(Length::Fill)
        .style(|_| styles::card_style(self.theme_mode));

        column![header, Space::with_height(16), author]
            .spacing(4)
            .width(Length::Fill)
            .into()
    }

    fn view_settings_tab<'a>(&'a self, top: &'a TopologyConfig) -> Element<'a, Message> {
        let icon_color = if self.theme_mode == ThemeMode::Dark {
            "#F8FAFC"
        } else {
            "#17223B"
        };

        let language_button = |preference, label| {
            let selected = self.language == preference;
            button(text(label).size(13))
                .style(move |_, status| {
                    if selected {
                        styles::primary_button(status)
                    } else {
                        styles::secondary_button(self.theme_mode, status)
                    }
                })
                .padding([7, 14])
                .on_press(Message::SetLanguage(preference))
        };

        let language_box = container(
            column![
                text("Язык интерфейса").size(16),
                text("Следовать языку интерфейса Windows или выбрать его вручную.")
                    .size(12)
                    .style(|_| text::Style {
                        color: Some(self.theme_mode.text_muted()),
                    }),
                row![
                    language_button(LanguagePreference::System, "Как в Windows"),
                    language_button(LanguagePreference::Russian, "Русский"),
                    language_button(LanguagePreference::English, "Английский"),
                    language_button(LanguagePreference::Arabic, "Арабский"),
                ]
                .spacing(8),
                row![
                    language_button(LanguagePreference::Spanish, "Испанский"),
                    language_button(LanguagePreference::German, "Немецкий"),
                    language_button(LanguagePreference::French, "Французский"),
                ]
                .spacing(8),
            ]
            .spacing(8),
        )
        .padding(16)
        .width(Length::Fill)
        .style(|_| styles::card_style(self.theme_mode));

        // Section 1: OSD HUD Notifications
        let osd_chk = checkbox(
            i18n::translate(
                "Включить всплывающие уведомления (OSD HUD) при смене активного экрана",
            ),
            top.osd_enabled,
        )
        .size(16)
        .on_toggle(Message::ToggleOsd);

        let osd_slider = row![
            text(i18n::display_duration(top.osd_duration_ms)).size(13),
            slider(
                300..=3000u32,
                top.osd_duration_ms,
                Message::ChangeOsdDuration
            )
            .on_release(Message::SaveTopology)
            .width(Length::Fixed(220.0)),
        ]
        .spacing(12)
        .align_y(Alignment::Center);

        let osd_layout_chk = checkbox(
            i18n::translate("Показывать компактную схему экранов и выделять активный экран"),
            top.osd_show_layout,
        )
        .size(16)
        .on_toggle(Message::ToggleOsdLayout);

        let osd_physical_switches_chk = checkbox(
            i18n::translate(
                "Не показывать уведомления при переключении между физическими дисплеями",
            ),
            top.osd_hide_physical_to_physical,
        )
        .size(16)
        .on_toggle(Message::ToggleOsdPhysicalSwitches);

        let osd_box = container(
            column![
                row![
                    render_svg(ICON_BELL, 16.0, Some(icon_color)),
                    text("Всплывающие уведомления (OSD HUD)").size(16),
                ].spacing(8).align_y(Alignment::Center),
                text("Отображает полупрозрачный индикатор в центре экрана с именем монитора и подсказкой при переключении.").size(12).style(|_| text::Style {
                    color: Some(self.theme_mode.text_muted()),
                }),
                Space::with_height(4),
                osd_chk,
                if top.osd_enabled {
                    column![osd_physical_switches_chk, osd_layout_chk, osd_slider].spacing(8)
                } else {
                    column![]
                },
            ]
            .spacing(8),
        )
        .padding(16)
        .width(Length::Fill)
        .style(|_| styles::card_style(self.theme_mode));

        // Section 2: Auto-Gaming Guard
        let gaming_chk = checkbox(
            i18n::translate("Auto-Gaming Guard: Автоматически блокировать переход мыши в полноэкранных 3D-играх"),
            top.gaming_guard_enabled,
        )
        .size(16)
        .on_toggle(Message::ToggleGamingGuard);

        let gaming_box = container(
            column![
                row![
                    render_svg(ICON_GAMEPAD, 16.0, Some(icon_color)),
                    text("Игровой режим (Auto-Gaming Guard)").size(16),
                ].spacing(8).align_y(Alignment::Center),
                text("Служба проверяет запуск игр в полноэкранном режиме и блокирует случайный вылет курсора на соседние мониторы. Быстрая пауза: Win+Alt+P.").size(12).style(|_| text::Style {
                    color: Some(self.theme_mode.text_muted()),
                }),
                Space::with_height(4),
                gaming_chk,
            ]
            .spacing(8),
        )
        .padding(16)
        .width(Length::Fill)
        .style(|_| styles::card_style(self.theme_mode));

        // Section 3: Smart Focus & Window Teleportation
        let focus_chk = checkbox(
            i18n::translate("Smart Auto-Focus: Автоматически передавать фокус окну под курсором при переходе на монитор"),
            top.smart_focus_enabled,
        )
        .size(16)
        .on_toggle(Message::ToggleSmartFocus);

        let drag_chk = checkbox(
            i18n::translate("Drag-to-Teleport: Мгновенно переносить окно на монитор при зажатой ЛКМ на краю экрана"),
            top.drag_teleport_enabled,
        )
        .size(16)
        .on_toggle(Message::ToggleDragTeleport);

        let match_mode_chk = checkbox(
            i18n::translate(
                "Использовать разрешение и доступную частоту MAIN для виртуальных дисплеев",
            ),
            top.match_virtual_mode_to_primary,
        )
        .size(16)
        .on_toggle(Message::ToggleMatchVirtualMode);

        let follow_physical_window_chk = checkbox(
            i18n::translate(
                "Переходить с виртуального дисплея к окну, открывшемуся на физическом дисплее",
            ),
            top.follow_physical_window_activation,
        )
        .size(16)
        .on_toggle(Message::ToggleFollowPhysicalWindow);

        let activation_button = |action, label| {
            let selected = top.virtual_window_activation_action == action;
            button(text(i18n::translate(label)).size(12))
                .style(move |_, status| {
                    if selected {
                        styles::primary_button(status)
                    } else {
                        styles::secondary_button(self.theme_mode, status)
                    }
                })
                .padding([7, 12])
                .on_press(Message::SetVirtualWindowActivation(action))
        };

        let virtual_window_activation = column![
            text(i18n::translate(
                "При активации окна на виртуальном дисплее:",
            ))
            .size(13),
            row![
                activation_button(VirtualWindowActivationAction::None, "Как в Windows"),
                activation_button(
                    VirtualWindowActivationAction::SwitchToVirtual,
                    "Перейти на дисплей",
                ),
                activation_button(
                    VirtualWindowActivationAction::MoveToPhysical,
                    "Перенести окно сюда",
                ),
            ]
            .spacing(8),
            text(i18n::translate(
                "Работает при выборе окна на панели задач и через Alt+Tab.",
            ))
            .size(11)
            .style(|_| text::Style {
                color: Some(self.theme_mode.text_muted()),
            }),
        ]
        .spacing(7);

        let window_box = container(
            column![
                row![
                    render_svg(ICON_WINDOW, 16.0, Some(icon_color)),
                    text("Управление окнами и фокусом").size(16),
                ].spacing(8).align_y(Alignment::Center),
                text("Обеспечивает естественное взаимодействие с окнами при пространственном переключении мониторов.").size(12).style(|_| text::Style {
                    color: Some(self.theme_mode.text_muted()),
                }),
                Space::with_height(4),
                focus_chk,
                drag_chk,
                match_mode_chk,
                follow_physical_window_chk,
                virtual_window_activation,
            ]
            .spacing(8),
        )
        .padding(16)
        .width(Length::Fill)
        .style(|_| styles::card_style(self.theme_mode));

        // Section 4: Picture-in-Picture (PiP)
        let pip_chk = checkbox(
            i18n::translate(
                "Включить режим Live PiP (компактная миниатюра монитора в углу экрана)",
            ),
            top.pip_enabled,
        )
        .size(16)
        .on_toggle(Message::TogglePip);

        let pip_slider = row![
            text(i18n::pip_scale(top.pip_scale_percent)).size(13),
            slider(10..=50u32, top.pip_scale_percent, Message::ChangePipScale)
                .on_release(Message::SaveTopology)
                .width(Length::Fixed(220.0)),
        ]
        .spacing(12)
        .align_y(Alignment::Center);

        let pip_box = container(
            column![
                row![
                    render_svg(ICON_PIP, 16.0, Some(icon_color)),
                    text("Картинка-в-картинке (Live PiP)").size(16),
                ].spacing(8).align_y(Alignment::Center),
                text("Позволяет непрерывно видеть виртуальный экран в компактном окне. Окно можно свободно растягивать мышью за любые края и перетаскивать за центр в любое место экрана. Хоткей: Win+Alt+V.").size(12).style(|_| text::Style {
                    color: Some(self.theme_mode.text_muted()),
                }),
                Space::with_height(4),
                pip_chk,
                if top.pip_enabled {
                    pip_slider
                } else {
                    row![]
                },
            ]
            .spacing(8),
        )
        .padding(16)
        .width(Length::Fill)
        .style(|_| styles::card_style(self.theme_mode));

        // Section 5: Windows Autostart & Service Management
        let svc_status_row = if self.is_connected {
            row![
                text("Служба: Активна (работает)")
                    .size(13)
                    .style(|_| text::Style {
                        color: Some(SUCCESS),
                    }),
                Space::with_width(8),
                button(text("Перезапустить службу").size(12))
                    .style(|_, status| styles::secondary_button(self.theme_mode, status))
                    .padding([4, 12])
                    .on_press(Message::RestartService),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
        } else {
            row![
                text("Служба: Не подключена")
                    .size(13)
                    .style(|_| text::Style {
                        color: Some(DANGER),
                    }),
                Space::with_width(8),
                button(text("Запустить службу").size(12))
                    .style(|_, status| styles::primary_button(status))
                    .padding([4, 12])
                    .on_press(Message::StartService),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
        };

        let autostart_chk = checkbox(
            i18n::translate("Запускать фоновую службу EvertyDisplay автоматически при входе в Windows (HKCU Run)"),
            top.autostart_enabled,
        )
        .size(16)
        .on_toggle(Message::ToggleAutostart);

        let autostart_box = container(
            column![
                row![
                    render_svg(ICON_ROCKET, 16.0, Some(icon_color)),
                    text("Автозапуск и системные службы").size(16),
                ].spacing(8).align_y(Alignment::Center),
                text("Служба работает в фоне в системном трее Windows без консольных окон, обеспечивая бесшовное перемещение курсора, хоткеи и виртуальные мониторы.").size(12).style(|_| text::Style {
                    color: Some(self.theme_mode.text_muted()),
                }),
                Space::with_height(4),
                svc_status_row,
                Space::with_height(2),
                autostart_chk,
            ]
            .spacing(8),
        )
        .padding(16)
        .width(Length::Fill)
        .style(|_| styles::card_style(self.theme_mode));

        // Section 6: Hotkeys Reference
        let hotkeys_box = container(
            column![
                row![
                    render_svg(ICON_KEYBOARD, 16.0, Some(icon_color)),
                    text("Памятка горячих клавиш EvertyDisplay").size(16),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                text("Все комбинации работают глобально в любых приложениях:")
                    .size(12)
                    .style(|_| text::Style {
                        color: Some(self.theme_mode.text_muted()),
                    }),
                Space::with_height(4),
                row![
                    text("Win + Shift + Стрелки (Влево / Вправо / Вверх / Вниз)")
                        .size(13)
                        .style(|_| text::Style {
                            color: Some(PRIMARY),
                        }),
                    text("— Телепортация активного окна на соседний экран").size(13),
                ]
                .spacing(8),
                row![
                    text("Win + Alt + Стрелки (Влево / Вправо / Вверх / Вниз)")
                        .size(13)
                        .style(|_| text::Style {
                            color: Some(PRIMARY),
                        }),
                    text("— Мгновенное переключение экрана Viewport").size(13),
                ]
                .spacing(8),
                row![
                    text("Win + Alt + P").size(13).style(|_| text::Style {
                        color: Some(PRIMARY),
                    }),
                    text("— Пауза / возобновление переключения мыши (Игровой режим)").size(13),
                ]
                .spacing(8),
                row![
                    text("Win + Alt + V").size(13).style(|_| text::Style {
                        color: Some(PRIMARY),
                    }),
                    text("— Переключение режима Viewport (Полный экран / PiP / Выкл)").size(13),
                ]
                .spacing(8),
                row![
                    text("Зажатая ЛКМ на краю экрана")
                        .size(13)
                        .style(|_| text::Style {
                            color: Some(PRIMARY),
                        }),
                    text("— Перетаскивание окна на соседний экран (Drag-to-Teleport)").size(13),
                ]
                .spacing(8),
            ]
            .spacing(6),
        )
        .padding(16)
        .width(Length::Fill)
        .style(|_| styles::card_style(self.theme_mode));

        // Section 7: Danger Zone / Driver Uninstallation
        let danger_box = container(
            column![
                row![
                    render_svg(ICON_TRASH, 16.0, Some("#EF4444")),
                    text("Опасная зона: Удаление драйвера").size(16).style(|_| text::Style {
                        color: Some(DANGER),
                    }),
                ].spacing(8).align_y(Alignment::Center),
                text("Полное удаление драйвера виртуального монитора IddCx (MttVDD) из Windows Driver Store и системного реестра. Все виртуальные экраны будут немедленно отключены. Нажмите для запуска деинсталляции с правами Администратора.").size(12).style(|_| text::Style {
                    color: Some(self.theme_mode.text_muted()),
                }),
                Space::with_height(4),
                button(text("Удалить драйвер виртуального дисплея...").size(13))
                    .style(|_, status| styles::danger_button(status))
                    .padding([8, 18])
                    .on_press(Message::RequestUninstallDriver),
            ]
            .spacing(8),
        )
        .padding(16)
        .width(Length::Fill)
        .style(|_| {
            let mut s = styles::card_style(self.theme_mode);
            s.border.color = Color::from_rgb(0.94, 0.27, 0.27);
            s.border.width = 1.2;
            s
        });

        scrollable(
            column![
                language_box,
                osd_box,
                gaming_box,
                window_box,
                pip_box,
                autostart_box,
                hotkeys_box,
                danger_box,
                Space::with_height(14),
            ]
            .spacing(12)
            .width(Length::Fill),
        )
        .height(Length::Fill)
        .into()
    }
}

async fn fetch_data() -> Result<(TopologyConfig, Vec<DisplayInfo>, ProductCapabilities), String> {
    let top_resp = ipc_client::send_ipc_request(&IpcRequest::GetTopology)
        .await
        .map_err(|e| e.to_string())?;

    let top = match top_resp {
        IpcResponse::Topology(t) => t,
        _ => return Err("Unexpected topology response".to_string()),
    };

    let disp_resp = ipc_client::send_ipc_request(&IpcRequest::GetDisplays)
        .await
        .map_err(|e| e.to_string())?;

    let disps = match disp_resp {
        IpcResponse::Displays(d) => d,
        _ => Vec::new(),
    };

    let capabilities = match ipc_client::send_ipc_request(&IpcRequest::GetProductCapabilities).await
    {
        Ok(IpcResponse::ProductCapabilities(_capabilities)) => ProductCapabilities {
            edition: ProductEdition::Lite,
            max_virtual_displays: 1,
            multi_display_layouts: false,
            cloud_features: false,
        },
        _ => ProductCapabilities {
            edition: ProductEdition::Lite,
            max_virtual_displays: 1,
            multi_display_layouts: false,
            cloud_features: false,
        },
    };

    Ok((top, disps, capabilities))
}

async fn send_monitor_action(request: IpcRequest) -> Result<(), String> {
    let response = ipc_client::send_ipc_request(&request)
        .await
        .map_err(|error| error.to_string())?;
    match response {
        IpcResponse::Success | IpcResponse::MonitorRemoved(_) => Ok(()),
        IpcResponse::Error(message) => Err(message),
        _ => Err("Служба вернула неожиданный ответ".to_string()),
    }
}

#[cfg(test)]
mod driver_settings_tests {
    use super::add_global_refresh_rate_xml;

    #[test]
    fn refresh_rate_merge_preserves_existing_driver_settings() {
        let original = "<vdd_settings><monitors><count>3</count></monitors><global>\n<g_refresh_rate>60</g_refresh_rate>\n</global><resolutions><refresh_rate>180</refresh_rate></resolutions></vdd_settings>";
        let updated = add_global_refresh_rate_xml(original, 180).unwrap();

        assert!(updated.contains("<count>3</count>"));
        assert!(updated.contains("<g_refresh_rate>60</g_refresh_rate>"));
        assert!(updated.contains("<g_refresh_rate>180</g_refresh_rate>"));
        assert!(add_global_refresh_rate_xml(&updated, 180).is_none());
    }
}
