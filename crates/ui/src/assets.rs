#![allow(dead_code)]
use iced::widget::svg;
use iced::{Element, Length};

pub const ICON_PLUS: &[u8] = include_bytes!("../../../assets/sprites/ui/plus.svg");
pub const ICON_SETTINGS: &[u8] = include_bytes!("../../../assets/sprites/ui/settings.svg");
pub const ICON_REFRESH: &[u8] = include_bytes!("../../../assets/sprites/ui/refresh.svg");
pub const ICON_GRID: &[u8] = include_bytes!("../../../assets/sprites/ui/grid.svg");
pub const ICON_TRASH: &[u8] = include_bytes!("../../../assets/sprites/ui/trash.svg");
pub const ICON_ARROW_LEFT: &[u8] = include_bytes!("../../../assets/sprites/ui/arrow_left.svg");
pub const ICON_ARROW_RIGHT: &[u8] = include_bytes!("../../../assets/sprites/ui/arrow_right.svg");
pub const ICON_EYE: &[u8] = include_bytes!("../../../assets/sprites/ui/eye.svg");
pub const ICON_MONITOR: &[u8] = include_bytes!("../../../assets/sprites/ui/hard_drive.svg");
pub const ICON_INFO: &[u8] = include_bytes!("../../../assets/sprites/ui/info.svg");
pub const ICON_STATUS_PAUSE: &[u8] =
    include_bytes!("../../../assets/sprites/status/status_pause.svg");
pub const ICON_STATUS_PLAY: &[u8] =
    include_bytes!("../../../assets/sprites/status/status_play.svg");
pub const ICON_STATUS_CHECK: &[u8] =
    include_bytes!("../../../assets/sprites/status/status_check.svg");
pub const ICON_STATUS_WARN: &[u8] =
    include_bytes!("../../../assets/sprites/status/status_warning.svg");
pub const ICON_SUN: &[u8] = include_bytes!("../../../assets/sprites/ui/sun.svg");
pub const ICON_MOON: &[u8] = include_bytes!("../../../assets/sprites/ui/moon.svg");
pub const ICON_ZAP: &[u8] = include_bytes!("../../../assets/sprites/ui/zap.svg");
pub const ICON_ROCKET: &[u8] = include_bytes!("../../../assets/sprites/ui/rocket.svg");
pub const ICON_BELL: &[u8] = include_bytes!("../../../assets/sprites/ui/bell.svg");
pub const ICON_GAMEPAD: &[u8] = include_bytes!("../../../assets/sprites/ui/gamepad.svg");
pub const ICON_WINDOW: &[u8] = include_bytes!("../../../assets/sprites/ui/window.svg");
pub const ICON_PIP: &[u8] = include_bytes!("../../../assets/sprites/ui/pip.svg");
pub const ICON_KEYBOARD: &[u8] = include_bytes!("../../../assets/sprites/ui/keyboard.svg");
pub const ICON_MINUS: &[u8] = include_bytes!("../../../assets/sprites/ui/minus.svg");
pub const ICON_MAXIMIZE: &[u8] = include_bytes!("../../../assets/sprites/ui/maximize.svg");
pub const ICON_RESTORE: &[u8] = include_bytes!("../../../assets/sprites/ui/restore.svg");
pub const ICON_CLOSE: &[u8] = include_bytes!("../../../assets/sprites/ui/close.svg");
pub const LOGO_MARK: &[u8] = include_bytes!("../../../assets/logo_mark.svg");

pub const LOGO_IN_APP: &[u8] = include_bytes!("../../../assets/logo_in_app_evertydisplay.png");
pub const LOGO_ICON: &[u8] = include_bytes!("../../../assets/logo_icon_evertydisplay.png");

pub fn render_svg<'a, Message: 'a>(
    raw_bytes: &'static [u8],
    size: f32,
    hex_color: Option<&str>,
) -> Element<'a, Message> {
    let handle = if let Some(hex) = hex_color {
        let content = std::str::from_utf8(raw_bytes).unwrap_or("");
        let colored = content
            .replace("color=\"#17223B\"", &format!("color=\"{}\"", hex))
            .replace("currentColor", hex);
        svg::Handle::from_memory(colored.into_bytes())
    } else {
        svg::Handle::from_memory(raw_bytes)
    };

    svg(handle)
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .into()
}
