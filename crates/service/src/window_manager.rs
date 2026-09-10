use crate::topology::TopologyManager;
use multitor_ipc::{EdgeDirection, MonitorConfig, SwitchReason};
use tracing::{info, warn};
use windows::Win32::Foundation::{BOOL, HWND, POINT, RECT};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    GetAncestor, GetForegroundWindow, GetShellWindow, GetWindowRect, GetWindowThreadProcessId,
    IsIconic, IsWindow, IsWindowVisible, IsZoomed, SetForegroundWindow, SetWindowPos, ShowWindow,
    WindowFromPoint, GA_ROOT, HWND_TOP, SWP_SHOWWINDOW, SW_MAXIMIZE, SW_RESTORE,
};

#[link(name = "user32")]
extern "system" {
    #[link_name = "AttachThreadInput"]
    fn attach_thread_input(id_attach: u32, id_attach_to: u32, attach: BOOL) -> BOOL;
}

pub struct WindowManager;

pub struct ForegroundWindowLocation {
    pub monitor_id: u32,
    pub cursor_x: i32,
    pub cursor_y: i32,
    pub title: String,
    pub is_virtual: bool,
}

fn monitor_for_window<'a>(monitors: &'a [MonitorConfig], rect: &RECT) -> Option<&'a MonitorConfig> {
    if rect.right <= rect.left || rect.bottom <= rect.top {
        return None;
    }
    let center_x = rect.left + (rect.right - rect.left) / 2;
    let center_y = rect.top + (rect.bottom - rect.top) / 2;
    monitors
        .iter()
        .find(|monitor| monitor.is_enabled && monitor.bounds.contains_point(center_x, center_y))
}

fn is_browser_window_class(class_name: &str) -> bool {
    class_name.starts_with("Chrome_WidgetWin_") || class_name == "MozillaWindowClass"
}

impl WindowManager {
    /// Stable token for detecting foreground-window changes without installing
    /// a global WinEvent hook. HWND is only compared, never dereferenced later.
    pub fn foreground_window_token() -> isize {
        unsafe { GetForegroundWindow().0 as isize }
    }

    pub fn root_window_token(token: isize) -> isize {
        if token == 0 {
            return 0;
        }
        let hwnd = HWND(token as *mut core::ffi::c_void);
        let root = unsafe { GetAncestor(hwnd, GA_ROOT) };
        if root.is_invalid() {
            token
        } else {
            root.0 as isize
        }
    }

    /// Windows automatically exposes another window after the foreground one
    /// is minimized or destroyed. That is not an explicit taskbar/Alt+Tab
    /// activation and must never trigger a jump to a virtual display.
    pub fn foreground_window_was_dismissed(token: isize) -> bool {
        if token == 0 {
            return false;
        }
        let hwnd = windows::Win32::Foundation::HWND(token as *mut core::ffi::c_void);
        unsafe { !IsWindow(hwnd).as_bool() || IsIconic(hwnd).as_bool() }
    }

    /// Returns the display containing the newly activated foreground window.
    /// The window is identified by its center, matching Windows' own monitor
    /// selection behavior for windows spanning multiple displays.
    pub fn foreground_window_location(
        topology: &TopologyManager,
    ) -> Option<ForegroundWindowLocation> {
        let token = Self::foreground_window_token();
        Self::window_location(token, topology)
    }

    pub fn window_location(
        token: isize,
        topology: &TopologyManager,
    ) -> Option<ForegroundWindowLocation> {
        unsafe {
            let root_token = Self::root_window_token(token);
            if root_token == 0 {
                return None;
            }
            let hwnd = HWND(root_token as *mut core::ffi::c_void);
            if !Self::is_movable_window(hwnd) {
                return None;
            }

            let mut rect = RECT::default();
            if GetWindowRect(hwnd, &mut rect).is_err()
                || rect.right <= rect.left
                || rect.bottom <= rect.top
            {
                return None;
            }

            let center_x = rect.left + (rect.right - rect.left) / 2;
            let center_y = rect.top + (rect.bottom - rect.top) / 2;
            let monitor = monitor_for_window(&topology.config().monitors, &rect)?;

            Some(ForegroundWindowLocation {
                monitor_id: monitor.id,
                cursor_x: center_x,
                cursor_y: center_y,
                title: Self::get_window_title(hwnd),
                is_virtual: monitor.is_virtual,
            })
        }
    }

    pub fn is_browser_window(token: isize) -> bool {
        let root_token = Self::root_window_token(token);
        if root_token == 0 {
            return false;
        }
        let hwnd = HWND(root_token as *mut core::ffi::c_void);
        let mut class_buf = [0u16; 128];
        let len =
            unsafe { windows::Win32::UI::WindowsAndMessaging::GetClassNameW(hwnd, &mut class_buf) };
        if len == 0 {
            return false;
        }
        let class_name = String::from_utf16_lossy(&class_buf[..len as usize]);
        is_browser_window_class(&class_name)
    }

    pub fn activate_window(token: isize) -> bool {
        let root_token = Self::root_window_token(token);
        if root_token == 0 {
            return false;
        }
        let hwnd = HWND(root_token as *mut core::ffi::c_void);
        if !Self::is_movable_window(hwnd) {
            return false;
        }
        unsafe { Self::activate_without_taskbar_attention(hwnd) }
    }

    pub fn foreground_virtual_window(
        topology: &TopologyManager,
    ) -> Option<ForegroundWindowLocation> {
        Self::foreground_window_location(topology).filter(|window| window.is_virtual)
    }

    /// If the foreground window lives on a virtual display, move that one window
    /// to the active physical display. This makes taskbar and Alt+Tab activation
    /// visible without moving the cursor or disturbing other virtual windows.
    pub fn bring_foreground_virtual_window_to_active_physical(
        topology: &TopologyManager,
    ) -> Option<(String, u32)> {
        unsafe {
            let hwnd = GetForegroundWindow();
            if !Self::is_movable_window(hwnd) {
                return None;
            }

            let target = topology
                .active_monitor()
                .filter(|monitor| monitor.is_enabled && !monitor.is_virtual)
                .cloned()?;

            let mut rect = RECT::default();
            if GetWindowRect(hwnd, &mut rect).is_err() {
                return None;
            }
            let win_w = rect.right - rect.left;
            let win_h = rect.bottom - rect.top;
            if win_w <= 0 || win_h <= 0 {
                return None;
            }
            let center_x = rect.left + win_w / 2;
            let center_y = rect.top + win_h / 2;
            let source = topology
                .config()
                .monitors
                .iter()
                .find(|monitor| {
                    monitor.is_enabled
                        && monitor.is_virtual
                        && monitor.bounds.contains_point(center_x, center_y)
                })
                .cloned()?;

            let is_maximized = IsZoomed(hwnd).as_bool();
            if is_maximized {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }

            let rel_x = (rect.left - source.bounds.x) as f32 / source.bounds.width.max(1) as f32;
            let rel_y = (rect.top - source.bounds.y) as f32 / source.bounds.height.max(1) as f32;
            let rel_w = win_w as f32 / source.bounds.width.max(1) as f32;
            let rel_h = win_h as f32 / source.bounds.height.max(1) as f32;
            let target_w = target.bounds.width.max(1) as i32;
            let target_h = target.bounds.height.max(1) as i32;
            let new_w = ((rel_w * target_w as f32).max(400.0) as i32).min(target_w);
            let new_h = ((rel_h * target_h as f32).max(300.0) as i32).min(target_h);
            let proposed_x = target.bounds.x + (rel_x * target_w as f32) as i32;
            let proposed_y = target.bounds.y + (rel_y * target_h as f32) as i32;
            let new_x =
                proposed_x.clamp(target.bounds.x, target.bounds.right().saturating_sub(new_w));
            let new_y = proposed_y.clamp(
                target.bounds.y,
                target.bounds.bottom().saturating_sub(new_h),
            );

            if SetWindowPos(hwnd, HWND_TOP, new_x, new_y, new_w, new_h, SWP_SHOWWINDOW).is_err() {
                if is_maximized {
                    let _ = ShowWindow(hwnd, SW_MAXIMIZE);
                }
                warn!("Windows rejected bringing the activated virtual-display window back");
                return None;
            }
            if is_maximized {
                let _ = ShowWindow(hwnd, SW_MAXIMIZE);
            }
            let title = Self::get_window_title(hwnd);
            info!(
                "Brought activated window '{}' from virtual monitor {} to physical monitor {}",
                title, source.id, target.id
            );
            Some((title, target.id))
        }
    }

    /// Move the current foreground window to the neighbor monitor in the given direction
    pub fn move_foreground_window(
        topology: &mut TopologyManager,
        direction: EdgeDirection,
    ) -> Option<(String, u32)> {
        unsafe {
            let hwnd = GetForegroundWindow();
            if !Self::is_movable_window(hwnd) {
                return None;
            }

            let mut rect: RECT = std::mem::zeroed();
            if GetWindowRect(hwnd, &mut rect).is_err() {
                return None;
            }

            let win_w = rect.right - rect.left;
            let win_h = rect.bottom - rect.top;
            if win_w <= 0 || win_h <= 0 {
                return None;
            }

            let win_center_x = rect.left + win_w / 2;
            let win_center_y = rect.top + win_h / 2;

            // Find which monitor currently hosts this window
            let cur_mon = topology
                .config()
                .monitors
                .iter()
                .find(|m| m.bounds.contains_point(win_center_x, win_center_y))
                .or_else(|| topology.active_monitor())
                .cloned();

            let cur_mon = cur_mon?;

            let Some(target_id) = topology.find_neighbor_on_edge(cur_mon.id, direction) else {
                warn!(
                    "No neighbor monitor found in direction {:?} from monitor {}",
                    direction, cur_mon.id
                );
                return None;
            };

            let target_mon = topology.get_monitor(target_id).cloned()?;

            let is_maximized = IsZoomed(hwnd).as_bool();

            if is_maximized {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }

            // Calculate new position proportionally
            let rel_x = (rect.left - cur_mon.bounds.x) as f32 / cur_mon.bounds.width.max(1) as f32;
            let rel_y = (rect.top - cur_mon.bounds.y) as f32 / cur_mon.bounds.height.max(1) as f32;
            let rel_w = win_w as f32 / cur_mon.bounds.width.max(1) as f32;
            let rel_h = win_h as f32 / cur_mon.bounds.height.max(1) as f32;

            let target_w = target_mon.bounds.width.max(1) as i32;
            let target_h = target_mon.bounds.height.max(1) as i32;
            let new_w = ((rel_w * target_w as f32).max(400.0) as i32).min(target_w);
            let new_h = ((rel_h * target_h as f32).max(300.0) as i32).min(target_h);
            let proposed_x = target_mon.bounds.x + (rel_x * target_w as f32) as i32;
            let proposed_y = target_mon.bounds.y + (rel_y * target_h as f32) as i32;
            let new_x = proposed_x.clamp(
                target_mon.bounds.x,
                target_mon.bounds.right().saturating_sub(new_w),
            );
            let new_y = proposed_y.clamp(
                target_mon.bounds.y,
                target_mon.bounds.bottom().saturating_sub(new_h),
            );

            if SetWindowPos(hwnd, HWND_TOP, new_x, new_y, new_w, new_h, SWP_SHOWWINDOW).is_err() {
                if is_maximized {
                    let _ = ShowWindow(hwnd, SW_MAXIMIZE);
                }
                warn!("Windows rejected moving the foreground window");
                return None;
            }

            if is_maximized {
                let _ = ShowWindow(hwnd, SW_MAXIMIZE);
            }

            let title = Self::get_window_title(hwnd);
            info!(
                "Teleported active window '{}' to monitor {} ({:?}) -> ({}, {}) {}x{}",
                title, target_id, direction, new_x, new_y, new_w, new_h
            );

            // Also switch active monitor to target monitor and teleport cursor so user keeps focus!
            let new_cursor_x = new_x + new_w / 2;
            let new_cursor_y = new_y + new_h / 2;
            crate::cursor::CursorTracker::clip_cursor_to(None);
            crate::cursor::CursorTracker::teleport_cursor(new_cursor_x, new_cursor_y);
            topology.set_active_monitor(target_id, SwitchReason::Hotkey);

            Some((title, target_id))
        }
    }

    /// Check if a window can be moved/teleported
    pub fn is_movable_window(hwnd: windows::Win32::Foundation::HWND) -> bool {
        unsafe {
            if hwnd.is_invalid() || !IsWindow(hwnd).as_bool() || !IsWindowVisible(hwnd).as_bool() {
                return false;
            }
            let shell = GetShellWindow();
            if hwnd == shell {
                return false;
            }
            let mut class_buf = [0u16; 64];
            let len = windows::Win32::UI::WindowsAndMessaging::GetClassNameW(hwnd, &mut class_buf);
            if len > 0 {
                let class_name = String::from_utf16_lossy(&class_buf[..len as usize]);
                if class_name == "Progman"
                    || class_name == "WorkerW"
                    || class_name == "Shell_TrayWnd"
                    || class_name.contains("Multitor")
                    || class_name.contains("EvertyDisplay")
                {
                    return false;
                }
            }
            true
        }
    }

    /// Retrieve title text from window handle
    pub fn get_window_title(hwnd: windows::Win32::Foundation::HWND) -> String {
        unsafe {
            let mut buf = [0u16; 256];
            let len = windows::Win32::UI::WindowsAndMessaging::GetWindowTextW(hwnd, &mut buf);
            if len > 0 {
                String::from_utf16_lossy(&buf[..len as usize])
            } else {
                "Окно приложения".to_string()
            }
        }
    }

    /// Activate and focus the top-level window at the given screen point
    pub fn focus_window_at_point(x: i32, y: i32) -> bool {
        unsafe {
            let pt = POINT { x, y };
            let raw_hwnd = WindowFromPoint(pt);
            if raw_hwnd.is_invalid() {
                return false;
            }

            let root_hwnd = GetAncestor(raw_hwnd, GA_ROOT);
            if root_hwnd.is_invalid() || !IsWindow(root_hwnd).as_bool() {
                return false;
            }

            // Desktop is a valid target: focusing it when leaving a virtual
            // display ensures the previously visible virtual window is no longer
            // foreground, so clicking its taskbar button can activate it again.
            let shell = GetShellWindow();

            let mut class_buf = [0u16; 64];
            let len =
                windows::Win32::UI::WindowsAndMessaging::GetClassNameW(root_hwnd, &mut class_buf);
            if len > 0 {
                let class_name = String::from_utf16_lossy(&class_buf[..len as usize]);
                if class_name.contains("Multitor")
                    || class_name.contains("EvertyDisplay")
                    || class_name == "Shell_TrayWnd"
                {
                    return false;
                }
                if class_name == "Progman" || class_name == "WorkerW" {
                    return Self::activate_without_taskbar_attention(shell);
                }
            }

            Self::activate_without_taskbar_attention(root_hwnd)
        }
    }

    /// Clear an off-screen foreground window when no ordinary window exists at
    /// the cursor point (desktop, taskbar or an EvertyDisplay overlay).
    pub fn focus_desktop() -> bool {
        unsafe { Self::activate_without_taskbar_attention(GetShellWindow()) }
    }

    /// Background services are normally not allowed to call SetForegroundWindow.
    /// A rejected call makes Windows flash the application's taskbar button. By
    /// temporarily joining the foreground and target input queues, focus changes
    /// as part of the user's monitor transition instead of requesting attention.
    unsafe fn activate_without_taskbar_attention(target: windows::Win32::Foundation::HWND) -> bool {
        if target.is_invalid() || !IsWindow(target).as_bool() {
            return false;
        }
        let foreground = GetForegroundWindow();
        if foreground == target {
            return true;
        }
        if foreground.is_invalid() {
            return false;
        }

        let current_thread = GetCurrentThreadId();
        let foreground_thread = GetWindowThreadProcessId(foreground, None);
        let target_thread = GetWindowThreadProcessId(target, None);
        if foreground_thread == 0 || target_thread == 0 {
            return false;
        }

        let attached_foreground = current_thread != foreground_thread
            && attach_thread_input(current_thread, foreground_thread, BOOL(1)).as_bool();
        if current_thread != foreground_thread && !attached_foreground {
            return false;
        }
        let attached_target = target_thread != current_thread
            && target_thread != foreground_thread
            && attach_thread_input(current_thread, target_thread, BOOL(1)).as_bool();
        if target_thread != current_thread && target_thread != foreground_thread && !attached_target
        {
            if attached_foreground {
                let _ = attach_thread_input(current_thread, foreground_thread, BOOL(0));
            }
            return false;
        }

        let activated = SetForegroundWindow(target).as_bool();

        if attached_target {
            let _ = attach_thread_input(current_thread, target_thread, BOOL(0));
        }
        if attached_foreground {
            let _ = attach_thread_input(current_thread, foreground_thread, BOOL(0));
        }
        activated
    }

    /// Check if a fullscreen game / application is currently running in the foreground
    pub fn is_fullscreen_game_active() -> bool {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_invalid() || !IsWindow(hwnd).as_bool() {
                return false;
            }

            let shell = GetShellWindow();
            if hwnd == shell {
                return false;
            }

            let mut class_buf = [0u16; 64];
            let len = windows::Win32::UI::WindowsAndMessaging::GetClassNameW(hwnd, &mut class_buf);
            if len > 0 {
                let class_name = String::from_utf16_lossy(&class_buf[..len as usize]);
                if class_name == "Progman"
                    || class_name == "WorkerW"
                    || class_name == "Shell_TrayWnd"
                    || class_name.contains("Multitor")
                    || class_name.contains("EvertyDisplay")
                    // Browser fullscreen video is not a game. Treating it as one used to
                    // freeze active-monitor tracking while YouTube was fullscreen.
                    || class_name.starts_with("Chrome_WidgetWin_")
                    || class_name == "MozillaWindowClass"
                    || class_name == "ApplicationFrameWindow"
                    || class_name == "WinUIDesktopWin32WindowClass"
                {
                    return false;
                }
            }

            let mut rect: RECT = std::mem::zeroed();
            if GetWindowRect(hwnd, &mut rect).is_err() {
                return false;
            }

            let win_w = rect.right - rect.left;
            let win_h = rect.bottom - rect.top;

            let style = windows::Win32::UI::WindowsAndMessaging::GetWindowLongW(
                hwnd,
                windows::Win32::UI::WindowsAndMessaging::GWL_STYLE,
            ) as u32;

            let is_popup_or_borderless =
                (style & windows::Win32::UI::WindowsAndMessaging::WS_CAPTION.0) == 0;

            if is_popup_or_borderless && win_w >= 1280 && win_h >= 720 {
                return true;
            }

            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use multitor_ipc::{DisplayBounds, Neighbors};

    fn monitor(id: u32, x: i32, is_virtual: bool, is_enabled: bool) -> MonitorConfig {
        MonitorConfig {
            id,
            device_name: format!(r"\\.\DISPLAY{id}"),
            name: format!("Display {id}"),
            bounds: DisplayBounds::new(x, 0, 1920, 1080),
            refresh_rate: 60,
            neighbors: Neighbors::default(),
            edge_activation_delay_ms: 15,
            is_enabled,
            is_virtual,
            layout_x: Some(x),
            layout_y: Some(0),
        }
    }

    #[test]
    fn activated_window_uses_its_center_to_select_display() {
        let monitors = vec![
            monitor(1, 0, false, true),
            monitor(2, 1920, true, true),
            monitor(3, 3840, true, false),
        ];

        // Most of this window is still on display 1, but its center belongs to
        // display 2, which is the stable rule used throughout window movement.
        let spanning = RECT {
            left: 1700,
            top: 100,
            right: 2300,
            bottom: 700,
        };
        assert_eq!(
            monitor_for_window(&monitors, &spanning).map(|m| m.id),
            Some(2)
        );

        let physical = RECT {
            left: 100,
            top: 100,
            right: 800,
            bottom: 700,
        };
        assert_eq!(
            monitor_for_window(&monitors, &physical).map(|m| m.id),
            Some(1)
        );

        let disabled_virtual = RECT {
            left: 4000,
            top: 100,
            right: 4600,
            bottom: 700,
        };
        assert!(monitor_for_window(&monitors, &disabled_virtual).is_none());
    }

    #[test]
    fn browser_focus_filter_covers_chromium_and_firefox_only() {
        assert!(is_browser_window_class("Chrome_WidgetWin_1"));
        assert!(is_browser_window_class("Chrome_WidgetWin_0"));
        assert!(is_browser_window_class("MozillaWindowClass"));
        assert!(!is_browser_window_class("CabinetWClass"));
        assert!(!is_browser_window_class("EvertyDisplay"));
    }
}
