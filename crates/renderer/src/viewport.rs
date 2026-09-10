use crate::cursor_overlay::CursorOverlay;
use crate::dxgi::DxgiOutputDuplicator;
use anyhow::{Context, Result};
use multitor_ipc::DisplayBounds;
use tracing::{info, warn};
use windows::core::{w, Interface, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Direct3D::Fxc::D3DCompile;
use windows::Win32::Graphics::Direct3D::{
    ID3DBlob, D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_11_0, D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11PixelShader,
    ID3D11RenderTargetView, ID3D11SamplerState, ID3D11Texture2D, ID3D11VertexShader,
    D3D11_COMPARISON_NEVER, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_FILTER_MIN_MAG_MIP_LINEAR,
    D3D11_FLOAT32_MAX, D3D11_SAMPLER_DESC, D3D11_SDK_VERSION, D3D11_TEXTURE_ADDRESS_CLAMP,
    D3D11_VIEWPORT,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::{
    IDXGIFactory2, IDXGISwapChain1, DXGI_PRESENT, DXGI_PRESENT_ALLOW_TEARING,
    DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING, DXGI_SWAP_EFFECT_FLIP_DISCARD,
    DXGI_USAGE_RENDER_TARGET_OUTPUT,
};
use windows::Win32::Graphics::Gdi::{CreateRoundRectRgn, SetWindowRgn, HRGN};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DispatchMessageW,
    GetClientRect, GetCursorPos, GetWindowLongPtrW, GetWindowRect, IsWindowVisible, PeekMessageW,
    RegisterClassW, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    TrackPopupMenu, CS_HREDRAW, CS_VREDRAW, GWLP_USERDATA, GWL_EXSTYLE, HTBOTTOM, HTBOTTOMLEFT,
    HTBOTTOMRIGHT, HTCAPTION, HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT, HTTOPRIGHT, HTTRANSPARENT,
    HWND_TOPMOST, MF_STRING, MSG, PM_REMOVE, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_SHOWWINDOW,
    SW_HIDE, SW_SHOW, SW_SHOWNOACTIVATE, TPM_LEFTALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_COMMAND,
    WM_CONTEXTMENU, WM_DESTROY, WM_NCHITTEST, WM_NCRBUTTONUP, WM_RBUTTONUP, WM_SIZE, WM_SIZING,
    WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

/// Atomic flag used by wnd_proc to communicate PiP context menu selections back to service.
/// 0 = no action, 1 = hide PiP, 2 = go to virtual monitor
static PIP_ACTION: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
static PIP_ASPECT_BITS: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(1.777_777_8_f32.to_bits());

const VIEWPORT_PIXEL_SHADER: &[u8] = b"Texture2D tex : register(t0);\nSamplerState samLinear : register(s0);\nfloat4 main(float4 pos : SV_POSITION, float2 uv : TEXCOORD0) : SV_Target {\n    uint width, height;\n    tex.GetDimensions(width, height);\n    float2 texel = 1.0f / float2(width, height);\n    float2 safeMin = 6.5f * texel;\n    float2 safeMax = 1.0f - safeMin;\n    float2 safeUv = clamp(uv, safeMin, safeMax);\n    return tex.SampleLevel(samLinear, safeUv, 0.0f);\n}\n\0";

fn map_source_axis(
    source_position: i32,
    source_extent: u32,
    destination_origin: i32,
    destination_extent: u32,
) -> i32 {
    destination_origin
        + ((source_position as f32 / source_extent.max(1) as f32) * destination_extent as f32)
            .round() as i32
}

/// Actions that can be triggered from the PiP right-click context menu.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PipAction {
    HidePip,
    GoToMonitor,
}

pub struct ViewportWindow {
    pub hwnd: HWND,
    pub width: u32,
    pub height: u32,
    pub is_pip: bool,
}

impl ViewportWindow {
    pub fn create_borderless(x: i32, y: i32, width: u32, height: u32, title: &str) -> Result<Self> {
        let h_instance = HINSTANCE::default();
        let class_name = w!("MultitorViewportWindowClass");

        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            hInstance: h_instance,
            lpszClassName: class_name,
            ..Default::default()
        };

        unsafe {
            RegisterClassW(&wc);
        }

        let title_w: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();

        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE,
                class_name,
                PCWSTR(title_w.as_ptr()),
                WS_POPUP,
                x,
                y,
                width as i32,
                height as i32,
                HWND::default(),
                None,
                h_instance,
                None,
            )?
        };

        unsafe {
            SetWindowPos(
                hwnd,
                HWND::default(),
                x,
                y,
                width as i32,
                height as i32,
                SWP_FRAMECHANGED | SWP_NOACTIVATE,
            )?;
        }

        info!(
            "Created Viewport Window at [{},{} {}x{}]",
            x, y, width, height
        );

        Ok(Self {
            hwnd,
            width,
            height,
            is_pip: false,
        })
    }

    pub fn is_pip(&self) -> bool {
        self.is_pip
    }

    pub fn is_visible(&self) -> bool {
        unsafe { IsWindowVisible(self.hwnd).as_bool() }
    }

    pub fn set_fullscreen(&mut self, x: i32, y: i32, width: u32, height: u32) {
        self.is_pip = false;
        self.width = width;
        self.height = height;
        unsafe {
            let _ = SetWindowLongPtrW(
                self.hwnd,
                GWL_EXSTYLE,
                (WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE).0
                    as isize,
            );
            let _ = SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
            let _ = SetWindowRgn(self.hwnd, HRGN::default(), true);
            let _ = SetWindowPos(
                self.hwnd,
                HWND_TOPMOST,
                x,
                y,
                width as i32,
                height as i32,
                SWP_FRAMECHANGED | SWP_SHOWWINDOW | SWP_NOACTIVATE,
            );
        }
    }

    pub fn set_pip(&mut self, x: i32, y: i32, width: u32, height: u32) {
        self.is_pip = true;
        self.width = width;
        self.height = height;
        if height > 0 {
            PIP_ASPECT_BITS.store(
                (width as f32 / height as f32).to_bits(),
                std::sync::atomic::Ordering::Release,
            );
        }
        unsafe {
            // Remove WS_EX_TRANSPARENT so the PiP window can be resized by edges and dragged by mouse
            let _ = SetWindowLongPtrW(
                self.hwnd,
                GWL_EXSTYLE,
                (WS_EX_TOPMOST | WS_EX_TOOLWINDOW).0 as isize,
            );
            let _ = SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 1);
            let rgn = CreateRoundRectRgn(0, 0, width as i32, height as i32, 16, 16);
            let _ = SetWindowRgn(self.hwnd, rgn, true);
            let _ = SetWindowPos(
                self.hwnd,
                HWND_TOPMOST,
                x,
                y,
                width as i32,
                height as i32,
                SWP_FRAMECHANGED | SWP_SHOWWINDOW | SWP_NOACTIVATE,
            );
        }
    }

    pub fn show(&self) {
        unsafe {
            let command = if self.is_pip {
                SW_SHOW
            } else {
                SW_SHOWNOACTIVATE
            };
            let _ = ShowWindow(self.hwnd, command);
        }
    }

    pub fn hide(&self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
    }

    /// Current outer window bounds. Keeping this in the window abstraction lets
    /// the service preserve geometry before temporarily turning PiP fullscreen.
    pub fn bounds(&self) -> Option<DisplayBounds> {
        let mut rect = RECT::default();
        unsafe { GetWindowRect(self.hwnd, &mut rect).ok()? };
        let width = (rect.right - rect.left).max(1) as u32;
        let height = (rect.bottom - rect.top).max(1) as u32;
        Some(DisplayBounds::new(rect.left, rect.top, width, height))
    }

    /// Returns a pending PiP context menu action and clears it, or None if no action.
    pub fn pop_pip_action(&self) -> Option<PipAction> {
        use std::sync::atomic::Ordering;
        match PIP_ACTION.swap(0, Ordering::AcqRel) {
            1 => Some(PipAction::HidePip),
            2 => Some(PipAction::GoToMonitor),
            _ => None,
        }
    }

    pub fn poll_events(&self) -> bool {
        let mut msg = MSG::default();
        unsafe {
            while PeekMessageW(&mut msg, self.hwnd, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_DESTROY {
                    return false;
                }
                DispatchMessageW(&msg);
            }
        }
        true
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let is_pip = GetWindowLongPtrW(hwnd, GWLP_USERDATA) == 1;

    match msg {
        WM_NCHITTEST => {
            if !is_pip {
                return LRESULT(HTTRANSPARENT as isize);
            }

            let mut rect = RECT::default();
            if GetWindowRect(hwnd, &mut rect).is_err() {
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            }

            let x = (lparam.0 & 0xffff) as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
            let border = 12; // 12px grip margin around borders for comfortable mouse stretching

            let on_left = x >= rect.left && x < rect.left + border;
            let on_right = x <= rect.right && x > rect.right - border;
            let on_top = y >= rect.top && y < rect.top + border;
            let on_bottom = y <= rect.bottom && y > rect.bottom - border;

            if on_top && on_left {
                LRESULT(HTTOPLEFT as isize)
            } else if on_top && on_right {
                LRESULT(HTTOPRIGHT as isize)
            } else if on_bottom && on_left {
                LRESULT(HTBOTTOMLEFT as isize)
            } else if on_bottom && on_right {
                LRESULT(HTBOTTOMRIGHT as isize)
            } else if on_left {
                LRESULT(HTLEFT as isize)
            } else if on_right {
                LRESULT(HTRIGHT as isize)
            } else if on_top {
                LRESULT(HTTOP as isize)
            } else if on_bottom {
                LRESULT(HTBOTTOM as isize)
            } else {
                // Clicking inside the PiP window allows dragging it anywhere on screen
                LRESULT(HTCAPTION as isize)
            }
        }
        WM_SIZING => {
            if is_pip {
                let rect_ptr = lparam.0 as *mut RECT;
                if !rect_ptr.is_null() {
                    let aspect =
                        f32::from_bits(PIP_ASPECT_BITS.load(std::sync::atomic::Ordering::Acquire));
                    constrain_pip_rect(&mut *rect_ptr, wparam.0, aspect);
                }
                return LRESULT(1);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_SIZE => {
            if is_pip {
                let w = (lparam.0 & 0xffff) as u32;
                let h = ((lparam.0 >> 16) & 0xffff) as u32;
                if w > 0 && h > 0 {
                    let rgn = CreateRoundRectRgn(0, 0, w as i32, h as i32, 16, 16);
                    let _ = SetWindowRgn(hwnd, rgn, true);
                }
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_NCRBUTTONUP | WM_RBUTTONUP | WM_CONTEXTMENU => {
            if is_pip {
                show_pip_popup_menu(hwnd);
                return LRESULT(0);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_COMMAND => {
            let item_id = (wparam.0 & 0xffff) as i32;
            if item_id == 1 || item_id == 2 {
                PIP_ACTION.store(item_id, std::sync::atomic::Ordering::Release);
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn constrain_pip_rect(rect: &mut RECT, sizing_edge: usize, aspect: f32) {
    let aspect = if aspect.is_finite() && aspect > 0.1 {
        aspect
    } else {
        16.0 / 9.0
    };
    let width = (rect.right - rect.left).max(240);
    let height = (rect.bottom - rect.top).max((240.0 / aspect).round() as i32);

    // WMSZ_TOP (3) and WMSZ_BOTTOM (6) are height-driven. Horizontal edges and
    // corners are width-driven, matching the edge the user is actively moving.
    let (new_width, new_height) = if matches!(sizing_edge, 3 | 6) {
        ((height as f32 * aspect).round() as i32, height)
    } else {
        (width, (width as f32 / aspect).round() as i32)
    };

    match sizing_edge {
        1 | 4 | 7 => rect.left = rect.right - new_width, // left edge
        _ => rect.right = rect.left + new_width,
    }
    match sizing_edge {
        3..=5 => rect.top = rect.bottom - new_height, // top edge
        _ => rect.bottom = rect.top + new_height,
    }
}

unsafe fn show_pip_popup_menu(hwnd: HWND) {
    let mut cursor_pos = POINT::default();
    let _ = GetCursorPos(&mut cursor_pos);
    if let Ok(hmenu) = CreatePopupMenu() {
        let (goto_text, hide_text) = pip_menu_labels();
        let goto_label: Vec<u16> = goto_text.encode_utf16().chain(std::iter::once(0)).collect();
        let hide_label: Vec<u16> = hide_text.encode_utf16().chain(std::iter::once(0)).collect();
        let _ = AppendMenuW(hmenu, MF_STRING, 2, PCWSTR(goto_label.as_ptr()));
        let _ = AppendMenuW(hmenu, MF_STRING, 1, PCWSTR(hide_label.as_ptr()));
        let _ = SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(
            hmenu,
            TPM_LEFTALIGN | TPM_RIGHTBUTTON | TPM_RETURNCMD,
            cursor_pos.x,
            cursor_pos.y,
            0,
            hwnd,
            None,
        );
        let _ = DestroyMenu(hmenu);
        if cmd.0 == 1 || cmd.0 == 2 {
            PIP_ACTION.store(cmd.0, std::sync::atomic::Ordering::Release);
        }
    }
}

fn pip_menu_labels() -> (&'static str, &'static str) {
    let preference = std::env::var_os("APPDATA")
        .map(std::path::PathBuf::from)
        .map(|path| path.join("EvertyDisplay").join("ui-settings.json"))
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|value| value.get("language")?.as_str().map(str::to_owned));
    let language = match preference.as_deref() {
        Some("russian") => 0,
        Some("english") => 1,
        Some("arabic") => 2,
        Some("spanish") => 3,
        Some("german") => 4,
        Some("french") => 5,
        _ => {
            let lang_id = unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() };
            match lang_id & 0x03ff {
                0x01 => 2,
                0x0a => 3,
                0x07 => 4,
                0x0c => 5,
                0x19 => 0,
                _ => 1,
            }
        }
    };
    match language {
        0 => ("Перейти на виртуальный монитор", "Скрыть PiP"),
        2 => ("الانتقال إلى الشاشة الافتراضية", "إخفاء PiP"),
        3 => ("Ir a la pantalla virtual", "Ocultar PiP"),
        4 => ("Zum virtuellen Bildschirm wechseln", "PiP ausblenden"),
        5 => ("Accéder à l’écran virtuel", "Masquer PiP"),
        _ => ("Go to virtual display", "Hide PiP"),
    }
}

fn compile_shader(code: &[u8], entry: &str, target: &str) -> Result<ID3DBlob> {
    use windows::core::PCSTR;

    let entry_c = std::ffi::CString::new(entry)?;
    let target_c = std::ffi::CString::new(target)?;

    let mut blob: Option<ID3DBlob> = None;
    let mut err_blob: Option<ID3DBlob> = None;

    let hr = unsafe {
        D3DCompile(
            code.as_ptr() as *const _,
            code.len(),
            PCSTR::null(),
            None,
            None,
            PCSTR(entry_c.as_ptr() as *const u8),
            PCSTR(target_c.as_ptr() as *const u8),
            0,
            0,
            &mut blob,
            Some(&mut err_blob),
        )
    };

    if let Err(e) = hr {
        if let Some(err) = err_blob {
            let msg = unsafe {
                let p = err.GetBufferPointer() as *const u8;
                let s = err.GetBufferSize();
                String::from_utf8_lossy(std::slice::from_raw_parts(p, s)).to_string()
            };
            anyhow::bail!("Shader compilation failed: {} ({:?})", msg, e);
        }
        return Err(e.into());
    }

    blob.context("No shader blob returned")
}

pub struct ViewportRenderer {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    swap_chain: IDXGISwapChain1,
    back_buffer: Option<ID3D11Texture2D>,
    render_target_view: Option<ID3D11RenderTargetView>,
    vertex_shader: ID3D11VertexShader,
    pixel_shader: ID3D11PixelShader,
    sampler: ID3D11SamplerState,
    scaling_texture: Option<(
        ID3D11Texture2D,
        windows::Win32::Graphics::Direct3D11::ID3D11ShaderResourceView,
        u32,
        u32,
    )>,
    duplicator: Option<DxgiOutputDuplicator>,
    current_source_device: Option<String>,
    cursor_overlay: CursorOverlay,
    cursor_overlay_required: bool,
    window: ViewportWindow,
}

impl ViewportRenderer {
    pub fn new(window: ViewportWindow) -> Result<Self> {
        let mut device = None;
        let mut context = None;
        let mut feature_level = D3D_FEATURE_LEVEL_11_0;

        let feature_levels = [D3D_FEATURE_LEVEL_11_0];

        unsafe {
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                None,
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                Some(&feature_levels),
                D3D11_SDK_VERSION,
                Some(&mut device),
                Some(&mut feature_level),
                Some(&mut context),
            )?;
        }

        let device = device.context("Failed to create D3D11 Device")?;
        let context = context.context("Failed to get D3D11 DeviceContext")?;

        let dxgi_device: windows::Win32::Graphics::Dxgi::IDXGIDevice = device.cast()?;
        let adapter = unsafe { dxgi_device.GetAdapter()? };
        let factory: IDXGIFactory2 = unsafe { adapter.GetParent()? };

        let swap_chain_desc = DXGI_SWAP_CHAIN_DESC1 {
            Width: window.width,
            Height: window.height,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            Stereo: false.into(),
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: 2,
            Scaling: windows::Win32::Graphics::Dxgi::DXGI_SCALING_NONE,
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
            AlphaMode: windows::Win32::Graphics::Dxgi::Common::DXGI_ALPHA_MODE_IGNORE,
            Flags: DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING.0 as u32,
        };

        let swap_chain = unsafe {
            factory.CreateSwapChainForHwnd(&device, window.hwnd, &swap_chain_desc, None, None)?
        };

        let back_buffer: ID3D11Texture2D = unsafe { swap_chain.GetBuffer(0)? };
        let mut render_target_view = None;
        unsafe {
            device.CreateRenderTargetView(&back_buffer, None, Some(&mut render_target_view))?;
        }
        let render_target_view = render_target_view.context("Failed to create RTV")?;

        // Compile fullscreen quad shaders for seamless scaling of any resolution to physical screen
        let vs_source = b"struct VSOutput { float4 pos : SV_POSITION; float2 uv : TEXCOORD0; };\nVSOutput main(uint id : SV_VertexID) {\n    VSOutput output;\n    output.uv = float2((id << 1) & 2, id & 2);\n    output.pos = float4(output.uv * float2(2.0f, -2.0f) + float2(-1.0f, 1.0f), 0.0f, 1.0f);\n    return output;\n}\n\0";
        // A borderless virtual display has no physical bezel to hide the DWM shadow that
        // neighboring windows paint a few pixels across the shared desktop edge. Keep the
        // Windows monitor rectangles touching for native cursor/window movement, but crop a
        // tiny visual safe area from the duplicated image before scaling it to the viewport.

        let vs_blob = compile_shader(vs_source, "main", "vs_4_0")?;
        let vs_slice = unsafe {
            std::slice::from_raw_parts(
                vs_blob.GetBufferPointer() as *const u8,
                vs_blob.GetBufferSize(),
            )
        };
        let mut vertex_shader = None;
        unsafe {
            device.CreateVertexShader(vs_slice, None, Some(&mut vertex_shader))?;
        }
        let vertex_shader = vertex_shader.context("Failed to create vertex shader")?;

        let ps_blob = compile_shader(VIEWPORT_PIXEL_SHADER, "main", "ps_4_0")?;
        let ps_slice = unsafe {
            std::slice::from_raw_parts(
                ps_blob.GetBufferPointer() as *const u8,
                ps_blob.GetBufferSize(),
            )
        };
        let mut pixel_shader = None;
        unsafe {
            device.CreatePixelShader(ps_slice, None, Some(&mut pixel_shader))?;
        }
        let pixel_shader = pixel_shader.context("Failed to create pixel shader")?;

        let sampler_desc = D3D11_SAMPLER_DESC {
            Filter: D3D11_FILTER_MIN_MAG_MIP_LINEAR,
            AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
            AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
            AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
            ComparisonFunc: D3D11_COMPARISON_NEVER,
            MinLOD: 0.0,
            MaxLOD: D3D11_FLOAT32_MAX,
            ..Default::default()
        };
        let mut sampler = None;
        unsafe {
            device.CreateSamplerState(&sampler_desc, Some(&mut sampler))?;
        }
        let sampler = sampler.context("Failed to create sampler")?;

        info!("ViewportRenderer initialized: D3D11 & SwapChain ready (with GPU bilinear scaler)");

        let cursor_overlay = CursorOverlay::new()?;

        Ok(Self {
            device,
            context,
            swap_chain,
            back_buffer: Some(back_buffer),
            render_target_view: Some(render_target_view),
            vertex_shader,
            pixel_shader,
            sampler,
            scaling_texture: None,
            duplicator: None,
            current_source_device: None,
            cursor_overlay,
            cursor_overlay_required: false,
            window,
        })
    }

    pub fn is_pip(&self) -> bool {
        self.window.is_pip()
    }

    pub fn is_visible(&self) -> bool {
        self.window.is_visible()
    }

    pub fn resize_buffers(&mut self, width: u32, height: u32) -> Result<()> {
        unsafe {
            self.context.OMSetRenderTargets(None, None);
            self.render_target_view = None;
            self.back_buffer = None;

            self.swap_chain.ResizeBuffers(
                0,
                width,
                height,
                windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_UNKNOWN,
                windows::Win32::Graphics::Dxgi::DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING,
            )?;

            let back_buffer: ID3D11Texture2D = self.swap_chain.GetBuffer(0)?;
            let mut render_target_view = None;
            self.device.CreateRenderTargetView(
                &back_buffer,
                None,
                Some(&mut render_target_view),
            )?;
            let render_target_view =
                render_target_view.context("Failed to create RTV after buffer resize")?;

            self.back_buffer = Some(back_buffer);
            self.render_target_view = Some(render_target_view);
        }
        info!("Resized Viewport SwapChain buffers to {}x{}", width, height);
        Ok(())
    }

    pub fn set_fullscreen(&mut self, x: i32, y: i32, width: u32, height: u32) {
        self.window.set_fullscreen(x, y, width, height);
        if let Err(error) = self.resize_buffers(width, height) {
            warn!("Could not resize fullscreen viewport buffers: {error:?}");
            self.window.width = 0;
            self.window.height = 0;
        }
    }

    pub fn set_pip(&mut self, x: i32, y: i32, width: u32, height: u32) {
        self.window.set_pip(x, y, width, height);
        self.cursor_overlay.hide();
        if let Err(error) = self.resize_buffers(width, height) {
            warn!("Could not resize PiP buffers: {error:?}");
            self.window.width = 0;
            self.window.height = 0;
        }
    }

    /// Switch the captured source to a different display device (e.g. "\\\\.\\DISPLAY2")
    pub fn switch_source(&mut self, device_name: &str) -> Result<()> {
        self.switch_source_internal(device_name, false)
    }

    /// Force re-initialization of the current source (e.g. after mode or refresh rate change)
    pub fn force_reinit_source(&mut self) -> Result<()> {
        if let Some(ref cur) = self.current_source_device.clone() {
            self.switch_source_internal(cur, true)
        } else {
            Ok(())
        }
    }

    fn switch_source_internal(&mut self, device_name: &str, force: bool) -> Result<()> {
        if !force {
            if let Some(ref cur) = self.current_source_device {
                if cur.eq_ignore_ascii_case(device_name) && self.duplicator.is_some() {
                    return Ok(());
                }
            }
        }

        info!(
            "Switching viewport source to: {} (force={})",
            device_name, force
        );
        self.duplicator = None; // Explicitly drop previous duplicator
        self.cursor_overlay_required = false;
        self.cursor_overlay.hide();

        match DxgiOutputDuplicator::new_for_device(&self.device, device_name) {
            Ok(duplicator) => {
                self.duplicator = Some(duplicator);
                self.current_source_device = Some(device_name.to_string());
                Ok(())
            }
            Err(e) => {
                warn!("Could not create duplicator for {}: {:?}", device_name, e);
                Err(e)
            }
        }
    }

    /// Return width and height of the currently captured desktop source
    pub fn get_source_dimensions(&self) -> Option<(u32, u32)> {
        self.duplicator.as_ref().map(|d| (d.width(), d.height()))
    }

    /// Render one frame from the active duplicator to physical screen
    /// Returns true if a frame was acquired and presented
    pub fn render_frame(&mut self, timeout_ms: u32) -> Result<bool> {
        // If the user stretched or resized the window with the mouse, update SwapChain buffers dynamically
        unsafe {
            let mut client_rect = RECT::default();
            if GetClientRect(self.window.hwnd, &mut client_rect).is_ok() {
                let cw = (client_rect.right - client_rect.left).max(1) as u32;
                let ch = (client_rect.bottom - client_rect.top).max(1) as u32;
                if cw != self.window.width || ch != self.window.height {
                    match self.resize_buffers(cw, ch) {
                        Ok(()) => {
                            self.window.width = cw;
                            self.window.height = ch;
                        }
                        Err(error) => warn!("Could not update resized PiP buffers: {error:?}"),
                    }
                }
            }
        }

        let Some(ref mut duplicator) = self.duplicator else {
            return Ok(false);
        };

        let (src_texture, frame_info) = match duplicator.acquire_next_frame(timeout_ms) {
            Ok(Some(frame)) => frame,
            Ok(None) => return Ok(false), // Timeout, no new desktop update
            Err(e) => {
                warn!(
                    "Frame acquisition error: {:?}, dropping duplicator and re-initializing",
                    e
                );
                self.duplicator = None;
                if let Some(name) = self.current_source_device.clone() {
                    let _ = self.switch_source_internal(&name, true);
                }
                return Ok(false);
            }
        };

        // Desktop Duplication can deliver the pointer either already composed
        // into the texture or as a separate hardware overlay. Drawing our own
        // cursor in the former case creates the visible double-pointer effect.
        // A zero timestamp means this frame carries no pointer-state update, so
        // preserve the decision from the previous frame.
        if frame_info.LastMouseUpdateTime != 0 {
            self.cursor_overlay_required = frame_info.PointerPosition.Visible.as_bool();
            if !self.cursor_overlay_required {
                self.cursor_overlay.hide();
            }
        }

        if self.back_buffer.is_none() {
            return Ok(false);
        }
        let Some(ref render_target_view) = self.render_target_view else {
            return Ok(false);
        };

        // Fast GPU copy/blit:
        unsafe {
            let mut src_desc =
                windows::Win32::Graphics::Direct3D11::D3D11_TEXTURE2D_DESC::default();
            src_texture.GetDesc(&mut src_desc);

            {
                // Always use the shader, even at matching resolutions, so the visual safe-area
                // crop is applied consistently and neighboring window shadows cannot leak in.
                let mut need_new = true;
                if let Some((_, _, w, h)) = &self.scaling_texture {
                    if *w == src_desc.Width && *h == src_desc.Height {
                        need_new = false;
                    }
                }

                if need_new {
                    let desc = windows::Win32::Graphics::Direct3D11::D3D11_TEXTURE2D_DESC {
                        Width: src_desc.Width,
                        Height: src_desc.Height,
                        MipLevels: 1,
                        ArraySize: 1,
                        Format: src_desc.Format,
                        SampleDesc: DXGI_SAMPLE_DESC {
                            Count: 1,
                            Quality: 0,
                        },
                        Usage: windows::Win32::Graphics::Direct3D11::D3D11_USAGE_DEFAULT,
                        BindFlags: windows::Win32::Graphics::Direct3D11::D3D11_BIND_SHADER_RESOURCE
                            .0 as u32,
                        ..Default::default()
                    };

                    let mut tex = None;
                    if self
                        .device
                        .CreateTexture2D(&desc, None, Some(&mut tex))
                        .is_ok()
                    {
                        if let Some(tex) = tex {
                            let mut srv = None;
                            if self
                                .device
                                .CreateShaderResourceView(&tex, None, Some(&mut srv))
                                .is_ok()
                            {
                                if let Some(srv) = srv {
                                    self.scaling_texture =
                                        Some((tex, srv, src_desc.Width, src_desc.Height));
                                }
                            }
                        }
                    }
                }

                if let Some((ref scale_tex, ref scale_srv, _, _)) = self.scaling_texture {
                    self.context.CopyResource(scale_tex, &src_texture);

                    self.context
                        .OMSetRenderTargets(Some(&[Some(render_target_view.clone())]), None);
                    self.context
                        .IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
                    self.context.IASetInputLayout(None);
                    self.context.VSSetShader(&self.vertex_shader, None);
                    self.context.PSSetShader(&self.pixel_shader, None);
                    self.context
                        .PSSetShaderResources(0, Some(&[Some(scale_srv.clone())]));
                    self.context
                        .PSSetSamplers(0, Some(&[Some(self.sampler.clone())]));

                    let vp = D3D11_VIEWPORT {
                        TopLeftX: 0.0,
                        TopLeftY: 0.0,
                        Width: self.window.width as f32,
                        Height: self.window.height as f32,
                        MinDepth: 0.0,
                        MaxDepth: 1.0,
                    };
                    self.context.RSSetViewports(Some(&[vp]));
                    self.context.Draw(3, 0);

                    // Unbind SRV so texture isn't held locked by pipeline
                    self.context.PSSetShaderResources(0, Some(&[None]));
                }
            }
        }

        // Release the frame so DWM can reuse surface
        duplicator.release_frame()?;

        // Present with ALLOW_TEARING for lowest latency (< 1ms)
        unsafe {
            let flags = DXGI_PRESENT_ALLOW_TEARING.0;
            self.swap_chain.Present(0, DXGI_PRESENT(flags)).ok()?;
        }

        Ok(true)
    }

    pub fn show(&self) {
        self.window.show();
    }

    pub fn hide(&mut self) {
        self.cursor_overlay.hide();
        self.window.hide();
    }

    pub fn update_cursor(&mut self, screen_x: i32, screen_y: i32) {
        if self.cursor_overlay_required {
            self.cursor_overlay.update(screen_x, screen_y);
        } else {
            self.cursor_overlay.hide();
        }
    }

    /// Map a cursor hotspot from source-display pixels to the viewport using
    /// exactly the same safe-area crop as the pixel shader.
    pub fn map_source_cursor_to_window(&self, source_x: i32, source_y: i32) -> Option<(i32, i32)> {
        let (source_width, source_height) = self.get_source_dimensions()?;
        let destination = self.window.bounds()?;
        Some((
            map_source_axis(source_x, source_width, destination.x, destination.width),
            map_source_axis(source_y, source_height, destination.y, destination.height),
        ))
    }

    pub fn hide_cursor(&mut self) {
        self.cursor_overlay.hide();
    }

    pub fn window(&self) -> &ViewportWindow {
        &self.window
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewport_safe_area_shader_compiles() {
        compile_shader(VIEWPORT_PIXEL_SHADER, "main", "ps_4_0")
            .expect("viewport pixel shader must compile");
    }

    #[test]
    fn cursor_mapping_preserves_one_to_one_source_coordinates() {
        assert_eq!(map_source_axis(0, 1920, 100, 1920), 100);
        assert_eq!(map_source_axis(6, 1920, 100, 1920), 106);
        assert_eq!(map_source_axis(960, 1920, 100, 1920), 1060);
        assert_eq!(map_source_axis(1919, 1920, 100, 1920), 2019);
    }

    #[test]
    fn pip_resize_preserves_source_aspect_and_dragged_edge() {
        let mut right = RECT {
            left: 100,
            top: 100,
            right: 580,
            bottom: 300,
        };
        constrain_pip_rect(&mut right, 2, 16.0 / 9.0);
        assert_eq!(right.left, 100);
        assert_eq!(right.right, 580);
        assert_eq!(right.bottom - right.top, 270);

        let mut top = RECT {
            left: 100,
            top: 100,
            right: 500,
            bottom: 460,
        };
        constrain_pip_rect(&mut top, 3, 16.0 / 9.0);
        assert_eq!(top.bottom, 460);
        assert_eq!(top.bottom - top.top, 360);
        assert_eq!(top.right - top.left, 640);
    }
}
