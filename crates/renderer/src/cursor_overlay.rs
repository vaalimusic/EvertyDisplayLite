use anyhow::Result;
use std::mem::zeroed;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DrawIconEx, GetCursorInfo, RegisterClassW,
    SetWindowPos, ShowWindow, UpdateLayeredWindow, CS_HREDRAW, CS_VREDRAW, CURSORINFO, DI_NORMAL,
    HCURSOR, HWND_TOPMOST, SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE, ULW_ALPHA, WNDCLASSW,
    WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

pub struct CursorOverlay {
    hwnd: HWND,
    current_cursor: Option<isize>,
    hotspot_x: i32,
    hotspot_y: i32,
    dib_dc: HDC,
    dib_bitmap: HBITMAP,
    old_bitmap: HGDIOBJ,
    bits_ptr: *mut u8,
    width: i32,
    height: i32,
}

impl CursorOverlay {
    pub fn new() -> Result<Self> {
        let width = 32;
        let height = 32;
        let h_instance = HINSTANCE::default();
        let class_name = w!("MultitorCursorOverlayWindowClass");

        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(cursor_wnd_proc),
            hInstance: h_instance,
            lpszClassName: class_name,
            ..Default::default()
        };
        unsafe {
            RegisterClassW(&wc);
        }

        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
                class_name,
                PCWSTR::null(),
                WS_POPUP,
                0,
                0,
                width,
                height,
                HWND::default(),
                None,
                h_instance,
                None,
            )?
        };

        let dib_dc = unsafe { CreateCompatibleDC(HDC::default()) };

        let bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height, // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let dib_bitmap =
            unsafe { CreateDIBSection(dib_dc, &bi, DIB_RGB_COLORS, &mut bits, None, 0)? };

        let old_bitmap = unsafe { SelectObject(dib_dc, dib_bitmap) };

        Ok(Self {
            hwnd,
            current_cursor: None,
            hotspot_x: 0,
            hotspot_y: 0,
            dib_dc,
            dib_bitmap,
            old_bitmap,
            bits_ptr: bits as *mut u8,
            width,
            height,
        })
    }

    pub fn update(&mut self, screen_x: i32, screen_y: i32) {
        unsafe {
            let mut ci: CURSORINFO = zeroed();
            ci.cbSize = std::mem::size_of::<CURSORINFO>() as u32;

            if GetCursorInfo(&mut ci).is_ok() {
                // CURSOR_SHOWING = 0x00000001
                if (ci.flags.0 & 1) != 0 && !ci.hCursor.is_invalid() {
                    let cursor_val = ci.hCursor.0 as isize;
                    if self.current_cursor != Some(cursor_val) {
                        self.redraw_cursor(ci.hCursor);
                        self.current_cursor = Some(cursor_val);
                    }

                    let pt_dst = POINT {
                        x: screen_x - self.hotspot_x,
                        y: screen_y - self.hotspot_y,
                    };
                    let size_dst = SIZE {
                        cx: self.width,
                        cy: self.height,
                    };
                    let pt_src = POINT { x: 0, y: 0 };
                    let blend = BLENDFUNCTION {
                        BlendOp: 0, // AC_SRC_OVER
                        BlendFlags: 0,
                        SourceConstantAlpha: 255,
                        AlphaFormat: 1, // AC_SRC_ALPHA
                    };

                    let _ = UpdateLayeredWindow(
                        self.hwnd,
                        HDC::default(),
                        Some(&pt_dst),
                        Some(&size_dst),
                        self.dib_dc,
                        Some(&pt_src),
                        COLORREF(0),
                        Some(&blend),
                        ULW_ALPHA,
                    );

                    let _ = SetWindowPos(
                        self.hwnd,
                        HWND_TOPMOST,
                        screen_x - self.hotspot_x,
                        screen_y - self.hotspot_y,
                        self.width,
                        self.height,
                        SWP_NOACTIVATE | SWP_SHOWWINDOW,
                    );
                }
            }
        }
    }

    fn redraw_cursor(&mut self, hcursor: HCURSOR) {
        use windows::Win32::UI::WindowsAndMessaging::{GetIconInfo, HICON, ICONINFO};
        unsafe {
            let mut ii: ICONINFO = zeroed();
            if GetIconInfo(HICON(hcursor.0), &mut ii).is_ok() {
                self.hotspot_x = ii.xHotspot as i32;
                self.hotspot_y = ii.yHotspot as i32;
                if !ii.hbmMask.is_invalid() {
                    let _ = DeleteObject(ii.hbmMask);
                }
                if !ii.hbmColor.is_invalid() {
                    let _ = DeleteObject(ii.hbmColor);
                }
            } else {
                self.hotspot_x = 0;
                self.hotspot_y = 0;
            }

            if !self.bits_ptr.is_null() {
                std::ptr::write_bytes(self.bits_ptr, 0, (self.width * self.height * 4) as usize);
            }
            let _ = DrawIconEx(
                self.dib_dc,
                0,
                0,
                hcursor,
                self.width,
                self.height,
                0,
                windows::Win32::Graphics::Gdi::HBRUSH::default(),
                DI_NORMAL,
            );
        }
    }

    pub fn hide(&mut self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
        self.current_cursor = None;
    }
}

impl Drop for CursorOverlay {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dib_dc, self.old_bitmap);
            let _ = DeleteObject(self.dib_bitmap);
            let _ = DeleteDC(self.dib_dc);
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

unsafe extern "system" fn cursor_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::{HTTRANSPARENT, WM_NCHITTEST};
    match msg {
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
