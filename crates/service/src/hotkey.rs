use anyhow::Result;
use tracing::{info, warn};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN,
    VK_DOWN, VK_LEFT, VK_RIGHT, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{PeekMessageW, MSG, PM_REMOVE, WM_HOTKEY};

pub const HOTKEY_LEFT: i32 = 1001;
pub const HOTKEY_RIGHT: i32 = 1002;
pub const HOTKEY_UP: i32 = 1003;
pub const HOTKEY_DOWN: i32 = 1004;
pub const HOTKEY_PAUSE: i32 = 1005;
pub const HOTKEY_NUM_BASE: i32 = 1010; // 1011..1016 for 1..6

pub const HOTKEY_WIN_SHIFT_LEFT: i32 = 1020;
pub const HOTKEY_WIN_SHIFT_RIGHT: i32 = 1021;
pub const HOTKEY_WIN_CTRL_LEFT: i32 = 1022;
pub const HOTKEY_WIN_CTRL_RIGHT: i32 = 1023;
pub const HOTKEY_WIN_ALT_SHIFT_LEFT: i32 = 1024;
pub const HOTKEY_WIN_ALT_SHIFT_RIGHT: i32 = 1025;
pub const HOTKEY_TOGGLE_PIP: i32 = 1030;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyAction {
    NavigateLeft,
    NavigateRight,
    NavigateUp,
    NavigateDown,
    TogglePause,
    SelectMonitor(u32),
    MoveWindowLeft,
    MoveWindowRight,
    TogglePip,
}

pub struct HotkeyManager {
    registered_ids: Vec<i32>,
}

impl HotkeyManager {
    pub fn new() -> Self {
        Self {
            registered_ids: Vec::new(),
        }
    }

    pub fn register_all(&mut self) -> Result<()> {
        let modifiers = MOD_WIN | MOD_ALT | MOD_NOREPEAT;

        let mappings = [
            (HOTKEY_LEFT, VK_LEFT.0 as u32),
            (HOTKEY_RIGHT, VK_RIGHT.0 as u32),
            (HOTKEY_UP, VK_UP.0 as u32),
            (HOTKEY_DOWN, VK_DOWN.0 as u32),
            (HOTKEY_PAUSE, 0x50),      // 'P'
            (HOTKEY_TOGGLE_PIP, 0x56), // 'V'
        ];

        for (id, vk) in mappings {
            unsafe {
                if RegisterHotKey(HWND::default(), id, modifiers, vk).is_ok() {
                    self.registered_ids.push(id);
                } else {
                    warn!("Failed to register hotkey ID {}", id);
                }
            }
        }

        // Win + Alt + 1..6
        for num in 1..=6 {
            let id = HOTKEY_NUM_BASE + num;
            let vk = 0x30 + num as u32; // '1' is 0x31
            unsafe {
                if RegisterHotKey(HWND::default(), id, modifiers, vk).is_ok() {
                    self.registered_ids.push(id);
                }
            }
        }

        // Window Teleport Hotkeys:
        // 1. Win + Shift + Left / Right
        let win_shift = MOD_WIN | MOD_SHIFT | MOD_NOREPEAT;
        unsafe {
            if RegisterHotKey(
                HWND::default(),
                HOTKEY_WIN_SHIFT_LEFT,
                win_shift,
                VK_LEFT.0 as u32,
            )
            .is_ok()
            {
                self.registered_ids.push(HOTKEY_WIN_SHIFT_LEFT);
            }
            if RegisterHotKey(
                HWND::default(),
                HOTKEY_WIN_SHIFT_RIGHT,
                win_shift,
                VK_RIGHT.0 as u32,
            )
            .is_ok()
            {
                self.registered_ids.push(HOTKEY_WIN_SHIFT_RIGHT);
            }
        }

        // 2. Win + Ctrl + Left / Right (reliable fallback if Win+Shift intercepted)
        let win_ctrl = MOD_WIN | MOD_CONTROL | MOD_NOREPEAT;
        unsafe {
            if RegisterHotKey(
                HWND::default(),
                HOTKEY_WIN_CTRL_LEFT,
                win_ctrl,
                VK_LEFT.0 as u32,
            )
            .is_ok()
            {
                self.registered_ids.push(HOTKEY_WIN_CTRL_LEFT);
            }
            if RegisterHotKey(
                HWND::default(),
                HOTKEY_WIN_CTRL_RIGHT,
                win_ctrl,
                VK_RIGHT.0 as u32,
            )
            .is_ok()
            {
                self.registered_ids.push(HOTKEY_WIN_CTRL_RIGHT);
            }
        }

        // 3. Win + Alt + Shift + Left / Right
        let win_alt_shift = MOD_WIN | MOD_ALT | MOD_SHIFT | MOD_NOREPEAT;
        unsafe {
            if RegisterHotKey(
                HWND::default(),
                HOTKEY_WIN_ALT_SHIFT_LEFT,
                win_alt_shift,
                VK_LEFT.0 as u32,
            )
            .is_ok()
            {
                self.registered_ids.push(HOTKEY_WIN_ALT_SHIFT_LEFT);
            }
            if RegisterHotKey(
                HWND::default(),
                HOTKEY_WIN_ALT_SHIFT_RIGHT,
                win_alt_shift,
                VK_RIGHT.0 as u32,
            )
            .is_ok()
            {
                self.registered_ids.push(HOTKEY_WIN_ALT_SHIFT_RIGHT);
            }
        }

        info!("Registered {} global hotkey(s)", self.registered_ids.len());
        Ok(())
    }

    /// Poll for pending hotkey message (non-blocking)
    pub fn poll_hotkey(&self) -> Option<HotkeyAction> {
        let mut msg = MSG::default();
        unsafe {
            if PeekMessageW(&mut msg, HWND::default(), WM_HOTKEY, WM_HOTKEY, PM_REMOVE).as_bool() {
                let id = msg.wParam.0 as i32;
                match id {
                    HOTKEY_LEFT => Some(HotkeyAction::NavigateLeft),
                    HOTKEY_RIGHT => Some(HotkeyAction::NavigateRight),
                    HOTKEY_UP => Some(HotkeyAction::NavigateUp),
                    HOTKEY_DOWN => Some(HotkeyAction::NavigateDown),
                    HOTKEY_PAUSE => Some(HotkeyAction::TogglePause),
                    HOTKEY_TOGGLE_PIP => Some(HotkeyAction::TogglePip),
                    HOTKEY_WIN_SHIFT_LEFT | HOTKEY_WIN_CTRL_LEFT | HOTKEY_WIN_ALT_SHIFT_LEFT => {
                        Some(HotkeyAction::MoveWindowLeft)
                    }
                    HOTKEY_WIN_SHIFT_RIGHT | HOTKEY_WIN_CTRL_RIGHT | HOTKEY_WIN_ALT_SHIFT_RIGHT => {
                        Some(HotkeyAction::MoveWindowRight)
                    }
                    num_id if (HOTKEY_NUM_BASE + 1..=HOTKEY_NUM_BASE + 6).contains(&num_id) => {
                        let mon_num = (num_id - HOTKEY_NUM_BASE) as u32;
                        Some(HotkeyAction::SelectMonitor(mon_num))
                    }
                    _ => None,
                }
            } else {
                None
            }
        }
    }
}

impl Drop for HotkeyManager {
    fn drop(&mut self) {
        for &id in &self.registered_ids {
            unsafe {
                let _ = UnregisterHotKey(HWND::default(), id);
            }
        }
    }
}
