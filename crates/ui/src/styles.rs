#![allow(dead_code)]
use iced::widget::{button, container, text_input};
use iced::{Background, Border, Color, Shadow, Vector};

// ---------------------------------------------------------------------------
// EvertyCloud Design Tokens
// ---------------------------------------------------------------------------

pub const PRIMARY: Color = Color::from_rgb(0.357, 0.298, 1.0); // #5B4CFF
pub const PRIMARY_HOVER: Color = Color::from_rgb(0.424, 0.298, 1.0); // #6C4CFF
pub const PRIMARY_PRESSED: Color = Color::from_rgb(0.302, 0.200, 0.800); // #4D33CC
pub const PRIMARY_SOFT: Color = Color::from_rgb(0.933, 0.949, 1.0); // #EEF2FF

pub const ACCENT_CYAN: Color = Color::from_rgb(0.220, 0.741, 0.973); // #38BDF8
pub const SUCCESS: Color = Color::from_rgb(0.133, 0.773, 0.369); // #22C55E
pub const WARNING: Color = Color::from_rgb(0.961, 0.620, 0.043); // #F59E0B
pub const DANGER: Color = Color::from_rgb(0.937, 0.267, 0.267); // #EF4444

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Dark,
    Light,
}

impl ThemeMode {
    pub fn bg_app(&self) -> Color {
        match self {
            ThemeMode::Light => Color::from_rgb(0.969, 0.973, 0.988), // #F7F8FC
            ThemeMode::Dark => Color::from_rgb(0.059, 0.067, 0.090),  // #0F1117
        }
    }

    pub fn bg_sidebar(&self) -> Color {
        match self {
            ThemeMode::Light => Color::WHITE,
            ThemeMode::Dark => Color::from_rgb(0.075, 0.086, 0.125), // #131620
        }
    }

    pub fn bg_surface(&self) -> Color {
        match self {
            ThemeMode::Light => Color::WHITE,
            ThemeMode::Dark => Color::from_rgb(0.094, 0.106, 0.149), // #181B26
        }
    }

    pub fn bg_surface_subtle(&self) -> Color {
        match self {
            ThemeMode::Light => Color::from_rgb(0.953, 0.957, 0.976), // #F3F4F9
            ThemeMode::Dark => Color::from_rgb(0.125, 0.141, 0.200),  // #202433
        }
    }

    pub fn bg_hover(&self) -> Color {
        match self {
            ThemeMode::Light => Color::from_rgb(0.961, 0.953, 1.0), // #F5F3FF
            ThemeMode::Dark => Color::from_rgb(0.145, 0.165, 0.239), // #252A3D
        }
    }

    pub fn border(&self) -> Color {
        match self {
            ThemeMode::Light => Color::from_rgb(0.906, 0.918, 0.941), // #E7EAF0
            ThemeMode::Dark => Color::from_rgb(0.157, 0.180, 0.259),  // #282E42
        }
    }

    pub fn border_focus(&self) -> Color {
        PRIMARY
    }

    pub fn text_primary(&self) -> Color {
        match self {
            ThemeMode::Light => Color::from_rgb(0.090, 0.133, 0.231), // #17223B
            ThemeMode::Dark => Color::from_rgb(0.973, 0.980, 0.988),  // #F8FAFC
        }
    }

    pub fn text_secondary(&self) -> Color {
        match self {
            ThemeMode::Light => Color::from_rgb(0.400, 0.439, 0.522), // #667085
            ThemeMode::Dark => Color::from_rgb(0.580, 0.639, 0.722),  // #94A3B8
        }
    }

    pub fn text_muted(&self) -> Color {
        match self {
            ThemeMode::Light => Color::from_rgb(0.596, 0.635, 0.702), // #98A2B3
            ThemeMode::Dark => Color::from_rgb(0.392, 0.455, 0.545),  // #64748B
        }
    }

    pub fn shadow_card(&self) -> Shadow {
        match self {
            ThemeMode::Light => Shadow {
                color: Color::from_rgba(0.063, 0.094, 0.157, 0.06),
                offset: Vector::new(0.0, 3.0),
                blur_radius: 10.0,
            },
            ThemeMode::Dark => Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.35),
                offset: Vector::new(0.0, 3.0),
                blur_radius: 10.0,
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Reusable Style Builders
// ---------------------------------------------------------------------------

pub fn card_style(theme: ThemeMode) -> container::Style {
    container::Style {
        background: Some(Background::Color(theme.bg_surface())),
        border: Border {
            color: theme.border(),
            width: 1.0,
            radius: 12.0.into(),
        },
        shadow: theme.shadow_card(),
        text_color: Some(theme.text_primary()),
    }
}

pub fn active_card_style(theme: ThemeMode) -> container::Style {
    container::Style {
        background: Some(Background::Color(match theme {
            ThemeMode::Light => Color::from_rgb(0.961, 0.953, 1.0),
            ThemeMode::Dark => Color::from_rgb(0.122, 0.129, 0.220),
        })),
        border: Border {
            color: PRIMARY,
            width: 2.0,
            radius: 12.0.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.357, 0.298, 1.0, 0.2),
            offset: Vector::new(0.0, 4.0),
            blur_radius: 14.0,
        },
        text_color: Some(theme.text_primary()),
    }
}

pub fn sidebar_style(theme: ThemeMode) -> container::Style {
    container::Style {
        background: Some(Background::Color(theme.bg_sidebar())),
        border: Border {
            color: theme.border(),
            width: 1.0,
            radius: 0.0.into(),
        },
        shadow: Shadow::default(),
        text_color: Some(theme.text_primary()),
    }
}

pub fn header_style(theme: ThemeMode) -> container::Style {
    container::Style {
        background: Some(Background::Color(theme.bg_surface())),
        border: Border {
            color: theme.border(),
            width: 1.0,
            radius: 12.0.into(),
        },
        shadow: theme.shadow_card(),
        text_color: Some(theme.text_primary()),
    }
}

pub fn primary_button(status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => PRIMARY_HOVER,
        button::Status::Pressed => PRIMARY_PRESSED,
        button::Status::Disabled => Color::from_rgb(0.792, 0.769, 0.992),
        _ => PRIMARY,
    };
    button::Style {
        background: Some(bg.into()),
        text_color: Color::WHITE,
        border: Border {
            radius: 10.0.into(),
            ..Default::default()
        },
        shadow: Shadow {
            color: Color::from_rgba(0.357, 0.298, 1.0, 0.25),
            offset: Vector::new(0.0, 2.0),
            blur_radius: 6.0,
        },
    }
}

pub fn secondary_button(theme: ThemeMode, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => theme.bg_hover(),
        button::Status::Pressed => theme.bg_surface_subtle(),
        _ => theme.bg_surface(),
    };
    button::Style {
        background: Some(bg.into()),
        text_color: theme.text_primary(),
        border: Border {
            color: match status {
                button::Status::Hovered => PRIMARY,
                _ => theme.border(),
            },
            width: 1.0,
            radius: 10.0.into(),
        },
        ..Default::default()
    }
}

pub fn danger_button(status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color::from_rgb(0.863, 0.149, 0.149),
        button::Status::Pressed => Color::from_rgb(0.725, 0.110, 0.110),
        _ => DANGER,
    };
    button::Style {
        background: Some(bg.into()),
        text_color: Color::WHITE,
        border: Border {
            radius: 10.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn nav_item_button(theme: ThemeMode, is_active: bool, status: button::Status) -> button::Style {
    let bg = if is_active {
        match theme {
            ThemeMode::Light => Color::from_rgb(0.933, 0.949, 1.0),
            ThemeMode::Dark => Color::from_rgb(0.137, 0.149, 0.251),
        }
    } else {
        match status {
            button::Status::Hovered => theme.bg_hover(),
            _ => Color::TRANSPARENT,
        }
    };

    let text_color = if is_active {
        PRIMARY
    } else {
        theme.text_secondary()
    };

    button::Style {
        background: Some(bg.into()),
        text_color,
        border: Border {
            color: if is_active {
                Color::from_rgba(0.357, 0.298, 1.0, 0.3)
            } else {
                Color::TRANSPARENT
            },
            width: if is_active { 1.0 } else { 0.0 },
            radius: 10.0.into(),
        },
        ..Default::default()
    }
}

pub fn input_style(theme: ThemeMode, status: text_input::Status) -> text_input::Style {
    let border_color = match status {
        text_input::Status::Focused => PRIMARY,
        text_input::Status::Hovered => theme.border_focus(),
        _ => theme.border(),
    };

    text_input::Style {
        background: Background::Color(theme.bg_surface()),
        border: Border {
            color: border_color,
            width: 1.0,
            radius: 8.0.into(),
        },
        icon: theme.text_muted(),
        placeholder: theme.text_muted(),
        value: theme.text_primary(),
        selection: Color::from_rgb(0.933, 0.949, 1.0),
    }
}

pub fn titlebar_style(theme: ThemeMode) -> container::Style {
    container::Style {
        background: Some(Background::Color(theme.bg_sidebar())),
        border: Border {
            color: theme.border(),
            width: 1.0,
            radius: 0.0.into(),
        },
        text_color: Some(theme.text_primary()),
        ..Default::default()
    }
}

pub fn window_control_button(
    theme: ThemeMode,
    is_close: bool,
    status: button::Status,
) -> button::Style {
    let (bg, icon_color) = if is_close {
        match status {
            button::Status::Hovered => (Color::from_rgb(0.937, 0.267, 0.267), Color::WHITE),
            button::Status::Pressed => (Color::from_rgb(0.800, 0.200, 0.200), Color::WHITE),
            _ => (Color::TRANSPARENT, theme.text_secondary()),
        }
    } else {
        match status {
            button::Status::Hovered => (theme.bg_hover(), theme.text_primary()),
            button::Status::Pressed => (theme.border(), theme.text_primary()),
            _ => (Color::TRANSPARENT, theme.text_secondary()),
        }
    };

    button::Style {
        background: Some(Background::Color(bg)),
        text_color: icon_color,
        border: Border::default(),
        shadow: Shadow::default(),
    }
}

pub fn tooltip_style(theme: ThemeMode) -> container::Style {
    container::Style {
        background: Some(Background::Color(match theme {
            ThemeMode::Light => Color::from_rgb(0.090, 0.133, 0.231),
            ThemeMode::Dark => Color::from_rgb(0.125, 0.141, 0.200),
        })),
        border: Border {
            color: match theme {
                ThemeMode::Light => Color::TRANSPARENT,
                ThemeMode::Dark => Color::from_rgb(0.25, 0.28, 0.38),
            },
            width: 1.0,
            radius: 6.0.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.25),
            offset: Vector::new(0.0, 2.0),
            blur_radius: 8.0,
        },
        text_color: Some(Color::WHITE),
    }
}
