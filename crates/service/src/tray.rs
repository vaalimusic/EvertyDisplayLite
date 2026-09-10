use crate::autostart;
use std::env;
use std::process::Command;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use tracing::{error, info, warn};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DispatchMessageW,
    FindWindowW, GetCursorPos, LoadIconW, LoadImageW, PeekMessageW, PostMessageW, PostQuitMessage,
    RegisterClassW, SetForegroundWindow, TrackPopupMenu, TranslateMessage, CS_HREDRAW, CS_VREDRAW,
    HICON, IDI_APPLICATION, IMAGE_ICON, LR_LOADFROMFILE, MF_CHECKED, MF_GRAYED, MF_SEPARATOR,
    MF_STRING, MF_UNCHECKED, MSG, PM_REMOVE, TPM_BOTTOMALIGN, TPM_RIGHTBUTTON, WM_APP, WM_CLOSE,
    WM_COMMAND, WM_DESTROY, WM_LBUTTONDBLCLK, WM_LBUTTONUP, WM_RBUTTONUP, WNDCLASSW, WS_POPUP,
};

const WM_TRAY_CALLBACK: u32 = WM_APP + 10;
const ID_HEADER: usize = 1001;
const ID_OPEN_UI: usize = 1002;
const ID_TOGGLE_PAUSE: usize = 1003;
const ID_TOGGLE_AUTOSTART: usize = 1004;
const ID_TOGGLE_PIP: usize = 1005;
const ID_EXIT: usize = 1006;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEvent {
    OpenUi,
    TogglePause,
    ToggleAutostart,
    TogglePip,
    Exit,
}

pub struct TrayManager {
    rx: Receiver<TrayEvent>,
}

impl TrayManager {
    pub fn new() -> Self {
        let (tx, rx) = channel::<TrayEvent>();

        thread::spawn(move || {
            run_tray_loop(tx);
        });

        Self { rx }
    }

    pub fn poll_event(&self) -> Option<TrayEvent> {
        self.rx.try_recv().ok()
    }
}

static mut TRAY_TX: Option<Sender<TrayEvent>> = None;
static mut IS_PAUSED_CACHE: bool = false;
static mut IS_PIP_CACHE: bool = false;

pub fn set_tray_paused_state(paused: bool) {
    unsafe {
        IS_PAUSED_CACHE = paused;
    }
}

pub fn set_tray_pip_state(pip: bool) {
    unsafe {
        IS_PIP_CACHE = pip;
    }
}

unsafe extern "system" fn tray_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_TRAY_CALLBACK => {
            let event = lparam.0 as u32;
            match event {
                WM_RBUTTONUP => {
                    show_context_menu(hwnd);
                    LRESULT(0)
                }
                WM_LBUTTONUP | WM_LBUTTONDBLCLK => {
                    if let Some(ref tx) = TRAY_TX {
                        let _ = tx.send(TrayEvent::OpenUi);
                    }
                    LRESULT(0)
                }
                _ => LRESULT(0),
            }
        }
        WM_COMMAND => {
            let id = wparam.0 & 0xFFFF;
            if let Some(ref tx) = TRAY_TX {
                match id {
                    ID_OPEN_UI => {
                        let _ = tx.send(TrayEvent::OpenUi);
                    }
                    ID_TOGGLE_PAUSE => {
                        let _ = tx.send(TrayEvent::TogglePause);
                    }
                    ID_TOGGLE_AUTOSTART => {
                        let _ = tx.send(TrayEvent::ToggleAutostart);
                    }
                    ID_TOGGLE_PIP => {
                        let _ = tx.send(TrayEvent::TogglePip);
                    }
                    ID_EXIT => {
                        let _ = tx.send(TrayEvent::Exit);
                    }
                    _ => {}
                }
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn show_context_menu(hwnd: HWND) {
    let mut pt = POINT::default();
    let _ = GetCursorPos(&mut pt);

    let hmenu = match CreatePopupMenu() {
        Ok(m) => m,
        Err(_) => return,
    };

    let header = crate::localization::tray_text(
        "EvertyDisplay — Виртуальные экраны",
        "EvertyDisplay — Virtual displays",
        "EvertyDisplay — الشاشات الافتراضية",
        "EvertyDisplay — Pantallas virtuales",
        "EvertyDisplay — Virtuelle Bildschirme",
        "EvertyDisplay — Écrans virtuels",
    );
    let open_settings = crate::localization::tray_text(
        "⚙ Открыть настройки",
        "⚙ Open settings",
        "⚙ فتح الإعدادات",
        "⚙ Abrir ajustes",
        "⚙ Einstellungen öffnen",
        "⚙ Ouvrir les paramètres",
    );
    let pause = crate::localization::tray_text(
        "⏸ Пауза переходов мыши",
        "⏸ Pause mouse transitions",
        "⏸ إيقاف انتقال المؤشر",
        "⏸ Pausar transiciones del ratón",
        "⏸ Mausübergänge pausieren",
        "⏸ Suspendre les transitions de la souris",
    );
    let pip = crate::localization::tray_text(
        "📺 Картинка-в-картинке (PiP)",
        "📺 Picture-in-Picture (PiP)",
        "📺 صورة داخل صورة (PiP)",
        "📺 Imagen en imagen (PiP)",
        "📺 Bild-in-Bild (PiP)",
        "📺 Image dans l’image (PiP)",
    );
    let autostart = crate::localization::tray_text(
        "🚀 Запускать вместе с Windows",
        "🚀 Start with Windows",
        "🚀 التشغيل مع Windows",
        "🚀 Iniciar con Windows",
        "🚀 Mit Windows starten",
        "🚀 Démarrer avec Windows",
    );
    let exit = crate::localization::tray_text(
        "❌ Выход из EvertyDisplay",
        "❌ Exit EvertyDisplay",
        "❌ الخروج من EvertyDisplay",
        "❌ Salir de EvertyDisplay",
        "❌ EvertyDisplay beenden",
        "❌ Quitter EvertyDisplay",
    );

    // Header
    let _ = AppendMenuW(
        hmenu,
        MF_STRING | MF_GRAYED,
        ID_HEADER,
        PCWSTR(header.as_ptr()),
    );
    let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, None);

    // Open UI
    let _ = AppendMenuW(hmenu, MF_STRING, ID_OPEN_UI, PCWSTR(open_settings.as_ptr()));

    // Toggle Pause
    let pause_flag = if IS_PAUSED_CACHE {
        MF_CHECKED
    } else {
        MF_UNCHECKED
    };
    let _ = AppendMenuW(
        hmenu,
        MF_STRING | pause_flag,
        ID_TOGGLE_PAUSE,
        PCWSTR(pause.as_ptr()),
    );

    // Toggle PiP
    let pip_flag = if IS_PIP_CACHE {
        MF_CHECKED
    } else {
        MF_UNCHECKED
    };
    let _ = AppendMenuW(
        hmenu,
        MF_STRING | pip_flag,
        ID_TOGGLE_PIP,
        PCWSTR(pip.as_ptr()),
    );

    // Toggle Autostart
    let is_auto = autostart::is_autostart_enabled();
    let auto_flag = if is_auto { MF_CHECKED } else { MF_UNCHECKED };
    let _ = AppendMenuW(
        hmenu,
        MF_STRING | auto_flag,
        ID_TOGGLE_AUTOSTART,
        PCWSTR(autostart.as_ptr()),
    );

    let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, None);

    // Exit
    let _ = AppendMenuW(hmenu, MF_STRING, ID_EXIT, PCWSTR(exit.as_ptr()));

    let _ = SetForegroundWindow(hwnd);
    let _ = TrackPopupMenu(
        hmenu,
        TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
        pt.x,
        pt.y,
        0,
        hwnd,
        None,
    );
    let _ = DestroyMenu(hmenu);
}

fn run_tray_loop(tx: Sender<TrayEvent>) {
    unsafe {
        TRAY_TX = Some(tx);

        let class_name = w!("EvertyDisplayTrayMsgWindowClass");
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(tray_wnd_proc),
            hInstance: HINSTANCE::default(),
            lpszClassName: class_name,
            ..Default::default()
        };
        let _ = RegisterClassW(&wc);

        let hwnd = match CreateWindowExW(
            Default::default(),
            class_name,
            w!("EvertyDisplayTrayWindow"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            HWND::default(),
            None,
            HINSTANCE::default(),
            None,
        ) {
            Ok(h) => h,
            Err(e) => {
                error!(
                    "Failed to create EvertyDisplay tray message window: {:?}",
                    e
                );
                return;
            }
        };

        // Load EvertyDisplay branded app icon if available, otherwise default
        let custom_icon: Option<HICON> = (|| {
            let candidates = [
                std::env::current_exe()
                    .ok()?
                    .parent()?
                    .join("evertydisplay.ico"),
                std::path::PathBuf::from("evertydisplay.ico"),
                std::path::PathBuf::from("target/release/evertydisplay.ico"),
            ];
            for path in &candidates {
                if path.exists() {
                    let path_w: Vec<u16> = path
                        .to_string_lossy()
                        .encode_utf16()
                        .chain(std::iter::once(0))
                        .collect();
                    if let Ok(handle) = LoadImageW(
                        HINSTANCE::default(),
                        windows::core::PCWSTR(path_w.as_ptr()),
                        IMAGE_ICON,
                        32,
                        32,
                        LR_LOADFROMFILE,
                    ) {
                        return Some(HICON(handle.0));
                    }
                }
            }
            None
        })();
        let icon = custom_icon.unwrap_or_else(|| {
            LoadIconW(HINSTANCE::default(), IDI_APPLICATION).unwrap_or_default()
        });

        let mut tip_chars = [0u16; 128];
        let tip_str = "EvertyDisplay";
        for (i, c) in tip_str.encode_utf16().enumerate().take(127) {
            tip_chars[i] = c;
        }

        let nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: 1,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
            uCallbackMessage: WM_TRAY_CALLBACK,
            hIcon: icon,
            szTip: tip_chars,
            ..Default::default()
        };

        if !Shell_NotifyIconW(NIM_ADD, &nid).as_bool() {
            warn!("Failed to add EvertyDisplay icon to System Tray");
        } else {
            info!("EvertyDisplay System Tray icon registered successfully");
        }

        let mut msg = MSG::default();
        loop {
            while PeekMessageW(&mut msg, hwnd, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_DESTROY {
                    let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
                    return;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            thread::sleep(std::time::Duration::from_millis(50));
        }
    }
}

#[cfg(windows)]
use std::os::windows::process::CommandExt;

const CREATE_NO_WINDOW: u32 = 0x08000000;

pub fn launch_ui_app() {
    thread::spawn(|| {
        if let Ok(cur_exe) = env::current_exe() {
            if let Some(dir) = cur_exe.parent() {
                let everty_path = dir.join("EvertyDisplay.exe");
                if everty_path.exists() {
                    let mut cmd = Command::new(everty_path);
                    #[cfg(windows)]
                    cmd.creation_flags(CREATE_NO_WINDOW);
                    let _ = cmd.spawn();
                    return;
                }
                let ui_path = dir.join("multitor-ui.exe");
                if ui_path.exists() {
                    let mut cmd = Command::new(ui_path);
                    #[cfg(windows)]
                    cmd.creation_flags(CREATE_NO_WINDOW);
                    let _ = cmd.spawn();
                    return;
                }
            }
        }
        // Fallback: run via cargo
        let mut cmd = Command::new("cargo");
        cmd.args(["run", "-p", "multitor-ui"]);
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        let _ = cmd.spawn();
    });
}

/// Close the editor together with the tray service. A normal WM_CLOSE gives
/// Iced a chance to tear the window down cleanly. The short fallback handles a
/// hung UI and, importantly, prevents its reconnect loop from starting the
/// service again immediately after the user explicitly selected Exit.
pub fn close_ui_app() {
    const WINDOW_TITLES: [&str; 2] = [
        "EvertyDisplay",
        "EvertyDisplay - Пространственные виртуальные мониторы",
    ];

    let mut found_window = false;
    for title in WINDOW_TITLES {
        let wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
        if let Ok(hwnd) = unsafe { FindWindowW(None, PCWSTR(wide.as_ptr())) } {
            found_window = true;
            let _ = unsafe { PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0)) };
        }
    }

    if found_window {
        thread::sleep(std::time::Duration::from_millis(500));

        let still_open = WINDOW_TITLES.iter().any(|title| {
            let wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
            unsafe { FindWindowW(None, PCWSTR(wide.as_ptr())) }.is_ok()
        });

        if still_open {
            let mut command = Command::new("taskkill");
            command.args(["/F", "/IM", "EvertyDisplay.exe"]);
            #[cfg(windows)]
            command.creation_flags(CREATE_NO_WINDOW);
            let _ = command.status();
        }
    }
}
