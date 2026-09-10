use std::sync::mpsc::{channel, Sender};
use std::thread;
use std::time::Duration;
use tracing::error;
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateRoundRectRgn, CreateSolidBrush, DeleteObject, DrawTextW,
    EndPaint, FillRect, FrameRgn, SelectObject, SetBkMode, SetTextColor, SetWindowRgn, DT_CENTER,
    DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, FW_BOLD, FW_NORMAL, HDC, PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetSystemMetrics, KillTimer, PeekMessageW,
    PostQuitMessage, RegisterClassW, SetLayeredWindowAttributes, SetTimer, SetWindowPos,
    ShowWindow, TranslateMessage, CS_HREDRAW, CS_VREDRAW, HWND_TOPMOST, LWA_ALPHA, MSG, PM_REMOVE,
    SM_CXSCREEN, SWP_NOACTIVATE, SW_HIDE, SW_SHOWNOACTIVATE, WM_DESTROY, WM_PAINT, WM_TIMER,
    WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
    WS_POPUP,
};

#[derive(Debug, Clone, Copy)]
pub enum OsdKind {
    MonitorSwitch,
    WindowTeleport,
    GamingGuard,
    Info,
}

struct OsdData {
    title: String,
    subtitle: String,
    kind: OsdKind,
    duration_ms: u32,
    layout: Option<Vec<OsdLayoutMonitor>>,
}

#[derive(Debug, Clone)]
struct OsdLayoutMonitor {
    id: u32,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    active: bool,
}

#[derive(Clone)]
pub struct OsdNotifier {
    tx: Sender<OsdData>,
}

static mut OSD_ENABLED: bool = true;
static mut OSD_DEFAULT_DURATION: u32 = 1500;

pub fn set_osd_enabled(enabled: bool) {
    unsafe {
        OSD_ENABLED = enabled;
    }
}

pub fn set_osd_duration(ms: u32) {
    unsafe {
        OSD_DEFAULT_DURATION = ms.max(300);
    }
}

impl OsdNotifier {
    pub fn new() -> Self {
        let (tx, rx) = channel::<OsdData>();

        thread::spawn(move || {
            run_osd_window(rx);
        });

        Self { tx }
    }

    pub fn show(&self, title: impl Into<String>, subtitle: impl Into<String>, kind: OsdKind) {
        unsafe {
            if !OSD_ENABLED {
                return;
            }
            let _ = self.tx.send(OsdData {
                title: crate::localization::translate(title),
                subtitle: crate::localization::translate(subtitle),
                kind,
                duration_ms: OSD_DEFAULT_DURATION,
                layout: None,
            });
        }
    }

    pub fn show_monitor_switch(
        &self,
        title: impl Into<String>,
        subtitle: impl Into<String>,
        monitors: &[multitor_ipc::MonitorConfig],
        active_id: u32,
        show_layout: bool,
    ) {
        unsafe {
            if !OSD_ENABLED {
                return;
            }
            let layout = show_layout.then(|| {
                monitors
                    .iter()
                    .filter(|monitor| monitor.is_enabled)
                    .map(|monitor| {
                        let bounds = monitor.layout_bounds();
                        OsdLayoutMonitor {
                            id: monitor.id,
                            x: bounds.x,
                            y: bounds.y,
                            width: bounds.width,
                            height: bounds.height,
                            active: monitor.id == active_id,
                        }
                    })
                    .collect()
            });
            let _ = self.tx.send(OsdData {
                title: crate::localization::translate(title),
                subtitle: crate::localization::translate(subtitle),
                kind: OsdKind::MonitorSwitch,
                duration_ms: OSD_DEFAULT_DURATION,
                layout,
            });
        }
    }

    #[allow(dead_code)]
    pub fn show_timed(
        &self,
        title: impl Into<String>,
        subtitle: impl Into<String>,
        kind: OsdKind,
        duration_ms: u32,
    ) {
        unsafe {
            if !OSD_ENABLED {
                return;
            }
            let _ = self.tx.send(OsdData {
                title: crate::localization::translate(title),
                subtitle: crate::localization::translate(subtitle),
                kind,
                duration_ms,
                layout: None,
            });
        }
    }
}

static mut CURRENT_DATA: Option<OsdData> = None;

unsafe extern "system" fn osd_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_TIMER => {
            let _ = KillTimer(hwnd, 1);
            let _ = ShowWindow(hwnd, SW_HIDE);
            LRESULT(0)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);

            if let Some(ref data) = CURRENT_DATA {
                draw_osd(hwnd, hdc, data);
            }

            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn draw_osd(_hwnd: HWND, hdc: HDC, data: &OsdData) {
    let width = 420;
    let height = if data.layout.as_ref().is_some_and(|items| !items.is_empty()) {
        150
    } else {
        76
    };

    let bg_color = COLORREF(0x00181412); // Deep dark slate (BGR)
    let bg_brush = CreateSolidBrush(bg_color);
    let full_rect = RECT {
        left: 0,
        top: 0,
        right: width,
        bottom: height,
    };
    FillRect(hdc, &full_rect, bg_brush);
    let _ = DeleteObject(bg_brush);

    // Accent line / pill at the top
    let accent_bgr = match data.kind {
        OsdKind::MonitorSwitch => COLORREF(0x00FFC800), // Cyan BGR (R:0, G:200, B:255)
        OsdKind::WindowTeleport => COLORREF(0x0028BEFF), // Gold BGR (R:255, G:190, B:40)
        OsdKind::GamingGuard => COLORREF(0x00DC50DC),   // Magenta BGR
        OsdKind::Info => COLORREF(0x0078DC50),          // Emerald BGR
    };
    let accent_brush = CreateSolidBrush(accent_bgr);
    let accent_rect = RECT {
        left: 20,
        top: 0,
        right: width - 20,
        bottom: 3,
    };
    FillRect(hdc, &accent_rect, accent_brush);
    let _ = DeleteObject(accent_brush);

    // Rounded border outline
    let rgn = CreateRoundRectRgn(0, 0, width, height, 20, 20);
    let border_brush = CreateSolidBrush(accent_bgr);
    let _ = FrameRgn(hdc, rgn, border_brush, 1, 1);
    let _ = DeleteObject(border_brush);
    let _ = DeleteObject(rgn);

    SetBkMode(hdc, TRANSPARENT);

    // 1. Draw Title
    let title_font = CreateFontW(
        22, // height
        0,
        0,
        0,
        FW_BOLD.0 as i32,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        w!("Segoe UI"),
    );
    let old_font = SelectObject(hdc, title_font);
    SetTextColor(hdc, COLORREF(0x00FAFAFA)); // White

    let mut title_rect = RECT {
        left: 16,
        top: 10,
        right: width - 16,
        bottom: 38,
    };
    let title_u16: Vec<u16> = data.title.encode_utf16().collect();
    DrawTextW(
        hdc,
        &mut title_u16.clone(),
        &mut title_rect,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );

    if let Some(layout) = data.layout.as_ref().filter(|items| !items.is_empty()) {
        draw_layout(hdc, layout, accent_bgr);
    }

    // 2. Draw Subtitle
    let sub_font = CreateFontW(
        16, // height
        0,
        0,
        0,
        FW_NORMAL.0 as i32,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        w!("Segoe UI"),
    );
    SelectObject(hdc, sub_font);
    SetTextColor(hdc, COLORREF(0x00C8B4A0)); // Soft silver / light cyan tint

    let mut sub_rect = RECT {
        left: 16,
        top: 40,
        right: width - 16,
        bottom: 66,
    };
    let sub_u16: Vec<u16> = data.subtitle.encode_utf16().collect();
    DrawTextW(
        hdc,
        &mut sub_u16.clone(),
        &mut sub_rect,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );

    SelectObject(hdc, old_font);
    let _ = DeleteObject(title_font);
    let _ = DeleteObject(sub_font);
}

unsafe fn draw_layout(hdc: HDC, monitors: &[OsdLayoutMonitor], accent_bgr: COLORREF) {
    let min_x = monitors.iter().map(|monitor| monitor.x).min().unwrap_or(0);
    let min_y = monitors.iter().map(|monitor| monitor.y).min().unwrap_or(0);
    let max_x = monitors
        .iter()
        .map(|monitor| i64::from(monitor.x) + i64::from(monitor.width))
        .max()
        .unwrap_or(1);
    let max_y = monitors
        .iter()
        .map(|monitor| i64::from(monitor.y) + i64::from(monitor.height))
        .max()
        .unwrap_or(1);
    let topology_w = (max_x - i64::from(min_x)).max(1) as f32;
    let topology_h = (max_y - i64::from(min_y)).max(1) as f32;
    let area_left = 24.0f32;
    let area_top = 79.0f32;
    let area_width = 372.0f32;
    let area_height = 57.0f32;
    let scale = (area_width / topology_w)
        .min(area_height / topology_h)
        .max(0.001);
    let offset_x = area_left + (area_width - topology_w * scale) / 2.0;
    let offset_y = area_top + (area_height - topology_h * scale) / 2.0;

    let number_font = CreateFontW(
        14,
        0,
        0,
        0,
        FW_BOLD.0 as i32,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        w!("Segoe UI"),
    );
    let old_font = SelectObject(hdc, number_font);
    SetBkMode(hdc, TRANSPARENT);

    for monitor in monitors {
        let relative_x = i64::from(monitor.x) - i64::from(min_x);
        let relative_y = i64::from(monitor.y) - i64::from(min_y);
        let left = (offset_x + relative_x as f32 * scale).round() as i32;
        let top = (offset_y + relative_y as f32 * scale).round() as i32;
        let tile_w = (monitor.width as f32 * scale).round().max(16.0) as i32;
        let tile_h = (monitor.height as f32 * scale).round().max(12.0) as i32;
        let mut rect = RECT {
            left,
            top,
            right: left + tile_w,
            bottom: top + tile_h,
        };
        let fill = CreateSolidBrush(if monitor.active {
            accent_bgr
        } else {
            COLORREF(0x00483E39)
        });
        FillRect(hdc, &rect, fill);
        let _ = DeleteObject(fill);

        let border = CreateSolidBrush(if monitor.active {
            COLORREF(0x00FFFFFF)
        } else {
            COLORREF(0x00807870)
        });
        let region = CreateRoundRectRgn(rect.left, rect.top, rect.right, rect.bottom, 5, 5);
        let _ = FrameRgn(hdc, region, border, 1, 1);
        let _ = DeleteObject(border);
        let _ = DeleteObject(region);

        SetTextColor(
            hdc,
            if monitor.active {
                COLORREF(0x00181412)
            } else {
                COLORREF(0x00FFFFFF)
            },
        );
        let mut number: Vec<u16> = monitor.id.to_string().encode_utf16().collect();
        DrawTextW(
            hdc,
            &mut number,
            &mut rect,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );
    }

    SelectObject(hdc, old_font);
    let _ = DeleteObject(number_font);
}

fn run_osd_window(rx: std::sync::mpsc::Receiver<OsdData>) {
    unsafe {
        let class_name = w!("MultitorOsdClass");
        let wnd_class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(osd_wnd_proc),
            hInstance: HINSTANCE::default(),
            lpszClassName: class_name,
            ..Default::default()
        };

        let _ = RegisterClassW(&wnd_class);

        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let win_w = 420;
        let win_h = 150;
        let win_x = (screen_w - win_w) / 2;
        let win_y = 35; // 35px from top

        let hwnd = match CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class_name,
            w!("Multitor OSD"),
            WS_POPUP,
            win_x,
            win_y,
            win_w,
            win_h,
            HWND::default(),
            None,
            HINSTANCE::default(),
            None,
        ) {
            Ok(h) => h,
            Err(e) => {
                error!("Failed to create OSD window: {:?}", e);
                return;
            }
        };

        // Rounded corners
        let rgn = CreateRoundRectRgn(0, 0, win_w, win_h, 20, 20);
        let _ = SetWindowRgn(hwnd, rgn, true);

        // Alpha transparency: 235 / 255 (~92% solid, sleek glass feel)
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 235, LWA_ALPHA);

        let mut msg = MSG::default();

        loop {
            // Check incoming notifications
            while let Ok(data) = rx.try_recv() {
                let duration = data.duration_ms;
                let display_height = if data.layout.as_ref().is_some_and(|items| !items.is_empty())
                {
                    150
                } else {
                    76
                };
                CURRENT_DATA = Some(data);

                // Invalidate window to redraw new text
                let _ = windows::Win32::Graphics::Gdi::InvalidateRect(hwnd, None, true);

                // Show topmost without stealing focus
                let _ = SetWindowPos(
                    hwnd,
                    HWND_TOPMOST,
                    win_x,
                    win_y,
                    win_w,
                    display_height,
                    SWP_NOACTIVATE,
                );
                let rgn = CreateRoundRectRgn(0, 0, win_w, display_height, 20, 20);
                let _ = SetWindowRgn(hwnd, rgn, true);
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);

                // Reset timer for auto-hide
                let _ = KillTimer(hwnd, 1);
                let _ = SetTimer(hwnd, 1, duration, None);
            }

            // Pump window messages
            while PeekMessageW(&mut msg, hwnd, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_DESTROY {
                    return;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            thread::sleep(Duration::from_millis(15));
        }
    }
}
